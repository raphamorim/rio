use crate::window::keys::*;
use std::io::Write;
use std::collections::HashMap;
use tty::Process;
use winit::event::{ModifiersState, VirtualKeyCode};

pub struct Input {
    modifiers: ModifiersState,
    key_map: HashMap<VirtualKeyCode, u16>
}


fn build_map() -> HashMap<VirtualKeyCode, u16> {
    [
        (VirtualKeyCode::A, kVK_ANSI_A),
        (VirtualKeyCode::S, kVK_ANSI_S),
        (VirtualKeyCode::D, kVK_ANSI_D),
        (VirtualKeyCode::F, kVK_ANSI_F),
        (VirtualKeyCode::H, kVK_ANSI_H),
        (VirtualKeyCode::G, kVK_ANSI_G),
        // (kVK_ANSI_Z, PhysKeyCode::Z),
        // (kVK_ANSI_X, PhysKeyCode::X),
        // (kVK_ANSI_C, PhysKeyCode::C),
        // (kVK_ANSI_V, PhysKeyCode::V),
        // (kVK_ANSI_B, PhysKeyCode::B),
        // (kVK_ANSI_Q, PhysKeyCode::Q),
        // (kVK_ANSI_W, PhysKeyCode::W),
        // (kVK_ANSI_E, PhysKeyCode::E),
        // (kVK_ANSI_R, PhysKeyCode::R),
        // (kVK_ANSI_Y, PhysKeyCode::Y),
        // (kVK_ANSI_T, PhysKeyCode::T),
        // (kVK_ANSI_1, PhysKeyCode::K1),
        // (kVK_ANSI_2, PhysKeyCode::K2),
        // (kVK_ANSI_3, PhysKeyCode::K3),
        // (kVK_ANSI_4, PhysKeyCode::K4),
        // (kVK_ANSI_6, PhysKeyCode::K6),
        // (kVK_ANSI_5, PhysKeyCode::K5),
        // (kVK_ANSI_Equal, PhysKeyCode::Equal),
        // (kVK_ANSI_9, PhysKeyCode::K9),
        // (kVK_ANSI_7, PhysKeyCode::K7),
        // (kVK_ANSI_Minus, PhysKeyCode::Minus),
        // (kVK_ANSI_8, PhysKeyCode::K8),
        // (kVK_ANSI_0, PhysKeyCode::K0),
        // (kVK_ANSI_RightBracket, PhysKeyCode::RightBracket),
        // (kVK_ANSI_O, PhysKeyCode::O),
        // (kVK_ANSI_U, PhysKeyCode::U),
        // (kVK_ANSI_LeftBracket, PhysKeyCode::LeftBracket),
        // (kVK_ANSI_I, PhysKeyCode::I),
        // (kVK_ANSI_P, PhysKeyCode::P),
        // (kVK_ANSI_L, PhysKeyCode::L),
        // (kVK_ANSI_J, PhysKeyCode::J),
        // (kVK_ANSI_Quote, PhysKeyCode::Quote),
        // (kVK_ANSI_K, PhysKeyCode::K),
        // (kVK_ANSI_Semicolon, PhysKeyCode::Semicolon),
        // (kVK_ANSI_Backslash, PhysKeyCode::Backslash),
        // (kVK_ANSI_Comma, PhysKeyCode::Comma),
        // (kVK_ANSI_Slash, PhysKeyCode::Slash),
        // (kVK_ANSI_N, PhysKeyCode::N),
        // (kVK_ANSI_M, PhysKeyCode::M),
        // (kVK_ANSI_Period, PhysKeyCode::Period),
        // (kVK_ANSI_Grave, PhysKeyCode::Grave),
        // (kVK_ANSI_KeypadDecimal, PhysKeyCode::KeypadDecimal),
        // (kVK_ANSI_KeypadMultiply, PhysKeyCode::KeypadMultiply),
        // (kVK_ANSI_KeypadPlus, PhysKeyCode::KeypadAdd),
        // (kVK_ANSI_KeypadClear, PhysKeyCode::KeypadClear),
        // (kVK_ANSI_KeypadDivide, PhysKeyCode::KeypadDivide),
        // (kVK_ANSI_KeypadEnter, PhysKeyCode::KeypadEnter),
        // (kVK_ANSI_KeypadMinus, PhysKeyCode::KeypadSubtract),
        // (kVK_ANSI_KeypadEquals, PhysKeyCode::KeypadEquals),
        // (kVK_ANSI_Keypad0, PhysKeyCode::Keypad0),
        // (kVK_ANSI_Keypad1, PhysKeyCode::Keypad1),
        // (kVK_ANSI_Keypad2, PhysKeyCode::Keypad2),
        // (kVK_ANSI_Keypad3, PhysKeyCode::Keypad3),
        // (kVK_ANSI_Keypad4, PhysKeyCode::Keypad4),
        // (kVK_ANSI_Keypad5, PhysKeyCode::Keypad5),
        // (kVK_ANSI_Keypad6, PhysKeyCode::Keypad6),
        // (kVK_ANSI_Keypad7, PhysKeyCode::Keypad7),
        // (kVK_ANSI_Keypad8, PhysKeyCode::Keypad8),
        // (kVK_ANSI_Keypad9, PhysKeyCode::Keypad9),
        // (kVK_Return, PhysKeyCode::Return),
        // (kVK_Tab, PhysKeyCode::Tab),
        // (kVK_Space, PhysKeyCode::Space),
        // (kVK_Delete, PhysKeyCode::Backspace),
        // (kVK_Escape, PhysKeyCode::Escape),
        // (kVK_Command, PhysKeyCode::LeftWindows),
        // (kVK_Shift, PhysKeyCode::LeftShift),
        // (kVK_CapsLock, PhysKeyCode::CapsLock),
        // (kVK_Option, PhysKeyCode::LeftAlt),
        // (kVK_Control, PhysKeyCode::LeftControl),
        // (kVK_RightCommand, PhysKeyCode::RightWindows),
        // (kVK_RightShift, PhysKeyCode::RightShift),
        // (kVK_RightOption, PhysKeyCode::RightAlt),
        // (kVK_RightControl, PhysKeyCode::RightControl),
        // (kVK_Function, PhysKeyCode::Function),
        // (kVK_F17, PhysKeyCode::F17),
        // (kVK_VolumeUp, PhysKeyCode::VolumeUp),
        // (kVK_VolumeDown, PhysKeyCode::VolumeDown),
        // (kVK_Mute, PhysKeyCode::VolumeMute),
        // (kVK_F18, PhysKeyCode::F18),
        // (kVK_F19, PhysKeyCode::F19),
        // (kVK_F20, PhysKeyCode::F20),
        // (kVK_F5, PhysKeyCode::F5),
        // (kVK_F6, PhysKeyCode::F6),
        // (kVK_F7, PhysKeyCode::F7),
        // (kVK_F3, PhysKeyCode::F3),
        // (kVK_F8, PhysKeyCode::F8),
        // (kVK_F9, PhysKeyCode::F9),
        // (kVK_F11, PhysKeyCode::F11),
        // (kVK_F13, PhysKeyCode::F13),
        // (kVK_F16, PhysKeyCode::F16),
        // (kVK_F14, PhysKeyCode::F14),
        // (kVK_F10, PhysKeyCode::F10),
        // (kVK_F12, PhysKeyCode::F12),
        // (kVK_F15, PhysKeyCode::F15),
        // (kVK_Help, PhysKeyCode::Help),
        // (kVK_Home, PhysKeyCode::Home),
        // (kVK_PageUp, PhysKeyCode::PageUp),
        // (kVK_ForwardDelete, PhysKeyCode::Delete),
        // (kVK_F4, PhysKeyCode::F4),
        // (kVK_End, PhysKeyCode::End),
        // (kVK_F2, PhysKeyCode::F2),
        // (kVK_PageDown, PhysKeyCode::PageDown),
        // (kVK_F1, PhysKeyCode::F1),
        // (kVK_LeftArrow, PhysKeyCode::LeftArrow),
        // (kVK_RightArrow, PhysKeyCode::RightArrow),
        // (kVK_DownArrow, PhysKeyCode::DownArrow),
        // (kVK_UpArrow, PhysKeyCode::UpArrow),
    ]
    .iter()
    .map(|&tuple| tuple)
    .collect()
}

