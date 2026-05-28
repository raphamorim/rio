use crate::config::defaults::default_bool_true;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Mouse {
    #[serde(default = "default_bool_true", rename = "wheel-zoom")]
    pub wheel_zoom: bool,
}

impl Default for Mouse {
    fn default() -> Self {
        Mouse {
            wheel_zoom: default_bool_true(),
        }
    }
}
