pub mod handler;
mod osc;
#[cfg(test)]
pub(crate) use osc::MAX_HYPERLINK_BYTES;
pub mod parser;

/// Empty an accumulation buffer between uses. A buffer grown past
/// `retain` by one large sequence gives its memory back rather than
/// pinning that capacity for the session; a smaller one keeps its
/// allocation so the next sequence does not reallocate.
pub(crate) fn clear_or_release(buf: &mut Vec<u8>, retain: usize) {
    if buf.capacity() > retain {
        *buf = Vec::new();
    } else {
        buf.clear();
    }
}

#[cfg(all(test, feature = "pty"))]
mod tests;

#[cfg(feature = "pty")]
use crate::crosswords::Crosswords;
#[cfg(feature = "pty")]
use crate::event::sync::TerminalLock;
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
use std::io::{self, ErrorKind, Read};
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

/// Ask the scheduler to treat the calling thread as serving the user:
/// on macOS a thread without a QoS class may be scheduled on the
/// efficiency cores, which throttles the read and parse loop behind
/// everything on screen. Elsewhere this does nothing.
#[cfg(feature = "pty")]
pub fn raise_io_thread_priority() {
    #[cfg(target_os = "macos")]
    // SAFETY: sets a scheduling attribute of the calling thread only;
    // no memory is touched and the call has no preconditions.
    unsafe {
        libc::pthread_set_qos_class_self_np(
            libc::qos_class_t::QOS_CLASS_USER_INITIATED,
            0,
        );
    }
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
pub const READ_CHUNK: usize = 65536;
/// Max bytes to parse per `pty_read` call before the loop serves its
/// channel again: ~1ms of parsing at full SGR density, well inside a
/// 120Hz frame budget. The terminal itself is let go of after every
/// chunk, so a thread waiting for it never waits this long.
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

/// The reader thread of a terminal: reads the PTY and parses into the
/// terminal. Writing to the PTY is another thread's, the [`Writer`]
/// spawned alongside, so input never waits behind parsing; what this
/// thread has to say to the PTY, the parser's replies, goes to the
/// writer like any keystroke.
#[cfg(feature = "pty")]
pub struct Machine<T: teletypewriter::EventedPty, U: EventListener> {
    /// Control from the writer thread: resizes and the shutdown, which
    /// the owner sends to the one channel every owner holds, the
    /// writer's, and the writer passes on.
    receiver: PeekableReceiver<Msg>,
    /// The writer's inbox: the owner's input and answers, the parser's
    /// replies, resizes and the shutdown.
    writes: channel::Sender<Msg>,
    pty: T,
    /// The writer, until spawned and once joined again, so the write
    /// half of the PTY is dropped here, after `pty`: on Windows the
    /// input pipe must close after the console.
    writer: Option<Writer<T::WriteHalf, U>>,
    writer_thread: Option<JoinHandle<Writer<T::WriteHalf, U>>>,
    poll: corcovado::Poll,
    terminal: Arc<TerminalLock<Crosswords<U>>>,
    event_proxy: U,
    window_id: WindowId,
    route_id: usize,
}

#[cfg(feature = "pty")]
#[derive(Default)]
pub struct State {
    parser: handler::Processor,
    /// Deadline of the terminal's active synchronized update, refreshed
    /// whenever the terminal lock is held.
    sync_deadline: Option<Instant>,
}

#[cfg(feature = "pty")]
impl<T, U> Machine<T, U>
where
    T: teletypewriter::EventedPty + Send + 'static,
    U: EventListener + Send + 'static,
{
    pub fn new(
        terminal: Arc<TerminalLock<Crosswords<U>>>,
        mut pty: T,
        event_proxy: U,
        window_id: WindowId,
        route_id: usize,
    ) -> Result<Machine<T, U>, Box<dyn std::error::Error>> {
        let (control, receiver) = channel::channel();
        let (writes, inbox) = channel::channel();
        let poll = corcovado::Poll::new()?;

        // Replies the parser produces go to the writer thread the moment
        // it produces them, like everything else bound for the PTY, so
        // they keep their place among the owner's input and answers.
        let reply_writes = writes.clone();
        terminal
            .lock()
            .set_reply_sink(Some(crate::crosswords::ReplySink(Box::new(
                move |reply: String| {
                    let _ = reply_writes.send(Msg::Input(Cow::Owned(reply.into_bytes())));
                },
            ))));

        let writer = Writer {
            inbox: PeekableReceiver::new(inbox),
            control,
            poll: corcovado::Poll::new()?,
            half: pty.write_half()?,
            pending: Vec::new(),
            written: 0,
            terminal: Arc::clone(&terminal),
        };

        Ok(Machine {
            receiver: PeekableReceiver::new(receiver),
            writes,
            writer: Some(writer),
            writer_thread: None,
            poll,
            pty,
            terminal,
            event_proxy,
            window_id,
            route_id,
        })
    }

    /// Wake the renderer unless an earlier wakeup is still unconsumed.
    #[inline]
    fn notify_damage(&self, terminal: &mut Crosswords<U>) {
        if !terminal.damage_event_in_flight {
            terminal.damage_event_in_flight = true;
            self.event_proxy
                .send_event(RioEvent::TerminalDamaged(self.route_id), self.window_id);
        }
    }

    /// Read from the PTY and parse into the terminal.
    ///
    /// Reads until the PTY returns `WouldBlock` or `MAX_LOCKED_READ`
    /// bytes were parsed. A PTY hands back a few KiB per `read`, so
    /// stopping on a short read would pay a full poll round trip per
    /// chunk instead of per burst and cap drain throughput; the
    /// confirming read that ends a burst costs a single `EAGAIN`.
    ///
    /// The terminal is held for one chunk at a time, and a thread
    /// waiting for it, the renderer for its frame or the UI thread for
    /// a keystroke, gets it after the chunk being parsed rather than
    /// after the burst; see `let_go`.
    #[inline]
    fn pty_read(&mut self, state: &mut State, buf: &mut [u8]) -> io::Result<ReadOutcome> {
        let mut unprocessed = 0;
        let mut processed = 0;
        let mut result = Ok(ReadOutcome::Budget);
        let mut held_before = None;
        let mut ended = false;

        loop {
            // Read from the PTY.
            let cap = (unprocessed + READ_CHUNK).min(buf.len());
            let stopped = match self.pty.reader().read(&mut buf[unprocessed..cap]) {
                Ok(0) => {
                    result = Ok(ReadOutcome::Closed);
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

            // Take the terminal. While another thread has it the reading
            // goes on into the buffer; the end of the burst or a full
            // buffer waits for it.
            let mut terminal = match self.terminal.try_lock_reader() {
                None if stopped || unprocessed == buf.len() => {
                    self.terminal.lock_reader()
                }
                None => continue,
                Some(terminal) => terminal,
            };
            held_before.get_or_insert(terminal.synchronized_output());

            // Parse what was read, a chunk at a time. Bytes that piled
            // up while the terminal was busy are let go of between
            // chunks like reads are, so a thread waiting for the
            // terminal gets it within one chunk either way.
            let mut parsed = 0;
            while parsed < unprocessed {
                let end = (parsed + READ_CHUNK).min(unprocessed);
                if parsed > 0 {
                    self.let_go(terminal);
                    terminal = self.terminal.lock_reader();
                }
                state.parser.advance(&mut *terminal, &buf[parsed..end]);
                parsed = end;
            }
            processed += unprocessed;
            unprocessed = 0;

            if stopped || processed >= MAX_LOCKED_READ {
                self.end_burst(state, &mut terminal, held_before);
                self.let_go(terminal);
                ended = true;
                break;
            }
            self.let_go(terminal);
        }

        // A burst that ended with an empty read after its last chunk was
        // let go of: the renderer still has to hear of it.
        if processed > 0 && !ended {
            let mut terminal = self.terminal.lock_reader();
            self.end_burst(state, &mut terminal, held_before);
            self.let_go(terminal);
        }

        result
    }

    /// The end of a burst, under the lock: the synchronized update
    /// deadline is refreshed and the renderer told of new damage,
    /// unless an event is already in flight, since it will extract all
    /// accumulated damage when it locks the terminal.
    fn end_burst(
        &self,
        state: &mut State,
        terminal: &mut Crosswords<U>,
        held_before: Option<bool>,
    ) {
        state.sync_deadline = terminal.sync_update_deadline();
        let held = terminal.synchronized_output();
        let hold_ended = held_before == Some(true) && !held;
        if hold_ended || (!held && terminal.peek_damage_event().is_some()) {
            self.notify_damage(terminal);
        }
    }

    /// Let go of the terminal after a chunk. If a thread waits for the
    /// terminal, the unlock is fair, so one parked on it gets it now,
    /// and this thread does not ask for the lock again until the
    /// waiters have it, so one still spinning gets it too; see
    /// `TerminalLock::yield_to_demand`.
    fn let_go(&self, terminal: parking_lot::MutexGuard<'_, Crosswords<U>>) {
        if self.terminal.has_demand() {
            parking_lot::MutexGuard::unlock_fair(terminal);
            self.terminal.yield_to_demand();
        } else {
            drop(terminal);
        }
    }

    /// Drain the channel.
    ///
    /// Returns `false` when a shutdown message was received.
    fn drain_recv_channel(&mut self) -> bool {
        while let Some(msg) = self.receiver.recv() {
            match msg {
                // Bytes for the PTY are the writer's; this channel only
                // ever carries what the writer passed on.
                Msg::Input(_) | Msg::Reply(_) => {
                    let _ = self.writes.send(msg);
                }
                Msg::Resize(window_size) => {
                    let _ = self.pty.set_winsize(window_size.into());
                }
                Msg::Shutdown => return false,
            }
        }

        true
    }

    /// The one channel an owner holds: the writer's inbox. Input and
    /// answers are written from there; resizes and the shutdown are
    /// passed on to the reader.
    pub fn channel(&self) -> channel::Sender<Msg> {
        self.writes.clone()
    }

    fn spawn_writer(&mut self) {
        if let Some(mut writer) = self.writer.take() {
            self.writer_thread = Some(spawn_named("PTY writer", move || {
                raise_io_thread_priority();
                if let Err(err) = writer.run() {
                    // Without a writer the terminal cannot be typed to;
                    // the reader shuts it down, as its own failure would.
                    error!("PTY writer failed: {err}");
                    let _ = writer.control.send(Msg::Shutdown);
                }
                writer
            }));
        }
    }

    /// Stop the writer and wait for it; nothing is left to write to.
    /// The writer comes back to be dropped with the machine.
    fn stop_writer(&mut self) {
        let _ = self.writes.send(Msg::Shutdown);
        if let Some(writer) = self.writer_thread.take() {
            if let Ok(writer) = writer.join() {
                self.writer = Some(writer);
            }
        }
    }

    /// Start both threads. The writer is joined by the reader on its
    /// way out, so the one handle covers both.
    pub fn spawn(mut self) -> JoinHandle<(Self, State)> {
        self.spawn_writer();
        spawn_named("PTY reader", move || {
            raise_io_thread_priority();
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
        // each event. Only reading is of interest here; the writer
        // thread watches the write side.
        let poll_opts = PollOpt::level();

        // Register TTY through EventedRW interface.
        self.pty
            .register(&self.poll, &mut tokens, Ready::readable(), poll_opts)?;

        let mut events = Events::with_capacity(1024);

        loop {
            // Wakeup the event loop when a synchronized update timeout was reached.
            let timeout = state
                .sync_deadline
                .map(|deadline| deadline.saturating_duration_since(Instant::now()));

            events.clear();
            if let Err(err) = self.poll.poll(&mut events, timeout) {
                match err.kind() {
                    ErrorKind::Interrupted => continue,
                    _ => return Err(err),
                }
            }

            // Handle synchronized update timeout.
            if state
                .sync_deadline
                .is_some_and(|deadline| deadline <= Instant::now())
            {
                let mut terminal = self.terminal.lock();
                if terminal.sync_update_expired(Instant::now()) {
                    terminal.end_sync_update();
                    self.notify_damage(&mut terminal);
                }
                state.sync_deadline = terminal.sync_update_deadline();
            }

            if events.is_empty() && self.receiver.peek().is_none() {
                continue;
            }

            // Handle control messages, if there are any.
            if !self.drain_recv_channel() {
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
                            self.pty_read(state, buf)?;
                        }
                    }
                    _ => (),
                }
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
            // Windows Closed means the ConPTY ring is momentarily empty
            // (never WouldBlock, and no real EOF exists until the drain's
            // own shutdown closes the pseudoconsole), while the pump
            // thread delivers final output after the exit event. Retry it
            // through a short quiet window, refreshed by each batch of
            // data, so a close is not taxed the full deadline. Unix
            // Closed is a real EOF and ends the drain at once.
            let quiet = std::time::Duration::from_millis(25);
            let mut quiet_deadline = Instant::now() + quiet;
            loop {
                match self.pty_read(state, buf) {
                    Ok(ReadOutcome::Budget) if Instant::now() < deadline => {
                        quiet_deadline = Instant::now() + quiet;
                        continue;
                    }
                    Ok(ReadOutcome::Closed)
                        if cfg!(windows)
                            && Instant::now() < deadline
                            && Instant::now() < quiet_deadline =>
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
        let pending_sync = {
            let mut terminal = self.terminal.lock();
            let pending = terminal.synchronized_output();
            terminal.end_sync_update();
            if pending {
                self.notify_damage(&mut terminal);
            }
            pending
        };
        match &reason {
            Ok(ExitReason::ChildExited(status)) => {
                self.event_proxy.send_event(
                    RioEvent::ChildExited(self.route_id, *status),
                    self.window_id,
                );
                self.terminal.lock().exit();
            }
            // A reader failure also closes the terminal, since shutdown()
            // below kills the child. The close is driven by exit() (it
            // emits CloseTerminal, the event frontends act on); the
            // ChildExited alongside is informational for embedders.
            // Removing the exit() would leave a dead pane open forever.
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
        // The writer has nothing left to write to; it stops and is
        // joined here, so the one handle the owner holds covers both.
        self.stop_writer();
        // These objects are retained in the returned Machine. Deregistration is
        // best effort because setup may have failed before registering them.
        let _ = self.poll.deregister(&self.receiver.rx);
        let _ = self.pty.deregister(&self.poll);
    }
}

/// The writer thread of a terminal: everything that goes to the PTY,
/// the owner's input and answers and the reader's parser replies,
/// arrives in one inbox and is written in that order. Nothing here
/// waits on the reader, so a keystroke is written while a burst is
/// parsed and while the terminal is handed to the renderer. Resizes and
/// the shutdown, which the owner sends to the same inbox, are passed on
/// to the reader.
#[cfg(feature = "pty")]
struct Writer<W, U: EventListener> {
    inbox: PeekableReceiver<Msg>,
    control: channel::Sender<Msg>,
    poll: corcovado::Poll,
    half: W,
    /// Bytes not yet written, in order; `written` of them already are.
    /// Adjacent messages are written with one call.
    pending: Vec<u8>,
    written: usize,
    terminal: Arc<TerminalLock<Crosswords<U>>>,
}

#[cfg(feature = "pty")]
impl<W, U> Writer<W, U>
where
    W: io::Write + corcovado::event::Evented,
    U: EventListener,
{
    const INBOX: corcovado::Token = corcovado::Token(0);
    const PTY: corcovado::Token = corcovado::Token(1);

    fn run(&mut self) -> io::Result<()> {
        self.poll.register(
            &self.inbox.rx,
            Self::INBOX,
            Ready::readable(),
            PollOpt::edge(),
        )?;
        // Writable interest only while bytes wait, or a writable PTY
        // would wake this loop for nothing.
        self.poll
            .register(&self.half, Self::PTY, Ready::empty(), PollOpt::level())?;
        let mut armed = false;
        // Set once the PTY hung up: it is left unwatched, since a
        // hangup is reported whatever the interest and would wake this
        // loop for nothing until the reader stops it.
        let mut hung_up = false;
        let mut events = Events::with_capacity(16);

        loop {
            events.clear();
            if let Err(err) = self.poll.poll(&mut events, None) {
                match err.kind() {
                    ErrorKind::Interrupted => continue,
                    _ => return Err(err),
                }
            }

            let writable = events.iter().any(|event| {
                event.token() == Self::PTY && event.readiness().is_writable()
            });
            #[cfg(unix)]
            let hup_now = events.iter().any(|event| {
                event.token() == Self::PTY && UnixReady::from(event.readiness()).is_hup()
            });
            #[cfg(not(unix))]
            let hup_now = false;
            if hup_now && !hung_up {
                hung_up = true;
                self.poll.deregister(&self.half)?;
            }

            while let Some(msg) = self.inbox.recv() {
                match msg {
                    Msg::Input(bytes) => self.pending.extend_from_slice(&bytes),
                    Msg::Reply(answer) => {
                        self.pending.extend_from_slice(&answer);
                        // Queued ahead of any reply the reader produces
                        // from here on, so the terminal may hand the
                        // replies behind the query back to the reader.
                        self.terminal.lock().ui_query_answered();
                    }
                    Msg::Resize(size) => {
                        let _ = self.control.send(Msg::Resize(size));
                    }
                    Msg::Shutdown => {
                        let _ = self.control.send(Msg::Shutdown);
                        return Ok(());
                    }
                }
            }

            // Bytes refused by a full PTY wait for its writable event;
            // they are not retried on every message that arrives
            // meanwhile, which would cost a failing write each.
            if !armed || writable {
                if let Err(err) = self.flush() {
                    // The PTY is gone, a child that exited and closed
                    // its side; what was pending has nowhere to go. The
                    // reader notices the exit and stops this thread.
                    tracing::debug!("PTY write failed, bytes dropped: {err}");
                    self.pending.clear();
                    self.written = 0;
                }
            }

            let backlog = self.written < self.pending.len();
            if backlog != armed && !hung_up {
                let interest = if backlog {
                    Ready::writable()
                } else {
                    Ready::empty()
                };
                self.poll.reregister(
                    &self.half,
                    Self::PTY,
                    interest,
                    PollOpt::level(),
                )?;
                armed = backlog;
            }
        }
    }

    /// Write what is pending, as much as the PTY takes; the rest waits
    /// for the writable event.
    fn flush(&mut self) -> io::Result<()> {
        while self.written < self.pending.len() {
            match self.half.write(&self.pending[self.written..]) {
                Ok(0) => break,
                Ok(n) => self.written += n,
                Err(err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) => return Err(err),
            }
        }
        if self.written == self.pending.len() {
            self.pending.clear();
            self.written = 0;
        } else if self.written >= READ_CHUNK {
            self.pending.drain(..self.written);
            self.written = 0;
        }
        Ok(())
    }
}
