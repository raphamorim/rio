#![cfg(unix)]

use std::{io::Write, os::unix::net::UnixStream};

use crate::ipc::protocol::IpcCommand;

use super::*;

pub struct Client {
    stream: UnixStream,
}

impl Client {
    pub fn try_connect() -> Option<Self> {
        let path = server::get_socket_path();

        match UnixStream::connect(path) {
            Ok(stream) => Some(Self { stream }),
            Err(_) => None,
        }
    }

    pub fn create_window(
        &mut self,
        working_dir: Option<std::path::PathBuf>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_request(IpcCommand::CreateWindow { working_dir })
    }

    fn send_request(
        &mut self,
        req: protocol::IpcCommand,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Serialize IpcCommand to Vec<u8>
        let data = postcard::to_allocvec(&req)?;

        // Send request header (len in u8)
        let len = (data.len() as u32).to_le_bytes();
        self.stream.write_all(&len)?;

        // Send request data
        self.stream.write_all(&data)?;

        Ok(())
    }
}
