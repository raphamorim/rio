use super::*;
use crate::ansi::CursorShape;
use crate::crosswords::pos::{Column, Line};
use crate::crosswords::CrosswordsSize;
use crate::event::VoidListener;
use std::sync::mpsc;
use std::time::Duration;
use teletypewriter::{ChildEvent, EventedPty, ProcessReadWrite, WinsizeBuilder};

struct TestPty {
    bytes: io::Cursor<Vec<u8>>,
    /// Later writes by the program, each read as its own burst once
    /// `bytes` is exhausted and the test signals readiness again.
    feed: Option<mpsc::Receiver<Vec<u8>>>,
    /// Announces every read that finds nothing, so a test knows the
    /// burst is over before it feeds the next write.
    idle: Option<mpsc::Sender<()>>,
    ending: Option<ErrorKind>,
    exhausted: Option<mpsc::Sender<()>>,
    /// Runs on the reader thread at the start of each read, with the
    /// read's ordinal, so a test can act exactly between two chunks.
    before_read: Option<Box<dyn FnMut(usize) + Send>>,
    reads: usize,
    writer: RecordingWriter,
    child: Option<corcovado::Registration>,
    /// The other half of `child`, for `reregister` to re-fire readiness
    /// like a level triggered PTY would.
    readiness: Option<corcovado::SetReadiness>,
    registration_error: bool,
    read_event: bool,
    shutdown_calls: usize,
    deregister_calls: usize,
    raw_error: Option<i32>,
}

impl Read for TestPty {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reads += 1;
        if let Some(hook) = self.before_read.as_mut() {
            hook(self.reads);
        }
        let got = self.bytes.read(buf)?;
        if got != 0 {
            return Ok(got);
        }
        if let Some(Ok(next)) = self.feed.as_ref().map(|feed| feed.try_recv()) {
            self.bytes = io::Cursor::new(next);
            return self.bytes.read(buf);
        }
        if let Some(idle) = &self.idle {
            let _ = idle.send(());
        }
        if let Some(sender) = self.exhausted.take() {
            sender.send(()).unwrap();
        }
        if let Some(code) = self.raw_error {
            return Err(io::Error::from_raw_os_error(code));
        }
        match self.ending {
            Some(kind) => Err(io::Error::from(kind)),
            None => Ok(0),
        }
    }
}

/// Keeps every byte written and announces each write, so a test can
/// wait for a write instead of sleeping.
#[derive(Default)]
struct RecordingWriter {
    bytes: Vec<u8>,
    written: Option<mpsc::Sender<Vec<u8>>>,
    /// A full PTY: every write is refused with `WouldBlock`.
    blocked: bool,
    /// Every write attempt, refused or not.
    attempts: Option<mpsc::Sender<()>>,
}

impl Write for RecordingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Some(sender) = &self.attempts {
            let _ = sender.send(());
        }
        if self.blocked {
            return Err(io::Error::from(ErrorKind::WouldBlock));
        }
        self.bytes.extend_from_slice(buf);
        if let Some(sender) = &self.written {
            let _ = sender.send(buf.to_vec());
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl ProcessReadWrite for TestPty {
    type Reader = Self;
    type Writer = RecordingWriter;
    fn reader(&mut self) -> &mut Self {
        self
    }
    fn writer(&mut self) -> &mut Self::Writer {
        &mut self.writer
    }
    fn read_token(&self) -> corcovado::Token {
        1.into()
    }
    fn write_token(&self) -> corcovado::Token {
        1.into()
    }
    fn set_winsize(&mut self, _: WinsizeBuilder) -> io::Result<()> {
        Ok(())
    }
    fn register(
        &mut self,
        poll: &corcovado::Poll,
        _: &mut dyn Iterator<Item = corcovado::Token>,
        _: Ready,
        _: PollOpt,
    ) -> io::Result<()> {
        if self.registration_error {
            return Err(io::Error::from(ErrorKind::Other));
        }
        if let Some(child) = &self.child {
            #[cfg(unix)]
            let interest = Ready::readable() | Ready::from(UnixReady::hup());
            #[cfg(not(unix))]
            let interest = Ready::readable();
            poll.register(
                child,
                if self.read_event {
                    self.read_token()
                } else {
                    self.child_event_token()
                },
                interest,
                PollOpt::edge(),
            )?;
        }
        Ok(())
    }
    fn reregister(
        &mut self,
        _: &corcovado::Poll,
        interest: Ready,
        _: PollOpt,
    ) -> io::Result<()> {
        // A real PTY is level triggered: data still there when reading
        // is re-armed fires at once. The registration here is an edge,
        // so re-arm it by hand when bytes remain.
        if interest.is_readable()
            && self.bytes.position() < self.bytes.get_ref().len() as u64
        {
            if let Some(readiness) = &self.readiness {
                readiness.set_readiness(Ready::empty()).unwrap();
                readiness.set_readiness(Ready::readable()).unwrap();
            }
        }
        Ok(())
    }
    fn deregister(&mut self, _: &corcovado::Poll) -> io::Result<()> {
        self.deregister_calls += 1;
        Ok(())
    }
}

impl EventedPty for TestPty {
    fn shutdown(&mut self) -> io::Result<()> {
        self.shutdown_calls += 1;
        Ok(())
    }
    fn child_event_token(&self) -> corcovado::Token {
        2.into()
    }
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        Some(ChildEvent::Exited(Some(0)))
    }
}

fn machine(bytes: Vec<u8>, ending: Option<ErrorKind>) -> Machine<TestPty, VoidListener> {
    let terminal = Crosswords::new(
        CrosswordsSize::new(80, 24),
        CursorShape::Block,
        VoidListener,
        WindowId::from(0),
        0,
        0,
    );
    Machine::new(
        Arc::new(TerminalLock::new(terminal)),
        TestPty {
            bytes: io::Cursor::new(bytes),
            ending,
            feed: None,
            idle: None,
            exhausted: None,
            before_read: None,
            reads: 0,
            writer: RecordingWriter::default(),
            child: None,
            readiness: None,
            registration_error: false,
            read_event: false,
            shutdown_calls: 0,
            deregister_calls: 0,
            raw_error: None,
        },
        VoidListener,
        WindowId::from(0),
        0,
    )
    .unwrap()
}

fn first_line<L: EventListener>(machine: &Machine<TestPty, L>, len: usize) -> String {
    let terminal = machine.terminal.lock();
    (0..len)
        .map(|col| terminal.grid[Line(0)][Column(col)].c())
        .collect()
}

