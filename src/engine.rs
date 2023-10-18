use nix::{
    libc::{dup2, open, O_APPEND, O_CREAT, O_WRONLY, STDERR_FILENO, STDOUT_FILENO},
    unistd::{fork, ForkResult},
};

use crate::service::Service;
use log::info;
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
}

impl Engine {
    /// Create a new service runner engine
    pub fn new(services: Vec<Service>) -> Self {
        info!("Creating a new Engine...");
        Self {
            service_files: services,
            _db: HashMap::new(),
        }
    }

    /// Start the engine and manage the services
    pub fn run(&self) {
        for service in self.service_files.iter() {
            info!("Handing service creation for {service:?}");
            match unsafe { fork() }.unwrap() {
                ForkResult::Parent { child } => {
                    let status = nix::sys::wait::waitpid(child, None).unwrap();
                    info!("Status {status:?}")
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
                        CString::new(format!("/tmp/{}.log", service.name)).unwrap();
                    let log_fd =
                        unsafe { open(stdout_file_path.as_ptr(), O_WRONLY | O_CREAT | O_APPEND) };

                    // set the stdout and stderr to the log file
                    unsafe {
                        dup2(log_fd, STDOUT_FILENO);
                        dup2(log_fd, STDERR_FILENO);
                    }

                    let _res = unsafe { nix::libc::execv(exe_path.as_ptr(), args.as_ptr()) };
                }
            }
        }
    }
}
