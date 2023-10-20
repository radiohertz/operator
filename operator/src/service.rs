use log::{error, info};
use nix::errno::{errno, Errno};
use serde::{Deserialize, Serialize};
use std::{ffi::CString, path::PathBuf, process::exit};

use crate::helper::{op_service_dir, op_service_log_dir};
use nix::libc::{
    dup2, open, O_APPEND, O_CREAT, O_WRONLY, STDERR_FILENO, STDOUT_FILENO, S_IRGRP, S_IRUSR,
    S_IWGRP, S_IWUSR,
};

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
    /// Start the service.
    ///
    /// This should only be run in the context of a forked child process.
    ///
    /// This will not return.
    pub fn start(&self) -> ! {
        info!("{}: executing {:?}", self.name, self.executable);

        let exe_path = CString::new(self.executable.to_str().unwrap()).unwrap();

        let mut args = if let Some(ref args) = self.args {
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
            CString::new(format!("{}/{}.log", op_service_log_dir(), self.name)).unwrap();
        let log_fd = unsafe {
            open(
                stdout_file_path.as_ptr(),
                O_WRONLY | O_CREAT | O_APPEND,
                (S_IRUSR | S_IWUSR | S_IRGRP | S_IWGRP) as std::ffi::c_uint,
            )
        };

        if log_fd == -1 {
            error!("Failed to create log file {}", Errno::from_i32(errno()));
        }

        info!(
            "Creating log file for {} at {:?} [FD {log_fd}]",
            self.name, stdout_file_path
        );

        // set the stdout and stderr to the log file
        unsafe {
            dup2(log_fd, STDOUT_FILENO);
            dup2(log_fd, STDERR_FILENO);
        }

        let res = unsafe { nix::libc::execv(exe_path.as_ptr(), args.as_ptr()) };

        error!("exec() Failed with {res}");
        error!("errno: {}", Errno::from_i32(errno()));
        exit(-1)
    }

    /// Read the services files located in /tmp/op
    pub fn read_service_files() -> std::io::Result<Vec<Service>> {
        let mut services = vec![];
        let dir = std::fs::read_dir(op_service_dir())?.flatten();

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
