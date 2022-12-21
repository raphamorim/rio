use crate::window::keys;
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
    #[allow(dead_code)]
    key_map: HashMap<u32, u8>,
}

pub const VK_ANSI_A: u32 = 0x00;
pub const VK_ANSI_S: u32 = 0x01;
pub const VK_ANSI_D: u32 = 0x02;
pub const VK_ANSI_F: u32 = 0x03;
pub const VK_ANSI_H: u32 = 0x04;
pub const VK_ANSI_G: u32 = 0x05;
pub const VK_ANSI_Z: u32 = 0x06;
pub const VK_ANSI_X: u32 = 0x07;
pub const VK_ANSI_C: u32 = 0x08;
pub const VK_ANSI_V: u32 = 0x09;
pub const VK_ANSI_B: u32 = 0x0B;
pub const VK_ANSI_Q: u32 = 0x0C;
pub const VK_ANSI_W: u32 = 0x0D;
pub const VK_ANSI_E: u32 = 0x0E;
pub const VK_ANSI_R: u32 = 0x0F;
pub const VK_ANSI_Y: u32 = 0x10;
pub const VK_ANSI_T: u32 = 0x11;
pub const VK_ANSI_1: u32 = 0x12;
pub const VK_ANSI_2: u32 = 0x13;
pub const VK_ANSI_3: u32 = 0x14;
pub const VK_ANSI_4: u32 = 0x15;
pub const VK_ANSI_6: u32 = 0x16;
pub const VK_ANSI_5: u32 = 0x17;
pub const VK_ANSI_EQUAL: u32 = 0x18;
pub const VK_ANSI_9: u32 = 0x19;
pub const VK_ANSI_7: u32 = 0x1A;
pub const VK_ANSI_MINUS: u32 = 0x1B;
pub const VK_ANSI_8: u32 = 0x1C;
pub const VK_ANSI_0: u32 = 0x1D;
pub const VK_ANSI_RIGHTBRACKET: u32 = 0x1E;
pub const VK_ANSI_O: u32 = 0x1F;
pub const VK_ANSI_U: u32 = 0x20;
pub const VK_ANSI_LEFTBRACKET: u32 = 0x21;
pub const VK_ANSI_I: u32 = 0x22;
pub const VK_ANSI_P: u32 = 0x23;
pub const VK_ANSI_L: u32 = 0x25;
pub const VK_ANSI_J: u32 = 0x26;
pub const VK_ANSI_QUOTE: u32 = 0x27;
pub const VK_ANSI_K: u32 = 0x28;
pub const VK_ANSI_SEMICOLON: u32 = 0x29;
pub const VK_ANSI_BACKSLASH: u32 = 0x2A;
pub const VK_ANSI_COMMA: u32 = 0x2B;
pub const VK_ANSI_SLASH: u32 = 0x2C;
pub const VK_ANSI_N: u32 = 0x2D;
pub const VK_ANSI_M: u32 = 0x2E;
pub const VK_ANSI_PERIOD: u32 = 0x2F;
pub const VK_ANSI_GRAVE: u32 = 0x32;
pub const VK_ANSI_KEYPADDECIMAL: u32 = 0x41;
pub const VK_ANSI_KEYPADMULTIPLY: u32 = 0x43;
pub const VK_ANSI_KEYPADPLUS: u32 = 0x45;
pub const VK_ANSI_KEYPADCLEAR: u32 = 0x47;
pub const VK_ANSI_KEYPADDIVIDE: u32 = 0x4B;
pub const VK_ANSI_KEYPADENTER: u32 = 0x4C;
pub const VK_ANSI_KEYPADMINUS: u32 = 0x4E;
pub const VK_ANSI_KEYPADEQUALS: u32 = 0x51;
pub const VK_ANSI_KEYPAD0: u32 = 0x52;
pub const VK_ANSI_KEYPAD1: u32 = 0x53;
pub const VK_ANSI_KEYPAD2: u32 = 0x54;
pub const VK_ANSI_KEYPAD3: u32 = 0x55;
pub const VK_ANSI_KEYPAD4: u32 = 0x56;
pub const VK_ANSI_KEYPAD5: u32 = 0x57;
pub const VK_ANSI_KEYPAD6: u32 = 0x58;
pub const VK_ANSI_KEYPAD7: u32 = 0x59;
pub const VK_ANSI_KEYPAD8: u32 = 0x5B;
pub const VK_ANSI_KEYPAD9: u32 = 0x5C;

