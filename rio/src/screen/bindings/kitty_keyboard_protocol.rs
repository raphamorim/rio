use crate::screen::ElementState;
use crate::screen::Mode;
use crate::screen::ModifiersState;
use std::borrow::Cow;
use winit::event::KeyEvent;
use winit::keyboard::Key;
use winit::keyboard::KeyLocation;

#[inline(never)]
pub fn build_key_sequence(key: KeyEvent, mods: ModifiersState, mode: Mode) -> Vec<u8> {
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
        Key::Enter if csi_u_numpad => ("57414".into(), 'u'),
        Key::Character("=") if csi_u_numpad => ("57415".into(), 'u'),
        // KP_SEPARATOR if csi_u_numpad => ("57415".into(), 'u'),
        Key::ArrowLeft if csi_u_numpad => ("57417".into(), 'u'),
        Key::ArrowRight if csi_u_numpad => ("57418".into(), 'u'),
        Key::ArrowUp if csi_u_numpad => ("57419".into(), 'u'),
        Key::ArrowDown if csi_u_numpad => ("57420".into(), 'u'),
        Key::PageUp if csi_u_numpad => ("57421".into(), 'u'),
        Key::PageDown if csi_u_numpad => ("57422".into(), 'u'),
        Key::Home if csi_u_numpad => ("57423".into(), 'u'),
        Key::End if csi_u_numpad => ("57424".into(), 'u'),
        Key::Insert if csi_u_numpad => ("57425".into(), 'u'),
        Key::Delete if csi_u_numpad => ("57426".into(), 'u'),
        // KP_BEGIN if csi_u_numpad => ("57427".into(), 'u'),
        // Handle common keys.
        Key::ArrowLeft if mods.is_empty() && !send_event_type => ("".into(), 'D'),
        Key::ArrowLeft => ("1".into(), 'D'),
        Key::ArrowRight if mods.is_empty() && !send_event_type => ("".into(), 'C'),
        Key::ArrowRight => ("1".into(), 'C'),
        Key::ArrowUp if mods.is_empty() && !send_event_type => ("".into(), 'A'),
        Key::ArrowUp => ("1".into(), 'A'),
        Key::ArrowDown if mods.is_empty() && !send_event_type => ("".into(), 'B'),
        Key::ArrowDown => ("1".into(), 'B'),
        Key::Home if mods.is_empty() && !send_event_type => ("".into(), 'H'),
        Key::Home => ("1".into(), 'H'),
        Key::End if mods.is_empty() && !send_event_type => ("".into(), 'F'),
        Key::End => ("1".into(), 'F'),
        Key::PageUp => ("5".into(), '~'),
        Key::PageDown => ("6".into(), '~'),
        Key::Insert => ("2".into(), '~'),
        Key::Delete => ("3".into(), '~'),
        Key::F1 if mods.is_empty() && named_csi_u && !send_event_type => ("".into(), 'P'),
        Key::F1 if !mods.is_empty() || send_event_type => ("1".into(), 'P'),
        Key::F2 if mods.is_empty() && named_csi_u && !send_event_type => ("".into(), 'Q'),
        Key::F2 if !mods.is_empty() || send_event_type => ("1".into(), 'Q'),
        // F3 diverges from alacritty's terminfo for CSI u modes.
        Key::F3 if named_csi_u => ("13".into(), '~'),
        Key::F3 if !mods.is_empty() => ("1".into(), 'R'),
        Key::F4 if mods.is_empty() && named_csi_u && !send_event_type => ("".into(), 'S'),
        Key::F4 if !mods.is_empty() || send_event_type => ("1".into(), 'S'),
        Key::F5 => ("15".into(), '~'),
        Key::F6 => ("17".into(), '~'),
        Key::F7 => ("18".into(), '~'),
        Key::F8 => ("19".into(), '~'),
        Key::F9 => ("20".into(), '~'),
        Key::F10 => ("21".into(), '~'),
        Key::F11 => ("23".into(), '~'),
        Key::F12 => ("24".into(), '~'),
        // These keys are enabled regardless of mode and reported with the CSI u.
        Key::F13 => ("57376".into(), 'u'),
        Key::F14 => ("57377".into(), 'u'),
        Key::F15 => ("57378".into(), 'u'),
        Key::F16 => ("57379".into(), 'u'),
        Key::F17 => ("57380".into(), 'u'),
        Key::F18 => ("57381".into(), 'u'),
        Key::F19 => ("57382".into(), 'u'),
        Key::F20 => ("57383".into(), 'u'),
        Key::F21 => ("57384".into(), 'u'),
        Key::F22 => ("57385".into(), 'u'),
        Key::F23 => ("57386".into(), 'u'),
        Key::F24 => ("57387".into(), 'u'),
        Key::F25 => ("57388".into(), 'u'),
        Key::F26 => ("57389".into(), 'u'),
        Key::F27 => ("57390".into(), 'u'),
        Key::F28 => ("57391".into(), 'u'),
        Key::F29 => ("57392".into(), 'u'),
        Key::F30 => ("57393".into(), 'u'),
        Key::F31 => ("57394".into(), 'u'),
        Key::F32 => ("57395".into(), 'u'),
        Key::F33 => ("57396".into(), 'u'),
        Key::F34 => ("57397".into(), 'u'),
        Key::F35 => ("57398".into(), 'u'),
        Key::ScrollLock => ("57359".into(), 'u'),
        Key::PrintScreen => ("57361".into(), 'u'),
        Key::Pause => ("57362".into(), 'u'),
        Key::ContextMenu => ("57363".into(), 'u'),
        Key::MediaPlay => ("57428".into(), 'u'),
        Key::MediaPause => ("57429".into(), 'u'),
        Key::MediaPlayPause => ("57430".into(), 'u'),
        // Key::MediaReverse => ("57431".into(), 'u'),
        Key::MediaStop => ("57432".into(), 'u'),
        Key::MediaFastForward => ("57433".into(), 'u'),
        Key::MediaRewind => ("57434".into(), 'u'),
        Key::MediaTrackNext => ("57435".into(), 'u'),
        Key::MediaTrackPrevious => ("57436".into(), 'u'),
        Key::MediaRecord => ("57437".into(), 'u'),
        Key::AudioVolumeDown => ("57438".into(), 'u'),
        Key::AudioVolumeUp => ("57439".into(), 'u'),
        Key::AudioVolumeMute => ("57440".into(), 'u'),
        Key::Escape if named_csi_u => ("27".into(), 'u'),
        // Keys which are reported only when all key must be reported
        Key::CapsLock if encode_all => ("57358".into(), 'u'),
        Key::NumLock if encode_all => ("57360".into(), 'u'),
        // Left mods.
        Key::Shift if key.location == KeyLocation::Left && encode_all => {
            ("57441".into(), 'u')
        }
        Key::Control if key.location == KeyLocation::Left && encode_all => {
            ("57442".into(), 'u')
        }
        Key::Alt if key.location == KeyLocation::Left && encode_all => {
            ("57443".into(), 'u')
        }
        Key::Super if key.location == KeyLocation::Left && encode_all => {
            ("57444".into(), 'u')
        }
        Key::Hyper if key.location == KeyLocation::Left && encode_all => {
            ("57445".into(), 'u')
        }
        Key::Meta if key.location == KeyLocation::Left && encode_all => {
            ("57446".into(), 'u')
        }
        // Right mods.
        Key::Shift if key.location == KeyLocation::Right && encode_all => {
            ("57447".into(), 'u')
        }
        Key::Control if key.location == KeyLocation::Right && encode_all => {
            ("57448".into(), 'u')
        }
        Key::Alt if key.location == KeyLocation::Right && encode_all => {
            ("57449".into(), 'u')
        }
        Key::Super if key.location == KeyLocation::Right && encode_all => {
            ("57450".into(), 'u')
        }
        Key::Hyper if key.location == KeyLocation::Right && encode_all => {
            ("57451".into(), 'u')
        }
        Key::Meta if key.location == KeyLocation::Right && encode_all => {
            ("57452".into(), 'u')
        }

        Key::Enter if encode_all => ("13".into(), 'u'),
        Key::Tab if encode_all => ("9".into(), 'u'),
        Key::Backspace if encode_all => ("127".into(), 'u'),
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
        if let Some(text) = key.text {
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
