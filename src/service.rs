use serde::{Deserialize, Serialize};
use std::{ffi::CString, path::PathBuf};

/// Represents a service
#[derive(Serialize, Deserialize, Debug)]
pub struct Service {
    /// Name of the service
    pub name: String,
    /// The path to the executable
    pub executable: PathBuf,
    /// Arguments to the program
    pub args: Option<Vec<CString>>,
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