pub const VK_RETURN: u32 = 0x24;
pub const VK_TAB: u32 = 0x30;
pub const VK_SPACE: u32 = 0x31;
pub const VK_DELETE: u32 = 0x33;
pub const VK_ESCAPE: u32 = 0x35;
pub const VK_COMMAND: u32 = 0x37;
pub const VK_SHIFT: u32 = 0x38;
pub const VK_CAPSLOCK: u32 = 0x39;
pub const VK_OPTION: u32 = 0x3A;
pub const VK_CONTROL: u32 = 0x3B;
pub const VK_RIGHTCOMMAND: u32 = 0x36;
pub const VK_RIGHTSHIFT: u32 = 0x3C;
pub const VK_RIGHTOPTION: u32 = 0x3D;
pub const VK_RIGHTCONTROL: u32 = 0x3E;
pub const VK_FUNCTION: u32 = 0x3F;
pub const VK_F17: u32 = 0x40;
pub const VK_VOLUMEUP: u32 = 0x48;
pub const VK_VOLUMEDOWN: u32 = 0x49;
pub const VK_MUTE: u32 = 0x4A;
pub const VK_F18: u32 = 0x4F;
pub const VK_F19: u32 = 0x50;
pub const VK_F20: u32 = 0x5A;
pub const VK_F5: u32 = 0x60;
pub const VK_F6: u32 = 0x61;
pub const VK_F7: u32 = 0x62;
pub const VK_F3: u32 = 0x63;
pub const VK_F8: u32 = 0x64;
pub const VK_F9: u32 = 0x65;
pub const VK_F11: u32 = 0x67;
pub const VK_F13: u32 = 0x69;
pub const VK_F16: u32 = 0x6A;
pub const VK_F14: u32 = 0x6B;
pub const VK_F10: u32 = 0x6D;
pub const VK_F12: u32 = 0x6F;
pub const VK_F15: u32 = 0x71;
pub const VK_HELP: u32 = 0x72;
pub const VK_HOME: u32 = 0x73;
pub const VK_PAGEUP: u32 = 0x74;
pub const VK_FORWARDDELETE: u32 = 0x75;
pub const VK_F4: u32 = 0x76;
pub const VK_END: u32 = 0x77;
pub const VK_F2: u32 = 0x78;
pub const VK_PAGEDOWN: u32 = 0x79;
pub const VK_F1: u32 = 0x7A;
pub const VK_LEFTARROW: u32 = 0x7B;
pub const VK_RIGHTARROW: u32 = 0x7C;
pub const VK_DOWNARROW: u32 = 0x7D;
pub const VK_UPARROW: u32 = 0x7E;

