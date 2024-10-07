use crate::config::default_bool_true;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub struct Split {
    #[serde(default = "default_bool_true")]
    pub enable: bool,
}

#[allow(clippy::derivable_impls)]
impl Default for Split {
    fn default() -> Split {
        Split { enable: true }
    }
}
