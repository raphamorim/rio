use crate::config::default_bool_true;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub struct Split {
    #[serde(default = "default_bool_true")]
    pub enable: bool,
}
