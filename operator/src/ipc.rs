//! IPC stuff for operator.
//!
//! It contains helpers for creating a IPC server and clients.

use std::{
    os::{
        fd::{AsFd, BorrowedFd},
        unix::net::{SocketAddr, UnixListener, UnixStream},
    },
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::service::ServiceStatus;

/// Message format used to communicate b/w operator and operatorctl.
#[derive(Debug, Serialize, Deserialize)]
pub enum IPCMessage {
    /// Start a service.
    Start { name: String },
    /// Stop a service.
    Stop { name: String },
    /// Status of a service.
    Status { name: String },

    /// Response for the [IPCMessage::Status] command.
    StatusResponse(Option<(i32, ServiceStatus)>),
}

/// An Unix socket stream.
pub struct IPCStream(UnixStream, SocketAddr);

impl IPCStream {
    /// Connect to a unix socket.
    pub fn connect(path: &str) -> anyhow::Result<Self> {
        let stream = UnixStream::connect(path)?;
        let addr = stream.peer_addr()?;

        Ok(Self(stream, addr))
    }

    /// Read a message from the unix socket.
    pub fn read(&self) -> anyhow::Result<IPCMessage> {
        bincode::deserialize_from(&self.0).map_err(|err| anyhow::Error::msg(format!("{err}")))
    }

    /// Write a message to the unix socket.
    pub fn write(&self, msg: &IPCMessage) -> anyhow::Result<()> {
        bincode::serialize_into(&self.0, msg)
            .map_err(|err| anyhow::Error::msg(format!("{err}")))?;
        Ok(())
    }
}

/// IPC Server for comms b/w operator and operatorctl.
pub struct IPCServer(UnixListener);

impl IPCServer {
    /// Create a new IPC server.
    pub fn new() -> anyhow::Result<Self> {
        let socket_path = Path::new("/tmp/operator.sock");
        if Path::exists(socket_path) {
            _ = std::fs::remove_file(socket_path)
        }

        let listener = UnixListener::bind(socket_path)?;
        listener.set_nonblocking(true)?;
        Ok(Self(listener))
    }

    /// Accept a new incoming connection.
    pub fn accept(&self) -> anyhow::Result<IPCStream> {
        let (stream, addr) = self.0.accept()?;
        Ok(IPCStream(stream, addr))
    }

    /// Get the underlying fd.
    ///
    /// NOTE: we use it to poll instead of blocking.
    pub fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}
