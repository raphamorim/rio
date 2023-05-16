use crate::event::Msg;
use std::borrow::Cow;
use teletypewriter::WinsizeBuilder;
use winit::event::ModifiersState;

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

    pub fn get_modifiers(&mut self) -> ModifiersState {
        self.modifiers
    }

    pub fn send_character(&mut self, character: char) {
        let utf8_len = character.len_utf8();
        let mut bytes = vec![0; utf8_len];
        character.encode_utf8(&mut bytes[..]);

        self.send_write(bytes);
    }

    pub fn send_bytes(&mut self, string: Vec<u8>) {
        self.send_write(string);
    }

    fn send_write<B: Into<Cow<'static, [u8]>>>(&self, data: B) {
        let _ = self.channel.send(Msg::Input(data.into()));
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
}