// fn build_map() -> HashMap<VirtualKeyCode, u16> {
fn build_map() -> HashMap<u32, u8> {
    [
        (VK_ANSI_A, keys::A),
        (VK_ANSI_B, keys::B),
        (VK_ANSI_C, keys::C),
        (VK_ANSI_D, keys::D),
        (VK_ANSI_E, keys::E),
        (VK_ANSI_F, keys::F),
        (VK_ANSI_G, keys::G),
        (VK_ANSI_H, keys::H),
        (VK_ANSI_Q, keys::Q),
        (VK_ANSI_R, keys::R),
        (VK_ANSI_S, keys::S),
        (VK_ANSI_T, keys::T),
        (VK_ANSI_V, keys::V),
        (VK_ANSI_W, keys::W),
        (VK_ANSI_X, keys::X),
        (VK_ANSI_Y, keys::Y),
        (VK_ANSI_Z, keys::Z),
        (VK_ANSI_1, keys::K1),
        (VK_ANSI_2, keys::K2),
        (VK_ANSI_3, keys::K3),
        (VK_ANSI_4, keys::K4),
        (VK_ANSI_6, keys::K6),
        (VK_ANSI_5, keys::K5),
        (VK_ANSI_EQUAL, keys::EQUAL),
        (VK_ANSI_9, keys::K9),
        (VK_ANSI_7, keys::K7),
        (VK_ANSI_MINUS, keys::MINUS),
        (VK_ANSI_8, keys::K8),
        (VK_ANSI_0, keys::K0),
        // (VK_ANSI_RIGHTBRACKET, keys::RightBracket),
        (VK_ANSI_O, keys::O),
        (VK_ANSI_U, keys::U),
        // (VK_ANSI_LEFTBRACKET, keys::LeftBracket),
        (VK_ANSI_I, keys::I),
        (VK_ANSI_P, keys::P),
        (VK_ANSI_L, keys::L),
        (VK_ANSI_J, keys::J),
        // (VK_ANSI_QUOTE, keys::Quote),
        (VK_ANSI_K, keys::K),
        // (VK_ANSI_SEMICOLON, keys::Semicolon),
        // (VK_ANSI_BACKSLASH, keys::Backslash),
        // (VK_ANSI_COMMA, keys::Comma),
        // (VK_ANSI_SLASH, keys::Slash),
        (VK_ANSI_N, keys::N),
        (VK_ANSI_M, keys::M),
        (VK_ANSI_PERIOD, keys::PERIOD),
        // (VK_ANSI_GRAVE, keys::Grave),
        // (VK_ANSI_KEYPADDECIMAL, keys::KeypadDecimal),
        // (VK_ANSI_KEYPADMULTIPLY, keys::KeypadMultiply),
        // (VK_ANSI_KEYPADPLUS, keys::KeypadAdd),
        // (VK_ANSI_KEYPADCLEAR, keys::KeypadClear),
        // (VK_ANSI_KEYPADDIVIDE, keys::KeypadDivide),
        // (VK_ANSI_KEYPADENTER, keys::KeypadEnter),
        // (VK_ANSI_KEYPADMINUS, keys::KeypadSubtract),
        // (VK_ANSI_KEYPADEQUALS, keys::KeypadEquals),
        (VK_ANSI_KEYPAD0, keys::KEYPAD0),
        (VK_ANSI_KEYPAD1, keys::KEYPAD1),
        (VK_ANSI_KEYPAD2, keys::KEYPAD2),
        (VK_ANSI_KEYPAD3, keys::KEYPAD3),
        (VK_ANSI_KEYPAD4, keys::KEYPAD4),
        (VK_ANSI_KEYPAD5, keys::KEYPAD5),
        (VK_ANSI_KEYPAD6, keys::KEYPAD6),
        (VK_ANSI_KEYPAD7, keys::KEYPAD7),
        (VK_ANSI_KEYPAD8, keys::KEYPAD8),
        (VK_ANSI_KEYPAD9, keys::KEYPAD9),
        (VK_RETURN, keys::RETURN),
        (VK_TAB, keys::TAB),
        (VK_SPACE, keys::SPACE),
        (VK_DELETE, keys::BACKSPACE),
        // (VK_ESCAPE, keys::Escape),
        // (VK_COMMAND, keys::LeftWindows),
        // (VK_SHIFT, keys::LeftShift),
        // (VK_CAPSLOCK, keys::CapsLock),
        // (VK_OPTION, keys::LeftAlt),
        // (VK_CONTROL, keys::LeftControl),
        // (VK_RIGHTCOMMAND, keys::RightWindows),
        // (VK_RIGHTSHIFT, keys::RightShift),
        // (VK_RIGHTOPTION, keys::RightAlt),
        // (VK_RIGHTCONTROL, keys::RightControl),
        // (VK_FUNCTION, keys::Function),
        // (VK_F17, keys::F17),
        // (VK_VOLUMEUP, keys::VolumeUp),
        // (VK_VOLUMEDOWN, keys::VolumeDown),
        // (VK_MUTE, keys::VolumeMute),
        // (VK_F18, keys::F18),
        // (VK_F19, keys::F19),
        // (VK_F20, keys::F20),
        // (VK_F5, keys::F5),
        // (VK_F6, keys::F6),
        // (VK_F7, keys::F7),
        // (VK_F3, keys::F3),
        // (VK_F8, keys::F8),
        // (VK_F9, keys::F9),
        // (VK_F11, keys::F11),
        // (VK_F13, keys::F13),
        // (VK_F16, keys::F16),
        // (VK_F14, keys::F14),
        // (VK_F10, keys::F10),
        // (VK_F12, keys::F12),
        // (VK_F15, keys::F15),
        // (VK_HELP, keys::Help),
        // (VK_HOME, keys::Home),
        // (VK_PAGEUP, keys::PageUp),
        // (VK_FORWARDDELETE, keys::Delete),
        // (VK_F4, keys::F4),
        // (VK_END, keys::End),
        // (VK_F2, keys::F2),
        // (VK_PAGEDOWN, keys::PageDown),
        // (VK_F1, keys::F1),
        // (VK_LEFTARROW, keys::LeftArrow),
        // (VK_RIGHTARROW, keys::RightArrow),
        // (VK_DOWNARROW, keys::DownArrow),
        // (VK_UPARROW, keys::UpArrow),
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

    // pub fn virtual_key_code_to_ansi(&self, vkey: VirtualKeyCode) -> Result<u8, ()> {
    //     match self.key_map.get(&vkey) {
    //         Some(val) => Ok(*val),
    //         None => Err(()),
    //     }
    // }

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
