//! IPC stuff for operator

use std::os::{
    fd::{AsFd, BorrowedFd},
    unix::net::{SocketAddr, UnixListener, UnixStream},
};

use serde::{Deserialize, Serialize};

use crate::service::ServiceStatus;

#[derive(Debug, Serialize, Deserialize)]
pub enum IPCMessage {
    /// Start a service
    Start {
        name: String,
    },
    /// Stop a service
    Stop {
        name: String,
    },
    /// Status of a service
    Status {
        name: String,
    },

    StatusResponse(Option<(i32, ServiceStatus)>),
}

pub struct IPCStream(UnixStream, SocketAddr);

impl IPCStream {
    pub fn connect(path: &str) -> anyhow::Result<Self> {
        let stream = UnixStream::connect(path)?;
        let addr = stream.peer_addr()?;

        Ok(Self(stream, addr))
    }

    pub fn read(&self) -> anyhow::Result<IPCMessage> {
        bincode::deserialize_from(&self.0).map_err(|err| anyhow::Error::msg(format!("{err}")))
    }

    pub fn write(&self, msg: &IPCMessage) -> anyhow::Result<()> {
        bincode::serialize_into(&self.0, msg)
            .map_err(|err| anyhow::Error::msg(format!("{err}")))?;
        Ok(())
    }
}

pub struct IPCServer(UnixListener);

impl IPCServer {
    /// Create a new IPC server
    pub fn new() -> anyhow::Result<Self> {
        _ = std::fs::remove_file("/tmp/operator.sock");

        let listener = UnixListener::bind("/tmp/operator.sock")?;
        listener.set_nonblocking(true)?;
        Ok(Self(listener))
    }

    pub fn accept(&self) -> anyhow::Result<IPCStream> {
        let (stream, addr) = self.0.accept()?;
        Ok(IPCStream(stream, addr))
    }

    /// Get the underlying fd
    pub fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}
