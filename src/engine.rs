use nix::{
    errno::{self, errno, Errno},
    libc::{
        dup2, open, siginfo_t, O_APPEND, O_CREAT, O_WRONLY, STDERR_FILENO, STDOUT_FILENO, S_IRGRP,
        S_IRUSR, S_IWGRP, S_IWUSR,
    },
    poll::{poll, PollFd, PollFlags},
    sys::signal::{sigaction, SaFlags, SigAction, SigSet, Signal},
    unistd::{fork, ForkResult},
};

use crate::service::Service;
use log::{error, info};
use std::{collections::HashMap, ffi::CString};

#[allow(dead_code)]
/// Status of the service
pub enum ServiceStatus {
    /// The service is running
    Running,
    /// The service exited
    Finished,
}

/// Handles the services
pub struct Engine {
    service_files: Vec<Service>,
    _db: HashMap<Service, ServiceStatus>,
    #[allow(dead_code)]
    op_service_dir: String,
    op_service_log_dir: String,
}

impl Engine {
    /// Create a new service runner engine
    pub fn new() -> Self {
        info!("Creating a new Engine...");

        let op_service_dir =
            std::env::var("OP_SERVICE_DIR").unwrap_or_else(|_| "/tmp/op".to_string());

        let op_service_log_dir =
            std::env::var("OP_SERVICE_LOG_DIR").unwrap_or_else(|_| "/tmp/oplogs".to_string());

        let service_files = Service::read_service_files(&op_service_dir).unwrap();

        Self {
            service_files,
            op_service_dir,
            op_service_log_dir,
            _db: HashMap::new(),
        }
    }

    extern "C" fn signal_handler(
        _sig: std::ffi::c_int,
        s_info: *mut siginfo_t,
        _ctx: *mut std::ffi::c_void,
    ) {
        let s_info = unsafe { s_info.as_ref() }.unwrap();
        let data = unsafe {
            comms::SignalData {
                pid: s_info.si_pid(),
                uid: s_info.si_uid(),
                status: s_info.si_status(),
                errno: s_info.si_errno,
                code: s_info.si_code,
            }
        };

        if let Err(e) = comms::write_to_pipe(data) {
            error!("Failed to write to pipe: {e}");
        }
    }

    /// Start the engine and manage the services
    pub fn run(&self) {
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

        for service in self.service_files.iter() {
            info!("Handing service creation for {service:?}");
            match unsafe { fork() }.unwrap() {
                ForkResult::Parent { .. } => {
                    // TODO: book keep the process
                }
                ForkResult::Child => {
                    info!("{}: executing {:?}", service.name, service.executable);

                    let exe_path = CString::new(service.executable.to_str().unwrap()).unwrap();

                    let args = if let Some(ref args) = service.args {
                        [exe_path.as_ptr()]
                            .into_iter()
                            .chain(args.iter().map(|arg| arg.as_ptr()))
                            .collect::<Vec<_>>()
                    } else {
                        vec![exe_path.as_ptr()]
                    };

                    // create the log file for the service
                    let stdout_file_path =
                        CString::new(format!("{}/{}.log", self.op_service_log_dir, service.name))
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

                    let _res = unsafe { nix::libc::execv(exe_path.as_ptr(), args.as_ptr()) };
                }
            }
        }

        // fd for the read end of the pipe
        let r_fd = comms::read_fd();
        loop {
            // wait for notification to read from the pipe
            let mut fds = vec![PollFd::new(&r_fd, PollFlags::POLLIN)];
            while let Err(e) = poll(&mut fds, -1) {
                match e {
                    Errno::EINTR => continue,
                    e => {
                        panic!("select() failed with {e}");
                    }
                }
            }

            // read from the pipe for childs that have exited
            match comms::read_from_pipe() {
                Ok(val) => {
                    // TODO: update child process status
                    info!("Got signal data: {val:?}");
                }
                Err(_e) => {}
            }
        }
    }
}

/// Helper functions for communicating b/w single handler and engine using pipes.
mod comms {
    use std::os::fd::BorrowedFd;

    use anyhow::Error;
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
    pub fn read_from_pipe() -> anyhow::Result<SignalData> {
        // create a buffer and set the len
        let mut buf = vec![0; std::mem::size_of::<SignalData>()];

        let n_bytes = read(PIPES.0, &mut buf)?;
        if n_bytes == 0 {
            anyhow::bail!("Faild to read, probably invalid")
        } else {
            debug_assert!(n_bytes == buf.len());

            let val = bincode::deserialize(&buf).map_err(|err| Error::msg(format!("{err}")))?;
            Ok(val)
        }
    }

    /// Write signal data to a pipe
    ///
    /// NOTE: Does not block
    pub fn write_to_pipe(val: SignalData) -> anyhow::Result<()> {
        let data = bincode::serialize(&val).map_err(|err| Error::msg(format!("{err}")))?;
        let n_bytes = write(PIPES.1, &data)?;
        debug_assert!(n_bytes == data.len());
        Ok(())
    }

    /// Returns a BorrowedFd
    pub fn read_fd<'a>() -> BorrowedFd<'a> {
        unsafe { BorrowedFd::borrow_raw(PIPES.0) }
    }
}