// Holding the terminal until the confirming read guarantees the first batch
// cannot be parsed before EOF/error. No timing assumptions or sleeps are needed.
fn read_with_contended_terminal(ending: Option<ErrorKind>, raw_error: Option<i32>) {
    let mut machine = machine(b"final output".to_vec(), ending);
    machine.pty.raw_error = raw_error;
    let terminal = Arc::clone(&machine.terminal);
    let guard = terminal.lock();
    let (sender, receiver) = mpsc::channel();
    machine.pty.exhausted = Some(sender);
    let worker = std::thread::spawn(move || {
        let result =
            machine.pty_read(&mut State::default(), &mut vec![0; READ_BUFFER_SIZE]);
        (machine, result)
    });
    let exhausted = receiver.recv_timeout(Duration::from_secs(5));
    drop(guard);
    let (machine, result) = worker.join().unwrap();
    exhausted.unwrap();
    if let Some(code) = raw_error {
        assert_eq!(result.unwrap_err().raw_os_error(), Some(code));
    } else {
        match ending {
            Some(kind) => assert_eq!(result.unwrap_err().kind(), kind),
            None => assert_eq!(result.unwrap(), ReadOutcome::Closed),
        }
    }
    assert_eq!(first_line(&machine, 12), "final output");
}

#[test]
fn eof_preserves_buffered_output_under_lock_contention() {
    read_with_contended_terminal(None, None);
}

#[test]
fn read_error_preserves_buffered_output_under_lock_contention() {
    read_with_contended_terminal(Some(ErrorKind::Other), None);
}

#[test]
fn final_output_can_be_drained_across_multiple_parse_budgets() {
    // NUL padding consumes parse budgets without scrolling the final marker.
    let mut bytes = vec![0; MAX_LOCKED_READ * 2 + 1];
    bytes.extend_from_slice(b"final output");
    let mut machine = machine(bytes, None);
    let mut state = State::default();
    let mut buf = vec![0; READ_BUFFER_SIZE];
    assert_eq!(
        machine.pty_read(&mut state, &mut buf).unwrap(),
        ReadOutcome::Budget
    );
    assert_eq!(
        machine.pty_read(&mut state, &mut buf).unwrap(),
        ReadOutcome::Budget
    );
    assert_eq!(
        machine.pty_read(&mut state, &mut buf).unwrap(),
        ReadOutcome::Closed
    );
    assert_eq!(first_line(&machine, 12), "final output");
}

#[test]
fn child_exit_drains_multiple_budgets_and_finishes_pending_sync() {
    let mut bytes = vec![0; MAX_LOCKED_READ * 2 + 1];
    bytes.extend_from_slice(b"\x1b[?2026hfinal output");
    // WouldBlock models a descendant retaining the slave after the child exits.
    let mut machine = machine(bytes, Some(ErrorKind::WouldBlock));
    let (registration, readiness) = corcovado::Registration::new2();
    machine.pty.child = Some(registration);
    readiness.set_readiness(Ready::readable()).unwrap();
    let (machine, _) = machine.spawn().join().unwrap();
    assert_eq!(first_line(&machine, 12), "final output");
    assert!(!machine.terminal.lock().synchronized_output());
    assert_eq!(machine.pty.shutdown_calls, 1);
    assert_eq!(machine.pty.deregister_calls, 1);
}

#[cfg(unix)]
#[test]
fn eio_preserves_buffered_output_under_lock_contention() {
    read_with_contended_terminal(None, Some(libc::EIO));
}

#[test]
fn registration_failure_shuts_down_pty() {
    let mut machine = machine(Vec::new(), None);
    machine.pty.registration_error = true;
    let (machine, _) = machine.spawn().join().unwrap();
    assert_eq!(machine.pty.shutdown_calls, 1);
    assert_eq!(machine.pty.deregister_calls, 1);
}

#[test]
fn read_failure_shuts_down_pty_and_finishes_pending_sync() {
    let mut machine =
        machine(b"\x1b[?2026hfinal output".to_vec(), Some(ErrorKind::Other));
    let (registration, readiness) = corcovado::Registration::new2();
    machine.pty.child = Some(registration);
    machine.pty.read_event = true;
    readiness.set_readiness(Ready::readable()).unwrap();
    let (machine, _) = machine.spawn().join().unwrap();
    assert_eq!(machine.pty.shutdown_calls, 1);
    assert_eq!(machine.pty.deregister_calls, 1);
    assert_eq!(first_line(&machine, 12), "final output");
    assert!(!machine.terminal.lock().synchronized_output());
}

#[derive(Clone)]
struct ExitObserver {
    terminal: Arc<std::sync::Mutex<std::sync::Weak<TerminalLock<Crosswords<Self>>>>>,
    observed: mpsc::Sender<String>,
}

impl EventListener for ExitObserver {
    fn send_event(&self, event: RioEvent, _: WindowId) {
        if matches!(event, RioEvent::ChildExited(..)) {
            let terminal = self.terminal.lock().unwrap().upgrade().unwrap();
            let terminal = terminal.lock();
            let text = (0..12)
                .map(|col| terminal.grid[Line(0)][Column(col)].c())
                .collect();
            self.observed.send(text).unwrap();
        }
    }
}

#[test]
fn pending_sync_is_visible_before_child_exited_notification() {
    let (observed, receiver) = mpsc::channel();
    let observer = ExitObserver {
        terminal: Arc::new(std::sync::Mutex::new(std::sync::Weak::new())),
        observed,
    };
    let terminal = Arc::new(TerminalLock::new(Crosswords::new(
        CrosswordsSize::new(80, 24),
        CursorShape::Block,
        observer.clone(),
        WindowId::from(0),
        0,
        0,
    )));
    *observer.terminal.lock().unwrap() = Arc::downgrade(&terminal);
    let mut pty = machine(
        b"\x1b[?2026hfinal output".to_vec(),
        Some(ErrorKind::WouldBlock),
    )
    .pty;
    let (registration, readiness) = corcovado::Registration::new2();
    pty.child = Some(registration);
    readiness.set_readiness(Ready::readable()).unwrap();
    let machine = Machine::new(terminal, pty, observer, WindowId::from(0), 0).unwrap();
    let worker = machine.spawn();
    assert_eq!(
        receiver.recv_timeout(Duration::from_secs(5)).unwrap(),
        "final output"
    );
    worker.join().unwrap();
}

#[cfg(unix)]
#[test]
fn hangup_without_readable_readiness_drains_residual_output() {
    let mut machine = machine(b"final output".to_vec(), Some(ErrorKind::Other));
    let (registration, readiness) = corcovado::Registration::new2();
    machine.pty.child = Some(registration);
    machine.pty.read_event = true;
    readiness.set_readiness(UnixReady::hup().into()).unwrap();
    let shutdown = machine.channel();
    let (completed, receiver) = mpsc::channel();
    let worker = machine.spawn();
    std::thread::spawn(move || completed.send(worker.join().unwrap()).unwrap());
    let result = receiver.recv_timeout(Duration::from_secs(5));
    if result.is_err() {
        // Unblock the worker even if HUP handling regresses.
        shutdown.send(Msg::Shutdown).unwrap();
    }
    let (machine, _) = result.expect("HUP must be handled without readable readiness");
    assert_eq!(first_line(&machine, 12), "final output");
    assert_eq!(machine.pty.shutdown_calls, 1);
    assert_eq!(machine.pty.deregister_calls, 1);
}

