use serde::{Deserialize, Serialize};

use super::defaults::{default_title_content, default_title_placeholder};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Title {
    #[serde(default = "default_title_placeholder")]
    pub placeholder: Option<String>,
    #[serde(default = "default_title_content")]
    pub content: String,
}

#[allow(clippy::derivable_impls)]
impl Default for Title {
    fn default() -> Title {
        Title {
            placeholder: default_title_placeholder(),
            content: default_title_content(),
        }
    }
}
