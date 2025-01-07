// cli.rs was retired originally from https://github.com/alacritty/alacritty/blob/e35e5ad14fce8456afdd89f2b392b9924bb27471/alacritty/src/cli.rs
// which is licensed under Apache 2.0 license.

use clap::{Args, Parser, ValueHint};
use rio_backend::config::Shell;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Parser, Default, Debug)]
#[clap(author, about, version)]
pub struct Cli {
    /// Options which can be passed via IPC.
    #[clap(flatten)]
    pub window_options: WindowOptions,
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

    /// Start the shell in the specified working directory.
    #[clap(short, long, value_hint = ValueHint::FilePath)]
    pub working_dir: Option<String>,

    /// Writes the config to a given path or the default location.
    #[clap(long, value_name = "PATH", value_hint = ValueHint::FilePath)]
    pub write_config: Option<Option<PathBuf>>,

    /// Writes the logs to a file inside the config directory.
    #[clap(long)]
    pub enable_log_file: bool,

    /// Start window with specified title
    #[clap(long, name = "title-placeholder")]
    pub title_placeholder: Option<String>,
}

impl TerminalOptions {
    /// Shell override passed through the CLI.
    pub fn command(&self) -> Option<Shell> {
        let (program, args) = self.command.split_first()?;
        if program.is_empty() {
            return None;
        }

        Some(Shell {
            program: program.clone(),
            args: args.to_vec(),
        })
    }

    // pub fn override_pty_config(&self, pty_config: &mut PtyConfig) {
    //     if let Some(working_directory) = &self.working_directory {
    //         if working_directory.is_dir() {
    //             pty_config.working_directory = Some(working_directory.to_owned());
    //         } else {
    //             error!("Invalid working directory: {:?}", working_directory);
    //         }
    //     }

    //     if let Some(command) = self.command() {
    //         pty_config.shell = Some(command);
    //     }

    //     pty_config.hold |= self.hold;
    // }
}
