use serde::{Deserialize, Serialize};

use super::defaults::{default_bool_true, default_disable_ctlseqs_alt};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub struct Keyboard {
    // Enable kitty keyboard protocol
    #[serde(default = "default_bool_true", rename = "use-kitty-keyboard-protocol")]
    pub use_kitty_keyboard_protocol: bool,
    // Disable ctlseqs with ALT keys
    // For example: Terminal.app does not deal with ctlseqs with ALT keys
    #[serde(
        default = "default_disable_ctlseqs_alt",
        rename = "disable-ctlseqs-alt"
    )]
    pub disable_ctlseqs_alt: bool,
}

#[allow(clippy::derivable_impls)]
impl Default for Keyboard {
    fn default() -> Keyboard {
        Keyboard {
            use_kitty_keyboard_protocol: true,
            #[cfg(target_os = "macos")]
            disable_ctlseqs_alt: true,
            #[cfg(not(target_os = "macos"))]
            disable_ctlseqs_alt: false,
        }
    }
}
