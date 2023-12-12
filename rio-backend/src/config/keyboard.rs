use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Default, Deserialize, PartialEq, Clone, Copy)]
pub struct Keyboard {
    // Enable kitty keyboard protocol
    #[serde(default = "bool::default", rename = "use-kitty-keyboard-protocol")]
    pub use_kitty_keyboard_protocol: bool,
    // Disable ctlseqs with ALT keys
    // For example: Terminal.app does not deal with ctlseqs with ALT keys
    #[serde(default = "bool::default", rename = "disable-ctlseqs-alt")]
    pub disable_ctlseqs_alt: bool,
}
