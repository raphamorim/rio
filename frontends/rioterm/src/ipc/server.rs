#![cfg(unix)]

use std::{
    io::{ErrorKind, Read},
    os::unix::net::{UnixListener, UnixStream},
};

use rio_backend::event::EventProxy;
use rio_window::window::WindowId;

use crate::ipc::protocol::IpcCommand;

const SOCKET_FILE_NAME: &str = "rioterm";

pub fn start(event_proxy: EventProxy) -> Result<(), std::io::Error> {
    let path = get_socket_path();

    // Check if socket file already exists. Remove it if so
    match std::fs::remove_file(&path) {
        Ok(_) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }

    let listener = UnixListener::bind(path)?;

    // Create server listener at background thread
    let _ = std::thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let event_proxy = event_proxy.clone();
                    std::thread::spawn(move || {
                        if let Err(e) = handle_connection(stream, event_proxy) {
                            tracing::error!("Error while processing IPC Request: {}", e);
                        }
                    })
                }

                Err(err) => match err.kind() {
                    ErrorKind::ConnectionAborted | ErrorKind::Interrupted => {
                        tracing::debug!("IPC server error: {}", err);
                        continue;
                    }

                    _ => {
                        tracing::error!(
                            "IPC server critical error: {}. IPC server is stopping!",
                            err
                        );
                        break;
                    }
                },
            };
        }
    });

    Ok(())
}

fn handle_connection(
    mut stream: UnixStream,
    event_proxy: EventProxy,
) -> Result<(), Box<dyn std::error::Error>> {
    // Set read timeout to 500 millis to prevent thread stucking
    stream.set_read_timeout(Some(std::time::Duration::from_millis(500)))?;

    // Read request header (len in u8)
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes)?;
    let len = u32::from_le_bytes(len_bytes);

    // Read request data
    let mut buf = vec![0u8; len as usize];
    stream.read_exact(&mut buf)?;

    // Parse data to IpcCommand (protocol)
    let command = postcard::from_bytes::<IpcCommand>(&buf)?;

    match command {
        IpcCommand::CreateWindow { working_dir } => {
            event_proxy.send_event(
                rio_backend::event::RioEventType::Rio(
                    rio_backend::event::RioEvent::IpcCreateWindow(working_dir),
                ),
                unsafe { WindowId::dummy() },
            );
        }
    }

    Ok(())
}

pub fn get_socket_path() -> std::path::PathBuf {
    let mut base_dir = match std::env::var("XDG_RUNTIME_DIR") {
        Ok(dir) => std::path::PathBuf::from(dir),
        Err(_) => std::env::temp_dir(),
    };

    base_dir.push(get_socket_filename());
    base_dir
}

fn get_socket_filename() -> String {
    let uid = uzers::get_current_uid();
    let display = std::env::var("WAYLAND_DISPLAY")
        .or_else(|_| std::env::var("DISPLAY"))
        .unwrap_or_else(|_| "default".to_string());
    let display = display.replace('/', "-");

    format!("{}-{}-{}.sock", SOCKET_FILE_NAME, uid, display)
}
