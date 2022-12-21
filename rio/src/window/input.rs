use crate::window::{keys, ansi};
use std::collections::HashMap;
use std::io::Write;
use tty::Process;
use winit::event::{ModifiersState};

// pub struct ModifiersState {
//     pub shift: bool,
//     pub ctrl: bool,
//     pub alt: bool,
//     pub logo: bool,
//     ^ This is the "windows" key on PC and "command" key on Mac.
// }

pub struct Input {
    modifiers: ModifiersState,
    key_map: HashMap<u32, u8>,
}

// fn build_map() -> HashMap<VirtualKeyCode, u16> {
fn build_map() -> HashMap<u32, u8> {
    [
        (keys::VK_ANSI_A, ansi::A),
        (keys::VK_ANSI_B, ansi::B),
        (keys::VK_ANSI_C, ansi::C),
        (keys::VK_ANSI_D, ansi::D),
        (keys::VK_ANSI_E, ansi::E),
        (keys::VK_ANSI_F, ansi::F),
        (keys::VK_ANSI_G, ansi::G),
        (keys::VK_ANSI_H, ansi::H),
        (keys::VK_ANSI_Q, ansi::Q),
        (keys::VK_ANSI_R, ansi::R),
        (keys::VK_ANSI_S, ansi::S),
        (keys::VK_ANSI_T, ansi::T),
        (keys::VK_ANSI_V, ansi::V),
        (keys::VK_ANSI_W, ansi::W),
        (keys::VK_ANSI_X, ansi::X),
        (keys::VK_ANSI_Y, ansi::Y),
        (keys::VK_ANSI_Z, ansi::Z),
        (keys::VK_ANSI_1, ansi::K1),
        (keys::VK_ANSI_2, ansi::K2),
        (keys::VK_ANSI_3, ansi::K3),
        (keys::VK_ANSI_4, ansi::K4),
        (keys::VK_ANSI_6, ansi::K6),
        (keys::VK_ANSI_5, ansi::K5),
        (keys::VK_ANSI_EQUAL, ansi::EQUAL),
        (keys::VK_ANSI_9, ansi::K9),
        (keys::VK_ANSI_7, ansi::K7),
        (keys::VK_ANSI_MINUS, ansi::MINUS),
        (keys::VK_ANSI_8, ansi::K8),
        (keys::VK_ANSI_0, ansi::K0),
        // (keys::VK_ANSI_RIGHTBRACKET, ansi::RightBracket),
        (keys::VK_ANSI_O, ansi::O),
        (keys::VK_ANSI_U, ansi::U),
        // (keys::VK_ANSI_LEFTBRACKET, ansi::LeftBracket),
        (keys::VK_ANSI_I, ansi::I),
        (keys::VK_ANSI_P, ansi::P),
        (keys::VK_ANSI_L, ansi::L),
        (keys::VK_ANSI_J, ansi::J),
        // (keys::VK_ANSI_QUOTE, ansi::Quote),
        (keys::VK_ANSI_K, ansi::K),
        // (keys::VK_ANSI_SEMICOLON, ansi::Semicolon),
        // (keys::VK_ANSI_BACKSLASH, ansi::Backslash),
        // (keys::VK_ANSI_COMMA, ansi::Comma),
        // (keys::VK_ANSI_SLASH, ansi::Slash),
        (keys::VK_ANSI_N, ansi::N),
        (keys::VK_ANSI_M, ansi::M),
        (keys::VK_ANSI_PERIOD, ansi::PERIOD),
        // (keys::VK_ANSI_GRAVE, ansi::Grave),
        // (keys::VK_ANSI_KEYPADDECIMAL, ansi::KeypadDecimal),
        // (keys::VK_ANSI_KEYPADMULTIPLY, ansi::KeypadMultiply),
        // (keys::VK_ANSI_KEYPADPLUS, ansi::KeypadAdd),
        // (keys::VK_ANSI_KEYPADCLEAR, ansi::KeypadClear),
        // (keys::VK_ANSI_KEYPADDIVIDE, ansi::KeypadDivide),
        // (keys::VK_ANSI_KEYPADENTER, ansi::KeypadEnter),
        // (keys::VK_ANSI_KEYPADMINUS, ansi::KeypadSubtract),
        // (keys::VK_ANSI_KEYPADEQUALS, ansi::KeypadEquals),
        (keys::VK_ANSI_KEYPAD0, ansi::KEYPAD0),
        (keys::VK_ANSI_KEYPAD1, ansi::KEYPAD1),
        (keys::VK_ANSI_KEYPAD2, ansi::KEYPAD2),
        (keys::VK_ANSI_KEYPAD3, ansi::KEYPAD3),
        (keys::VK_ANSI_KEYPAD4, ansi::KEYPAD4),
        (keys::VK_ANSI_KEYPAD5, ansi::KEYPAD5),
        (keys::VK_ANSI_KEYPAD6, ansi::KEYPAD6),
        (keys::VK_ANSI_KEYPAD7, ansi::KEYPAD7),
        (keys::VK_ANSI_KEYPAD8, ansi::KEYPAD8),
        (keys::VK_ANSI_KEYPAD9, ansi::KEYPAD9),
        (keys::VK_RETURN, ansi::RETURN),
        (keys::VK_TAB, ansi::TAB),
        (keys::VK_SPACE, ansi::SPACE),
        (keys::VK_DELETE, ansi::BACKSPACE),
        // (keys::VK_ESCAPE, ansi::Escape),
        // (keys::VK_COMMAND, ansi::LeftWindows),
        // (keys::VK_SHIFT, ansi::LeftShift),
        // (keys::VK_CAPSLOCK, ansi::CapsLock),
        // (keys::VK_OPTION, ansi::LeftAlt),
        // (keys::VK_CONTROL, ansi::LeftControl),
        // (keys::VK_RIGHTCOMMAND, ansi::RightWindows),
        // (keys::VK_RIGHTSHIFT, ansi::RightShift),
        // (keys::VK_RIGHTOPTION, ansi::RightAlt),
        // (keys::VK_RIGHTCONTROL, ansi::RightControl),
        // (keys::VK_FUNCTION, ansi::Function),
        // (keys::VK_F17, ansi::F17),
        // (keys::VK_VOLUMEUP, ansi::VolumeUp),
        // (keys::VK_VOLUMEDOWN, ansi::VolumeDown),
        // (keys::VK_MUTE, ansi::VolumeMute),
        // (keys::VK_F18, ansi::F18),
        // (keys::VK_F19, ansi::F19),
        // (keys::VK_F20, ansi::F20),
        // (keys::VK_F5, ansi::F5),
        // (keys::VK_F6, ansi::F6),
        // (keys::VK_F7, ansi::F7),
        // (keys::VK_F3, ansi::F3),
        // (keys::VK_F8, ansi::F8),
        // (keys::VK_F9, ansi::F9),
        // (keys::VK_F11, ansi::F11),
        // (keys::VK_F13, ansi::F13),
        // (keys::VK_F16, ansi::F16),
        // (keys::VK_F14, ansi::F14),
        // (keys::VK_F10, ansi::F10),
        // (keys::VK_F12, ansi::F12),
        // (keys::VK_F15, ansi::F15),
        // (keys::VK_HELP, ansi::Help),
        // (keys::VK_HOME, ansi::Home),
        // (keys::VK_PAGEUP, ansi::PageUp),
        // (keys::VK_FORWARDDELETE, ansi::Delete),
        // (keys::VK_F4, ansi::F4),
        // (keys::VK_END, ansi::End),
        // (keys::VK_F2, ansi::F2),
        // (keys::VK_PAGEDOWN, ansi::PageDown),
        // (keys::VK_F1, ansi::F1),
        // (keys::VK_LEFTARROW, ansi::LeftArrow),
        // (keys::VK_RIGHTARROW, ansi::RightArrow),
        // (keys::VK_DOWNARROW, ansi::DownArrow),
        // (keys::VK_UPARROW, ansi::UpArrow),
    ]
    .iter()
    .copied()
    .collect()
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
        // println!("set_modifiers {:?} {:?} {:?}", modifiers, modifiers.shift(), modifiers.logo());
        self.modifiers = modifiers;
    }

    pub fn keydown(&mut self, keycode: u32, stream: &mut Process) {
        let code = self.physical_key_code_to_ansi(keycode);
        println!("keydown {:?} {:?}", keycode, code);

        match code {
            Ok(val) => {
                stream.write_all(&[val]).unwrap();
            },
            Err(()) => {}
        }
    }
}
