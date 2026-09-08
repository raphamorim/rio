use crate::config::defaults::default_bool_true;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bell {
    #[serde(default = "default_audio_bell")]
    pub audio: bool,
    /// Mark background tabs that rang the bell with a 🔔 in the tab strip.
    #[serde(default = "default_bool_true", rename = "tab-indicator")]
    pub tab_indicator: bool,
}

impl Default for Bell {
    fn default() -> Self {
        Bell {
            audio: default_audio_bell(),
            tab_indicator: default_bool_true(),
        }
    }
}

fn default_audio_bell() -> bool {
    // Enable audio bell by default on macOS and Windows since they use the system sound
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        true
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        false
    }
}
