use crate::event::Msg;
use std::borrow::Cow;
use teletypewriter::WinsizeBuilder;

pub struct Messenger {
    channel: corcovado::channel::Sender<Msg>,
}

impl Messenger {
    pub fn new(channel: corcovado::channel::Sender<Msg>) -> Messenger {
        Messenger { channel }
    }

    #[inline]
    pub fn send_bytes(&mut self, string: Vec<u8>) {
        self.send_write(string);
    }

    #[inline]
    pub fn send_write<B: Into<Cow<'static, [u8]>>>(&self, data: B) {
        let bytes = data.into();
        // terminal hangs if we send 0 bytes through.
        if bytes.len() == 0 {
            return;
        }

        let _ = self.channel.send(Msg::Input(bytes));
    }

    #[inline]
    pub fn send_resize(&self, new_size: WinsizeBuilder) -> Result<&str, String> {
        match self.channel.send(Msg::Resize(new_size)) {
            Ok(..) => Ok("Resized"),
            Err(..) => Err("Error sending message".to_string()),
        }
    }
}
