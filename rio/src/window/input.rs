use crate::window::{ansi, scancode};
use std::collections::HashMap;
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
        (VirtualKeyCode::Space, _) => b' ',
        (VirtualKeyCode::Key0, _) => b'0',
        (VirtualKeyCode::Key1, _) => b'1',
        (VirtualKeyCode::Key2, _) => b'2',
        (VirtualKeyCode::Key3, _) => b'3',
        (VirtualKeyCode::Key4, _) => b'4',
        (VirtualKeyCode::Key5, _) => b'5',
        (VirtualKeyCode::Key6, _) => b'6',
        (VirtualKeyCode::Key7, _) => b'7',
        (VirtualKeyCode::Key8, _) => b'8',
        (VirtualKeyCode::Key9, _) => b'9',
        (VirtualKeyCode::A, false) => b'a',
        (VirtualKeyCode::A, true) => b'A',
        (VirtualKeyCode::B, false) => b'b',
        (VirtualKeyCode::B, true) => b'B',
        (VirtualKeyCode::C, false) => b'c',
        (VirtualKeyCode::C, true) => b'C',
        (VirtualKeyCode::D, false) => b'd',
        (VirtualKeyCode::D, true) => b'D',
        (VirtualKeyCode::E, false) => b'e',
        (VirtualKeyCode::E, true) => b'E',
        (VirtualKeyCode::F, false) => b'f',
        (VirtualKeyCode::F, true) => b'F',
        (VirtualKeyCode::G, false) => b'g',
        (VirtualKeyCode::G, true) => b'G',
        (VirtualKeyCode::H, false) => b'h',
        (VirtualKeyCode::H, true) => b'H',
        (VirtualKeyCode::I, false) => b'i',
        (VirtualKeyCode::I, true) => b'I',
        (VirtualKeyCode::J, false) => b'j',
        (VirtualKeyCode::J, true) => b'J',
        (VirtualKeyCode::K, false) => b'k',
        (VirtualKeyCode::K, true) => b'K',
        (VirtualKeyCode::L, false) => b'l',
        (VirtualKeyCode::L, true) => b'L',
        (VirtualKeyCode::M, false) => b'm',
        (VirtualKeyCode::M, true) => b'M',
        (VirtualKeyCode::N, false) => b'n',
        (VirtualKeyCode::N, true) => b'N',
        (VirtualKeyCode::O, false) => b'o',
        (VirtualKeyCode::O, true) => b'O',
        (VirtualKeyCode::P, false) => b'p',
        (VirtualKeyCode::P, true) => b'P',
        (VirtualKeyCode::Q, false) => b'q',
        (VirtualKeyCode::Q, true) => b'Q',
        (VirtualKeyCode::R, false) => b'r',
        (VirtualKeyCode::R, true) => b'R',
        (VirtualKeyCode::S, false) => b's',
        (VirtualKeyCode::S, true) => b'S',
        (VirtualKeyCode::T, false) => b't',
        (VirtualKeyCode::T, true) => b'T',
        (VirtualKeyCode::U, false) => b'u',
        (VirtualKeyCode::U, true) => b'U',
        (VirtualKeyCode::V, false) => b'v',
        (VirtualKeyCode::V, true) => b'V',
        (VirtualKeyCode::W, false) => b'w',
        (VirtualKeyCode::W, true) => b'W',
        (VirtualKeyCode::X, false) => b'x',
        (VirtualKeyCode::X, true) => b'X',
        (VirtualKeyCode::Y, false) => b'y',
        (VirtualKeyCode::Y, true) => b'Y',
        (VirtualKeyCode::Z, false) => b'z',
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

        (VirtualKeyCode::Up, _) => ansi::BACKSPACE,

        _ => return None,
    })
}

