use std::ffi::c_void;

use core_foundation::{
    base::CFRelease,
    data::{CFDataGetBytePtr, CFDataRef},
};
use icrate::Foundation::MainThreadMarker;
use smol_str::SmolStr;

use super::appkit::{NSEvent, NSEventModifierFlags};
use crate::{
    event::{ElementState, KeyEvent, Modifiers},
    keyboard::{
        Key, KeyCode, KeyLocation, ModifiersKeys, ModifiersState, NamedKey, NativeKey,
        NativeKeyCode, PhysicalKey,
    },
    platform::{
        modifier_supplement::KeyEventExtModifierSupplement,
        scancode::PhysicalKeyExtScancode,
    },
    wa::platform_impl::platform::ffi,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyEventExtra {
    pub text_with_all_modifiers: Option<SmolStr>,
    pub key_without_modifiers: Key,
}

impl KeyEventExtModifierSupplement for KeyEvent {
    fn text_with_all_modifiers(&self) -> Option<&str> {
        self.platform_specific
            .text_with_all_modifiers
            .as_ref()
            .map(|s| s.as_str())
    }

    fn key_without_modifiers(&self) -> Key {
        self.platform_specific.key_without_modifiers.clone()
    }
}

pub fn get_modifierless_char(scancode: u16) -> Key {
    let mut string = [0; 16];
    let input_source;
    let layout;
    unsafe {
        input_source = ffi::TISCopyCurrentKeyboardLayoutInputSource();
        if input_source.is_null() {
            log::error!("`TISCopyCurrentKeyboardLayoutInputSource` returned null ptr");
            return Key::Unidentified(NativeKey::MacOS(scancode));
        }
        let layout_data = ffi::TISGetInputSourceProperty(
            input_source,
            ffi::kTISPropertyUnicodeKeyLayoutData,
        );
        if layout_data.is_null() {
            CFRelease(input_source as *mut c_void);
            log::error!("`TISGetInputSourceProperty` returned null ptr");
            return Key::Unidentified(NativeKey::MacOS(scancode));
        }
        layout =
            CFDataGetBytePtr(layout_data as CFDataRef) as *const ffi::UCKeyboardLayout;
    }
    let keyboard_type =
        MainThreadMarker::run_on_main(|_mtm| unsafe { ffi::LMGetKbdType() });

    let mut result_len = 0;
    let mut dead_keys = 0;
    let modifiers = 0;
    let translate_result = unsafe {
        ffi::UCKeyTranslate(
            layout,
            scancode,
            ffi::kUCKeyActionDisplay,
            modifiers,
            keyboard_type as u32,
            ffi::kUCKeyTranslateNoDeadKeysMask,
            &mut dead_keys,
            string.len() as ffi::UniCharCount,
            &mut result_len,
            string.as_mut_ptr(),
        )
    };
    unsafe {
        CFRelease(input_source as *mut c_void);
    }
    if translate_result != 0 {
        log::error!(
            "`UCKeyTranslate` returned with the non-zero value: {}",
            translate_result
        );
        return Key::Unidentified(NativeKey::MacOS(scancode));
    }
    if result_len == 0 {
        log::error!("`UCKeyTranslate` was succesful but gave a string of 0 length.");
        return Key::Unidentified(NativeKey::MacOS(scancode));
    }
    let chars = String::from_utf16_lossy(&string[0..result_len as usize]);
    Key::Character(SmolStr::new(chars))
}

fn get_logical_key_char(ns_event: &NSEvent, modifierless_chars: &str) -> Key {
    let string = ns_event
        .charactersIgnoringModifiers()
        .map(|s| s.to_string())
        .unwrap_or_default();
    if string.is_empty() {
        // Probably a dead key
        let first_char = modifierless_chars.chars().next();
        return Key::Dead(first_char);
    }
    Key::Character(SmolStr::new(string))
}

/// Create `KeyEvent` for the given `NSEvent`.
///
/// This function shouldn't be called when the IME input is in process.
pub(crate) fn create_key_event(
    ns_event: &NSEvent,
    is_press: bool,
    is_repeat: bool,
    key_override: Option<PhysicalKey>,
) -> KeyEvent {
    use ElementState::{Pressed, Released};
    let state = if is_press { Pressed } else { Released };

    let scancode = ns_event.key_code();
    let mut physical_key =
        key_override.unwrap_or_else(|| PhysicalKey::from_scancode(scancode as u32));

    let text_with_all_modifiers: Option<SmolStr> = if key_override.is_some() {
        None
    } else {
        let characters = ns_event
            .characters()
            .map(|s| s.to_string())
            .unwrap_or_default();
        if characters.is_empty() {
            None
        } else {
            if matches!(physical_key, PhysicalKey::Unidentified(_)) {
                // The key may be one of the funky function keys
                physical_key = extra_function_key_to_code(scancode, &characters);
            }
            Some(SmolStr::new(characters))
        }
    };

    let key_from_code = code_to_key(physical_key, scancode);
    let (logical_key, key_without_modifiers) =
        if matches!(key_from_code, Key::Unidentified(_)) {
            let key_without_modifiers = get_modifierless_char(scancode);

            let modifiers = NSEvent::modifierFlags(ns_event);
            let has_ctrl = modifiers.contains(NSEventModifierFlags::NSControlKeyMask);

            let logical_key = match text_with_all_modifiers.as_ref() {
                // Only checking for ctrl here, not checking for alt because we DO want to
                // include its effect in the key. For example if -on the Germay layout- one
                // presses alt+8, the logical key should be "{"
                // Also not checking if this is a release event because then this issue would
                // still affect the key release.
                Some(text) if !has_ctrl => Key::Character(text.clone()),
                _ => {
                    let modifierless_chars = match key_without_modifiers.as_ref() {
                        Key::Character(ch) => ch,
                        _ => "",
                    };
                    get_logical_key_char(ns_event, modifierless_chars)
                }
            };

            (logical_key, key_without_modifiers)
        } else {
            (key_from_code.clone(), key_from_code)
        };

    let text = if is_press {
        logical_key.to_text().map(SmolStr::new)
    } else {
        None
    };

    let location = code_to_location(physical_key);

    KeyEvent {
        location,
        logical_key,
        physical_key,
        repeat: is_repeat,
        state,
        text,
        platform_specific: KeyEventExtra {
            key_without_modifiers,
            text_with_all_modifiers,
        },
    }
}

pub fn code_to_key(key: PhysicalKey, scancode: u16) -> Key {
    let code = match key {
        PhysicalKey::Code(code) => code,
        PhysicalKey::Unidentified(code) => return Key::Unidentified(code.into()),
    };

    Key::Named(match code {
        KeyCode::Enter => NamedKey::Enter,
        KeyCode::Tab => NamedKey::Tab,
        KeyCode::Space => NamedKey::Space,
        KeyCode::Backspace => NamedKey::Backspace,
        KeyCode::Escape => NamedKey::Escape,
        KeyCode::SuperRight => NamedKey::Super,
        KeyCode::SuperLeft => NamedKey::Super,
        KeyCode::ShiftLeft => NamedKey::Shift,
        KeyCode::AltLeft => NamedKey::Alt,
        KeyCode::ControlLeft => NamedKey::Control,
        KeyCode::ShiftRight => NamedKey::Shift,
        KeyCode::AltRight => NamedKey::Alt,
        KeyCode::ControlRight => NamedKey::Control,

        KeyCode::NumLock => NamedKey::NumLock,
        KeyCode::AudioVolumeUp => NamedKey::AudioVolumeUp,
        KeyCode::AudioVolumeDown => NamedKey::AudioVolumeDown,

        // Other numpad keys all generate text on macOS (if I understand correctly)
        KeyCode::NumpadEnter => NamedKey::Enter,

        KeyCode::F1 => NamedKey::F1,
        KeyCode::F2 => NamedKey::F2,
        KeyCode::F3 => NamedKey::F3,
        KeyCode::F4 => NamedKey::F4,
        KeyCode::F5 => NamedKey::F5,
        KeyCode::F6 => NamedKey::F6,
        KeyCode::F7 => NamedKey::F7,
        KeyCode::F8 => NamedKey::F8,
        KeyCode::F9 => NamedKey::F9,
        KeyCode::F10 => NamedKey::F10,
        KeyCode::F11 => NamedKey::F11,
        KeyCode::F12 => NamedKey::F12,
        KeyCode::F13 => NamedKey::F13,
        KeyCode::F14 => NamedKey::F14,
        KeyCode::F15 => NamedKey::F15,
        KeyCode::F16 => NamedKey::F16,
        KeyCode::F17 => NamedKey::F17,
        KeyCode::F18 => NamedKey::F18,
        KeyCode::F19 => NamedKey::F19,
        KeyCode::F20 => NamedKey::F20,

        KeyCode::Insert => NamedKey::Insert,
        KeyCode::Home => NamedKey::Home,
        KeyCode::PageUp => NamedKey::PageUp,
        KeyCode::Delete => NamedKey::Delete,
        KeyCode::End => NamedKey::End,
        KeyCode::PageDown => NamedKey::PageDown,
        KeyCode::ArrowLeft => NamedKey::ArrowLeft,
        KeyCode::ArrowRight => NamedKey::ArrowRight,
        KeyCode::ArrowDown => NamedKey::ArrowDown,
        KeyCode::ArrowUp => NamedKey::ArrowUp,
        _ => return Key::Unidentified(NativeKey::MacOS(scancode)),
    })
}

pub fn code_to_location(key: PhysicalKey) -> KeyLocation {
    let code = match key {
        PhysicalKey::Code(code) => code,
        PhysicalKey::Unidentified(_) => return KeyLocation::Standard,
    };

    match code {
        KeyCode::SuperRight => KeyLocation::Right,
        KeyCode::SuperLeft => KeyLocation::Left,
        KeyCode::ShiftLeft => KeyLocation::Left,
        KeyCode::AltLeft => KeyLocation::Left,
        KeyCode::ControlLeft => KeyLocation::Left,
        KeyCode::ShiftRight => KeyLocation::Right,
        KeyCode::AltRight => KeyLocation::Right,
        KeyCode::ControlRight => KeyLocation::Right,

        KeyCode::NumLock => KeyLocation::Numpad,
        KeyCode::NumpadDecimal => KeyLocation::Numpad,
        KeyCode::NumpadMultiply => KeyLocation::Numpad,
        KeyCode::NumpadAdd => KeyLocation::Numpad,
        KeyCode::NumpadDivide => KeyLocation::Numpad,
        KeyCode::NumpadEnter => KeyLocation::Numpad,
        KeyCode::NumpadSubtract => KeyLocation::Numpad,
        KeyCode::NumpadEqual => KeyLocation::Numpad,
        KeyCode::Numpad0 => KeyLocation::Numpad,
        KeyCode::Numpad1 => KeyLocation::Numpad,
        KeyCode::Numpad2 => KeyLocation::Numpad,
        KeyCode::Numpad3 => KeyLocation::Numpad,
        KeyCode::Numpad4 => KeyLocation::Numpad,
        KeyCode::Numpad5 => KeyLocation::Numpad,
        KeyCode::Numpad6 => KeyLocation::Numpad,
        KeyCode::Numpad7 => KeyLocation::Numpad,
        KeyCode::Numpad8 => KeyLocation::Numpad,
        KeyCode::Numpad9 => KeyLocation::Numpad,

        _ => KeyLocation::Standard,
    }
}

// While F1-F20 have scancodes we can match on, we have to check against UTF-16
// constants for the rest.
// https://developer.apple.com/documentation/appkit/1535851-function-key_unicodes?preferredLanguage=occ
pub fn extra_function_key_to_code(scancode: u16, string: &str) -> PhysicalKey {
    if let Some(ch) = string.encode_utf16().next() {
        match ch {
            0xf718 => PhysicalKey::Code(KeyCode::F21),
            0xf719 => PhysicalKey::Code(KeyCode::F22),
            0xf71a => PhysicalKey::Code(KeyCode::F23),
            0xf71b => PhysicalKey::Code(KeyCode::F24),
            _ => PhysicalKey::Unidentified(NativeKeyCode::MacOS(scancode)),
        }
    } else {
        PhysicalKey::Unidentified(NativeKeyCode::MacOS(scancode))
    }
}

pub(super) fn event_mods(event: &NSEvent) -> Modifiers {
    let flags = event.modifierFlags();
    let mut state = ModifiersState::empty();
    let mut pressed_mods = ModifiersKeys::empty();

    state.set(
        ModifiersState::SHIFT,
        flags.contains(NSEventModifierFlags::NSShiftKeyMask),
    );

    pressed_mods.set(ModifiersKeys::LSHIFT, event.lshift_pressed());
    pressed_mods.set(ModifiersKeys::RSHIFT, event.rshift_pressed());

    state.set(
        ModifiersState::CONTROL,
        flags.contains(NSEventModifierFlags::NSControlKeyMask),
    );

    pressed_mods.set(ModifiersKeys::LCONTROL, event.lctrl_pressed());
    pressed_mods.set(ModifiersKeys::RCONTROL, event.rctrl_pressed());

    state.set(
        ModifiersState::ALT,
        flags.contains(NSEventModifierFlags::NSAlternateKeyMask),
    );

    pressed_mods.set(ModifiersKeys::LALT, event.lalt_pressed());
    pressed_mods.set(ModifiersKeys::RALT, event.ralt_pressed());

    state.set(
        ModifiersState::SUPER,
        flags.contains(NSEventModifierFlags::NSCommandKeyMask),
    );

    pressed_mods.set(ModifiersKeys::LSUPER, event.lcmd_pressed());
    pressed_mods.set(ModifiersKeys::RSUPER, event.rcmd_pressed());

    Modifiers {
        state,
        pressed_mods,
    }
}

impl PhysicalKeyExtScancode for PhysicalKey {
    fn to_scancode(self) -> Option<u32> {
        let code = match self {
            PhysicalKey::Code(code) => code,
            PhysicalKey::Unidentified(_) => return None,
        };

        match code {
            KeyCode::KeyA => Some(0x00),
            KeyCode::KeyS => Some(0x01),
            KeyCode::KeyD => Some(0x02),
            KeyCode::KeyF => Some(0x03),
            KeyCode::KeyH => Some(0x04),
            KeyCode::KeyG => Some(0x05),
            KeyCode::KeyZ => Some(0x06),
            KeyCode::KeyX => Some(0x07),
            KeyCode::KeyC => Some(0x08),
            KeyCode::KeyV => Some(0x09),
            KeyCode::KeyB => Some(0x0b),
            KeyCode::KeyQ => Some(0x0c),
            KeyCode::KeyW => Some(0x0d),
            KeyCode::KeyE => Some(0x0e),
            KeyCode::KeyR => Some(0x0f),
            KeyCode::KeyY => Some(0x10),
            KeyCode::KeyT => Some(0x11),
            KeyCode::Digit1 => Some(0x12),
            KeyCode::Digit2 => Some(0x13),
            KeyCode::Digit3 => Some(0x14),
            KeyCode::Digit4 => Some(0x15),
            KeyCode::Digit6 => Some(0x16),
            KeyCode::Digit5 => Some(0x17),
            KeyCode::Equal => Some(0x18),
            KeyCode::Digit9 => Some(0x19),
            KeyCode::Digit7 => Some(0x1a),
            KeyCode::Minus => Some(0x1b),
            KeyCode::Digit8 => Some(0x1c),
            KeyCode::Digit0 => Some(0x1d),
            KeyCode::BracketRight => Some(0x1e),
            KeyCode::KeyO => Some(0x1f),
            KeyCode::KeyU => Some(0x20),
            KeyCode::BracketLeft => Some(0x21),
            KeyCode::KeyI => Some(0x22),
            KeyCode::KeyP => Some(0x23),
            KeyCode::Enter => Some(0x24),
            KeyCode::KeyL => Some(0x25),
            KeyCode::KeyJ => Some(0x26),
            KeyCode::Quote => Some(0x27),
            KeyCode::KeyK => Some(0x28),
            KeyCode::Semicolon => Some(0x29),
            KeyCode::Backslash => Some(0x2a),
            KeyCode::Comma => Some(0x2b),
            KeyCode::Slash => Some(0x2c),
            KeyCode::KeyN => Some(0x2d),
            KeyCode::KeyM => Some(0x2e),
            KeyCode::Period => Some(0x2f),
            KeyCode::Tab => Some(0x30),
            KeyCode::Space => Some(0x31),
            KeyCode::Backquote => Some(0x32),
            KeyCode::Backspace => Some(0x33),
            KeyCode::Escape => Some(0x35),
            KeyCode::SuperRight => Some(0x36),
            KeyCode::SuperLeft => Some(0x37),
            KeyCode::ShiftLeft => Some(0x38),
            KeyCode::AltLeft => Some(0x3a),
            KeyCode::ControlLeft => Some(0x3b),
            KeyCode::ShiftRight => Some(0x3c),
            KeyCode::AltRight => Some(0x3d),
            KeyCode::ControlRight => Some(0x3e),
            KeyCode::F17 => Some(0x40),
            KeyCode::NumpadDecimal => Some(0x41),
            KeyCode::NumpadMultiply => Some(0x43),
            KeyCode::NumpadAdd => Some(0x45),
            KeyCode::NumLock => Some(0x47),
            KeyCode::AudioVolumeUp => Some(0x49),
            KeyCode::AudioVolumeDown => Some(0x4a),
            KeyCode::NumpadDivide => Some(0x4b),
            KeyCode::NumpadEnter => Some(0x4c),
            KeyCode::NumpadSubtract => Some(0x4e),
            KeyCode::F18 => Some(0x4f),
            KeyCode::F19 => Some(0x50),
            KeyCode::NumpadEqual => Some(0x51),
            KeyCode::Numpad0 => Some(0x52),
            KeyCode::Numpad1 => Some(0x53),
            KeyCode::Numpad2 => Some(0x54),
            KeyCode::Numpad3 => Some(0x55),
            KeyCode::Numpad4 => Some(0x56),
            KeyCode::Numpad5 => Some(0x57),
            KeyCode::Numpad6 => Some(0x58),
            KeyCode::Numpad7 => Some(0x59),
            KeyCode::F20 => Some(0x5a),
            KeyCode::Numpad8 => Some(0x5b),
            KeyCode::Numpad9 => Some(0x5c),
            KeyCode::IntlYen => Some(0x5d),
            KeyCode::F5 => Some(0x60),
            KeyCode::F6 => Some(0x61),
            KeyCode::F7 => Some(0x62),
            KeyCode::F3 => Some(0x63),
            KeyCode::F8 => Some(0x64),
            KeyCode::F9 => Some(0x65),
            KeyCode::F11 => Some(0x67),
            KeyCode::F13 => Some(0x69),
            KeyCode::F16 => Some(0x6a),
            KeyCode::F14 => Some(0x6b),
            KeyCode::F10 => Some(0x6d),
            KeyCode::F12 => Some(0x6f),
            KeyCode::F15 => Some(0x71),
            KeyCode::Insert => Some(0x72),
            KeyCode::Home => Some(0x73),
            KeyCode::PageUp => Some(0x74),
            KeyCode::Delete => Some(0x75),
            KeyCode::F4 => Some(0x76),
            KeyCode::End => Some(0x77),
            KeyCode::F2 => Some(0x78),
            KeyCode::PageDown => Some(0x79),
            KeyCode::F1 => Some(0x7a),
            KeyCode::ArrowLeft => Some(0x7b),
            KeyCode::ArrowRight => Some(0x7c),
            KeyCode::ArrowDown => Some(0x7d),
            KeyCode::ArrowUp => Some(0x7e),
            _ => None,
        }
    }

    fn from_scancode(scancode: u32) -> PhysicalKey {
        PhysicalKey::Code(match scancode {
            0x00 => KeyCode::KeyA,
            0x01 => KeyCode::KeyS,
            0x02 => KeyCode::KeyD,
            0x03 => KeyCode::KeyF,
            0x04 => KeyCode::KeyH,
            0x05 => KeyCode::KeyG,
            0x06 => KeyCode::KeyZ,
            0x07 => KeyCode::KeyX,
            0x08 => KeyCode::KeyC,
            0x09 => KeyCode::KeyV,
            //0x0a => World 1,
            0x0b => KeyCode::KeyB,
            0x0c => KeyCode::KeyQ,
            0x0d => KeyCode::KeyW,
            0x0e => KeyCode::KeyE,
            0x0f => KeyCode::KeyR,
            0x10 => KeyCode::KeyY,
            0x11 => KeyCode::KeyT,
            0x12 => KeyCode::Digit1,
            0x13 => KeyCode::Digit2,
            0x14 => KeyCode::Digit3,
            0x15 => KeyCode::Digit4,
            0x16 => KeyCode::Digit6,
            0x17 => KeyCode::Digit5,
            0x18 => KeyCode::Equal,
            0x19 => KeyCode::Digit9,
            0x1a => KeyCode::Digit7,
            0x1b => KeyCode::Minus,
            0x1c => KeyCode::Digit8,
            0x1d => KeyCode::Digit0,
            0x1e => KeyCode::BracketRight,
            0x1f => KeyCode::KeyO,
            0x20 => KeyCode::KeyU,
            0x21 => KeyCode::BracketLeft,
            0x22 => KeyCode::KeyI,
            0x23 => KeyCode::KeyP,
            0x24 => KeyCode::Enter,
            0x25 => KeyCode::KeyL,
            0x26 => KeyCode::KeyJ,
            0x27 => KeyCode::Quote,
            0x28 => KeyCode::KeyK,
            0x29 => KeyCode::Semicolon,
            0x2a => KeyCode::Backslash,
            0x2b => KeyCode::Comma,
            0x2c => KeyCode::Slash,
            0x2d => KeyCode::KeyN,
            0x2e => KeyCode::KeyM,
            0x2f => KeyCode::Period,
            0x30 => KeyCode::Tab,
            0x31 => KeyCode::Space,
            0x32 => KeyCode::Backquote,
            0x33 => KeyCode::Backspace,
            //0x34 => unknown,
            0x35 => KeyCode::Escape,
            0x36 => KeyCode::SuperRight,
            0x37 => KeyCode::SuperLeft,
            0x38 => KeyCode::ShiftLeft,
            0x39 => KeyCode::CapsLock,
            0x3a => KeyCode::AltLeft,
            0x3b => KeyCode::ControlLeft,
            0x3c => KeyCode::ShiftRight,
            0x3d => KeyCode::AltRight,
            0x3e => KeyCode::ControlRight,
            0x3f => KeyCode::Fn,
            0x40 => KeyCode::F17,
            0x41 => KeyCode::NumpadDecimal,
            //0x42 -> unknown,
            0x43 => KeyCode::NumpadMultiply,
            //0x44 => unknown,
            0x45 => KeyCode::NumpadAdd,
            //0x46 => unknown,
            0x47 => KeyCode::NumLock,
            //0x48 => KeyCode::NumpadClear,

            // TODO: (Artur) for me, kVK_VolumeUp is 0x48
            // macOS 10.11
            // /System/Library/Frameworks/Carbon.framework/Versions/A/Frameworks/HIToolbox.framework/Versions/A/Headers/Events.h
            0x49 => KeyCode::AudioVolumeUp,
            0x4a => KeyCode::AudioVolumeDown,
            0x4b => KeyCode::NumpadDivide,
            0x4c => KeyCode::NumpadEnter,
            //0x4d => unknown,
            0x4e => KeyCode::NumpadSubtract,
            0x4f => KeyCode::F18,
            0x50 => KeyCode::F19,
            0x51 => KeyCode::NumpadEqual,
            0x52 => KeyCode::Numpad0,
            0x53 => KeyCode::Numpad1,
            0x54 => KeyCode::Numpad2,
            0x55 => KeyCode::Numpad3,
            0x56 => KeyCode::Numpad4,
            0x57 => KeyCode::Numpad5,
            0x58 => KeyCode::Numpad6,
            0x59 => KeyCode::Numpad7,
            0x5a => KeyCode::F20,
            0x5b => KeyCode::Numpad8,
            0x5c => KeyCode::Numpad9,
            0x5d => KeyCode::IntlYen,
            //0x5e => JIS Ro,
            //0x5f => unknown,
            0x60 => KeyCode::F5,
            0x61 => KeyCode::F6,
            0x62 => KeyCode::F7,
            0x63 => KeyCode::F3,
            0x64 => KeyCode::F8,
            0x65 => KeyCode::F9,
            //0x66 => JIS Eisuu (macOS),
            0x67 => KeyCode::F11,
            //0x68 => JIS Kanna (macOS),
            0x69 => KeyCode::F13,
            0x6a => KeyCode::F16,
            0x6b => KeyCode::F14,
            //0x6c => unknown,
            0x6d => KeyCode::F10,
            //0x6e => unknown,
            0x6f => KeyCode::F12,
            //0x70 => unknown,
            0x71 => KeyCode::F15,
            0x72 => KeyCode::Insert,
            0x73 => KeyCode::Home,
            0x74 => KeyCode::PageUp,
            0x75 => KeyCode::Delete,
            0x76 => KeyCode::F4,
            0x77 => KeyCode::End,
            0x78 => KeyCode::F2,
            0x79 => KeyCode::PageDown,
            0x7a => KeyCode::F1,
            0x7b => KeyCode::ArrowLeft,
            0x7c => KeyCode::ArrowRight,
            0x7d => KeyCode::ArrowDown,
            0x7e => KeyCode::ArrowUp,
            //0x7f =>  unknown,

            // 0xA is the caret (^) an macOS's German QERTZ layout. This key is at the same location as
            // backquote (`) on Windows' US layout.
            0xa => KeyCode::Backquote,
            _ => return PhysicalKey::Unidentified(NativeKeyCode::MacOS(scancode as u16)),
        })
    }
}
