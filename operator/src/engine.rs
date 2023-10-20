use nix::{
    errno::Errno,
    libc::siginfo_t,
    poll::{poll, PollFd, PollFlags},
    sys::{
        signal::{kill, sigaction, SaFlags, SigAction, SigSet, Signal},
        wait::{waitpid, WaitStatus},
    },
    unistd::{fork, ForkResult, Pid},
};

use crate::{
    ipc::{self, IPCMessage},
    service::Service,
};
use log::{error, info, warn};
use std::{
    collections::HashMap,
    os::fd::{AsFd, AsRawFd},
};

/// Service handler for operator.
///
/// It Handles creation, termination, book-keeping  of the services.
#[derive(Default)]
pub struct Engine {
    /// list of all services loaded by operator.
    services: HashMap<i32, Service>,
}

impl Engine {
    /// Create a new engine.
    pub fn new() -> Self {
        info!("Creating a new Engine...");
        Self::default()
    }

    /// handler for SIGCHILD.
    extern "C" fn signal_handler(
        _: std::ffi::c_int,
        s_info: *mut siginfo_t,
        _: *mut std::ffi::c_void,
    ) {
        // since signals are not reentrant safe, we just pipe the pid to engine.
        if let Err(e) = comms::write_to_pipe(unsafe { s_info.as_ref().unwrap().si_pid() }) {
            error!("Failed to write to pipe: {e}");
        }
    }

    /// Start the engine and manage the services.
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

        let service_files = Service::read_service_files().unwrap();
        for mut service in service_files.into_iter() {
            info!("Handing service creation for {service:?}");

            match unsafe { fork() }.unwrap() {
                ForkResult::Parent { child } => {
                    service.status = Some(crate::service::Status::Running);
                    service.pid = Some(child.as_raw());

                    self.services.insert(child.as_raw(), service);
                }
                ForkResult::Child => {
                    service.start();
                }
            }
        }

        // create an ipc server for comms b/w operator and operatorctl.
        let ipc_server = ipc::IPCServer::new().unwrap();

        // we are polling on the read-end of the pipe in the signal handler and the ipc server.
        let r_fd = comms::read_fd();
        let ipc_fd = ipc_server.as_fd();
        loop {
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
                // fds that ready to be processed have revents value that is non zero.
                if fd.revents().unwrap().bits() < 1 {
                    continue;
                }

                if fd.as_fd().as_raw_fd() == r_fd.as_raw_fd() {
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
                                    service.status = Some(crate::service::Status::Stopped);
                                }
                                WaitStatus::Signaled(_, _, _) => {
                                    service.status = Some(crate::service::Status::Stopped);
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
                        IPCMessage::Stop { name } => {
                            if let Some((pid, _)) = self
                                .services
                                .iter()
                                .find(|(_, service)| service.name == name)
                            {
                                info!("Asking service {name} to terminate.");
                                if let Err(e) = kill(Pid::from_raw(*pid), Signal::SIGTERM) {
                                    error!("kill() failed with {e}");
                                }
                            } else {
                                warn!("No service found to kill")
                            }
                        }
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