// The test pty never reports writable readiness, so a keystroke can
// only reach it on the wakeup that delivered it.
#[test]
fn input_is_written_on_the_wakeup_that_delivers_it() {
    let mut machine = machine(Vec::new(), None);
    let (sender, written) = mpsc::channel();
    machine.pty.writer.written = Some(sender);
    let channel = machine.channel();
    let handle = machine.spawn();

    channel
        .send(Msg::Input(std::borrow::Cow::Borrowed(b"ls\r")))
        .unwrap();
    assert_eq!(
        written.recv_timeout(Duration::from_secs(5)).unwrap(),
        b"ls\r".to_vec()
    );

    channel.send(Msg::Shutdown).unwrap();
    let (machine, _) = handle.join().unwrap();
    assert_eq!(machine.pty.writer.bytes, b"ls\r".to_vec());
}

// A read wakeup that finds nothing to parse never held the terminal,
// so it does not yield to a thread waiting for it: that thread waits
// on whoever does hold it, and sleeping here would only delay the
// input this thread is about to handle.
#[test]
fn an_idle_wakeup_does_not_yield_to_demand() {
    let terminal = Arc::new(TerminalLock::new(Crosswords::new(
        CrosswordsSize::new(80, 24),
        CursorShape::Block,
        VoidListener,
        WindowId::from(0),
        0,
        0,
    )));
    let mut machine = Machine::new(
        Arc::clone(&terminal),
        TestPty {
            bytes: io::Cursor::new(Vec::new()),
            feed: None,
            idle: None,
            ending: Some(ErrorKind::WouldBlock),
            exhausted: None,
            before_read: None,
            reads: 0,
            writer: RecordingWriter::default(),
            child: None,
            readiness: None,
            registration_error: false,
            read_event: false,
            shutdown_calls: 0,
            deregister_calls: 0,
            raw_error: None,
        },
        VoidListener,
        WindowId::from(0),
        0,
    )
    .unwrap();

    // Someone else holds the terminal and a third thread waits for it.
    let (release_tx, release) = mpsc::channel::<()>();
    let (held_tx, held) = mpsc::channel::<()>();
    let holder_terminal = Arc::clone(&terminal);
    let holder = std::thread::spawn(move || {
        let guard = holder_terminal.lock_reader();
        held_tx.send(()).unwrap();
        release.recv().unwrap();
        drop(guard);
    });
    held.recv_timeout(Duration::from_secs(5)).unwrap();
    let demander_terminal = Arc::clone(&terminal);
    let demander = std::thread::spawn(move || drop(demander_terminal.lock()));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !terminal.has_demand() {
        assert!(std::time::Instant::now() < deadline, "no demand appeared");
        std::thread::yield_now();
    }

    let mut state = State::default();
    let mut buf = vec![0; READ_BUFFER_SIZE];
    let outcome = machine.pty_read(&mut state, &mut buf).unwrap();
    assert_eq!(outcome, ReadOutcome::Idle);
    assert_eq!(
        terminal.handoff_timeouts(),
        0,
        "the idle wakeup did not sleep for the demander"
    );

    release_tx.send(()).unwrap();
    holder.join().unwrap();
    demander.join().unwrap();
}

// Bytes refused by a full PTY wait for the writable event. Read
// wakeups in between do not retry them, and a reply the parse produces
// meanwhile queues behind them.
#[test]
fn a_stuck_write_is_not_retried_on_read_wakeups() {
    let (events, damaged) = mpsc::channel();
    let (registration, readiness) = corcovado::Registration::new2();
    let (attempts_tx, attempts) = mpsc::channel();
    let terminal = Crosswords::new(
        CrosswordsSize::new(80, 24),
        CursorShape::Block,
        EventRecorder(events.clone()),
        WindowId::from(0),
        0,
        0,
    );
    let machine = Machine::new(
        Arc::new(TerminalLock::new(terminal)),
        TestPty {
            // A query among the output: its reply must wait too.
            bytes: io::Cursor::new(b"out\x1b[cput".to_vec()),
            ending: None,
            feed: None,
            idle: None,
            exhausted: None,
            before_read: None,
            reads: 0,
            writer: RecordingWriter {
                blocked: true,
                attempts: Some(attempts_tx),
                ..RecordingWriter::default()
            },
            child: Some(registration),
            readiness: None,
            registration_error: false,
            read_event: true,
            shutdown_calls: 0,
            deregister_calls: 0,
            raw_error: None,
        },
        EventRecorder(events),
        WindowId::from(0),
        0,
    )
    .unwrap();
    let channel = machine.channel();
    let handle = machine.spawn();
    let wait = |rx: &mpsc::Receiver<()>| rx.recv_timeout(Duration::from_secs(5)).unwrap();

    // The keystroke is attempted on its wakeup and refused.
    channel
        .send(Msg::Input(std::borrow::Cow::Borrowed(b"a")))
        .unwrap();
    wait(&attempts);

    // A read wakeup: output arrives and is parsed, nothing is written.
    readiness.set_readiness(Ready::readable()).unwrap();
    assert_eq!(
        damaged.recv_timeout(Duration::from_secs(5)).unwrap(),
        "damaged"
    );

    channel.send(Msg::Shutdown).unwrap();
    let (machine, state) = handle.join().unwrap();
    assert!(
        attempts.try_recv().is_err(),
        "the read wakeup must not have retried the stuck bytes"
    );
    assert!(
        state.needs_write(),
        "the bytes are still queued for the writable event"
    );
    assert!(machine.pty.writer.bytes.is_empty());
    // The keystroke is still first in line, the reply behind it.
    assert_eq!(state.writing.as_ref().unwrap().source.as_ref(), b"a");
    assert_eq!(
        state.write_list.iter().collect::<Vec<_>>(),
        vec![&std::borrow::Cow::Borrowed(&b"\x1b[?62;4;6;22;52c"[..])],
        "the reply queued behind the stuck keystroke"
    );
}

