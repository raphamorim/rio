// IPC module for remote control of midterm
// Provides Unix socket-based communication for CLI control

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc;

/// Get the path to the IPC socket
pub fn socket_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    runtime_dir.join("midterm.sock")
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum IpcCommand {
    /// Trigger an action by name
    TriggerAction(String),
    /// Get current screen content (plain text or JSON)
    DumpScreen {
        format: Option<String>,  // "json" or "text" (default)
        start_line: Option<usize>,
        end_line: Option<usize>,
    },
    /// Get status information
    GetStatus,
    /// List available actions
    ListActions,
    /// List all tabs
    ListTabs,
    /// Ping to check if server is alive
    Ping,
    /// Send text input to the terminal
    SendInput(String),
    /// Check if screen contains pattern
    ScreenContains(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ScreenCell {
    pub c: char,
    pub fg: Option<String>,  // Foreground color in hex
    pub bg: Option<String>,  // Background color in hex
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TabInfo {
    pub index: usize,
    pub title: String,
    pub is_current: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum IpcResponse {
    /// Action was triggered successfully
    ActionTriggered(String),
    /// Screen content dump (plain text)
    ScreenDump {
        lines: Vec<String>,
        cursor_row: usize,
        cursor_col: usize,
    },
    /// Screen content dump (JSON with cell data)
    ScreenDumpJson {
        lines: Vec<Vec<ScreenCell>>,
        cursor_row: usize,
        cursor_col: usize,
        start_line: usize,
        end_line: usize,
    },
    /// Status information
    Status {
        tabs: usize,
        current_tab: usize,
        splits: usize,
        broadcast_mode: bool,
        current_directory: Option<String>,
        git_branch: Option<String>,
    },
    /// List of available actions
    Actions(Vec<String>),
    /// List of tabs
    Tabs(Vec<TabInfo>),
    /// Pong response
    Pong,
    /// Input was sent
    InputSent(usize),
    /// Screen contains check result
    ScreenContainsResult(bool),
    /// Error message
    Error(String),
}

/// Send a command to a running midterm instance
pub fn send_command(cmd: IpcCommand) -> Result<IpcResponse, String> {
    let path = socket_path();
    if !path.exists() {
        return Err("No midterm instance running (socket not found)".to_string());
    }

    let mut stream = UnixStream::connect(&path)
        .map_err(|e| format!("Failed to connect to midterm: {}", e))?;

    let cmd_json = serde_json::to_string(&cmd)
        .map_err(|e| format!("Failed to serialize command: {}", e))?;

    writeln!(stream, "{}", cmd_json)
        .map_err(|e| format!("Failed to send command: {}", e))?;

    stream.flush()
        .map_err(|e| format!("Failed to flush: {}", e))?;

    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)
        .map_err(|e| format!("Failed to read response: {}", e))?;

    serde_json::from_str(&response_line)
        .map_err(|e| format!("Failed to parse response: {}", e))
}

/// IPC server that listens for commands
pub struct IpcServer {
    listener: UnixListener,
}

impl IpcServer {
    /// Create and start the IPC server
    pub fn new() -> Result<Self, String> {
        let path = socket_path();

        // Remove existing socket if it exists
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }

        let listener = UnixListener::bind(&path)
            .map_err(|e| format!("Failed to bind IPC socket: {}", e))?;

        // Set non-blocking so we can poll
        listener.set_nonblocking(true)
            .map_err(|e| format!("Failed to set non-blocking: {}", e))?;

        tracing::info!("IPC server listening on {:?}", path);

        Ok(Self { listener })
    }

    /// Check for and handle incoming commands (non-blocking)
    /// Returns any commands that were received
    pub fn poll(&self) -> Vec<(IpcCommand, mpsc::Sender<IpcResponse>)> {
        let mut commands = Vec::new();

        // Accept connections (non-blocking)
        while let Ok((stream, _)) = self.listener.accept() {
            if let Some(cmd) = self.handle_connection(stream) {
                commands.push(cmd);
            }
        }

        commands
    }

    fn handle_connection(&self, stream: UnixStream) -> Option<(IpcCommand, mpsc::Sender<IpcResponse>)> {
        let mut reader = BufReader::new(stream.try_clone().ok()?);
        let mut line = String::new();

        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }

        let cmd: IpcCommand = serde_json::from_str(&line).ok()?;

        // Create a channel for the response
        let (tx, rx) = mpsc::channel();

        // Spawn a thread to wait for and send the response
        std::thread::spawn(move || {
            if let Ok(response) = rx.recv_timeout(std::time::Duration::from_secs(5)) {
                if let Ok(mut stream) = stream.try_clone() {
                    if let Ok(json) = serde_json::to_string(&response) {
                        let _ = writeln!(stream, "{}", json);
                        let _ = stream.flush();
                    }
                }
            }
        });

        Some((cmd, tx))
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        // Clean up socket file
        let _ = std::fs::remove_file(socket_path());
    }
}

/// Handle CLI commands that communicate with a running instance
pub fn handle_cli_command(
    action: Option<String>,
    dump_screen: bool,
    status: bool,
    list_actions: bool,
    send: Option<String>,
    wait_for: Option<String>,
    timeout: u64,
) -> bool {
    // If any IPC command is specified, handle it and exit
    if action.is_none() && !dump_screen && !status && !list_actions && send.is_none() && wait_for.is_none() {
        return false; // No IPC command, continue normal startup
    }

    // Handle wait_for with polling
    if let Some(pattern) = wait_for {
        let start = std::time::Instant::now();
        let timeout_duration = std::time::Duration::from_millis(timeout);

        loop {
            match send_command(IpcCommand::ScreenContains(pattern.clone())) {
                Ok(IpcResponse::ScreenContainsResult(true)) => {
                    println!("Pattern '{}' found", pattern);
                    return true;
                }
                Ok(IpcResponse::ScreenContainsResult(false)) => {
                    if start.elapsed() >= timeout_duration {
                        eprintln!("Timeout waiting for pattern '{}'", pattern);
                        std::process::exit(1);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Ok(IpcResponse::Error(e)) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
                _ => {
                    eprintln!("Unexpected response");
                    std::process::exit(1);
                }
            }
        }
    }

    let cmd = if let Some(action_name) = action {
        IpcCommand::TriggerAction(action_name)
    } else if dump_screen {
        IpcCommand::DumpScreen {
            format: None,  // Default to text format
            start_line: None,
            end_line: None,
        }
    } else if status {
        IpcCommand::GetStatus
    } else if list_actions {
        IpcCommand::ListActions
    } else if let Some(text) = send {
        IpcCommand::SendInput(text)
    } else {
        return false;
    };

    match send_command(cmd) {
        Ok(response) => {
            match response {
                IpcResponse::ActionTriggered(name) => {
                    println!("Action '{}' triggered", name);
                }
                IpcResponse::ScreenDump { lines, cursor_row, cursor_col } => {
                    println!("=== Screen Dump (cursor at {},{}) ===", cursor_row, cursor_col);
                    for line in lines {
                        println!("{}", line);
                    }
                }
                IpcResponse::ScreenDumpJson { lines, cursor_row, cursor_col, start_line, end_line } => {
                    let json = serde_json::to_string_pretty(&serde_json::json!({
                        "lines": lines,
                        "cursor_row": cursor_row,
                        "cursor_col": cursor_col,
                        "start_line": start_line,
                        "end_line": end_line,
                    })).unwrap();
                    println!("{}", json);
                }
                IpcResponse::Tabs(tabs) => {
                    let json = serde_json::to_string_pretty(&tabs).unwrap();
                    println!("{}", json);
                }
                IpcResponse::Status { tabs, current_tab, splits, broadcast_mode, current_directory, git_branch } => {
                    println!("{{");
                    println!("  \"tabs\": {},", tabs);
                    println!("  \"current_tab\": {},", current_tab);
                    println!("  \"splits\": {},", splits);
                    println!("  \"broadcast_mode\": {},", broadcast_mode);
                    println!("  \"current_directory\": {:?},", current_directory);
                    println!("  \"git_branch\": {:?}", git_branch);
                    println!("}}");
                }
                IpcResponse::Actions(actions) => {
                    println!("Available actions:");
                    for action in actions {
                        println!("  {}", action);
                    }
                }
                IpcResponse::Pong => {
                    println!("pong");
                }
                IpcResponse::InputSent(len) => {
                    println!("Sent {} bytes", len);
                }
                IpcResponse::ScreenContainsResult(found) => {
                    println!("{}", found);
                }
                IpcResponse::Error(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }

    true // Command was handled, exit
}
