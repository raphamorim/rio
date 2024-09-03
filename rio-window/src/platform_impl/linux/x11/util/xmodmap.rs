use std::collections::HashSet;
use std::slice;

use x11_dl::xlib::{KeyCode as XKeyCode, XModifierKeymap};

// Offsets within XModifierKeymap to each set of keycodes.
// We are only interested in Shift, Control, Alt, and Logo.
//
// There are 8 sets total. The order of keycode sets is:
//     Shift, Lock, Control, Mod1 (Alt), Mod2, Mod3, Mod4 (Logo), Mod5
//
// https://tronche.com/gui/x/xlib/input/XSetModifierMapping.html
const NUM_MODS: usize = 8;

/// Track which keys are modifiers, so we can properly replay them when they were filtered.
#[derive(Debug, Default)]
pub struct ModifierKeymap {
    // Maps keycodes to modifiers
    modifiers: HashSet<XKeyCode>,
}

impl ModifierKeymap {
    pub fn new() -> ModifierKeymap {
        ModifierKeymap::default()
    }

    pub fn is_modifier(&self, keycode: XKeyCode) -> bool {
        self.modifiers.contains(&keycode)
    }

    pub fn reload_from_x_connection(&mut self, xconn: &super::XConnection) {
        unsafe {
            let keymap = (xconn.xlib.XGetModifierMapping)(xconn.display);

            if keymap.is_null() {
                return;
            }

            self.reset_from_x_keymap(&*keymap);

            (xconn.xlib.XFreeModifiermap)(keymap);
        }
    }

    fn reset_from_x_keymap(&mut self, keymap: &XModifierKeymap) {
        let keys_per_mod = keymap.max_keypermod as usize;

        let keys = unsafe {
            slice::from_raw_parts(keymap.modifiermap as *const _, keys_per_mod * NUM_MODS)
        };
        self.modifiers.clear();
        for key in keys {
            self.modifiers.insert(*key);
        }
    }
}
