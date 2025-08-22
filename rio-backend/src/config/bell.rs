use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bell {
    #[serde(default = "default_visual_bell")]
    pub visual: bool,
}

impl Default for Bell {
    fn default() -> Self {
        Bell {
            visual: default_visual_bell(),
        }
    }
}

fn default_visual_bell() -> bool {
    true
}
