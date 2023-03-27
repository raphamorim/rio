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
        (VirtualKeyCode::Space, _) => ansi::SPACE,
        (VirtualKeyCode::Key0, _) => ansi::K0,
        (VirtualKeyCode::Key1, _) => ansi::K1,
        (VirtualKeyCode::Key2, _) => ansi::K2,
        (VirtualKeyCode::Key3, _) => ansi::K3,
        (VirtualKeyCode::Key4, _) => ansi::K4,
        (VirtualKeyCode::Key5, _) => ansi::K5,
        (VirtualKeyCode::Key6, _) => ansi::K6,
        (VirtualKeyCode::Key7, _) => ansi::K7,
        (VirtualKeyCode::Key8, _) => ansi::K8,
        (VirtualKeyCode::Key9, _) => ansi::K9,
        (VirtualKeyCode::A, false) => ansi::A,
        (VirtualKeyCode::A, true) => b'A',
        (VirtualKeyCode::B, false) => ansi::B,
        (VirtualKeyCode::B, true) => b'B',
        (VirtualKeyCode::C, false) => ansi::C,
        (VirtualKeyCode::C, true) => b'C',
        (VirtualKeyCode::D, false) => ansi::D,
        (VirtualKeyCode::D, true) => b'D',
        (VirtualKeyCode::E, false) => ansi::E,
        (VirtualKeyCode::E, true) => b'E',
        (VirtualKeyCode::F, false) => ansi::F,
        (VirtualKeyCode::F, true) => b'F',
        (VirtualKeyCode::G, false) => ansi::G,
        (VirtualKeyCode::G, true) => b'G',
        (VirtualKeyCode::H, false) => ansi::H,
        (VirtualKeyCode::H, true) => b'H',
        (VirtualKeyCode::I, false) => ansi::I,
        (VirtualKeyCode::I, true) => b'I',
        (VirtualKeyCode::J, false) => ansi::J,
        (VirtualKeyCode::J, true) => b'J',
        (VirtualKeyCode::K, false) => ansi::K,
        (VirtualKeyCode::K, true) => b'K',
        (VirtualKeyCode::L, false) => ansi::L,
        (VirtualKeyCode::L, true) => b'L',
        (VirtualKeyCode::M, false) => ansi::M,
        (VirtualKeyCode::M, true) => b'M',
        (VirtualKeyCode::N, false) => ansi::N,
        (VirtualKeyCode::N, true) => b'N',
        (VirtualKeyCode::O, false) => ansi::O,
        (VirtualKeyCode::O, true) => b'O',
        (VirtualKeyCode::P, false) => ansi::P,
        (VirtualKeyCode::P, true) => b'P',
        (VirtualKeyCode::Q, false) => ansi::Q,
        (VirtualKeyCode::Q, true) => b'Q',
        (VirtualKeyCode::R, false) => ansi::R,
        (VirtualKeyCode::R, true) => b'R',
        (VirtualKeyCode::S, false) => ansi::S,
        (VirtualKeyCode::S, true) => b'S',
        (VirtualKeyCode::T, false) => ansi::T,
        (VirtualKeyCode::T, true) => b'T',
        (VirtualKeyCode::U, false) => ansi::U,
        (VirtualKeyCode::U, true) => b'U',
        (VirtualKeyCode::V, false) => ansi::V,
        (VirtualKeyCode::V, true) => b'V',
        (VirtualKeyCode::W, false) => ansi::W,
        (VirtualKeyCode::W, true) => b'W',
        (VirtualKeyCode::X, false) => ansi::X,
        (VirtualKeyCode::X, true) => b'X',
        (VirtualKeyCode::Y, false) => ansi::Y,
        (VirtualKeyCode::Y, true) => b'Y',
        (VirtualKeyCode::Z, false) => ansi::Z,
        (VirtualKeyCode::Z, true) => b'Z',

        (VirtualKeyCode::Semicolon, false) => b';',
        (VirtualKeyCode::Semicolon, true) => b':',
        (VirtualKeyCode::Equals, false) => b'=',
        (VirtualKeyCode::Equals, true) => b'+',
        (VirtualKeyCode::Comma, false) => b',',
        (VirtualKeyCode::Comma, true) => b'<',
        (VirtualKeyCode::Minus, false) => b'-',
        (VirtualKeyCode::Minus, true) => b'_',
        (VirtualKeyCode::Period, false) => b'.',
        (VirtualKeyCode::Period, true) => b'>',
        (VirtualKeyCode::Slash, false) => b'/',
        (VirtualKeyCode::Slash, true) => b'?',
        (VirtualKeyCode::Grave, false) => b'`',
        (VirtualKeyCode::Grave, true) => b'~',
        (VirtualKeyCode::LBracket, false) => b'[',
        (VirtualKeyCode::LBracket, true) => b'{',
        (VirtualKeyCode::Backslash, false) => b'\\',
        (VirtualKeyCode::Backslash, true) => b'|',
        (VirtualKeyCode::RBracket, false) => b']',
        (VirtualKeyCode::RBracket, true) => b'}',
        (VirtualKeyCode::Apostrophe, false) => b'\'',
        (VirtualKeyCode::Apostrophe, true) => b'"',
        (VirtualKeyCode::NumpadMultiply, _) => b'*',
        (VirtualKeyCode::NumpadAdd, _) => b'+',
        (VirtualKeyCode::NumpadSubtract, _) => b'-',
        (VirtualKeyCode::NumpadDecimal, _) => b'.',
        (VirtualKeyCode::NumpadDivide, _) => b'/',

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