fn build_map() -> HashMap<u32, u8> {
    [
        (scancode::A, ansi::A),
        (scancode::B, ansi::B),
        (scancode::C, ansi::C),
        (scancode::D, ansi::D),
        (scancode::E, ansi::E),
        (scancode::F, ansi::F),
        (scancode::G, ansi::G),
        (scancode::H, ansi::H),
        (scancode::Q, ansi::Q),
        (scancode::R, ansi::R),
        (scancode::S, ansi::S),
        (scancode::T, ansi::T),
        (scancode::V, ansi::V),
        (scancode::W, ansi::W),
        (scancode::X, ansi::X),
        (scancode::Y, ansi::Y),
        (scancode::Z, ansi::Z),
        (scancode::K1, ansi::K1),
        (scancode::K2, ansi::K2),
        (scancode::K3, ansi::K3),
        (scancode::K4, ansi::K4),
        (scancode::K5, ansi::K5),
        (scancode::K6, ansi::K6),
        (scancode::EQUAL, ansi::EQUAL),
        (scancode::K9, ansi::K9),
        (scancode::K7, ansi::K7),
        (scancode::MINUS, ansi::MINUS),
        (scancode::K8, ansi::K8),
        (scancode::K0, ansi::K0),
        (scancode::RIGHTBRACKET, ansi::RIGHT_BRACKET),
        (scancode::O, ansi::O),
        (scancode::U, ansi::U),
        (scancode::LEFTBRACKET, ansi::LEFT_BRACKET),
        (scancode::I, ansi::I),
        (scancode::P, ansi::P),
        (scancode::L, ansi::L),
        (scancode::J, ansi::J),
        (scancode::QUOTE, ansi::QUOTE),
        (scancode::K, ansi::K),
        (scancode::SEMICOLON, ansi::SEMICOLON),
        (scancode::BACKSLASH, ansi::BACKSLASH),
        (scancode::COMMA, ansi::COMMA),
        (scancode::SLASH, ansi::SLASH),
        (scancode::N, ansi::N),
        (scancode::M, ansi::M),
        (scancode::PERIOD, ansi::PERIOD),
        (scancode::GRAVE, ansi::GRAVE),
        (scancode::KEYPADDECIMAL, ansi::KeypadDecimal),
        (scancode::KEYPADMULTIPLY, ansi::KeypadMultiply),
        (scancode::KEYPADPLUS, ansi::KeypadPlus),
        (scancode::KEYPADCLEAR, ansi::KeypadClear),
        (scancode::KEYPADDIVIDE, ansi::KeypadDivide),
        (scancode::KEYPADENTER, ansi::KeypadEnter),
        (scancode::KEYPADMINUS, ansi::KeypadMinus),
        (scancode::KEYPADEQUALS, ansi::KeypadEquals),
        (scancode::KEYPAD0, ansi::KEYPAD0),
        (scancode::KEYPAD1, ansi::KEYPAD1),
        (scancode::KEYPAD2, ansi::KEYPAD2),
        (scancode::KEYPAD3, ansi::KEYPAD3),
        (scancode::KEYPAD4, ansi::KEYPAD4),
        (scancode::KEYPAD5, ansi::KEYPAD5),
        (scancode::KEYPAD6, ansi::KEYPAD6),
        (scancode::KEYPAD7, ansi::KEYPAD7),
        (scancode::KEYPAD8, ansi::KEYPAD8),
        (scancode::KEYPAD9, ansi::KEYPAD9),
        (scancode::VK_RETURN, ansi::RETURN),
        (scancode::VK_TAB, ansi::TAB),
        (scancode::VK_SPACE, ansi::SPACE),
        (scancode::VK_DELETE, ansi::BACKSPACE),
        // (scancode::VK_ESCAPE, ansi::ESCAPE),
        (scancode::VK_COMMAND, ansi::COMMAND),
        // (scancode::VK_SHIFT, ansi::SHIFT_IN),
        (scancode::VK_CAPSLOCK, ansi::CAPS_LOCK),
        (scancode::VK_OPTION, ansi::OPTION),
        (scancode::VK_CONTROL, ansi::CONTROL),
        (scancode::VK_RIGHTCOMMAND, ansi::RIGHT_COMMAND),
        (scancode::VK_RIGHTSHIFT, ansi::RIGHT_SHIFT),
        (scancode::VK_RIGHTOPTION, ansi::RIGHT_OPTION),
        (scancode::VK_RIGHTCONTROL, ansi::RIGHT_CONTROL),
        (scancode::VK_FUNCTION, ansi::FUNCTION),
        (scancode::VK_F17, ansi::F17),
        (scancode::VK_VOLUMEUP, ansi::VOLUME_UP),
        (scancode::VK_VOLUMEDOWN, ansi::VOLUME_DOWN),
        (scancode::VK_MUTE, ansi::MUTE),
        (scancode::VK_F18, ansi::F18),
        (scancode::VK_F19, ansi::F19),
        (scancode::VK_F20, ansi::F20),
        (scancode::VK_F5, ansi::F5),
        (scancode::VK_F6, ansi::F6),
        (scancode::VK_F7, ansi::F7),
        (scancode::VK_F3, ansi::F3),
        (scancode::VK_F8, ansi::F8),
        (scancode::VK_F9, ansi::F9),
        (scancode::VK_F11, ansi::F11),
        (scancode::VK_F13, ansi::F13),
        (scancode::VK_F16, ansi::F16),
        (scancode::VK_F14, ansi::F14),
        (scancode::VK_F10, ansi::F10),
        (scancode::VK_F12, ansi::F12),
        (scancode::VK_F15, ansi::F15),
        (scancode::VK_HELP, ansi::HELP),
        (scancode::VK_HOME, ansi::HOME),
        (scancode::VK_PAGEUP, ansi::PAGE_UP),
        (scancode::VK_FORWARDDELETE, ansi::DELETE),
        (scancode::VK_F4, ansi::F4),
        (scancode::VK_END, ansi::END),
        (scancode::VK_F2, ansi::F2),
        (scancode::VK_PAGEDOWN, ansi::PAGE_DOWN),
        (scancode::VK_F1, ansi::F1),
        (scancode::VK_LEFTARROW, ansi::LEFT_ARROW),
        (scancode::VK_RIGHTARROW, ansi::RIGHT_ARROW),
        (scancode::VK_DOWNARROW, ansi::DOWN_ARROW),
        (scancode::VK_UPARROW, ansi::UP_ARROW),
    ]
    .iter()
    .copied()
    .collect()
}

pub struct Input {
    modifiers: ModifiersState,
    key_map: HashMap<u32, u8>,
}

impl Input {
    pub fn new() -> Input {
        let key_map: HashMap<u32, u8> = build_map();
        Input {
            modifiers: ModifiersState::default(),
            key_map,
        }
    }

    pub fn physical_key_code_to_ansi(&self, vkey: u32) -> Result<u8, ()> {
        match self.key_map.get(&vkey) {
            Some(val) => Ok(*val),
            None => Err(()),
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
        scancode: u32,
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

        // let code = self.physical_key_code_to_ansi(scancode);
        // if let Ok(val) = code {
        //     stream.write_all(&[val]).unwrap();
        //     stream.flush().unwrap();
        // }
    }
}
