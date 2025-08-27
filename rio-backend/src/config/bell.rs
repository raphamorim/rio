use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bell {
    #[serde(default = "default_visual_bell")]
    pub visual: bool,
    #[serde(default = "default_audio_bell")]
    pub audio: bool,
}

impl Default for Bell {
    fn default() -> Self {
        Bell {
            visual: default_visual_bell(),
            audio: default_audio_bell(),
        }
    }
}

fn default_visual_bell() -> bool {
    false
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
