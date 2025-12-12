use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bell {
    #[serde(default = "default_audio_bell")]
    pub audio: bool,
}

impl Default for Bell {
    fn default() -> Self {
        Bell {
            audio: default_audio_bell(),
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
