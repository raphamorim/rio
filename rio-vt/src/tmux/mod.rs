//! tmux control mode (`tmux -CC`).
//!
//! tmux announces control mode with `DCS 1000 p`; from there until `%exit`
//! the PTY carries a line protocol instead of terminal output. Bytes are
//! handed to `session::ControlSession` straight from the PTY reader,
//! bypassing the VT parser: `%output` lines hold raw UTF-8, which DCS
//! passthrough would drop or, for 0x9C, read as a terminator.
//!
//! The reader thread owns the protocol because ordering is what makes it
//! correct: a command's reply is matched by its position in the stream, and
//! `%output` only makes sense relative to `capture-pane` replies and layout
//! changes. Frontends see [`TmuxSnapshot`]s and turn them into tabs and
//! splits, without having to understand the wire format.
//!
//! Reference: <https://github.com/tmux/tmux/wiki/Control-Mode>

pub mod keys;
pub mod layout;
pub mod prompt;
pub mod protocol;
pub mod replies;
#[cfg(feature = "pty")]
pub mod session;
pub mod status;

use crate::time::Instant;
use keys::TmuxKeys;
use layout::LayoutNode;
use std::sync::Arc;

/// Longest run of `send-keys` bytes per command. tmux has no hard limit, but
/// a paste should not become a single multi-megabyte line.
const SEND_KEYS_CHUNK: usize = 512;

/// A tmux window as the frontend should display it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TmuxWindow {
    pub id: u32,
    pub name: String,
    /// The visible layout: while a pane is zoomed this holds just that pane.
    pub layout: LayoutNode,
    pub active_pane: Option<u32>,
}

/// The whole attached session, in tmux's window order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TmuxSnapshot {
    pub windows: Vec<TmuxWindow>,
    pub active_window: Option<u32>,
    pub keys: Arc<TmuxKeys>,
    /// Expanded formats asked for by a prompt, under the id it asked with.
    pub prompt_formats: Option<(u64, Vec<String>)>,
    /// What came of the commands of a prompt, for the user to read.
    pub prompt_reply: Option<String>,
}

/// Destination for one pane's output. Implemented by
/// `session::TerminalSink`; a trait so `Msg` stays free of the terminal's
/// listener type parameter.
pub trait PaneSink: Send + std::fmt::Debug {
    /// Parse raw pane output into the pane's terminal.
    fn write(&mut self, bytes: &[u8]);
    /// Apply the size tmux assigned to the pane.
    fn resize(&mut self, columns: usize, lines: usize);
    /// When the pane's active synchronized update expires, if it has one.
    fn sync_deadline(&self) -> Option<Instant>;
    /// End the pane's synchronized update if it outlived its deadline.
    fn expire_sync(&mut self, now: Instant);
}

/// Encode input for a pane as `send-keys -H` commands.
///
/// Hex rather than literal text: any byte is safe and nothing in the payload
/// can be read as tmux command syntax.
pub fn send_keys_commands(pane: u32, bytes: &[u8]) -> Vec<String> {
    use std::fmt::Write;

    bytes
        .chunks(SEND_KEYS_CHUNK)
        .map(|chunk| {
            let mut command = format!("send-keys -t %{pane} -H");
            for byte in chunk {
                let _ = write!(command, " {byte:02x}");
            }
            command
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_keys_is_hex_encoded() {
        assert_eq!(
            send_keys_commands(3, b"l;\r"),
            vec!["send-keys -t %3 -H 6c 3b 0d".to_string()]
        );
    }

    #[test]
    fn send_keys_chunks_long_input() {
        let commands = send_keys_commands(0, &[b'a'; SEND_KEYS_CHUNK + 1]);
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[1], "send-keys -t %0 -H 61");
    }

    #[test]
    fn send_keys_with_no_input_sends_nothing() {
        assert!(send_keys_commands(0, b"").is_empty());
    }
}
