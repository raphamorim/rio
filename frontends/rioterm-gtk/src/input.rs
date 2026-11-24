//! GDK key events as rio-vt sees them. Its encoder does the rest
//! (legacy, modifyOtherKeys, kitty), so this only has to say which key it
//! was, unshifted, and what text the platform produced for it.

use gtk::gdk;
use gtk::glib::translate::IntoGlib;
use gtk::prelude::*;
use rio_vt::key::{Key, KeyAction, KeyEvent, Keypad, Modifiers};

pub fn modifiers(state: gdk::ModifierType) -> Modifiers {
    let mut mods = Modifiers::empty();
    if state.contains(gdk::ModifierType::SHIFT_MASK) {
        mods |= Modifiers::SHIFT;
    }
    if state.contains(gdk::ModifierType::CONTROL_MASK) {
        mods |= Modifiers::CTRL;
    }
    if state.contains(gdk::ModifierType::ALT_MASK) {
        mods |= Modifiers::ALT;
    }
    if state.contains(gdk::ModifierType::SUPER_MASK) {
        mods |= Modifiers::SUPER;
    }
    mods
}

/// The first and last of GDK's F1..F24 keyvals, which are consecutive.
const F1: u32 = 0xffbe;
const F24: u32 = 0xffd5;

/// Keys that aren't text.
fn named(keyval: gdk::Key) -> Option<Key> {
    use gdk::Key as K;
    let key = match keyval {
        K::Return | K::ISO_Enter => Key::Enter,
        K::KP_Enter => Key::Keypad(Keypad::Enter),
        K::KP_0
        | K::KP_1
        | K::KP_2
        | K::KP_3
        | K::KP_4
        | K::KP_5
        | K::KP_6
        | K::KP_7
        | K::KP_8
        | K::KP_9 => Key::Keypad(Keypad::Digit(
            (keyval.into_glib() - K::KP_0.into_glib()) as u8,
        )),
        K::KP_Decimal | K::KP_Separator => Key::Keypad(Keypad::Decimal),
        K::KP_Divide => Key::Keypad(Keypad::Divide),
        K::KP_Multiply => Key::Keypad(Keypad::Multiply),
        K::KP_Subtract => Key::Keypad(Keypad::Subtract),
        K::KP_Add => Key::Keypad(Keypad::Add),
        K::KP_Equal => Key::Keypad(Keypad::Equal),
        K::Tab | K::ISO_Left_Tab | K::KP_Tab => Key::Tab,
        K::BackSpace => Key::Backspace,
        K::Escape => Key::Escape,
        K::Up | K::KP_Up => Key::Up,
        K::Down | K::KP_Down => Key::Down,
        K::Left | K::KP_Left => Key::Left,
        K::Right | K::KP_Right => Key::Right,
        K::Home | K::KP_Home => Key::Home,
        K::End | K::KP_End => Key::End,
        K::Page_Up | K::KP_Page_Up => Key::PageUp,
        K::Page_Down | K::KP_Page_Down => Key::PageDown,
        K::Insert | K::KP_Insert => Key::Insert,
        K::Delete | K::KP_Delete => Key::Delete,
        K::Caps_Lock => Key::CapsLock,
        K::Shift_L => Key::ShiftLeft,
        K::Shift_R => Key::ShiftRight,
        K::Control_L => Key::ControlLeft,
        K::Control_R => Key::ControlRight,
        K::Alt_L => Key::AltLeft,
        K::Alt_R => Key::AltRight,
        K::Super_L => Key::SuperLeft,
        K::Super_R => Key::SuperRight,
        _ => {
            let raw = keyval.into_glib();
            if (F1..=F24).contains(&raw) {
                Key::F((raw - F1 + 1) as u8)
            } else {
                return None;
            }
        }
    };
    Some(key)
}

/// The character key, unshifted (`shift+1` is `1`, not `!`): what the
/// same physical key gives with no modifiers on the event's layout.
fn unshifted(event: &gdk::KeyEvent) -> Option<Key> {
    let base = event
        .display()
        .and_then(|display| {
            display.translate_key(
                event.keycode(),
                gdk::ModifierType::empty(),
                event.layout() as i32,
            )
        })
        .map(|(keyval, ..)| keyval)
        .unwrap_or_else(|| event.keyval());
    base.to_lower()
        .to_unicode()
        .filter(|c| !c.is_control())
        .map(Key::Char)
}

/// The text a key press produced without an input method: its character,
/// shift applied, if it is one.
pub fn key_text(keyval: gdk::Key) -> Option<String> {
    keyval
        .to_unicode()
        .filter(|c| !c.is_control())
        .map(String::from)
}

pub fn key_event(
    event: &gdk::KeyEvent,
    action: KeyAction,
    text: Option<String>,
) -> KeyEvent {
    let keyval = event.keyval();
    // Alt (and AltGr, which GDK reports as the level it picked) spent on
    // producing the text isn't also meta; shift stays a modifier the
    // program sees, since the key is reported unshifted.
    let consumed = modifiers(event.consumed_modifiers()) & Modifiers::ALT;
    KeyEvent {
        action,
        key: named(keyval).or_else(|| unshifted(event)),
        mods: modifiers(event.modifier_state()),
        consumed_mods: consumed,
        text,
        composing: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_map_one_to_one() {
        use gdk::ModifierType as M;
        assert_eq!(modifiers(M::empty()), Modifiers::empty());
        assert_eq!(
            modifiers(M::SHIFT_MASK | M::CONTROL_MASK | M::ALT_MASK | M::SUPER_MASK),
            Modifiers::SHIFT | Modifiers::CTRL | Modifiers::ALT | Modifiers::SUPER
        );
        // Lock keys and mouse buttons aren't modifiers a program sees.
        assert_eq!(
            modifiers(M::LOCK_MASK | M::BUTTON1_MASK),
            Modifiers::empty()
        );
    }

    #[test]
    fn named_keys_and_their_keypad_twins() {
        use gdk::Key as K;
        for (keyval, key) in [
            (K::Return, Key::Enter),
            (K::KP_Enter, Key::Keypad(Keypad::Enter)),
            (K::KP_7, Key::Keypad(Keypad::Digit(7))),
            (K::KP_Add, Key::Keypad(Keypad::Add)),
            (K::ISO_Left_Tab, Key::Tab),
            (K::BackSpace, Key::Backspace),
            (K::KP_Up, Key::Up),
            (K::Page_Down, Key::PageDown),
            (K::KP_Delete, Key::Delete),
            (K::Control_R, Key::ControlRight),
        ] {
            assert_eq!(named(keyval), Some(key), "{keyval:?}");
        }
        assert_eq!(named(K::a), None, "text keys aren't named");
    }

    #[test]
    fn function_keys_count_from_one() {
        use gdk::Key as K;
        assert_eq!(named(K::F1), Some(Key::F(1)));
        assert_eq!(named(K::F12), Some(Key::F(12)));
        assert_eq!(named(K::F24), Some(Key::F(24)));
        assert_eq!(named(K::F25), None);
    }

    #[test]
    fn key_text_is_printable_only() {
        use gdk::Key as K;
        assert_eq!(key_text(K::A).as_deref(), Some("A"));
        assert_eq!(key_text(K::eacute).as_deref(), Some("é"));
        assert_eq!(key_text(K::Return), None);
        assert_eq!(key_text(K::Escape), None);
    }
}
