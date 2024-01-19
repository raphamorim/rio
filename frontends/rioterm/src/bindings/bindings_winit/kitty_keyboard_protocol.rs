// build_key_sequence was originally taken from alacritty
// which is licensed under Apache 2.0 license.

use crate::screen::ElementState;
use crate::screen::Mode;
use crate::screen::ModifiersState;
use std::borrow::Cow;
use winit::event::KeyEvent;
use winit::keyboard::Key;
use winit::keyboard::KeyLocation;
use winit::keyboard::NamedKey::*;

#[inline(never)]
pub fn build_key_sequence(key: &KeyEvent, mods: ModifiersState, mode: Mode) -> Vec<u8> {
    let mut modifiers = 0;
    if mods.shift_key() {
        modifiers |= 0b0001;
    }

    if mods.alt_key() {
        modifiers |= 0b0010;
    }

    if mods.control_key() {
        modifiers |= 0b0100;
    }

    if mods.super_key() {
        modifiers |= 0b1000;
    }

    // The `1` must be added to result.
    modifiers += 1;

    let named_csi_u = mode.intersects(
        Mode::KEYBOARD_REPORT_ALL_KEYS_AS_ESC
            | Mode::KEYBOARD_DISAMBIGUATE_ESC_CODES
            | Mode::KEYBOARD_REPORT_EVENT_TYPES,
    );
    // Send CSI u for numpad
    let csi_u_numpad = key.location == KeyLocation::Numpad && named_csi_u;
    let encode_all = mode.contains(Mode::KEYBOARD_REPORT_ALL_KEYS_AS_ESC);
    let send_event_type = mode.contains(Mode::KEYBOARD_REPORT_EVENT_TYPES)
        && (key.repeat || key.state == ElementState::Released);

    let (codepoint, suffix): (Cow<'static, str>, char) = match key.logical_key.as_ref() {
        // Special case numpad.
        Key::Character("0") if csi_u_numpad => ("57399".into(), 'u'),
        Key::Character("1") if csi_u_numpad => ("57400".into(), 'u'),
        Key::Character("2") if csi_u_numpad => ("57401".into(), 'u'),
        Key::Character("3") if csi_u_numpad => ("57402".into(), 'u'),
        Key::Character("4") if csi_u_numpad => ("57403".into(), 'u'),
        Key::Character("5") if csi_u_numpad => ("57404".into(), 'u'),
        Key::Character("6") if csi_u_numpad => ("57405".into(), 'u'),
        Key::Character("7") if csi_u_numpad => ("57406".into(), 'u'),
        Key::Character("8") if csi_u_numpad => ("57407".into(), 'u'),
        Key::Character("9") if csi_u_numpad => ("57408".into(), 'u'),
        Key::Character(".") if csi_u_numpad => ("57409".into(), 'u'),
        Key::Character("/") if csi_u_numpad => ("57410".into(), 'u'),
        Key::Character("*") if csi_u_numpad => ("57411".into(), 'u'),
        Key::Character("-") if csi_u_numpad => ("57412".into(), 'u'),
        Key::Character("+") if csi_u_numpad => ("57413".into(), 'u'),
        Key::Named(Enter) if csi_u_numpad => ("57414".into(), 'u'),
        Key::Character("=") if csi_u_numpad => ("57415".into(), 'u'),
        // KP_SEPARATOR if csi_u_numpad => ("57415".into(), 'u'),
        Key::Named(ArrowLeft) if csi_u_numpad => ("57417".into(), 'u'),
        Key::Named(ArrowRight) if csi_u_numpad => ("57418".into(), 'u'),
        Key::Named(ArrowUp) if csi_u_numpad => ("57419".into(), 'u'),
        Key::Named(ArrowDown) if csi_u_numpad => ("57420".into(), 'u'),
        Key::Named(PageUp) if csi_u_numpad => ("57421".into(), 'u'),
        Key::Named(PageDown) if csi_u_numpad => ("57422".into(), 'u'),
        Key::Named(Home) if csi_u_numpad => ("57423".into(), 'u'),
        Key::Named(End) if csi_u_numpad => ("57424".into(), 'u'),
        Key::Named(Insert) if csi_u_numpad => ("57425".into(), 'u'),
        Key::Named(Delete) if csi_u_numpad => ("57426".into(), 'u'),
        // KP_BEGIN if csi_u_numpad => ("57427".into(), 'u'),
        // Handle common keys.
        Key::Named(ArrowLeft) if mods.is_empty() && !send_event_type => ("".into(), 'D'),
        Key::Named(ArrowLeft) => ("1".into(), 'D'),
        Key::Named(ArrowRight) if mods.is_empty() && !send_event_type => ("".into(), 'C'),
        Key::Named(ArrowRight) => ("1".into(), 'C'),
        Key::Named(ArrowUp) if mods.is_empty() && !send_event_type => ("".into(), 'A'),
        Key::Named(ArrowUp) => ("1".into(), 'A'),
        Key::Named(ArrowDown) if mods.is_empty() && !send_event_type => ("".into(), 'B'),
        Key::Named(ArrowDown) => ("1".into(), 'B'),
        Key::Named(Home) if mods.is_empty() && !send_event_type => ("".into(), 'H'),
        Key::Named(Home) => ("1".into(), 'H'),
        Key::Named(End) if mods.is_empty() && !send_event_type => ("".into(), 'F'),
        Key::Named(End) => ("1".into(), 'F'),
        Key::Named(PageUp) => ("5".into(), '~'),
        Key::Named(PageDown) => ("6".into(), '~'),
        Key::Named(Insert) => ("2".into(), '~'),
        Key::Named(Delete) => ("3".into(), '~'),
        Key::Named(F1) if mods.is_empty() && named_csi_u && !send_event_type => {
            ("".into(), 'P')
        }
        Key::Named(F1) if !mods.is_empty() || send_event_type => ("1".into(), 'P'),
        Key::Named(F2) if mods.is_empty() && named_csi_u && !send_event_type => {
            ("".into(), 'Q')
        }
        Key::Named(F2) if !mods.is_empty() || send_event_type => ("1".into(), 'Q'),
        // F3 diverges from alacritty's terminfo for CSI u modes.
        Key::Named(F3) if named_csi_u => ("13".into(), '~'),
        Key::Named(F3) if !mods.is_empty() => ("1".into(), 'R'),
        Key::Named(F4) if mods.is_empty() && named_csi_u && !send_event_type => {
            ("".into(), 'S')
        }
        Key::Named(F4) if !mods.is_empty() || send_event_type => ("1".into(), 'S'),
        Key::Named(F5) => ("15".into(), '~'),
        Key::Named(F6) => ("17".into(), '~'),
        Key::Named(F7) => ("18".into(), '~'),
        Key::Named(F8) => ("19".into(), '~'),
        Key::Named(F9) => ("20".into(), '~'),
        Key::Named(F10) => ("21".into(), '~'),
        Key::Named(F11) => ("23".into(), '~'),
        Key::Named(F12) => ("24".into(), '~'),
        // These keys are enabled regardless of mode and reported with the CSI u.
        Key::Named(F13) => ("57376".into(), 'u'),
        Key::Named(F14) => ("57377".into(), 'u'),
        Key::Named(F15) => ("57378".into(), 'u'),
        Key::Named(F16) => ("57379".into(), 'u'),
        Key::Named(F17) => ("57380".into(), 'u'),
        Key::Named(F18) => ("57381".into(), 'u'),
        Key::Named(F19) => ("57382".into(), 'u'),
        Key::Named(F20) => ("57383".into(), 'u'),
        Key::Named(F21) => ("57384".into(), 'u'),
        Key::Named(F22) => ("57385".into(), 'u'),
        Key::Named(F23) => ("57386".into(), 'u'),
        Key::Named(F24) => ("57387".into(), 'u'),
        Key::Named(F25) => ("57388".into(), 'u'),
        Key::Named(F26) => ("57389".into(), 'u'),
        Key::Named(F27) => ("57390".into(), 'u'),
        Key::Named(F28) => ("57391".into(), 'u'),
        Key::Named(F29) => ("57392".into(), 'u'),
        Key::Named(F30) => ("57393".into(), 'u'),
        Key::Named(F31) => ("57394".into(), 'u'),
        Key::Named(F32) => ("57395".into(), 'u'),
        Key::Named(F33) => ("57396".into(), 'u'),
        Key::Named(F34) => ("57397".into(), 'u'),
        Key::Named(F35) => ("57398".into(), 'u'),
        Key::Named(ScrollLock) => ("57359".into(), 'u'),
        Key::Named(PrintScreen) => ("57361".into(), 'u'),
        Key::Named(Pause) => ("57362".into(), 'u'),
        Key::Named(ContextMenu) => ("57363".into(), 'u'),
        Key::Named(MediaPlay) => ("57428".into(), 'u'),
        Key::Named(MediaPause) => ("57429".into(), 'u'),
        Key::Named(MediaPlayPause) => ("57430".into(), 'u'),
        // Key::Named(MediaReverse) => ("57431".into(), 'u'),
        Key::Named(MediaStop) => ("57432".into(), 'u'),
        Key::Named(MediaFastForward) => ("57433".into(), 'u'),
        Key::Named(MediaRewind) => ("57434".into(), 'u'),
        Key::Named(MediaTrackNext) => ("57435".into(), 'u'),
        Key::Named(MediaTrackPrevious) => ("57436".into(), 'u'),
        Key::Named(MediaRecord) => ("57437".into(), 'u'),
        Key::Named(AudioVolumeDown) => ("57438".into(), 'u'),
        Key::Named(AudioVolumeUp) => ("57439".into(), 'u'),
        Key::Named(AudioVolumeMute) => ("57440".into(), 'u'),
        Key::Named(Escape) if named_csi_u => ("27".into(), 'u'),
        // Keys which are reported only when all key must be reported
        Key::Named(CapsLock) if encode_all => ("57358".into(), 'u'),
        Key::Named(NumLock) if encode_all => ("57360".into(), 'u'),
        // Left mods.
        Key::Named(Shift) if key.location == KeyLocation::Left && encode_all => {
            ("57441".into(), 'u')
        }
        Key::Named(Control) if key.location == KeyLocation::Left && encode_all => {
            ("57442".into(), 'u')
        }
        Key::Named(Alt) if key.location == KeyLocation::Left && encode_all => {
            ("57443".into(), 'u')
        }
        Key::Named(Super) if key.location == KeyLocation::Left && encode_all => {
            ("57444".into(), 'u')
        }
        Key::Named(Hyper) if key.location == KeyLocation::Left && encode_all => {
            ("57445".into(), 'u')
        }
        Key::Named(Meta) if key.location == KeyLocation::Left && encode_all => {
            ("57446".into(), 'u')
        }
        // Right mods.
        Key::Named(Shift) if key.location == KeyLocation::Right && encode_all => {
            ("57447".into(), 'u')
        }
        Key::Named(Control) if key.location == KeyLocation::Right && encode_all => {
            ("57448".into(), 'u')
        }
        Key::Named(Alt) if key.location == KeyLocation::Right && encode_all => {
            ("57449".into(), 'u')
        }
        Key::Named(Super) if key.location == KeyLocation::Right && encode_all => {
            ("57450".into(), 'u')
        }
        Key::Named(Hyper) if key.location == KeyLocation::Right && encode_all => {
            ("57451".into(), 'u')
        }
        Key::Named(Meta) if key.location == KeyLocation::Right && encode_all => {
            ("57452".into(), 'u')
        }

        Key::Named(Enter) if encode_all => ("13".into(), 'u'),
        Key::Named(Tab) if encode_all => ("9".into(), 'u'),
        Key::Named(Backspace) if encode_all => ("127".into(), 'u'),
        // When the character key ended up being a text, like when compose was done.
        Key::Character(c) if encode_all && c.chars().count() > 1 => ("0".into(), 'u'),
        Key::Character(c) => {
            let character = c.chars().next().unwrap();
            let base_character = character.to_lowercase().next().unwrap();

            let codepoint = u32::from(character);
            let base_codepoint = u32::from(base_character);

            let payload = if mode.contains(Mode::KEYBOARD_REPORT_ALTERNATE_KEYS)
                && codepoint != base_codepoint
            {
                format!("{codepoint}:{base_codepoint}")
            } else {
                codepoint.to_string()
            };

            (payload.into(), 'u')
        }
        // In case we have text attached to the key, but we don't have a
        // matching logical key with the text, likely due to winit not being
        // able to map it.
        _ if encode_all && key.text.is_some() => ("0".into(), 'u'),
        _ => return Vec::new(),
    };

    let mut payload = format!("\x1b[{codepoint}");

    // Add modifiers information. Check for text to push `;`.
    if send_event_type
        || modifiers > 1
        || (mode.contains(Mode::KEYBOARD_REPORT_ASSOCIATED_TEXT) && key.text.is_some())
    {
        payload.push_str(&format!(";{modifiers}"));
    }

    // Push event types. The `Press` is default, so we don't have to push it.
    if send_event_type {
        payload.push(':');
        let event_type = match key.state {
            _ if key.repeat => '2',
            ElementState::Pressed => '1',
            ElementState::Released => '3',
        };
        payload.push(event_type);
    }

    if mode.contains(Mode::KEYBOARD_REPORT_ASSOCIATED_TEXT)
        && key.state != ElementState::Released
    {
        if let Some(text) = &key.text {
            let mut codepoints = text.chars().map(u32::from);
            if let Some(codepoint) = codepoints.next() {
                payload.push_str(&format!(";{codepoint}"));
            }
            // Push the rest of the chars.
            for codepoint in codepoints {
                payload.push_str(&format!(":{codepoint}"));
            }
        }
    }

    // Terminate the sequence.
    payload.push(suffix);

    payload.into_bytes()
}
