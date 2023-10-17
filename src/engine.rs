use nix::unistd::{fork, ForkResult};

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
                    let status = unsafe { nix::sys::wait::waitpid(child, None) }.unwrap();
                    info!("Status {status:?}")
                    // TODO: book keep the process
                }
                ForkResult::Child => {
                    info!("{}: executing {:?}", service.name, service.executable);

                    let exe_path =
                        CString::new(service.executable.to_str().unwrap().to_string()).unwrap();

                    let args = if let Some(ref args) = service.args {
                        [exe_path.as_ptr()]
                            .into_iter()
                            .chain(args.iter().map(|arg| arg.as_ptr()))
                            .collect::<Vec<_>>()
                    } else {
                        vec![exe_path.as_ptr()]
                    };

                    unsafe { nix::libc::execv(exe_path.as_ptr(), args.as_ptr()) };
                }
            }
        }
    }
}
