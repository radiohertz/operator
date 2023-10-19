use nix::{
    errno::{self, errno},
    libc::{
        dup2, open, siginfo_t, O_APPEND, O_CREAT, O_WRONLY, STDERR_FILENO, STDOUT_FILENO, S_IRGRP,
        S_IRUSR, S_IWGRP, S_IWUSR,
    },
    sys::signal::{sigaction, SaFlags, SigAction, SigSet, Signal},
    unistd::{fork, ForkResult},
};

use crate::service::Service;
use log::{error, info};
use std::{collections::HashMap, ffi::CString, time::Duration};

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
        sig: std::ffi::c_int,
        sip: *mut siginfo_t,
        ctx: *mut std::ffi::c_void,
    ) {
        let sinfo = unsafe { sip.as_ref() };
        info!("Child is dead: {sinfo:?}");
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

        loop {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}
