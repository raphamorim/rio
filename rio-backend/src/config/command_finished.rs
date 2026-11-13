use serde::{Deserialize, Serialize};

/// When a finished shell command is announced. Needs a shell that
/// marks commands with OSC 133 `C` and `D`, as shell integration does.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum NotifyWhen {
    /// Never announce.
    #[default]
    Never,
    /// Only when the pane the command ran in is not the focused one:
    /// another window has focus, or another tab or split is active.
    Unfocused,
    /// Every time.
    Always,
}

/// `[notify-on-command-finish]`: announce a shell command that ran
/// longer than `after` milliseconds, through the bell, a desktop
/// notification, or both.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct NotifyOnCommandFinish {
    #[serde(default)]
    pub when: NotifyWhen,
    /// Ring the bell, with the same audio and tab indicator as BEL.
    #[serde(default = "default_bell")]
    pub bell: bool,
    /// Send a desktop notification naming the exit status and duration.
    #[serde(default)]
    pub notify: bool,
    /// Commands shorter than this many milliseconds are not announced.
    #[serde(default = "default_after")]
    pub after: u64,
}

impl Default for NotifyOnCommandFinish {
    fn default() -> Self {
        Self {
            when: NotifyWhen::Never,
            bell: default_bell(),
            notify: false,
            after: default_after(),
        }
    }
}

fn default_bell() -> bool {
    true
}

fn default_after() -> u64 {
    5000
}
