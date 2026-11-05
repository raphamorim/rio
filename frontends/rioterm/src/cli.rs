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

    /// Set a fixed window title, ignoring titles set by the shell
    #[clap(long)]
    pub title: Option<String>,

    /// Keep the window open after the command exits, until a key is pressed
    #[clap(long)]
    pub hold: bool,

    /// Set the Wayland app_id or X11 WM_CLASS (Linux/BSD only)
    #[clap(long)]
    pub app_id: Option<String>,
}

impl TerminalOptions {
    /// The flags a config reload must keep.
    pub fn overrides(&self) -> Overrides {
        Overrides {
            shell: self.command(),
            working_dir: self.working_dir.as_deref().map(resolve_working_dir),
            title: self.title.clone(),
            title_placeholder: self.title_placeholder.clone(),
            hold: self.hold,
        }
    }

    /// Shell override passed through the CLI.
    pub fn command(&self) -> Option<Shell> {
        let (program, args) = self.command.split_first()?;
        if program.is_empty() {
            return None;
        }

        Some(Shell {
            program: Some(program.clone()),
            args: args.to_vec(),
        })
    }
}

/// Launch flags applied on top of the config file, at startup and on reload.
#[derive(Clone, Debug, Default)]
pub struct Overrides {
    shell: Option<Shell>,
    /// The resolved `--working-dir`, or `Some(None)` when it was invalid.
    working_dir: Option<Option<String>>,
    title: Option<String>,
    title_placeholder: Option<String>,
    hold: bool,
}

/// Canonicalizes `--working-dir`, or warns and returns None when it is
/// not a directory.
fn resolve_working_dir(path: &str) -> Option<String> {
    #[cfg(target_os = "windows")]
    let canonical = dunce::canonicalize(path);
    #[cfg(not(target_os = "windows"))]
    let canonical = std::fs::canonicalize(path);

    let resolved = canonical.and_then(|path| {
        if !path.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotADirectory,
                "Path is not a directory",
            ));
        }
        path.into_os_string().into_string().map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid UTF-8 in path")
        })
    });

    match resolved {
        Ok(path) => Some(path),
        Err(e) => {
            tracing::warn!(
                "Failed to set working directory '{path}': {e}. Using default instead."
            );
            None
        }
    }
}

impl Overrides {
    /// Applies the platform blocks, then the launch flags, to a freshly
    /// loaded config.
    pub fn apply(&self, config: &mut rio_backend::config::Config) {
        config.overwrite_based_on_platform();

        if let Some(shell) = &self.shell {
            config.shell = shell.clone();
            config.use_fork = false;
        }
        if let Some(working_dir) = &self.working_dir {
            config.working_dir = working_dir.clone();
        }
        if let Some(title) = &self.title {
            config.title.placeholder = Some(title.clone());
            config.title.content = title.clone();
        } else if let Some(placeholder) = &self.title_placeholder {
            config.title.placeholder = Some(placeholder.clone());
        }
        if self.hold {
            config.hold = true;
        }

        #[cfg(target_os = "linux")]
        if std::path::Path::new("/.flatpak-info").exists() {
            config.use_fork = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rio_backend::config::Config;

    fn options(args: &[&str]) -> TerminalOptions {
        let mut argv = vec!["rio"];
        argv.extend_from_slice(args);
        Cli::parse_from(argv).window_options.terminal_options
    }

    #[test]
    fn title_pins_the_window_title() {
        let mut config = Config::default();
        options(&["--title=btop", "--title-placeholder=x"])
            .overrides()
            .apply(&mut config);
        assert_eq!(config.title.content, "btop");
        assert_eq!(config.title.placeholder.as_deref(), Some("btop"));
    }

    #[test]
    fn title_placeholder_keeps_the_title_template() {
        let mut config = Config::default();
        let content = config.title.content.clone();
        options(&["--title-placeholder=x"])
            .overrides()
            .apply(&mut config);
        assert_eq!(config.title.content, content);
        assert_eq!(config.title.placeholder.as_deref(), Some("x"));
    }

    #[test]
    fn no_flags_keep_the_configured_title() {
        let mut config = Config::default();
        config.title.placeholder = Some(String::from("Rio"));
        options(&[]).overrides().apply(&mut config);
        assert_eq!(config.title.placeholder.as_deref(), Some("Rio"));
        assert!(!config.hold);
    }

    #[test]
    fn command_survives_reapplying() {
        let overrides = options(&["-e", "htop"]).overrides();
        let mut config = Config::default();
        overrides.apply(&mut config);
        assert_eq!(config.shell.program.as_deref(), Some("htop"));
        assert!(!config.use_fork);
    }

    #[test]
    fn hold_flag_sets_hold() {
        let mut config = Config::default();
        options(&["--hold", "-e", "true"])
            .overrides()
            .apply(&mut config);
        assert!(config.hold);
    }
}
