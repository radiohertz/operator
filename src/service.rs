use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Represents a service
#[derive(Serialize, Deserialize, Debug)]
pub struct Service {
    /// Name of the service
    pub name: String,
    /// The path to the executable
    pub executable: PathBuf,
}

impl Service {
    pub const SERVICE_FILE_PATH: &str = "/tmp/op";

    /// Read the services files located in /tmp/op
    pub fn read_service_files() -> std::io::Result<Vec<Service>> {
        let mut services = vec![];
        let dir = std::fs::read_dir(Self::SERVICE_FILE_PATH)?.flatten();

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
