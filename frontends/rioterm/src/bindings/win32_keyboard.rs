// Win32 input mode support for Windows
// Implements the Win32 input mode protocol that sends Windows KEY_EVENT_RECORD data as escape sequences
// Format: ESC [ Vk ; Sc ; Uc ; Kd ; Cs ; Rc _

use rio_window::event::{ElementState, KeyEvent};
use rio_window::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};
use rio_window::platform::scancode::PhysicalKeyExtScancode;

/// Build Win32 input mode sequence for Windows.
/// Format: ESC [ Vk ; Sc ; Uc ; Kd ; Cs ; Rc _
///
/// Where:
/// - Vk: Virtual key code (wVirtualKeyCode)
/// - Sc: Virtual scan code (wVirtualScanCode)  
/// - Uc: Unicode character value (uChar.UnicodeChar)
/// - Kd: Key down flag (1 for down, 0 for up)
/// - Cs: Control key state (dwControlKeyState)
/// - Rc: Repeat count (wRepeatCount)
pub fn build_win32_sequence(key: &KeyEvent, mods: ModifiersState) -> Vec<u8> {
    // Get the scan code from the physical key
    let sc = key.physical_key.to_scancode().unwrap_or(0) as u16;

    // Get virtual key code from the logical key or physical key
    let vk = match &key.logical_key {
        Key::Named(named) => named_key_to_vk(named),
        Key::Character(s) if s.len() == 1 => {
            // For single characters, use the character's uppercase value
            s.chars().next().unwrap().to_ascii_uppercase() as u16
        }
        _ => {
            // Fall back to physical key mapping
            if let PhysicalKey::Code(code) = key.physical_key {
                keycode_to_vk(code)
            } else {
                0
            }
        }
    };

    // Get Unicode character value
    let uc = key
        .text_with_all_modifiers()
        .and_then(|s| s.chars().next())
        .map(|c| c as u16)
        .unwrap_or(0);

    // Key down flag (1 for pressed, 0 for released)
    let kd = if key.state == ElementState::Pressed {
        1
    } else {
        0
    };

    // Control key state - Windows format
    let mut cs = 0u16;
    if mods.shift_key() {
        cs |= 0x0010;
    } // SHIFT_PRESSED
    if mods.control_key() {
        cs |= 0x0008;
    } // LEFT_CTRL_PRESSED
    if mods.alt_key() {
        cs |= 0x0001;
    } // LEFT_ALT_PRESSED
    if mods.super_key() {
        cs |= 0x0008;
    } // Windows key maps to CTRL

    // Repeat count
    let rc = if key.repeat { 2 } else { 1 };

    format!("\x1b[{};{};{};{};{};{}_", vk, sc, uc, kd, cs, rc).into_bytes()
}

fn named_key_to_vk(key: &NamedKey) -> u16 {
    use NamedKey::*;
    match key {
        Backspace => 0x08,
        Tab => 0x09,
        Enter => 0x0D,
        Escape => 0x1B,
        Space => 0x20,
        Delete => 0x2E,
        ArrowLeft => 0x25,
        ArrowUp => 0x26,
        ArrowRight => 0x27,
        ArrowDown => 0x28,
        Insert => 0x2D,
        Home => 0x24,
        End => 0x23,
        PageUp => 0x21,
        PageDown => 0x22,
        F1 => 0x70,
        F2 => 0x71,
        F3 => 0x72,
        F4 => 0x73,
        F5 => 0x74,
        F6 => 0x75,
        F7 => 0x76,
        F8 => 0x77,
        F9 => 0x78,
        F10 => 0x79,
        F11 => 0x7A,
        F12 => 0x7B,
        _ => 0,
    }
}

fn keycode_to_vk(code: KeyCode) -> u16 {
    use KeyCode::*;
    match code {
        KeyA => 0x41,
        KeyB => 0x42,
        KeyC => 0x43,
        KeyD => 0x44,
        KeyE => 0x45,
        KeyF => 0x46,
        KeyG => 0x47,
        KeyH => 0x48,
        KeyI => 0x49,
        KeyJ => 0x4A,
        KeyK => 0x4B,
        KeyL => 0x4C,
        KeyM => 0x4D,
        KeyN => 0x4E,
        KeyO => 0x4F,
        KeyP => 0x50,
        KeyQ => 0x51,
        KeyR => 0x52,
        KeyS => 0x53,
        KeyT => 0x54,
        KeyU => 0x55,
        KeyV => 0x56,
        KeyW => 0x57,
        KeyX => 0x58,
        KeyY => 0x59,
        KeyZ => 0x5A,
        Digit0 => 0x30,
        Digit1 => 0x31,
        Digit2 => 0x32,
        Digit3 => 0x33,
        Digit4 => 0x34,
        Digit5 => 0x35,
        Digit6 => 0x36,
        Digit7 => 0x37,
        Digit8 => 0x38,
        Digit9 => 0x39,
        Space => 0x20,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rio_backend::crosswords::Mode;

    #[test]
    fn test_win32_input_mode_flag() {
        // Test that Win32 input mode flag is properly defined
        let mode = Mode::WIN32_INPUT;
        assert!(!mode.is_empty());

        // Test that it doesn't conflict with Kitty keyboard protocol
        let kitty_mode = Mode::KITTY_KEYBOARD_PROTOCOL;
        assert!(!mode.intersects(kitty_mode));
    }
}
