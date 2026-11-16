use crate::event::Msg;
use rio_backend::event::WindowSize;
use std::borrow::Cow;

pub struct Messenger {
    pub channel: corcovado::channel::Sender<Msg>,
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
    pub fn send_write<B: Into<Cow<'static, [u8]>>>(&self, bytes: B) {
        let bytes = bytes.into();
        // terminal hangs if we send 0 bytes through.
        if bytes.is_empty() {
            return;
        }

        let _ = self.channel.send(Msg::Input(bytes));
    }

    /// The answer to a query the terminal left to this thread (the
    /// clipboard, the text area in pixels, a color without a palette).
    /// Sent even when empty: the terminal needs to hear that the
    /// query is settled before it writes the replies behind it itself.
    #[inline]
    pub fn send_reply(&self, bytes: Vec<u8>) {
        let _ = self.channel.send(Msg::Reply(Cow::Owned(bytes)));
    }

    #[inline]
    pub fn send_resize(&self, new_size: WindowSize) -> Result<&str, String> {
        match self.channel.send(Msg::Resize(new_size)) {
            Ok(..) => Ok("Resized"),
            Err(..) => Err("Error sending message".to_string()),
        }
    }
}
