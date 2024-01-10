use rio_backend::crosswords::Mode;
use std::borrow::Cow;

use wa::{KeyCode, ModifiersState};

#[inline(never)]
pub fn build_key_sequence(
    key: &KeyCode,
    mods: ModifiersState,
    mode: Mode,
    is_pressed: bool,
    repeat: bool,
    text_with_modifiers: Option<smol_str::SmolStr>,
) -> Vec<u8> {
    let mut modifiers = 0;
    if mods.shift {
        modifiers |= 0b0001;
    }

    if mods.alt {
        modifiers |= 0b0010;
    }

    if mods.control {
        modifiers |= 0b0100;
    }

    if mods.logo {
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
    // let csi_u_numpad = key.location == KeyLocation::Numpad && named_csi_u;
    let csi_u_numpad = named_csi_u;
    let encode_all = mode.contains(Mode::KEYBOARD_REPORT_ALL_KEYS_AS_ESC);
    let send_event_type =
        mode.contains(Mode::KEYBOARD_REPORT_EVENT_TYPES) && (repeat || !is_pressed);

    let (codepoint, suffix): (Cow<'static, str>, char) = match key {
        // Special case numpad.
        KeyCode::Key0 if csi_u_numpad => ("57399".into(), 'u'),
        KeyCode::Key1 if csi_u_numpad => ("57400".into(), 'u'),
        KeyCode::Key2 if csi_u_numpad => ("57401".into(), 'u'),
        KeyCode::Key3 if csi_u_numpad => ("57402".into(), 'u'),
        KeyCode::Key4 if csi_u_numpad => ("57403".into(), 'u'),
        KeyCode::Key5 if csi_u_numpad => ("57404".into(), 'u'),
        KeyCode::Key6 if csi_u_numpad => ("57405".into(), 'u'),
        KeyCode::Key7 if csi_u_numpad => ("57406".into(), 'u'),
        KeyCode::Key8 if csi_u_numpad => ("57407".into(), 'u'),
        KeyCode::Key9 if csi_u_numpad => ("57408".into(), 'u'),
        // KeyCode::Character(".") if csi_u_numpad => ("57409".into(), 'u'),
        // KeyCode::Character("/") if csi_u_numpad => ("57410".into(), 'u'),
        // KeyCode::Character("*") if csi_u_numpad => ("57411".into(), 'u'),
        // KeyCode::Character("-") if csi_u_numpad => ("57412".into(), 'u'),
        // KeyCode::Character("+") if csi_u_numpad => ("57413".into(), 'u'),
        // KeyCode::Named(Enter) if csi_u_numpad => ("57414".into(), 'u'),
        // KeyCode::Character("=") if csi_u_numpad => ("57415".into(), 'u'),
        // // KP_SEPARATOR if csi_u_numpad => ("57415".into(), 'u'),
        // KeyCode::Named(ArrowLeft) if csi_u_numpad => ("57417".into(), 'u'),
        // KeyCode::Named(ArrowRight) if csi_u_numpad => ("57418".into(), 'u'),
        // KeyCode::Named(ArrowUp) if csi_u_numpad => ("57419".into(), 'u'),
        // KeyCode::Named(ArrowDown) if csi_u_numpad => ("57420".into(), 'u'),
        // KeyCode::Named(PageUp) if csi_u_numpad => ("57421".into(), 'u'),
        // KeyCode::Named(PageDown) if csi_u_numpad => ("57422".into(), 'u'),
        // KeyCode::Named(Home) if csi_u_numpad => ("57423".into(), 'u'),
        // KeyCode::Named(End) if csi_u_numpad => ("57424".into(), 'u'),
        // KeyCode::Named(Insert) if csi_u_numpad => ("57425".into(), 'u'),
        // KeyCode::Named(Delete) if csi_u_numpad => ("57426".into(), 'u'),
        // // KP_BEGIN if csi_u_numpad => ("57427".into(), 'u'),
        // // Handle common keys.
        // KeyCode::Named(ArrowLeft) if mods.is_empty() && !send_event_type => ("".into(), 'D'),
        // KeyCode::Named(ArrowLeft) => ("1".into(), 'D'),
        // KeyCode::Named(ArrowRight) if mods.is_empty() && !send_event_type => ("".into(), 'C'),
        // KeyCode::Named(ArrowRight) => ("1".into(), 'C'),
        // KeyCode::Named(ArrowUp) if mods.is_empty() && !send_event_type => ("".into(), 'A'),
        // KeyCode::Named(ArrowUp) => ("1".into(), 'A'),
        // KeyCode::Named(ArrowDown) if mods.is_empty() && !send_event_type => ("".into(), 'B'),
        // KeyCode::Named(ArrowDown) => ("1".into(), 'B'),
        // KeyCode::Named(Home) if mods.is_empty() && !send_event_type => ("".into(), 'H'),
        // KeyCode::Named(Home) => ("1".into(), 'H'),
        // KeyCode::Named(End) if mods.is_empty() && !send_event_type => ("".into(), 'F'),
        // KeyCode::Named(End) => ("1".into(), 'F'),
        // KeyCode::Named(PageUp) => ("5".into(), '~'),
        // KeyCode::Named(PageDown) => ("6".into(), '~'),
        // KeyCode::Named(Insert) => ("2".into(), '~'),
        // KeyCode::Named(Delete) => ("3".into(), '~'),
        // KeyCode::Named(F1) if mods.is_empty() && named_csi_u && !send_event_type => {
        //     ("".into(), 'P')
        // }
        // KeyCode::Named(F1) if !mods.is_empty() || send_event_type => ("1".into(), 'P'),
        // KeyCode::Named(F2) if mods.is_empty() && named_csi_u && !send_event_type => {
        //     ("".into(), 'Q')
        // }
        // KeyCode::Named(F2) if !mods.is_empty() || send_event_type => ("1".into(), 'Q'),
        // // F3 diverges from alacritty's terminfo for CSI u modes.
        // KeyCode::Named(F3) if named_csi_u => ("13".into(), '~'),
        // KeyCode::Named(F3) if !mods.is_empty() => ("1".into(), 'R'),
        // KeyCode::Named(F4) if mods.is_empty() && named_csi_u && !send_event_type => {
        //     ("".into(), 'S')
        // }
        // KeyCode::Named(F4) if !mods.is_empty() || send_event_type => ("1".into(), 'S'),
        // KeyCode::Named(F5) => ("15".into(), '~'),
        // KeyCode::Named(F6) => ("17".into(), '~'),
        // KeyCode::Named(F7) => ("18".into(), '~'),
        // KeyCode::Named(F8) => ("19".into(), '~'),
        // KeyCode::Named(F9) => ("20".into(), '~'),
        // KeyCode::Named(F10) => ("21".into(), '~'),
        // KeyCode::Named(F11) => ("23".into(), '~'),
        // KeyCode::Named(F12) => ("24".into(), '~'),
        // // These keys are enabled regardless of mode and reported with the CSI u.
        KeyCode::F13 => ("57376".into(), 'u'),
        KeyCode::F14 => ("57377".into(), 'u'),
        KeyCode::F15 => ("57378".into(), 'u'),
        KeyCode::F16 => ("57379".into(), 'u'),
        KeyCode::F17 => ("57380".into(), 'u'),
        KeyCode::F18 => ("57381".into(), 'u'),
        KeyCode::F19 => ("57382".into(), 'u'),
        KeyCode::F20 => ("57383".into(), 'u'),
        KeyCode::F21 => ("57384".into(), 'u'),
        KeyCode::F22 => ("57385".into(), 'u'),
        KeyCode::F23 => ("57386".into(), 'u'),
        KeyCode::F24 => ("57387".into(), 'u'),
        KeyCode::F25 => ("57388".into(), 'u'),
        // KeyCode::F26 => ("57389".into(), 'u'),
        // KeyCode::F27 => ("57390".into(), 'u'),
        // KeyCode::F28 => ("57391".into(), 'u'),
        // KeyCode::F29 => ("57392".into(), 'u'),
        // KeyCode::F30 => ("57393".into(), 'u'),
        // KeyCode::F31 => ("57394".into(), 'u'),
        // KeyCode::F32 => ("57395".into(), 'u'),
        // KeyCode::F33 => ("57396".into(), 'u'),
        // KeyCode::F34 => ("57397".into(), 'u'),
        // KeyCode::F35 => ("57398".into(), 'u'),
        // KeyCode::Named(ScrollLock) => ("57359".into(), 'u'),
        // KeyCode::Named(PrintScreen) => ("57361".into(), 'u'),
        // KeyCode::Named(Pause) => ("57362".into(), 'u'),
        // KeyCode::Named(ContextMenu) => ("57363".into(), 'u'),
        // KeyCode::Named(MediaPlay) => ("57428".into(), 'u'),
        // KeyCode::Named(MediaPause) => ("57429".into(), 'u'),
        // KeyCode::Named(MediaPlayPause) => ("57430".into(), 'u'),
        // // KeyCode::Named(MediaReverse) => ("57431".into(), 'u'),
        // KeyCode::Named(MediaStop) => ("57432".into(), 'u'),
        // KeyCode::Named(MediaFastForward) => ("57433".into(), 'u'),
        // KeyCode::Named(MediaRewind) => ("57434".into(), 'u'),
        // KeyCode::Named(MediaTrackNext) => ("57435".into(), 'u'),
        // KeyCode::Named(MediaTrackPrevious) => ("57436".into(), 'u'),
        // KeyCode::Named(MediaRecord) => ("57437".into(), 'u'),
        // KeyCode::Named(AudioVolumeDown) => ("57438".into(), 'u'),
        // KeyCode::Named(AudioVolumeUp) => ("57439".into(), 'u'),
        // KeyCode::Named(AudioVolumeMute) => ("57440".into(), 'u'),
        KeyCode::Escape if named_csi_u => ("27".into(), 'u'),
        // // Keys which are reported only when all key must be reported
        KeyCode::CapsLock if encode_all => ("57358".into(), 'u'),
        KeyCode::NumLock if encode_all => ("57360".into(), 'u'),
        // // Left mods.
        // KeyCode::Named(Shift) if key.location == KeyLocation::Left && encode_all => {
        //     ("57441".into(), 'u')
        // }
        // KeyCode::Named(Control) if key.location == KeyLocation::Left && encode_all => {
        //     ("57442".into(), 'u')
        // }
        // KeyCode::Named(Alt) if key.location == KeyLocation::Left && encode_all => {
        //     ("57443".into(), 'u')
        // }
        // KeyCode::Named(Super) if key.location == KeyLocation::Left && encode_all => {
        //     ("57444".into(), 'u')
        // }
        // KeyCode::Named(Hyper) if key.location == KeyLocation::Left && encode_all => {
        //     ("57445".into(), 'u')
        // }
        // KeyCode::Named(Meta) if key.location == KeyLocation::Left && encode_all => {
        //     ("57446".into(), 'u')
        // }
        // // Right mods.
        // KeyCode::Named(Shift) if key.location == KeyLocation::Right && encode_all => {
        //     ("57447".into(), 'u')
        // }
        // KeyCode::Named(Control) if key.location == KeyLocation::Right && encode_all => {
        //     ("57448".into(), 'u')
        // }
        // KeyCode::Named(Alt) if key.location == KeyLocation::Right && encode_all => {
        //     ("57449".into(), 'u')
        // }
        // KeyCode::Named(Super) if key.location == KeyLocation::Right && encode_all => {
        //     ("57450".into(), 'u')
        // }
        // KeyCode::Named(Hyper) if key.location == KeyLocation::Right && encode_all => {
        //     ("57451".into(), 'u')
        // }
        // KeyCode::Named(Meta) if key.location == KeyLocation::Right && encode_all => {
        //     ("57452".into(), 'u')
        // }

        // KeyCode::Named(Enter) if encode_all => ("13".into(), 'u'),
        // KeyCode::Named(Tab) if encode_all => ("9".into(), 'u'),
        // KeyCode::Named(Backspace) if encode_all => ("127".into(), 'u'),
        // // When the character key ended up being a text, like when compose was done.
        // KeyCode::Character(c) if encode_all && c.chars().count() > 1 => ("0".into(), 'u'),
        // KeyCode::Character(c) => {
        //     let character = c.chars().next().unwrap();
        //     let base_character = character.to_lowercase().next().unwrap();

        //     let codepoint = u32::from(character);
        //     let base_codepoint = u32::from(base_character);

        //     let payload = if mode.contains(Mode::KEYBOARD_REPORT_ALTERNATE_KEYS)
        //         && codepoint != base_codepoint
        //     {
        //         format!("{codepoint}:{base_codepoint}")
        //     } else {
        //         codepoint.to_string()
        //     };

        //     (payload.into(), 'u')
        // }
        // // In case we have text attached to the key, but we don't have a
        // // matching logical key with the text, likely due to winit not being
        // // able to map it.
        _ if encode_all && text_with_modifiers.is_some() => ("0".into(), 'u'),
        _ => return Vec::new(),
    };

    let mut payload = format!("\x1b[{codepoint}");

    // Add modifiers information. Check for text to push `;`.
    if send_event_type
        || modifiers > 1
        || (mode.contains(Mode::KEYBOARD_REPORT_ASSOCIATED_TEXT)
            && text_with_modifiers.is_some())
    {
        payload.push_str(&format!(";{modifiers}"));
    }

    // Push event types. The `Press` is default, so we don't have to push it.
    if send_event_type {
        payload.push(':');
        let event_type = match (repeat, is_pressed) {
            (true, _) => '2',
            (false, true) => '1',
            (false, false) => '3',
        };

        payload.push(event_type);
    }

    if mode.contains(Mode::KEYBOARD_REPORT_ASSOCIATED_TEXT) && is_pressed {
        if let Some(text) = text_with_modifiers {
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
