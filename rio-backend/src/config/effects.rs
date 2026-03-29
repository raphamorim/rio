use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Effects {
    #[serde(default = "bool::default", rename = "custom-cursor")]
    pub custom_cursor: bool,
}

impl Default for Effects {
    fn default() -> Effects {
        Effects {
            custom_cursor: false,
        }
    }
}
