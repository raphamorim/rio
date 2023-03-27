use crate::window::ansi;
use std::io::Write;
use teletypewriter::Process;
use winit::event::ModifiersState;
use winit::event::VirtualKeyCode;

// pub struct ModifiersState {
//     pub shift: bool,
//     pub ctrl: bool,
//     pub alt: bool,
//     pub logo: bool,
//     ^ This is the "windows" key on PC and "command" key on Mac.
// }

// https://github.com/ruffle-rs/ruffle/blob/6f7e491bc5dd11c2eb7d9867bc246a39d7f5967c/desktop/src/main.rs#L753
fn winit_key_to_char(key_code: VirtualKeyCode, is_shift_down: bool) -> Option<u8> {
    // We need to know the character that a keypress outputs for both key down and key up events,
    // but the winit keyboard API does not provide a way to do this (winit/#753).
    // CharacterReceived events are insufficent because they only fire on key down, not on key up.
    // This is a half-measure to map from keyboard keys back to a character, but does will not work fully
    // for international layouts.
    Some(match (key_code, is_shift_down) {
        // (VirtualKeyCode::Semicolon, false) => b';',
        // (VirtualKeyCode::Semicolon, true) => b':',
        // (VirtualKeyCode::Equals, false) => b'=',
        // (VirtualKeyCode::Equals, true) => b'+',
        // (VirtualKeyCode::Comma, false) => b',',
        // (VirtualKeyCode::Comma, true) => b'<',
        // (VirtualKeyCode::Minus, false) => b'-',
        // (VirtualKeyCode::Minus, true) => b'_',
        // (VirtualKeyCode::Period, false) => b'.',
        // (VirtualKeyCode::Period, true) => b'>',
        // (VirtualKeyCode::Slash, false) => b'/',
        // (VirtualKeyCode::Slash, true) => b'?',
        (VirtualKeyCode::Grave, false) => b'`',
        (VirtualKeyCode::Grave, true) => b'~',
        // (VirtualKeyCode::LBracket, false) => b'[',
        // (VirtualKeyCode::LBracket, true) => b'{',
        // (VirtualKeyCode::Backslash, false) => b'\\',
        // (VirtualKeyCode::Backslash, true) => b'|',
        // (VirtualKeyCode::RBracket, false) => b']',
        // (VirtualKeyCode::RBracket, true) => b'}',
        (VirtualKeyCode::Apostrophe, false) => b'\'',
        (VirtualKeyCode::Apostrophe, true) => b'"',
        // (VirtualKeyCode::NumpadMultiply, _) => b'*',
        // (VirtualKeyCode::NumpadAdd, _) => b'+',
        // (VirtualKeyCode::NumpadSubtract, _) => b'-',
        // (VirtualKeyCode::NumpadDecimal, _) => b'.',
        // (VirtualKeyCode::NumpadDivide, _) => b'/',
        (VirtualKeyCode::Numpad0, false) => ansi::KEYPAD0,
        (VirtualKeyCode::Numpad1, false) => ansi::KEYPAD1,
        (VirtualKeyCode::Numpad2, false) => ansi::KEYPAD2,
        (VirtualKeyCode::Numpad3, false) => ansi::KEYPAD3,
        (VirtualKeyCode::Numpad4, false) => ansi::KEYPAD4,
        (VirtualKeyCode::Numpad5, false) => ansi::KEYPAD5,
        (VirtualKeyCode::Numpad6, false) => ansi::KEYPAD6,
        (VirtualKeyCode::Numpad7, false) => ansi::KEYPAD7,
        (VirtualKeyCode::Numpad8, false) => ansi::KEYPAD8,
        (VirtualKeyCode::Numpad9, false) => ansi::KEYPAD9,
        (VirtualKeyCode::NumpadEnter, _) => ansi::RETURN,

        (VirtualKeyCode::Tab, _) => ansi::TAB,
        (VirtualKeyCode::Capital, _) => ansi::TAB,
        (VirtualKeyCode::Return, _) => ansi::RETURN,
        (VirtualKeyCode::Back, _) => ansi::BACKSPACE,

        // (VirtualKeyCode::Up, _) => 0x72,
        _ => return None,
    })
}

pub struct Input {
    modifiers: ModifiersState,
}

impl Input {
    pub fn new() -> Input {
        Input {
            modifiers: ModifiersState::default(),
        }
    }

    pub fn set_modifiers(&mut self, modifiers: ModifiersState) {
        println!(
            "set_modifiers {:?} {:?} {:?} {:?} {:?}",
            modifiers,
            modifiers.shift(),
            modifiers.logo(),
            modifiers.alt(),
            modifiers.ctrl()
        );
        self.modifiers = modifiers;
    }

    pub fn input_character(&mut self, character: char, stream: &mut Process) {
        stream.write_all(&[character as u8]).unwrap();
        stream.flush().unwrap();
    }

    pub fn keydown(
        &mut self,
        _scancode: u32,
        virtual_keycode: Option<VirtualKeyCode>,
        stream: &mut Process,
    ) {
        if let Some(keycode) = virtual_keycode {
            match winit_key_to_char(keycode, self.modifiers.shift()) {
                Some(key_char) => {
                    stream.write_all(&[key_char]).unwrap();
                    stream.flush().unwrap();
                }
                None => println!("key unimplemented!()"),
            }
        }
    }
}
