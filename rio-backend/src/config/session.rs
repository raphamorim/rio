use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Default)]
pub enum SessionRestore {
    #[serde(alias = "never")]
    #[default]
    Never,
    #[serde(alias = "prompt")]
    Prompt,
    #[serde(alias = "always")]
    Always,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub struct Session {
    #[serde(default)]
    pub restore: SessionRestore,
    /// Upper bound of history+screen lines dumped per pane on save.
    #[serde(
        default = "default_max_scrollback_lines",
        rename = "max-scrollback-lines"
    )]
    pub max_scrollback_lines: usize,
}

fn default_max_scrollback_lines() -> usize {
    2000
}

impl Default for Session {
    fn default() -> Session {
        Session {
            restore: SessionRestore::default(),
            max_scrollback_lines: default_max_scrollback_lines(),
        }
    }
}
