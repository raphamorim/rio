use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Effects {
    #[serde(default = "bool::default", rename = "custom-mouse-cursor")]
    pub custom_mouse_cursor: bool,
    #[serde(default = "default_trail_cursor", rename = "trail-cursor")]
    pub trail_cursor: bool,
}

fn default_trail_cursor() -> bool {
    true
}

impl Default for Effects {
    fn default() -> Effects {
        Effects {
            custom_mouse_cursor: false,
            trail_cursor: true,
        }
    }
}
