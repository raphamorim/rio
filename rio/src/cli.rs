use clap::{Args, Parser};
use serde::{Deserialize, Serialize};

#[derive(Parser, Default, Debug)]
#[clap(author, about, version)]
pub struct Options {
    /// Options which can be passed via IPC.
    #[clap(flatten)]
    pub window_options: WindowOptions,
}

impl Options {
    pub fn new() -> Self {
        Self::parse()
    }
}

#[derive(Serialize, Deserialize, Args, Default, Clone, Debug, PartialEq, Eq)]
pub struct WindowOptions {
    /// Terminal options which can be passed via IPC.
    #[clap(flatten)]
    pub terminal_options: TerminalOptions,
}

#[derive(Serialize, Deserialize, Args, Default, Debug, Clone, PartialEq, Eq)]
pub struct TerminalOptions {
    /// Command and args to execute (must be last argument).
    #[clap(short = 'e', long, allow_hyphen_values = true, num_args = 1..)]
    pub command: Vec<String>,
}
