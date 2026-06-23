use serde::{Deserialize, Serialize};

use super::defaults::{
    default_disable_ctlseqs_alt, default_forward_to_ime_modifier_mask,
    default_ime_cursor_positioning,
};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Keyboard {
    // Disable ctlseqs with ALT keys
    // For example: Terminal.app does not deal with ctlseqs with ALT keys
    #[serde(
        default = "default_disable_ctlseqs_alt",
        rename = "disable-ctlseqs-alt"
    )]
    pub disable_ctlseqs_alt: bool,

    // Enable IME cursor positioning
    // When enabled, the IME input popup will appear at the cursor position
    #[serde(
        default = "default_ime_cursor_positioning",
        rename = "ime-cursor-positioning"
    )]
    pub ime_cursor_positioning: bool,

    // Modifier mask deciding when a key event is forwarded to the macOS IME.
    // A key event is forwarded when no modifier is pressed, or when the
    // pressed modifiers intersect this mask. Otherwise the event is handled
    // directly by the application without going through the IME.
    //
    // Accepted values (case-insensitive): "shift", "ctrl", "alt", "super".
    // Useful for input methods like SKK that need to receive Ctrl+key
    // combinations directly.
    #[serde(
        default = "default_forward_to_ime_modifier_mask",
        rename = "forward-to-ime-modifier-mask"
    )]
    pub forward_to_ime_modifier_mask: Vec<String>,
}

#[allow(clippy::derivable_impls)]
impl Default for Keyboard {
    fn default() -> Keyboard {
        Keyboard {
            #[cfg(target_os = "macos")]
            disable_ctlseqs_alt: true,
            #[cfg(not(target_os = "macos"))]
            disable_ctlseqs_alt: false,
            ime_cursor_positioning: default_ime_cursor_positioning(),
            forward_to_ime_modifier_mask: default_forward_to_ime_modifier_mask(),
        }
    }
}
