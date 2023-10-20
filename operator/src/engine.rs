use nix::{
    errno::{self, errno, Errno},
    libc::{
        dup2, open, siginfo_t, O_APPEND, O_CREAT, O_WRONLY, STDERR_FILENO, STDOUT_FILENO, S_IRGRP,
        S_IRUSR, S_IWGRP, S_IWUSR,
    },
    poll::{poll, PollFd, PollFlags},
    sys::{
        signal::{sigaction, SaFlags, SigAction, SigSet, Signal},
        wait::{waitpid, WaitStatus},
    },
    unistd::{fork, ForkResult, Pid},
};

use crate::{
    ipc::{self, IPCMessage},
    service::Service,
};
use log::{error, info};
use std::{
    collections::HashMap,
    ffi::CString,
    os::fd::{AsFd, AsRawFd},
    process::exit,
};

/// Handles the services
#[derive(Default)]
pub struct Engine {
    services: HashMap<i32, Service>,
}

impl Engine {
    /// Create a new service runner engine
    pub fn new() -> Self {
        info!("Creating a new Engine...");
        Self::default()
    }

    extern "C" fn signal_handler(
        _sig: std::ffi::c_int,
        s_info: *mut siginfo_t,
        _ctx: *mut std::ffi::c_void,
    ) {
        if let Err(e) = comms::write_to_pipe(unsafe { s_info.as_ref().unwrap().si_pid() }) {
            error!("Failed to write to pipe: {e}");
        }
    }

