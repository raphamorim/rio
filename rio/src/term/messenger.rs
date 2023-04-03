use crate::term::ansi;
use std::borrow::Cow;
use teletypewriter::WinsizeBuilder;

use crate::event::Msg;

// use teletypewriter::Process;
use winit::event::ModifiersState;
use winit::event::VirtualKeyCode;

// As defined in: http://www.unicode.org/faq/private_use.html
pub fn is_private_use_character(c: char) -> bool {
    matches!(
        c,
        '\u{E000}'..='\u{F8FF}'
        | '\u{F0000}'..='\u{FFFFD}'
        | '\u{100000}'..='\u{10FFFD}'
    )
}

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
        (VirtualKeyCode::Return, _) => ansi::RETURN,
        _ => return None,
    })
}

pub struct Messenger {
    modifiers: ModifiersState,
    channel: mio_extras::channel::Sender<Msg>,
}

impl Messenger {
    pub fn new(channel: mio_extras::channel::Sender<Msg>) -> Messenger {
        Messenger {
            modifiers: ModifiersState::default(),
            channel,
        }
    }

    pub fn set_modifiers(&mut self, modifiers: ModifiersState) {
        self.modifiers = modifiers;
    }

    pub fn send_character(&mut self, character: char) {
        if !is_private_use_character(character) && character != '\r' && character != '\n'
        {
            self.send(character as u8);
        }
    }

    #[inline]
    fn send(&self, character: u8) {
        let val: Cow<'static, [u8]> = Cow::<'static, [u8]>::Owned(([character]).to_vec());

        let _ = self.channel.send(Msg::Input(val));
    }

    #[inline]
    pub fn send_resize(
        &self,
        width: u16,
        height: u16,
        cols: u16,
        rows: u16,
    ) -> Result<&str, String> {
        let new_size = WinsizeBuilder {
            rows,
            cols,
            width,
            height,
        };

        match self.channel.send(Msg::Resize(new_size)) {
            Ok(..) => Ok("Resized"),
            Err(..) => Err("Error sending message".to_string()),
        }
    }

    pub fn send_keycode(
        &mut self,
        virtual_keycode: VirtualKeyCode,
        // _scancode: u32,
    ) -> Result<(), String> {
        match winit_key_to_char(virtual_keycode, self.modifiers.shift()) {
            Some(key_char) => {
                self.send(key_char);
                Ok(())
            }
            None => Err("key unimplemented!()".to_string()),
        }
    }
}
