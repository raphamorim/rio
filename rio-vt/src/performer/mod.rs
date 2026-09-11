pub mod handler;
mod osc;
pub mod parser;

#[cfg(all(test, feature = "pty"))]
mod tests;

#[cfg(feature = "pty")]
use crate::crosswords::Crosswords;
#[cfg(feature = "pty")]
use crate::event::sync::FairMutex;
#[cfg(feature = "pty")]
use crate::event::RioEvent;
#[cfg(feature = "pty")]
use crate::event::{EventListener, Msg, WindowId};
#[cfg(feature = "pty")]
use corcovado::channel;
#[cfg(all(unix, feature = "pty"))]
use corcovado::unix::UnixReady;
#[cfg(feature = "pty")]
use corcovado::{self, Events, PollOpt, Ready};
#[cfg(feature = "pty")]
use std::borrow::Cow;
#[cfg(feature = "pty")]
use std::collections::VecDeque;
#[cfg(feature = "pty")]
use std::io::{self, ErrorKind, Read, Write};
#[cfg(feature = "pty")]
use std::sync::Arc;
#[cfg(feature = "pty")]
use std::thread::{Builder, JoinHandle};
#[cfg(feature = "pty")]
use std::time::Instant;
#[cfg(feature = "pty")]
use tracing::error;

/// Like `thread::spawn`, but with a `name` argument.
#[cfg(feature = "pty")]
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

#[cfg(feature = "pty")]
const READ_BUFFER_SIZE: usize = 0x10_0000;
/// Max bytes per read(2). Draining the whole tty queue in one giant
/// read and then parking in poll serializes against the writer;
/// reading in bounded chunks, with a parse between chunks, keeps the
/// queue non-empty so the slave's writer and this reader stream
/// concurrently. (The queue itself is sized by the pty's baud rate;
/// see `create_termp`.)
#[cfg(feature = "pty")]
const READ_CHUNK: usize = 65536;
/// Max bytes to parse per terminal lease. The lease spans the whole
/// `pty_read` call, so this bounds how long the renderer can wait for
/// its snapshot (~1ms of parsing at full SGR density, well inside a
/// 120Hz frame budget); measured ~3% more drain throughput than
/// yielding per chunk.
#[cfg(feature = "pty")]
const MAX_LOCKED_READ: usize = READ_CHUNK * 4;

#[cfg(feature = "pty")]
#[derive(Debug, PartialEq, Eq)]
enum ReadOutcome {
    Idle,
    Closed,
    Budget,
}

#[cfg(feature = "pty")]
enum ExitReason {
    Shutdown,
    ChildExited(Option<i32>),
}

// Guards the pairing that once regressed: a MAX_LOCKED_READ below
// READ_CHUNK ends the burst loop after a single partial chunk.
#[cfg(feature = "pty")]
const _: () = {
    assert!(MAX_LOCKED_READ >= READ_CHUNK);
    assert!(MAX_LOCKED_READ.is_multiple_of(READ_CHUNK));
    assert!(READ_BUFFER_SIZE >= MAX_LOCKED_READ);
};

#[cfg(feature = "pty")]
struct PeekableReceiver<T> {
    rx: channel::Receiver<T>,
    peeked: Option<T>,
}

#[cfg(feature = "pty")]
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

#[cfg(feature = "pty")]
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

#[cfg(feature = "pty")]
#[derive(Default)]
pub struct State {
    write_list: VecDeque<Cow<'static, [u8]>>,
    writing: Option<Writing>,
    parser: handler::Processor,
}

#[cfg(feature = "pty")]
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

#[cfg(feature = "pty")]
struct Writing {
    source: Cow<'static, [u8]>,
    written: usize,
}

#[cfg(feature = "pty")]
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

