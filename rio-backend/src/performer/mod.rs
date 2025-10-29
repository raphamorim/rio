pub mod handler;

use crate::crosswords::Crosswords;
use crate::event::sync::FairMutex;
use crate::event::RioEvent;
use crate::event::{EventListener, Msg, WindowId};
use corcovado::channel;
#[cfg(unix)]
use corcovado::unix::UnixReady;
use corcovado::{self, Events, PollOpt, Ready};
use std::borrow::Cow;
use std::collections::VecDeque;
use std::io::{self, ErrorKind, Read, Write};
use std::sync::Arc;
use std::thread::{Builder, JoinHandle};
use std::time::Instant;
use tracing::error;

/// Like `thread::spawn`, but with a `name` argument.
pub fn spawn_named<F, T, S>(name: S, f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
    S: Into<String>,
{
    Builder::new()
        .name(name.into())
        .spawn(f)
        .expect("thread spawn works")
}

const READ_BUFFER_SIZE: usize = 0x10_0000;
/// Max bytes to read from the PTY while the terminal is locked.
const MAX_LOCKED_READ: usize = u16::MAX as usize;

struct PeekableReceiver<T> {
    rx: channel::Receiver<T>,
    peeked: Option<T>,
}

impl<T> PeekableReceiver<T> {
    fn new(rx: channel::Receiver<T>) -> Self {
        Self { rx, peeked: None }
    }

    fn peek(&mut self) -> Option<&T> {
        if self.peeked.is_none() {
            self.peeked = self.rx.try_recv().ok();
        }

        self.peeked.as_ref()
    }

    fn recv(&mut self) -> Option<T> {
        if self.peeked.is_some() {
            self.peeked.take()
        } else {
            self.rx.try_recv().ok()
        }
    }
}

pub struct Machine<T: teletypewriter::EventedPty, U: EventListener> {
    sender: channel::Sender<Msg>,
    receiver: PeekableReceiver<Msg>,
    pty: T,
    poll: corcovado::Poll,
    terminal: Arc<FairMutex<Crosswords<U>>>,
    event_proxy: U,
    window_id: WindowId,
    route_id: usize,
}

#[derive(Default)]
pub struct State {
    write_list: VecDeque<Cow<'static, [u8]>>,
    writing: Option<Writing>,
    parser: handler::Processor,
}

impl State {
    #[inline]
    fn ensure_next(&mut self) {
        if self.writing.is_none() {
            self.goto_next();
        }
    }

    #[inline]
    fn goto_next(&mut self) {
        self.writing = self.write_list.pop_front().map(Writing::new);
    }

    #[inline]
    fn take_current(&mut self) -> Option<Writing> {
        self.writing.take()
    }

    #[inline]
    fn needs_write(&self) -> bool {
        self.writing.is_some() || !self.write_list.is_empty()
    }

    #[inline]
    fn set_current(&mut self, new: Option<Writing>) {
        self.writing = new;
    }
}

struct Writing {
    source: Cow<'static, [u8]>,
    written: usize,
}

impl Writing {
    #[inline]
    fn new(c: Cow<'static, [u8]>) -> Writing {
        Writing {
            source: c,
            written: 0,
        }
    }

    #[inline]
    fn advance(&mut self, n: usize) {
        self.written += n;
    }

    #[inline]
    fn remaining_bytes(&self) -> &[u8] {
        &self.source[self.written..]
    }

    #[inline]
    fn finished(&self) -> bool {
        self.written >= self.source.len()
    }
}

impl<T, U> Machine<T, U>
where
    T: teletypewriter::EventedPty + Send + 'static,
    U: EventListener + Send + 'static,
{
    pub fn new(
        terminal: Arc<FairMutex<Crosswords<U>>>,
        pty: T,
        event_proxy: U,
        window_id: WindowId,
        route_id: usize,
    ) -> Result<Machine<T, U>, Box<dyn std::error::Error>> {
        let (sender, receiver) = channel::channel();
        let poll = corcovado::Poll::new()?;

        Ok(Machine {
            sender,
            receiver: PeekableReceiver::new(receiver),
            poll,
            pty,
            terminal,
            event_proxy,
            window_id,
            route_id,
        })
    }

    #[inline]
    fn pty_read(&mut self, state: &mut State, buf: &mut [u8]) -> io::Result<()> {
        let mut unprocessed = 0;
        let mut processed = 0;

        // Reserve the next terminal lock for PTY reading.
        let _terminal_lease = Some(self.terminal.lease());
        let mut terminal = None;

        loop {
            // Read from the PTY.
            match self.pty.reader().read(&mut buf[unprocessed..]) {
                // This is received on Windows/macOS when no more data is readable from the PTY.
                Ok(0) if unprocessed == 0 => break,
                Ok(got) => unprocessed += got,
                Err(err) => match err.kind() {
                    ErrorKind::Interrupted | ErrorKind::WouldBlock => {
                        // Go back to mio if we're caught up on parsing and the PTY would block.
                        if unprocessed == 0 {
                            break;
                        }
                    }
                    _ => return Err(err),
                },
            }

            // Attempt to lock the terminal.
            let terminal = match &mut terminal {
                Some(terminal) => terminal,
                None => terminal.insert(match self.terminal.try_lock_unfair() {
                    // Force block if we are at the buffer size limit.
                    None if unprocessed >= READ_BUFFER_SIZE => {
                        self.terminal.lock_unfair()
                    }
                    None => continue,
                    Some(terminal) => terminal,
                }),
            };

            // Parse the incoming bytes.
            state.parser.advance(&mut **terminal, &buf[..unprocessed]);

            processed += unprocessed;
            unprocessed = 0;

            // Assure we're not blocking the terminal too long unnecessarily.
            if processed >= MAX_LOCKED_READ {
                break;
            }
        }

        // Queue terminal update processing unless all processed bytes were synchronized.
        // For non-synchronized updates, we send a Wakeup event which will coalesce
        // multiple rapid updates into a single render pass.
        if state.parser.sync_bytes_count() < processed && processed > 0 {
            tracing::trace!(
                "PTY read: Sending Wakeup event for {} bytes of non-sync data",
                processed
            );
            // Send a Wakeup event to coalesce renders
            self.event_proxy
                .send_event(RioEvent::Wakeup(self.route_id), self.window_id);
            // }
        }

        Ok(())
    }

    /// Drain the channel.
    ///
    /// Returns `false` when a shutdown message was received.
    fn drain_recv_channel(&mut self, state: &mut State) -> bool {
        while let Some(msg) = self.receiver.recv() {
            match msg {
                Msg::Input(input) => state.write_list.push_back(input),
                Msg::Resize(window_size) => {
                    let _ = self.pty.set_winsize(window_size);
                }
                Msg::Shutdown => return false,
            }
        }

        true
    }

    /// Returns a `bool` indicating whether or not the event loop should continue running.
    #[inline]
    fn channel_event(&mut self, token: corcovado::Token, state: &mut State) -> bool {
        if !self.drain_recv_channel(state) {
            return false;
        }

        self.poll
            .reregister(
                &self.receiver.rx,
                token,
                Ready::readable(),
                PollOpt::edge() | PollOpt::oneshot(),
            )
            .unwrap();

        true
    }

    #[inline]
    fn pty_write(&mut self, state: &mut State) -> io::Result<()> {
        state.ensure_next();

        'write_many: while let Some(mut current) = state.take_current() {
            'write_one: loop {
                match self.pty.writer().write(current.remaining_bytes()) {
                    Ok(0) => {
                        state.set_current(Some(current));
                        break 'write_many;
                    }
                    Ok(n) => {
                        current.advance(n);
                        if current.finished() {
                            state.goto_next();
                            break 'write_one;
                        }
                    }
                    Err(err) => {
                        state.set_current(Some(current));
                        match err.kind() {
                            ErrorKind::Interrupted | ErrorKind::WouldBlock => {
                                break 'write_many
                            }
                            _ => return Err(err),
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn channel(&self) -> channel::Sender<Msg> {
        self.sender.clone()
    }

    pub fn spawn(mut self) -> JoinHandle<(Self, State)> {
        spawn_named("PTY reader", move || {
            let mut state = State::default();
            let mut buf = [0u8; READ_BUFFER_SIZE];

            let mut tokens = (0..).map(Into::into);

            let poll_opts = PollOpt::edge() | PollOpt::oneshot();

            let channel_token = tokens.next().unwrap();
            self.poll
                .register(
                    &self.receiver.rx,
                    channel_token,
                    Ready::readable(),
                    poll_opts,
                )
                .unwrap();

            // Register TTY through EventedRW interface.
            self.pty
                .register(&self.poll, &mut tokens, Ready::readable(), poll_opts)
                .unwrap();

            let mut events = Events::with_capacity(1024);

            'event_loop: loop {
                // Wakeup the event loop when a synchronized update timeout was reached.
                let handler = state.parser.sync_timeout();
                let timeout = handler
                    .sync_timeout()
                    .map(|st| st.saturating_duration_since(Instant::now()));

                events.clear();
                if let Err(err) = self.poll.poll(&mut events, timeout) {
                    match err.kind() {
                        ErrorKind::Interrupted => continue,
                        _ => {
                            error!("Event loop polling error: {err}");
                            break 'event_loop;
                        }
                    }
                }

                // Handle synchronized update timeout.
                if events.is_empty() && self.receiver.peek().is_none() {
                    let mut terminal = self.terminal.lock();
                    state.parser.stop_sync(&mut *terminal);

                    // Emit damage event if there's any damage after processing sync buffer
                    self.event_proxy
                        .send_event(RioEvent::Wakeup(self.route_id), self.window_id);

                    continue;
                }

                // Handle channel events, if there are any.
                if !self.drain_recv_channel(&mut state) {
                    break;
                }

                for event in events.iter() {
                    match event.token() {
                        token if token == channel_token => {
                            // In case should shutdown by message
                            if !self.channel_event(channel_token, &mut state) {
                                break 'event_loop;
                            }
                        }
                        token if token == self.pty.child_event_token() => {
                            if let Some(teletypewriter::ChildEvent::Exited) =
                                self.pty.next_child_event()
                            {
                                // In the future allow configure exit
                                // if self.hold {
                                //     With hold enabled, make sure the PTY is drained.
                                //     let _ = self.pty_read(&mut state, &mut buf);
                                // } else {
                                //     // Without hold, shutdown the terminal.
                                //     self.terminal.lock().exit();
                                // }

                                self.terminal.lock().exit();

                                self.event_proxy
                                    .send_event(RioEvent::Render, self.window_id);

                                break 'event_loop;
                            }
                        }

                        token
                            if token == self.pty.read_token()
                                || token == self.pty.write_token() =>
                        {
                            #[cfg(unix)]
                            if UnixReady::from(event.readiness()).is_hup() {
                                // Don't try to do I/O on a dead PTY.
                                continue;
                            }
                            if event.readiness().is_readable() {
                                if let Err(err) = self.pty_read(&mut state, &mut buf) {
                                    // On Linux, a `read` on the master side of a PTY can fail
                                    // with `EIO` if the client side hangs up.  In that case,
                                    // just loop back round for the inevitable `Exited` event.
                                    #[cfg(target_os = "linux")]
                                    if err.raw_os_error() == Some(libc::EIO) {
                                        continue;
                                    }

                                    error!(
                                        "Error reading from PTY in event loop: {}",
                                        err
                                    );
                                    break 'event_loop;
                                }
                            }

                            if event.readiness().is_writable() {
                                if let Err(err) = self.pty_write(&mut state) {
                                    error!("Error writing to PTY in event loop: {}", err);
                                    break 'event_loop;
                                }
                            }
                        }
                        _ => (),
                    }
                }

                // Register write interest if necessary.
                let mut interest = Ready::readable();
                if state.needs_write() {
                    interest.insert(Ready::writable());
                }
                // Reregister with new interest.
                self.pty
                    .reregister(&self.poll, interest, poll_opts)
                    .unwrap();
            }

            // The evented instances are not dropped here so deregister them explicitly.
            let _ = self.poll.deregister(&self.receiver.rx);
            let _ = self.pty.deregister(&self.poll);

            (self, state)
        })
    }
}
