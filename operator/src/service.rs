use serde::{Deserialize, Serialize};
use std::{ffi::CString, path::PathBuf};

/// Status of the service
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum ServiceStatus {
    /// The service is running
    Running,
    /// The service Stopped
    Stopped,
    /// The process exited but waiting to be cleaned up
    Zombie,
}

/// Represents a service
#[derive(Serialize, Deserialize, Debug)]
pub struct Service {
    /// Name of the service
    pub name: String,
    /// The path to the executable
    pub executable: PathBuf,
    /// Arguments to the program
    pub args: Option<Vec<CString>>,

    /// The pid of the service
    #[serde(skip)]
    pub pid: Option<i32>,

    /// The status of the running service
    #[serde(skip)]
    pub status: Option<ServiceStatus>,

    /// The exit code of the service if it exited
    #[serde(skip)]
    pub exit_code: Option<u8>,
}

impl Service {
    /// Read the services files located in /tmp/op
    pub fn read_service_files(op_service_dir: &str) -> std::io::Result<Vec<Service>> {
        let mut services = vec![];
        let dir = std::fs::read_dir(op_service_dir)?.flatten();

        for entry in dir {
            if entry.file_type().unwrap().is_file() {
                let contents = std::fs::read_to_string(entry.path())?;
                match toml::from_str::<Service>(&contents) {
                    Ok(service) => services.push(service),
                    Err(e) => panic!("{e}"),
                }
            }
        }

        Ok(services)
    }
}