#[cfg(feature = "pty")]
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

    /// Read from the PTY and parse into the terminal.
    ///
    /// Reads until the PTY returns `WouldBlock` (or `MAX_LOCKED_READ`
    /// bytes were parsed under one lock hold). A PTY hands back a few
    /// KiB per `read`, so stopping on a short read would pay a full
    /// poll round trip per chunk instead of per burst and cap drain
    /// throughput; the confirming read that ends a burst costs a
    /// single `EAGAIN`.
    #[inline]
    fn pty_read(&mut self, state: &mut State, buf: &mut [u8]) -> io::Result<ReadOutcome> {
        let mut unprocessed = 0;
        let mut processed = 0;
        let mut result = Ok(ReadOutcome::Budget);

        // Reserve the next terminal lock for PTY reading.
        let _terminal_lease = Some(self.terminal.lease());
        let mut terminal = None;

        loop {
            // Read from the PTY.
            let cap = (unprocessed + READ_CHUNK).min(buf.len());
            let stopped = match self.pty.reader().read(&mut buf[unprocessed..cap]) {
                Ok(0) => {
                    // Unix: EOF, every slave fd is closed. Windows: the
                    // ConPTY ring is momentarily empty (its reader never
                    // returns WouldBlock), so nothing is closed yet.
                    result = Ok(if cfg!(unix) {
                        ReadOutcome::Closed
                    } else {
                        ReadOutcome::Idle
                    });
                    true
                }
                Ok(got) => {
                    unprocessed += got;
                    false
                }
                Err(err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) if err.kind() == ErrorKind::WouldBlock => {
                    result = Ok(ReadOutcome::Idle);
                    true
                }
                Err(err) => {
                    result = Err(err);
                    true
                }
            };
            if stopped && unprocessed == 0 {
                break;
            }

            // Attempt to lock the terminal.
            let terminal = match &mut terminal {
                Some(terminal) => terminal,
                None => terminal.insert(match self.terminal.try_lock_unfair() {
                    // Force block if we are at the buffer size limit.
                    None if stopped || unprocessed == buf.len() => {
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
            if stopped || processed >= MAX_LOCKED_READ {
                break;
            }
        }

        // Notify renderer that new damage is available.
        // Only send if no event is already in flight: the renderer will
        // extract all accumulated damage when it locks the terminal.
        if state.parser.sync_bytes_count() < processed && processed > 0 {
            if let Some(ref mut term) = terminal {
                if !term.damage_event_in_flight && term.peek_damage_event().is_some() {
                    term.damage_event_in_flight = true;
                    self.event_proxy.send_event(
                        RioEvent::TerminalDamaged(self.route_id),
                        self.window_id,
                    );
                }
            }
        }

        result
    }

    /// Drain the channel.
    ///
    /// Returns `false` when a shutdown message was received.
    fn drain_recv_channel(&mut self, state: &mut State) -> bool {
        while let Some(msg) = self.receiver.recv() {
            match msg {
                Msg::Input(input) => state.write_list.push_back(input),
                Msg::Resize(window_size) => {
                    let _ = self.pty.set_winsize(window_size.into());
                }
                Msg::Shutdown => return false,
            }
        }

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
            let reason = self.run(&mut state, &mut buf);
            self.finish(&mut state, &mut buf, reason);
            (self, state)
        })
    }

    fn run(&mut self, state: &mut State, buf: &mut [u8]) -> io::Result<ExitReason> {
        let mut tokens = (0..).map(Into::into);

        // The channel is drained to empty on every wakeup, which clears
        // its readiness and re-arms the next edge transition, so plain
        // edge (no oneshot, no re-registration) is enough. Level would go
        // through the readiness queue's re-enqueue path, which is much
        // more expensive per wakeup.
        let channel_token = tokens.next().unwrap();
        self.poll.register(
            &self.receiver.rx,
            channel_token,
            Ready::readable(),
            PollOpt::edge(),
        )?;

        // The PTY is level-triggered: pty_read may stop before draining
        // the fd (MAX_LOCKED_READ), which would lose an edge, and level
        // registrations stay armed so no re-registration is needed after
        // each event. The write interest must be dropped as soon as the
        // write queue drains or the poll would keep waking up for the
        // writable PTY.
        let poll_opts = PollOpt::level();

        // Register TTY through EventedRW interface.
        self.pty
            .register(&self.poll, &mut tokens, Ready::readable(), poll_opts)?;

        let mut events = Events::with_capacity(1024);
        let mut last_interest = Ready::readable();

        loop {
            // Wakeup the event loop when a synchronized update timeout was reached.
            let handler = state.parser.sync_timeout();
            let timeout = handler
                .sync_timeout()
                .map(|st| st.saturating_duration_since(Instant::now()));

            events.clear();
            if let Err(err) = self.poll.poll(&mut events, timeout) {
                match err.kind() {
                    ErrorKind::Interrupted => continue,
                    _ => return Err(err),
                }
            }

            // Handle synchronized update timeout.
            if events.is_empty() && self.receiver.peek().is_none() {
                let mut terminal = self.terminal.lock();
                state.parser.stop_sync(&mut *terminal);

                // Notify renderer if damage available and no event in flight
                if !terminal.damage_event_in_flight
                    && terminal.peek_damage_event().is_some()
                {
                    terminal.damage_event_in_flight = true;
                    self.event_proxy.send_event(
                        RioEvent::TerminalDamaged(self.route_id),
                        self.window_id,
                    );
                }

                continue;
            }

            // Handle channel events, if there are any.
            if !self.drain_recv_channel(state) {
                return Ok(ExitReason::Shutdown);
            }

            for event in events.iter() {
                match event.token() {
                    // Channel messages were already drained above.
                    token if token == channel_token => (),
                    token if token == self.pty.child_event_token() => {
                        if let Some(teletypewriter::ChildEvent::Exited(status)) =
                            self.pty.next_child_event()
                        {
                            return Ok(ExitReason::ChildExited(status));
                        }
                    }

                    token
                        if token == self.pty.read_token()
                            || token == self.pty.write_token() =>
                    {
                        #[cfg(unix)]
                        let hung_up = UnixReady::from(event.readiness()).is_hup();
                        #[cfg(not(unix))]
                        let hung_up = false;
                        // HUP can accompany unread final output.
                        if event.readiness().is_readable() || hung_up {
                            if let Err(err) = self.pty_read(state, buf) {
                                // On Linux, a `read` on the master side of a PTY can fail
                                // with `EIO` if the client side hangs up.  In that case,
                                // just loop back round for the inevitable `Exited` event.
                                #[cfg(target_os = "linux")]
                                if err.raw_os_error() == Some(libc::EIO) {
                                    continue;
                                }

                                return Err(err);
                            }
                        }

                        if !hung_up && event.readiness().is_writable() {
                            self.pty_write(state)?;
                        }
                    }
                    _ => (),
                }
            }

            // Update the PTY registration when write interest changed.
            let mut interest = Ready::readable();
            if state.needs_write() {
                interest.insert(Ready::writable());
            }
            if interest != last_interest {
                self.pty.reregister(&self.poll, interest, poll_opts)?;
                last_interest = interest;
            }
        }
    }

    /// All reader exits, including partial registration failures, finalize here.
    fn finish(
        &mut self,
        state: &mut State,
        buf: &mut [u8],
        reason: io::Result<ExitReason>,
    ) {
        if matches!(reason, Ok(ExitReason::ChildExited(_))) {
            // Do not wait for descendants to close the slave. Drain across
            // parsing budgets, limiting continuously available output to 100 ms
            // between batches (terminal lock waits can extend this interval).
            let deadline = Instant::now() + std::time::Duration::from_millis(100);
            loop {
                match self.pty_read(state, buf) {
                    Ok(ReadOutcome::Budget) if Instant::now() < deadline => continue,
                    // The ConPTY pump thread delivers final output after
                    // the exit event, so an empty ring is retried until
                    // the deadline instead of ending the drain.
                    Ok(ReadOutcome::Idle)
                        if cfg!(windows) && Instant::now() < deadline =>
                    {
                        std::thread::sleep(std::time::Duration::from_millis(2));
                        continue;
                    }
                    Err(err) => tracing::debug!("PTY final drain: {err}"),
                    _ => (),
                }
                break;
            }
        }

        // Flush exactly once before publishing any exit notification.
        let pending_sync = state.parser.sync_bytes_count() > 0;
        state.parser.stop_sync(&mut *self.terminal.lock());
        match &reason {
            Ok(ExitReason::ChildExited(status)) => {
                self.event_proxy.send_event(
                    RioEvent::ChildExited(self.route_id, *status),
                    self.window_id,
                );
                self.terminal.lock().exit();
            }
            // A reader failure also closes the terminal: shutdown() below
            // kills the child, so without a notification the frontend
            // would keep a dead pane open with no way to learn about it.
            Err(err) => {
                error!("PTY reader failed: {err}");
                self.event_proxy.send_event(
                    RioEvent::ChildExited(self.route_id, None),
                    self.window_id,
                );
                self.terminal.lock().exit();
            }
            Ok(ExitReason::Shutdown) => (),
        }
        if pending_sync || !matches!(reason, Ok(ExitReason::Shutdown)) {
            self.event_proxy
                .send_event(RioEvent::Render, self.window_id);
        }
        if let Err(err) = self.pty.shutdown() {
            error!("PTY shutdown failed: {err}");
        }
        // These objects are retained in the returned Machine. Deregistration is
        // best effort because setup may have failed before registering them.
        let _ = self.poll.deregister(&self.receiver.rx);
        let _ = self.pty.deregister(&self.poll);
    }
}