    /// Start the engine and manage the services
    pub fn run(&mut self) {
        // setup a signal handler for SIGCHILD
        let sa = SigAction::new(
            nix::sys::signal::SigHandler::SigAction(Self::signal_handler),
            SaFlags::SA_RESTART | SaFlags::SA_SIGINFO,
            SigSet::empty(),
        );

        match unsafe { sigaction(Signal::SIGCHLD, &sa) } {
            Ok(sigac) => {
                info!("Signal handler registered: {sigac:?}");
            }
            Err(e) => {
                error!("Failed to register signal handler: {e}");
                return;
            }
        }

        let op_service_dir =
            std::env::var("OP_SERVICE_DIR").unwrap_or_else(|_| "/tmp/op".to_string());
        let op_service_log_dir =
            std::env::var("OP_SERVICE_LOG_DIR").unwrap_or_else(|_| "/tmp/oplogs".to_string());

        let service_files = Service::read_service_files(&op_service_dir).unwrap();
        for mut service in service_files.into_iter() {
            info!("Handing service creation for {service:?}");
            match unsafe { fork() }.unwrap() {
                ForkResult::Parent { child } => {
                    service.status = Some(crate::service::ServiceStatus::Running);
                    service.pid = Some(child.as_raw());

                    self.services.insert(child.as_raw(), service);
                }
                ForkResult::Child => {
                    info!("{}: executing {:?}", service.name, service.executable);

                    let exe_path = CString::new(service.executable.to_str().unwrap()).unwrap();

                    let mut args = if let Some(ref args) = service.args {
                        [exe_path.as_ptr()]
                            .into_iter()
                            .chain(args.iter().map(|arg| arg.as_ptr()))
                            .collect::<Vec<_>>()
                    } else {
                        vec![exe_path.as_ptr()]
                    };

                    // null terminate the args array
                    args.push(core::ptr::null());

                    // create the log file for the service
                    let stdout_file_path =
                        CString::new(format!("{}/{}.log", op_service_log_dir, service.name))
                            .unwrap();
                    let log_fd = unsafe {
                        open(
                            stdout_file_path.as_ptr(),
                            O_WRONLY | O_CREAT | O_APPEND,
                            (S_IRUSR | S_IWUSR | S_IRGRP | S_IWGRP) as std::ffi::c_uint,
                        )
                    };

                    if log_fd == -1 {
                        error!(
                            "Failed to create log file {}",
                            errno::Errno::from_i32(errno())
                        );
                    }

                    info!(
                        "Creating log file for {} at {:?} [FD {log_fd}]",
                        service.name, stdout_file_path
                    );

                    // set the stdout and stderr to the log file
                    unsafe {
                        dup2(log_fd, STDOUT_FILENO);
                        dup2(log_fd, STDERR_FILENO);
                    }

                    let res = unsafe { nix::libc::execv(exe_path.as_ptr(), args.as_ptr()) };
                    if res == -1 {
                        error!("exec() Failed with {res}");
                        error!("errno: {}", Errno::from_i32(errno()));
                        exit(-1)
                    }
                }
            }
        }

        let ipc_server = ipc::IPCServer::new().unwrap();

        // fd for the read end of the pipe
        let r_fd = comms::read_fd();
        let ipc_fd = ipc_server.as_fd();
        loop {
            // wait for notification to read from the pipe
            let mut fds = vec![
                PollFd::new(&r_fd, PollFlags::POLLIN),
                PollFd::new(&ipc_fd, PollFlags::POLLIN),
            ];

            while let Err(e) = poll(&mut fds, -1) {
                match e {
                    Errno::EINTR => continue,
                    e => {
                        panic!("select() failed with {e}");
                    }
                }
            }

            for fd in fds {
                if fd.revents().unwrap().bits() >= 1 {
                    if fd.as_fd().as_raw_fd() == r_fd.as_raw_fd() {
                        //  this is the pipe fd
                        // read from the pipe for childs that have exited
                        if let Ok(pid) = comms::read_from_pipe() {
                            let wait_stat = match waitpid(Pid::from_raw(pid), None) {
                                Ok(ws) => ws,
                                Err(e) => {
                                    error!("waitpid() for PID {} failed : {e}.", pid);
                                    continue;
                                }
                            };

                            if let Some(service) = self.services.get_mut(&pid) {
                                match wait_stat {
                                    WaitStatus::Exited(_, _) => {
                                        service.status =
                                            Some(crate::service::ServiceStatus::Stopped);
                                    }
                                    e => {
                                        info!("waitpid() returned {e:?}")
                                    }
                                }
                            }
                        } else {
                            continue;
                        }
                    } else {
                        let stream = ipc_server.accept().unwrap();
                        let msg = stream.read().unwrap();

                        match msg {
                            IPCMessage::Start { .. } => {}
                            IPCMessage::Stop { .. } => {}
                            IPCMessage::Status { name } => {
                                if let Some((pid, service)) =
                                    self.services.iter().find(|(_, v)| v.name == name)
                                {
                                    stream
                                        .write(&IPCMessage::StatusResponse(Some((
                                            *pid,
                                            service.status.unwrap(),
                                        ))))
                                        .unwrap();
                                } else {
                                    stream.write(&IPCMessage::StatusResponse(None)).unwrap();
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

/// Helper functions for communicating b/w single handler and engine using pipes.
mod comms {
    use std::os::fd::BorrowedFd;

    use lazy_static::lazy_static;
    use nix::unistd::{pipe, read, write};

    use serde::{Deserialize, Serialize};

    /// All the signal data provided by signal handler
    #[derive(Debug, Serialize, Deserialize)]
    pub struct SignalData {
        /// process id of the child
        pub pid: i32,
        /// user id of the child
        pub uid: u32,
        /// status of the child
        pub status: i32,
        /// errno of the child
        pub errno: i32,
        /// im not sure actually what code is
        pub code: i32,
    }

    lazy_static! {
        /// This pipe is used to send data b/w signal handler and engine.
        ///
        /// PIPES.0 - read fd
        /// PIPES.1 - write fd
        static ref PIPES: (i32, i32) = pipe().unwrap();
    }

    /// Read signal data from the pipe if any
    ///
    /// NOTE: Does not block
    pub fn read_from_pipe() -> anyhow::Result<i32> {
        let mut buf = [0; 4];
        let n_bytes = read(PIPES.0, &mut buf)?;

        if n_bytes == 0 {
            anyhow::bail!("Faild to read, probably invalid")
        } else {
            debug_assert!(n_bytes == buf.len());
            Ok(i32::from_le_bytes(buf))
        }
    }

    /// Write signal data to a pipe
    ///
    /// NOTE: Does not block
    #[inline]
    pub fn write_to_pipe(val: i32) -> anyhow::Result<()> {
        let n_bytes = write(PIPES.1, &val.to_le_bytes())?;
        debug_assert!(n_bytes == std::mem::size_of::<i32>());
        Ok(())
    }

    /// Returns a BorrowedFd
    pub fn read_fd<'a>() -> BorrowedFd<'a> {
        unsafe { BorrowedFd::borrow_raw(PIPES.0) }
    }
}
