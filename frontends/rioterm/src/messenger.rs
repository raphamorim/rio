use crate::event::Msg;
use rio_backend::event::WindowSize;
use rio_backend::tmux::send_keys_commands;
use std::borrow::Cow;

pub struct Messenger {
    pub channel: corcovado::channel::Sender<Msg>,
    /// Set for a tmux pane. `channel` is then the PTY running the control
    /// client, which the pane shares with its siblings and does not own.
    tmux_pane: Option<u32>,
}

impl Messenger {
    pub fn new(channel: corcovado::channel::Sender<Msg>) -> Messenger {
        Messenger {
            channel,
            tmux_pane: None,
        }
    }

    pub fn for_tmux_pane(
        channel: corcovado::channel::Sender<Msg>,
        pane: u32,
    ) -> Messenger {
        Messenger {
            channel,
            tmux_pane: Some(pane),
        }
    }

    #[inline]
    pub fn tmux_pane(&self) -> Option<u32> {
        self.tmux_pane
    }

    /// Send the terminal's answer to something the application asked, such
    /// as a device attributes query.
    ///
    /// A tmux pane's answer goes by way of the session, which knows what
    /// tmux already answered: `%output` shows us those queries too, and a
    /// second answer would arrive at the application as typed input.
    #[inline]
    pub fn send_reply(&mut self, bytes: Vec<u8>) {
        match self.tmux_pane {
            Some(pane) => {
                let _ = self.channel.send(Msg::TmuxPaneReply(pane, bytes));
            }
            None => self.send_write(bytes),
        }
    }

    #[inline]
    pub fn send_write<B: Into<Cow<'static, [u8]>>>(&self, bytes: B) {
        let bytes = bytes.into();
        // terminal hangs if we send 0 bytes through.
        if bytes.is_empty() {
            return;
        }

        if let Some(pane) = self.tmux_pane {
            for command in send_keys_commands(pane, &bytes) {
                let _ = self.channel.send(Msg::TmuxCommand(command));
            }
            return;
        }

        let _ = self.channel.send(Msg::Input(bytes));
    }

    #[inline]
    pub fn send_resize(&self, new_size: WindowSize) -> Result<&str, String> {
        // tmux decides pane sizes, see `ContextManager::tmux_sync_size`.
        if self.tmux_pane.is_some() {
            return Ok("Resized");
        }

        match self.channel.send(Msg::Resize(new_size)) {
            Ok(..) => Ok("Resized"),
            Err(..) => Err("Error sending message".to_string()),
        }
    }

    #[inline]
    pub fn send_shutdown(&self) {
        if self.tmux_pane.is_none() {
            let _ = self.channel.send(Msg::Shutdown);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messengers() -> (Messenger, Messenger, corcovado::channel::Receiver<Msg>) {
        let (sender, receiver) = corcovado::channel::channel();
        (
            Messenger::new(sender.clone()),
            Messenger::for_tmux_pane(sender, 3),
            receiver,
        )
    }

    #[test]
    fn pane_input_becomes_send_keys() {
        let (_, pane, receiver) = messengers();
        pane.send_write(&b"ls\r"[..]);
        match receiver.try_recv() {
            Ok(Msg::TmuxCommand(command)) => {
                assert_eq!(command, "send-keys -t %3 -H 6c 73 0d");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn pane_replies_go_by_way_of_the_session() {
        let (_, mut pane, receiver) = messengers();
        pane.send_reply(b"\x1b[?6c".to_vec());
        match receiver.try_recv() {
            Ok(Msg::TmuxPaneReply(3, reply)) => assert_eq!(reply, b"\x1b[?6c"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn pane_cannot_resize_or_shut_down_the_host_pty() {
        let (_, pane, receiver) = messengers();
        pane.send_shutdown();
        let _ = pane.send_resize(WindowSize::default());
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn plain_terminals_are_unaffected() {
        let (mut plain, _, receiver) = messengers();
        plain.send_reply(b"reply".to_vec());
        assert!(matches!(receiver.try_recv(), Ok(Msg::Input(_))));
        plain.send_shutdown();
        assert!(matches!(receiver.try_recv(), Ok(Msg::Shutdown)));
    }
}