// A query from the program is answered by this thread, written to the
// PTY right after the parse that produced it, and never sent to the
// event loop.
#[test]
fn a_parser_reply_is_written_by_the_reader_thread() {
    let (events, recorded) = mpsc::channel();
    let (registration, readiness) = corcovado::Registration::new2();
    let (written_tx, written) = mpsc::channel();
    let terminal = Crosswords::new(
        CrosswordsSize::new(80, 24),
        CursorShape::Block,
        EventRecorder(events.clone()),
        WindowId::from(0),
        0,
        0,
    );
    let machine = Machine::new(
        Arc::new(TerminalLock::new(terminal)),
        TestPty {
            // Primary device attributes, then text.
            bytes: io::Cursor::new(b"\x1b[cok".to_vec()),
            ending: None,
            feed: None,
            idle: None,
            exhausted: None,
            before_read: None,
            reads: 0,
            writer: RecordingWriter {
                written: Some(written_tx),
                ..RecordingWriter::default()
            },
            child: Some(registration),
            readiness: None,
            registration_error: false,
            read_event: true,
            shutdown_calls: 0,
            deregister_calls: 0,
            raw_error: None,
        },
        EventRecorder(events),
        WindowId::from(0),
        0,
    )
    .unwrap();
    let channel = machine.channel();
    let handle = machine.spawn();

    readiness.set_readiness(Ready::readable()).unwrap();
    let reply = written.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(reply, b"\x1b[?62;4;6;22;52c".to_vec());

    channel.send(Msg::Shutdown).unwrap();
    let (machine, _) = handle.join().unwrap();
    assert_eq!(first_line(&machine, 2), "ok");
    let names: Vec<&str> = recorded.try_iter().collect();
    assert!(
        !names.contains(&"ptywrite"),
        "the reply never went to the event loop: {names:?}"
    );
}

/// What the waiter against a pile observed.
struct PileWaiter {
    /// The first cell when it held the terminal: the last parsed chunk.
    saw: char,
    /// From asking for the terminal to holding it.
    waited: Duration,
    /// Replies still queued in the terminal when it held it.
    queued_replies: usize,
    /// Whether the renderer had been told of the damage so far by then.
    damage_announced: bool,
    /// Whether a handoff gave up on the waiter within the bound, which
    /// only happens on an overloaded machine.
    timed_out: bool,
    /// The first line once the whole pile is parsed.
    first_line: String,
    /// Everything the reader wrote to the PTY.
    written: Vec<u8>,
}

type SharedTerminal = Arc<TerminalLock<Crosswords<OnTitle>>>;

/// Runs an action on its own thread the moment the parser reports a
/// title, that is while the reader holds the terminal, and returns once
/// that thread counts as demand. A waiter started this way cannot win
/// the lock outright; it gets it when the reader hands it over.
#[derive(Clone)]
struct OnTitle {
    terminal: Arc<std::sync::OnceLock<SharedTerminal>>,
    action: Arc<dyn Fn(SharedTerminal) + Send + Sync>,
    spawned: Arc<std::sync::atomic::AtomicBool>,
}

