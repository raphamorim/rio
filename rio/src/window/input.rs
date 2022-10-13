use std::io::Write;
use tty::Process;
use winit::event::{ModifiersState, VirtualKeyCode};

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
        // println!("set_modifiers {:?}", modifiers);
        self.modifiers = modifiers;
    }

    pub fn keydown(&mut self, keycode: VirtualKeyCode, stream: &mut Process) {
        let code: &[u8; 1] = match keycode {
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

            // Control
            VirtualKeyCode::Space => b" ",
            VirtualKeyCode::Return => b"\n",

            // TODO: Arrows
            VirtualKeyCode::Back => {
                // TODO: Delete last byte
                b"-"
            }
            _ => {
                println!("code not implemented {:?}", keycode);
                b"-"
            }
        };

        // println!("keydown {:?}", self.modifiers);

        stream.write_all(code).unwrap();
    }
}