impl Input {
    pub fn new() -> Input {
        let key_map: HashMap<VirtualKeyCode, u16> = build_map();
        Input {
            modifiers: ModifiersState::default(),
            key_map,
        }
    }

    pub fn virtual_key_code_to_ansi(&self, vkey: VirtualKeyCode) -> Option<u16> {
        self.key_map.get(&vkey).copied()
    }

    pub fn set_modifiers(&mut self, modifiers: ModifiersState) {
        // println!("set_modifiers {:?}", modifiers);
        self.modifiers = modifiers;
    }

    pub fn keydown(&mut self, keycode: VirtualKeyCode, stream: &mut Process) {
        let code: &[u8] = match keycode {
            // Numbers
            VirtualKeyCode::Key0 => b"0",
            VirtualKeyCode::Key1 => b"1",
            VirtualKeyCode::Key2 => b"2",
            VirtualKeyCode::Key3 => b"3",
            VirtualKeyCode::Key4 => b"4",
            VirtualKeyCode::Key5 => b"5",
            VirtualKeyCode::Key6 => b"6",
            VirtualKeyCode::Key7 => b"7",
            VirtualKeyCode::Key8 => b"8",
            VirtualKeyCode::Key9 => b"9",

            // Alphabet
            VirtualKeyCode::A => b"a",
            VirtualKeyCode::B => b"b",
            VirtualKeyCode::C => b"c",
            VirtualKeyCode::D => b"d",
            VirtualKeyCode::E => b"e",
            VirtualKeyCode::F => b"f",
            VirtualKeyCode::G => b"g",
            VirtualKeyCode::H => b"h",
            VirtualKeyCode::I => b"i",
            VirtualKeyCode::J => b"j",
            VirtualKeyCode::K => b"k",
            VirtualKeyCode::L => b"l",
            VirtualKeyCode::M => b"m",
            VirtualKeyCode::N => b"n",
            VirtualKeyCode::O => b"o",
            VirtualKeyCode::P => b"p",
            VirtualKeyCode::Q => b"q",
            VirtualKeyCode::R => b"r",
            VirtualKeyCode::S => b"s",
            VirtualKeyCode::T => b"t",
            VirtualKeyCode::U => b"u",
            VirtualKeyCode::V => b"v",
            VirtualKeyCode::W => b"w",
            VirtualKeyCode::X => b"x",
            VirtualKeyCode::Y => b"y",
            VirtualKeyCode::Z => b"z",

            // Special
            VirtualKeyCode::Backslash => b"\\",
            VirtualKeyCode::Slash => b"/",
            VirtualKeyCode::Period => b".",
            VirtualKeyCode::Comma => b",",
            VirtualKeyCode::Space => b" ",
            VirtualKeyCode::Minus => b"-",
            VirtualKeyCode::Equals => b"=",
            VirtualKeyCode::Grave => b"`",

            // Control
            VirtualKeyCode::Return => b"\n",
            VirtualKeyCode::LWin => b"",
            VirtualKeyCode::RWin => b"",

            // TODO: Arrows
            VirtualKeyCode::Back => {
                // TODO: Delete last byte
                b"-"
            }

            _ => {
                println!("code not implemented {:?}", keycode);
                b""
            }
        };

        // println!("keydown {:?}", self.modifiers);

        stream.write_all(code).unwrap();
    }
}