impl OnTitle {
    fn new(action: impl Fn(SharedTerminal) + Send + Sync + 'static) -> OnTitle {
        OnTitle {
            terminal: Arc::new(std::sync::OnceLock::new()),
            action: Arc::new(action),
            spawned: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// The terminal built with this listener, for the action to lock.
    fn watches(&self, terminal: &SharedTerminal) {
        self.terminal
            .set(Arc::clone(terminal))
            .ok()
            .expect("set once");
    }
}

impl EventListener for OnTitle {
    fn send_event(&self, event: RioEvent, _: WindowId) {
        if !matches!(event, RioEvent::Title(..))
            || self.spawned.swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        let terminal = Arc::clone(self.terminal.get().expect("set after construction"));
        let action = Arc::clone(&self.action);
        let probe = Arc::clone(&terminal);
        std::thread::spawn(move || action(terminal));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !probe.has_demand() {
            assert!(std::time::Instant::now() < deadline, "no waiter appeared");
            std::thread::yield_now();
        }
    }
}

/// A pile of `chunks` reads: every chunk homes the cursor and writes
/// its ordinal, padded with attribute resets and then NULs (which the
/// parser ignores) to exactly one read; the first chunk also carries a
/// DA query and sets the title that summons the waiter, and `extra`
/// goes into the chunk of that number.
fn pile(chunks: u8, extra: Option<(u8, &[u8])>) -> Vec<u8> {
    let mut payload = Vec::new();
    for n in 1..=chunks {
        payload.extend_from_slice(b"\x1b[H");
        // '1'..'9', then the ASCII characters after '9' for more.
        payload.push(b'0' + n);
        if n == 1 {
            payload.extend_from_slice(b"\x1b[c\x1b]0;pile\x1b\\");
        }
        if let Some((at, bytes)) = extra {
            if at == n {
                payload.extend_from_slice(bytes);
            }
        }
        while payload.len() + 4 <= READ_CHUNK * n as usize {
            payload.extend_from_slice(b"\x1b[0m");
        }
        payload.resize(READ_CHUNK * n as usize, 0);
    }
    payload
}

/// A burst of `chunks` reads piles up while another thread holds the
/// terminal; the holder lets go once the pile is complete, the reader
/// takes the terminal and starts on the pile, and a waiter appears
/// while it parses the first chunk. Every chunk homes the cursor and
/// writes its ordinal, padded with attribute resets, so the first cell
/// names the last parsed chunk; the first chunk also carries a DA
/// query, whose reply must leave with the reader, and sets the title
/// that summons the waiter.
fn waiter_against_a_pile(chunks: u8) -> PileWaiter {
    // A pile that fills the reader's buffer would force the lock before
    // the read hook below releases the holder.
    assert!(chunks as usize <= READ_BUFFER_SIZE / READ_CHUNK);
    let payload = pile(chunks, None);
    let (saw_tx, saw) = mpsc::channel();
    let listener = OnTitle::new(move |terminal| {
        let asked = std::time::Instant::now();
        let mut guard = terminal.lock();
        let waited = asked.elapsed();
        let mut queued = 0;
        guard.drain_pty_replies(|_| queued += 1);
        saw_tx
            .send((
                guard.grid[Line(0)][Column(0)].c(),
                waited,
                queued,
                guard.damage_event_in_flight,
            ))
            .unwrap();
    });
    let terminal = Arc::new(TerminalLock::new(Crosswords::new(
        CrosswordsSize::new(80, 24),
        CursorShape::Block,
        listener.clone(),
        WindowId::from(0),
        0,
        0,
    )));
    listener.watches(&terminal);

    // The holder keeps the terminal busy, unfairly like a renderer that
    // already has it, until the read after the pile. It must not start
    // before the machine, whose constructor takes the terminal lock,
    // is built.
    let (release_tx, release) = mpsc::channel::<()>();
    let before_read = move |read: usize| {
        if read == chunks as usize + 1 {
            release_tx.send(()).unwrap();
        }
    };
    let mut machine = Machine::new(
        Arc::clone(&terminal),
        TestPty {
            bytes: io::Cursor::new(payload),
            ending: None,
            exhausted: None,
            feed: None,
            idle: None,
            before_read: Some(Box::new(before_read)),
            reads: 0,
            writer: RecordingWriter::default(),
            child: None,
            readiness: None,
            registration_error: false,
            read_event: false,
            shutdown_calls: 0,
            deregister_calls: 0,
            raw_error: None,
        },
        listener,
        WindowId::from(0),
        0,
    )
    .unwrap();
    let mut state = State::default();
    // Room for the pile and one more read: a full buffer would force
    // the lock before the hook that releases the holder runs.
    let mut buf = vec![0; READ_BUFFER_SIZE + READ_CHUNK];

    let (held_tx, held) = mpsc::channel::<()>();
    let holder_terminal = Arc::clone(&terminal);
    let holder = std::thread::spawn(move || {
        let guard = holder_terminal.lock_reader();
        held_tx.send(()).unwrap();
        release.recv().unwrap();
        drop(guard);
    });
    held.recv_timeout(Duration::from_secs(5)).unwrap();

    // The event loop's part, in place: wait out each handoff, then take
    // up the rest of the pile.
    let outcome = loop {
        let outcome = machine.pty_read(&mut state, &mut buf).unwrap();
        if state.handoff.is_some() {
            terminal.yield_to_demand();
            machine.end_handoff(&mut state);
        }
        if state.carry.is_none() {
            break outcome;
        }
    };
    assert_eq!(outcome, ReadOutcome::Closed, "the whole pile was read");
    holder.join().unwrap();
    let (saw, waited, queued_replies, damage_announced) =
        saw.recv_timeout(Duration::from_secs(5)).unwrap();
    PileWaiter {
        saw,
        waited,
        queued_replies,
        damage_announced,
        timed_out: terminal.handoff_timeouts() > 0,
        first_line: first_line(&machine, 1),
        written: machine.pty.writer.bytes.clone(),
    }
}

// Bytes that piled up while another thread held the terminal are
// parsed a chunk at a time once it is free: a thread that asks for the
// terminal while the first chunk is parsed gets it right after that
// chunk, not after the whole pile, and the rest of the pile is parsed
// afterwards. The reply of the first chunk left with the reader and
// the renderer was told of its damage before the reader let go.
//
// A handoff that hit its bound is repeated: a machine too loaded to
// schedule the waiter within a millisecond does that now and then, a
// bug does it every time.
#[test]
fn a_waiting_thread_gets_the_terminal_after_the_first_chunk_of_a_pile() {
    for _ in 0..10 {
        let waiter = waiter_against_a_pile(4);
        assert_eq!(waiter.first_line, "4", "the rest of the pile followed");
        assert_eq!(
            waiter.written,
            b"\x1b[?62;4;6;22;52c".to_vec(),
            "the reader wrote the reply itself"
        );
        if waiter.timed_out {
            continue;
        }
        assert_eq!(
            waiter.saw, '1',
            "the waiter got the terminal right after the first chunk"
        );
        assert_eq!(
            waiter.queued_replies, 0,
            "no reply was left for the waiter to drain"
        );
        assert!(
            waiter.damage_announced,
            "the renderer was told of the first chunk"
        );
        return;
    }
    panic!("ten piles and every handoff hit its bound");
}

/// A spawned reader over `payload`, with `listener` as the terminal's
/// owner, every PTY write announced on the returned channel, and, when
/// `piled` is given, a holder that keeps the terminal until the read
/// after that many chunks so those reads pile up.
struct SpawnedReader {
    terminal: SharedTerminal,
    channel: channel::Sender<Msg>,
    readiness: corcovado::SetReadiness,
    written: mpsc::Receiver<Vec<u8>>,
    handle: JoinHandle<(Machine<TestPty, OnTitle>, State)>,
}

fn spawn_reader(payload: Vec<u8>, piled: Option<u8>, listener: OnTitle) -> SpawnedReader {
    let (registration, readiness) = corcovado::Registration::new2();
    let (written_tx, written) = mpsc::channel();
    let terminal = Arc::new(TerminalLock::new(Crosswords::new(
        CrosswordsSize::new(80, 24),
        CursorShape::Block,
        listener.clone(),
        WindowId::from(0),
        0,
        0,
    )));
    listener.watches(&terminal);
    let (release_tx, release) = mpsc::channel::<()>();
    let before_read: Option<Box<dyn FnMut(usize) + Send>> = piled.map(|piled| {
        Box::new(move |read: usize| {
            if read == piled as usize + 1 {
                release_tx.send(()).unwrap();
            }
        }) as Box<dyn FnMut(usize) + Send>
    });
    let machine = Machine::new(
        Arc::clone(&terminal),
        TestPty {
            bytes: io::Cursor::new(payload),
            feed: None,
            idle: None,
            ending: Some(ErrorKind::WouldBlock),
            exhausted: None,
            before_read,
            reads: 0,
            writer: RecordingWriter {
                written: Some(written_tx),
                ..RecordingWriter::default()
            },
            child: Some(registration),
            readiness: Some(readiness.clone()),
            registration_error: false,
            read_event: true,
            shutdown_calls: 0,
            deregister_calls: 0,
            raw_error: None,
        },
        listener,
        WindowId::from(0),
        0,
    )
    .unwrap();
    let channel = machine.channel();
    if piled.is_some() {
        // The holder keeps the terminal busy, unfairly like a renderer
        // that already has it, until the read after the pile; it starts
        // after the machine, whose constructor takes the lock.
        let (held_tx, held) = mpsc::channel::<()>();
        let holder_terminal = Arc::clone(&terminal);
        std::thread::spawn(move || {
            let guard = holder_terminal.lock_reader();
            held_tx.send(()).unwrap();
            release.recv().unwrap();
            drop(guard);
        });
        held.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    let handle = machine.spawn();
    SpawnedReader {
        terminal,
        channel,
        readiness,
        written,
        handle,
    }
}

// While the reader waits for a thread it handed the terminal to, it
// keeps serving its channel: a keystroke that arrives meanwhile is
// written to the PTY before that thread lets go, and the wait does not
// reach its bound on the keystroke's account.
#[test]
fn input_is_written_while_a_demander_holds_the_terminal() {
    // One chunk that sets the title; the demander it summons holds the
    // terminal until told to let go.
    let payload = b"\x1b[H1\x1b]0;hold\x1b\\".to_vec();
    let (held_tx, held) = mpsc::channel::<()>();
    let (go_tx, go) = mpsc::channel::<()>();
    let go = std::sync::Mutex::new(Some(go));
    let listener = OnTitle::new(move |terminal| {
        let guard = terminal.lock();
        held_tx.send(()).unwrap();
        if let Some(go) = go.lock().unwrap().take() {
            go.recv().unwrap();
        }
        drop(guard);
    });
    let reader = spawn_reader(payload, None, listener);
    let timeout = Duration::from_secs(5);

    reader.readiness.set_readiness(Ready::readable()).unwrap();
    held.recv_timeout(timeout).unwrap();
    // The demander holds the terminal and the reader waits for it.
    reader
        .channel
        .send(Msg::Input(std::borrow::Cow::Borrowed(b"k")))
        .unwrap();
    assert_eq!(reader.written.recv_timeout(timeout).unwrap(), b"k".to_vec());
    assert_eq!(
        reader.terminal.handoff_timeouts(),
        0,
        "the keystroke went out during the handoff, not after its bound"
    );
    go_tx.send(()).unwrap();

    reader.channel.send(Msg::Shutdown).unwrap();
    let (machine, _) = reader.handle.join().unwrap();
    assert_eq!(machine.pty.writer.bytes, b"k".to_vec());
}

// A thread that takes the terminal during a mid-pile handoff and
// parses bytes of its own, as an embedder injecting output does, sends
// their replies through the reader's channel; they are written before
// the reader goes on with the pile, so the PTY sees every reply in the
// order the bytes were parsed.
#[test]
fn an_injected_reply_during_a_handoff_keeps_its_place() {
    // DA in chunk 1 (always), DSR injected during the handoff after
    // chunk 1, DA again in chunk 3.
    let payload = pile(4, Some((3, b"\x1b[c")));
    let channel_slot: Arc<std::sync::OnceLock<channel::Sender<Msg>>> =
        Arc::new(std::sync::OnceLock::new());
    let listener = {
        let channel_slot = Arc::clone(&channel_slot);
        OnTitle::new(move |terminal| {
            let mut guard = terminal.lock();
            let mut processor = handler::Processor::default();
            processor.advance(&mut *guard, b"\x1b[6n");
            let channel = channel_slot.get().expect("set after spawn");
            guard.drain_pty_replies(|reply| {
                channel
                    .send(Msg::Input(std::borrow::Cow::Owned(reply.into_bytes())))
                    .unwrap()
            });
        })
    };
    let reader = spawn_reader(payload, Some(4), listener);
    assert!(channel_slot.set(reader.channel.clone()).is_ok(), "set once");
    let timeout = Duration::from_secs(5);

    reader.readiness.set_readiness(Ready::readable()).unwrap();
    let mut writes = Vec::new();
    while writes.len() < 3 {
        writes.push(reader.written.recv_timeout(timeout).unwrap());
    }
    assert_eq!(
        writes,
        vec![
            b"\x1b[?62;4;6;22;52c".to_vec(),
            b"\x1b[1;2R".to_vec(),
            b"\x1b[?62;4;6;22;52c".to_vec(),
        ],
        "chunk 1's DA, the injected DSR, chunk 3's DA"
    );

    reader.channel.send(Msg::Shutdown).unwrap();
    let (machine, _) = reader.handle.join().unwrap();
    assert_eq!(first_line(&machine, 1), "4", "the whole pile was parsed");
}

// A handoff that ends with nothing carried over, the common case of a
// burst cut between two reads, hands the PTY's data back to the reader:
// reading is re-armed before the poll, so the bytes still waiting in
// the PTY wake it. Without that the reader would poll for nothing,
// forever, with the child blocked on a full PTY.
#[test]
fn reading_resumes_after_a_handoff_that_carried_nothing() {
    // Two reads: the first sets the title and summons a demander that
    // holds the terminal briefly; the second carries a DA whose reply
    // shows it was read.
    let mut payload = b"\x1b[H1\x1b]0;cut\x1b\\".to_vec();
    payload.resize(READ_CHUNK, 0);
    payload.extend_from_slice(b"\x1b[H2\x1b[c");
    let listener = OnTitle::new(|terminal| {
        let guard = terminal.lock();
        std::thread::sleep(Duration::from_micros(200));
        drop(guard);
    });
    let reader = spawn_reader(payload, None, listener);
    let timeout = Duration::from_secs(5);

    reader.readiness.set_readiness(Ready::readable()).unwrap();
    assert_eq!(
        reader.written.recv_timeout(timeout).unwrap(),
        b"\x1b[?62;4;6;22;52c".to_vec(),
        "the second read was taken after the handoff"
    );

    reader.channel.send(Msg::Shutdown).unwrap();
    let (machine, _) = reader.handle.join().unwrap();
    assert_eq!(first_line(&machine, 1), "2");
}

// How long a renderer waits when it asks for the terminal while a
// 1 MiB pile is parsed. Run with
// `cargo test -p rio-vt --release -- --ignored --nocapture pile_wait`.
#[test]
#[ignore]
fn pile_wait_latency() {
    let mut waits = Vec::new();
    let mut timed_out = 0;
    while waits.len() < 20 {
        let waiter = waiter_against_a_pile(16);
        if waiter.timed_out {
            timed_out += 1;
        } else {
            assert_eq!(waiter.saw, '1');
            waits.push(waiter.waited);
        }
    }
    waits.sort();
    println!(
        "1 MiB pile, waiter wait: median {:?}, min {:?}, max {:?} ({timed_out} handoffs hit the bound)",
        waits[waits.len() / 2],
        waits[0],
        waits[waits.len() - 1]
    );
}

// A thread that asks for the terminal mid burst gets it after the
// chunk being parsed, not after the whole burst, and the remaining
// chunks are parsed on the next wakeup.
#[test]
fn a_waiting_thread_gets_the_terminal_after_the_current_chunk() {
    // Four chunks of one read each; every chunk homes the cursor,
    // writes its number and pads with attribute resets that change
    // nothing, so the first cell names the last parsed chunk.
    let mut payload = Vec::new();
    for n in 1..=4u8 {
        payload.extend_from_slice(b"\x1b[H");
        payload.push(b'0' + n);
        while payload.len() < READ_CHUNK * n as usize {
            payload.extend_from_slice(b"\x1b[0m");
        }
        assert_eq!(payload.len(), READ_CHUNK * n as usize);
    }
    let terminal = Arc::new(TerminalLock::new(Crosswords::new(
        CrosswordsSize::new(80, 24),
        CursorShape::Block,
        VoidListener,
        WindowId::from(0),
        0,
        0,
    )));
    let waiter_terminal = Arc::clone(&terminal);
    let (saw_tx, saw) = mpsc::channel();
    let before_read = move |read: usize| {
        if read != 3 {
            return;
        }
        // The reader thread holds the lock since chunk 1; this thread
        // now waits for it, as a renderer would for its frame.
        let terminal = Arc::clone(&waiter_terminal);
        let saw_tx = saw_tx.clone();
        std::thread::spawn(move || {
            let guard = terminal.lock();
            saw_tx.send(guard.grid[Line(0)][Column(0)].c()).unwrap();
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !waiter_terminal.has_demand() {
            assert!(std::time::Instant::now() < deadline, "no waiter appeared");
            std::thread::yield_now();
        }
    };
    let mut machine = Machine::new(
        Arc::clone(&terminal),
        TestPty {
            bytes: io::Cursor::new(payload),
            ending: None,
            exhausted: None,
            feed: None,
            idle: None,
            before_read: Some(Box::new(before_read)),
            reads: 0,
            writer: RecordingWriter::default(),
            child: None,
            readiness: None,
            registration_error: false,
            read_event: false,
            shutdown_calls: 0,
            deregister_calls: 0,
            raw_error: None,
        },
        VoidListener,
        WindowId::from(0),
        0,
    )
    .unwrap();
    let mut state = State::default();
    let mut buf = vec![0; READ_BUFFER_SIZE];

    let outcome = machine.pty_read(&mut state, &mut buf).unwrap();
    assert_eq!(outcome, ReadOutcome::Budget, "the burst stopped early");
    assert!(
        state.handoff.is_some(),
        "and the reader waits for the handoff"
    );
    assert_eq!(
        saw.recv_timeout(Duration::from_secs(5)).unwrap(),
        '3',
        "the waiter got the terminal right after the chunk being parsed"
    );
    terminal.yield_to_demand();
    machine.end_handoff(&mut state);

    let outcome = machine.pty_read(&mut state, &mut buf).unwrap();
    assert_eq!(outcome, ReadOutcome::Closed);
    assert_eq!(first_line(&machine, 1), "4", "the rest followed");
}

#[cfg(target_os = "macos")]
#[test]
fn the_reader_thread_serves_the_user() {
    let class = std::thread::spawn(|| {
        raise_io_thread_priority();
        let mut class = libc::qos_class_t::QOS_CLASS_UNSPECIFIED;
        // SAFETY: queries the calling thread's own QoS class into a
        // local; the relative priority out-parameter may be null.
        unsafe {
            libc::pthread_get_qos_class_np(
                libc::pthread_self(),
                &mut class,
                std::ptr::null_mut(),
            );
        }
        class
    })
    .join()
    .unwrap();
    assert_eq!(
        class as u32,
        libc::qos_class_t::QOS_CLASS_USER_INITIATED as u32
    );
}

#[test]
fn explicit_shutdown_finalizes_once() {
    let machine = machine(Vec::new(), None);
    machine.channel().send(Msg::Shutdown).unwrap();
    let (machine, _) = machine.spawn().join().unwrap();
    assert_eq!(machine.pty.shutdown_calls, 1);
    assert_eq!(machine.pty.deregister_calls, 1);
}

// A query only the owner answers, with the DA fence in a separate
// write: the program sends `CSI 14 t`, then `CSI c` once the first is
// flushed, and expects the size report before the attributes. The DA
// arrives in its own burst while the owner has not answered yet, so it
// goes through the owner too; the owner's answer settles the query and
// the next DA is written here again. The PTY sees the answer first.
#[test]
fn a_fence_in_a_later_write_waits_for_the_owners_answer() {
    let (events, recorded) = mpsc::channel();
    let (registration, readiness) = corcovado::Registration::new2();
    let (written_tx, written) = mpsc::channel();
    let (feed_tx, feed) = mpsc::channel();
    let (idle_tx, idle) = mpsc::channel();
    let terminal = Crosswords::new(
        CrosswordsSize::new(80, 24),
        CursorShape::Block,
        EventRecorder(events.clone()),
        WindowId::from(0),
        0,
        0,
    );
    let machine = Machine::new(
        Arc::new(TerminalLock::new(terminal)),
        TestPty {
            bytes: io::Cursor::new(b"\x1b[14t".to_vec()),
            feed: Some(feed),
            idle: Some(idle_tx),
            before_read: None,
            reads: 0,
            ending: Some(ErrorKind::WouldBlock),
            exhausted: None,
            writer: RecordingWriter {
                written: Some(written_tx),
                ..RecordingWriter::default()
            },
            child: Some(registration),
            readiness: None,
            registration_error: false,
            read_event: true,
            shutdown_calls: 0,
            deregister_calls: 0,
            raw_error: None,
        },
        EventRecorder(events),
        WindowId::from(0),
        0,
    )
    .unwrap();
    let channel = machine.channel();
    let handle = machine.spawn();
    let timeout = Duration::from_secs(5);
    let next_named = |name: &str| loop {
        if recorded.recv_timeout(timeout).unwrap() == name {
            break;
        }
    };
    let wake = || {
        readiness.set_readiness(Ready::empty()).unwrap();
        readiness.set_readiness(Ready::readable()).unwrap();
    };
    // The reader found nothing more: that burst is over.
    let burst_over = || idle.recv_timeout(timeout).unwrap();

    // First write: the query goes to the owner.
    wake();
    next_named("size");
    burst_over();

    // Second write, its own burst, before the owner answered: the DA
    // goes to the owner too, and nothing is written to the PTY.
    feed_tx.send(b"\x1b[c".to_vec()).unwrap();
    wake();
    next_named("ptywrite");
    burst_over();
    assert!(
        written.try_recv().is_err(),
        "the DA must not be written ahead of the size report"
    );

    // The owner answers: its answer is written, and the fence lifts.
    channel
        .send(Msg::Reply(std::borrow::Cow::Borrowed(b"\x1b[4;480;640t")))
        .unwrap();
    assert_eq!(written.recv_timeout(timeout).unwrap(), b"\x1b[4;480;640t");

    // Third write: answered here again.
    feed_tx.send(b"\x1b[c".to_vec()).unwrap();
    wake();
    assert_eq!(
        written.recv_timeout(timeout).unwrap(),
        b"\x1b[?62;4;6;22;52c".to_vec()
    );
    burst_over();

    channel.send(Msg::Shutdown).unwrap();
    let (machine, _) = handle.join().unwrap();
    assert_eq!(
        machine.pty.writer.bytes,
        b"\x1b[4;480;640t\x1b[?62;4;6;22;52c".to_vec(),
        "the size report reached the PTY before any attributes"
    );
    assert_eq!(
        recorded.try_iter().filter(|n| *n == "ptywrite").count(),
        0,
        "the last DA was not routed through the owner"
    );
}

// An owner that declines a query still settles it: an empty answer
// writes nothing and lifts the fence.
#[test]
fn a_declined_query_lifts_the_fence_without_writing() {
    let (events, recorded) = mpsc::channel();
    let (registration, readiness) = corcovado::Registration::new2();
    let (written_tx, written) = mpsc::channel();
    let (feed_tx, feed) = mpsc::channel();
    let (idle_tx, idle) = mpsc::channel();
    let terminal = Crosswords::new(
        CrosswordsSize::new(80, 24),
        CursorShape::Block,
        EventRecorder(events.clone()),
        WindowId::from(0),
        0,
        0,
    );
    let machine = Machine::new(
        Arc::new(TerminalLock::new(terminal)),
        TestPty {
            bytes: io::Cursor::new(b"\x1b[14t".to_vec()),
            feed: Some(feed),
            idle: Some(idle_tx),
            before_read: None,
            reads: 0,
            ending: Some(ErrorKind::WouldBlock),
            exhausted: None,
            writer: RecordingWriter {
                written: Some(written_tx),
                ..RecordingWriter::default()
            },
            child: Some(registration),
            readiness: None,
            registration_error: false,
            read_event: true,
            shutdown_calls: 0,
            deregister_calls: 0,
            raw_error: None,
        },
        EventRecorder(events),
        WindowId::from(0),
        0,
    )
    .unwrap();
    let channel = machine.channel();
    let handle = machine.spawn();
    let timeout = Duration::from_secs(5);

    readiness.set_readiness(Ready::readable()).unwrap();
    while recorded.recv_timeout(timeout).unwrap() != "size" {}
    idle.recv_timeout(timeout).unwrap();
    channel
        .send(Msg::Reply(std::borrow::Cow::Borrowed(b"")))
        .unwrap();

    feed_tx.send(b"\x1b[c".to_vec()).unwrap();
    readiness.set_readiness(Ready::empty()).unwrap();
    readiness.set_readiness(Ready::readable()).unwrap();
    assert_eq!(
        written.recv_timeout(timeout).unwrap(),
        b"\x1b[?62;4;6;22;52c".to_vec()
    );

    channel.send(Msg::Shutdown).unwrap();
    let (machine, _) = handle.join().unwrap();
    assert_eq!(machine.pty.writer.bytes, b"\x1b[?62;4;6;22;52c".to_vec());
}

#[derive(Clone)]
struct EventRecorder(mpsc::Sender<&'static str>);

impl EventListener for EventRecorder {
    fn send_event(&self, event: RioEvent, _: WindowId) {
        let name = match event {
            RioEvent::TerminalDamaged(_) => "damaged",
            RioEvent::ChildExited(..) => "exited",
            RioEvent::PtyWrite(..) => "ptywrite",
            RioEvent::TextAreaSizeRequest(..) => "size",
            _ => return,
        };
        let _ = self.0.send(name);
    }
}

/// A hold still open when the child exits is ended and announced as
/// damage, so the renderer repaints the panel it had stopped drawing.
#[test]
fn pending_sync_at_exit_is_announced_as_damage() {
    let (sender, receiver) = mpsc::channel();
    let recorder = EventRecorder(sender);
    let terminal = Arc::new(TerminalLock::new(Crosswords::new(
        CrosswordsSize::new(80, 24),
        CursorShape::Block,
        recorder.clone(),
        WindowId::from(0),
        0,
        0,
    )));
    let mut pty = machine(
        b"\x1b[?2026hfinal output".to_vec(),
        Some(ErrorKind::WouldBlock),
    )
    .pty;
    let (registration, readiness) = corcovado::Registration::new2();
    pty.child = Some(registration);
    readiness.set_readiness(Ready::readable()).unwrap();
    let machine =
        Machine::new(terminal.clone(), pty, recorder, WindowId::from(0), 0).unwrap();
    machine.spawn().join().unwrap();

    let events: Vec<_> = receiver.try_iter().collect();
    let damaged = events.iter().position(|e| *e == "damaged");
    let exited = events.iter().position(|e| *e == "exited");
    assert!(damaged.is_some(), "events: {events:?}");
    assert!(damaged < exited, "events: {events:?}");
    assert!(!terminal.lock().synchronized_output());
}

type RecordingMachine = (
    Machine<TestPty, EventRecorder>,
    mpsc::Receiver<&'static str>,
    Arc<TerminalLock<Crosswords<EventRecorder>>>,
);

fn recording_machine(bytes: Vec<u8>, ending: Option<ErrorKind>) -> RecordingMachine {
    let (sender, receiver) = mpsc::channel();
    let recorder = EventRecorder(sender);
    let terminal = Arc::new(TerminalLock::new(Crosswords::new(
        CrosswordsSize::new(80, 24),
        CursorShape::Block,
        recorder.clone(),
        WindowId::from(0),
        0,
        0,
    )));
    let pty = machine(bytes, ending).pty;
    let machine =
        Machine::new(terminal.clone(), pty, recorder, WindowId::from(0), 0).unwrap();
    (machine, receiver, terminal)
}

/// Reads inside a hold do not wake the renderer; the read that ends it
/// does, even when it changes no cells.
#[test]
fn sync_hold_suppresses_damage_until_esu() {
    let (mut machine, events, terminal) = recording_machine(
        b"\x1b[?2026h\x1b[1;1Hheld".to_vec(),
        Some(ErrorKind::WouldBlock),
    );
    let mut state = State::default();
    let mut buf = vec![0; READ_BUFFER_SIZE];

    machine.pty_read(&mut state, &mut buf).unwrap();
    assert!(terminal.lock().synchronized_output());
    assert_eq!(events.try_iter().collect::<Vec<_>>(), Vec::<&str>::new());

    machine.pty.bytes = io::Cursor::new(b"\x1b[?2026l".to_vec());
    machine.pty_read(&mut state, &mut buf).unwrap();
    assert!(!terminal.lock().synchronized_output());
    assert_eq!(events.try_iter().collect::<Vec<_>>(), vec!["damaged"]);
}

/// A hold that never ends is ended by the event loop once its deadline
/// passes, and the renderer is woken.
#[test]
fn sync_hold_times_out_and_wakes_the_renderer() {
    let (mut machine, events, terminal) = recording_machine(
        b"\x1b[?2026h\x1b[1;1Hheld".to_vec(),
        Some(ErrorKind::WouldBlock),
    );
    let (registration, readiness) = corcovado::Registration::new2();
    machine.pty.child = Some(registration);
    machine.pty.read_event = true;
    readiness.set_readiness(Ready::readable()).unwrap();
    let channel = machine.channel();
    let worker = machine.spawn();

    let started = std::time::Instant::now();
    let first = events.recv_timeout(Duration::from_secs(5));
    let elapsed = started.elapsed();
    channel.send(Msg::Shutdown).unwrap();
    worker.join().unwrap();

    assert_eq!(first, Ok("damaged"));
    assert!(
        elapsed >= crate::crosswords::SYNC_UPDATE_TIMEOUT / 2,
        "woken after {elapsed:?}, before the deadline"
    );
    assert!(!terminal.lock().synchronized_output());
}
