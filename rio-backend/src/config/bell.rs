use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bell {
    #[serde(default = "default_visual_bell")]
    pub visual: bool,
    #[serde(default = "default_audible_bell")]
    pub audible: bool,
}

impl Default for Bell {
    fn default() -> Self {
        Bell {
            visual: default_visual_bell(),
            audible: default_audible_bell(),
        }
    }
}

fn default_visual_bell() -> bool {
    false
}

fn default_audible_bell() -> bool {
    false
}
