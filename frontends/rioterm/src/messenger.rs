use crate::event::Msg;
use rio_backend::event::WindowSize;
use std::borrow::Cow;
use std::cell::Cell;
use std::time::Instant;

pub struct Messenger {
    pub channel: corcovado::channel::Sender<Msg>,
    /// When someone last used the terminal: input written to it (keys, a
    /// paste, mouse reports), or keys, the mouse or the wheel on its window
    /// (`mark_used`), scrolling it back included.
    last_use: Cell<Instant>,
}

impl Messenger {
    pub fn new(channel: corcovado::channel::Sender<Msg>) -> Messenger {
        Messenger {
            channel,
            last_use: Cell::new(Instant::now()),
        }
    }

    #[inline]
    pub fn send_write<B: Into<Cow<'static, [u8]>>>(&self, bytes: B) {
        let bytes = bytes.into();
        // terminal hangs if we send 0 bytes through.
        if bytes.is_empty() {
            return;
        }

        self.mark_used();
        let _ = self.channel.send(Msg::Input(bytes));
    }

    /// Someone is using the terminal now.
    #[inline]
    pub fn mark_used(&self) {
        self.last_use.set(Instant::now());
    }

    #[cfg(test)]
    pub fn mark_used_at(&self, at: Instant) {
        self.last_use.set(at);
    }

    /// When someone last used the terminal (or it was made).
    #[inline]
    pub fn last_use(&self) -> Instant {
        self.last_use.get()
    }

    /// The answer to a query the terminal left to this thread (the
    /// clipboard, the text area in pixels, a color without a palette,
    /// a glyph), or a reply the terminal routed through this thread
    /// behind such a query. Sent even when empty: the terminal needs
    /// to hear that each is settled before it writes the replies
    /// behind them itself.
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
