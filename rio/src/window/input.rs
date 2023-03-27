use crate::window::ansi;
use std::io::Write;
use teletypewriter::Process;
use winit::event::ModifiersState;
use winit::event::VirtualKeyCode;

fn winit_key_to_char(key_code: VirtualKeyCode, is_shift_down: bool) -> Option<u8> {
    Some(match (key_code, is_shift_down) {
        (VirtualKeyCode::Grave, false) => b'`',
        (VirtualKeyCode::Grave, true) => b'~',
        (VirtualKeyCode::Apostrophe, false) => b'\'',
        (VirtualKeyCode::Apostrophe, true) => b'"',
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
        // println!(
        //     "set_modifiers {:?} {:?} {:?} {:?} {:?}",
        //     modifiers,
        //     modifiers.shift(),
        //     modifiers.logo(),
        //     modifiers.alt(),
        //     modifiers.ctrl()
        // );
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
