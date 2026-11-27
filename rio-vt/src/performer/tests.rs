use super::*;
use crate::ansi::CursorShape;
use crate::crosswords::pos::{Column, Line};
use crate::crosswords::CrosswordsSize;
use crate::event::VoidListener;
use std::io::Write;
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
    registration_error: bool,
    read_event: bool,
    shutdown_calls: usize,
    deregister_calls: usize,
    winsize_calls: usize,
    raw_error: Option<i32>,
    /// Whether a child of ours runs on it; see `EventedPty::has_child`.
    has_child: bool,
    /// Announces each `stop_reading`.
    stopped: Option<mpsc::Sender<()>>,
    /// The child's exit, reported on the child event token.
    exit: Option<corcovado::Registration>,
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
/// wait for a write instead of sleeping. The write half the writer
/// thread takes is a clone sharing the record, so a test reads it from
/// the machine it gets back.
#[derive(Default, Clone)]
struct RecordingWriter {
    bytes: Arc<std::sync::Mutex<Vec<u8>>>,
    written: Option<mpsc::Sender<Vec<u8>>>,
    /// A full PTY: every write is refused with `WouldBlock`.
    blocked: bool,
    /// The write half cannot be watched: its registration fails, so
    /// the writer thread stops before it writes anything.
    refuses_registration: bool,
    /// The write half's readiness, for a test that hangs it up.
    half_readiness: Arc<std::sync::Mutex<Option<corcovado::SetReadiness>>>,
    /// Every write panics, as a bug in the writer thread would.
    panics: bool,
    /// The write half cannot be opened at all.
    no_half: bool,
    /// Every write attempt, refused or not.
    attempts: Option<mpsc::Sender<()>>,
}

impl RecordingWriter {
    fn recorded(&self) -> Vec<u8> {
        self.bytes.lock().unwrap().clone()
    }
}

impl Write for RecordingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Some(sender) = &self.attempts {
            let _ = sender.send(());
        }
        if self.blocked {
            return Err(io::Error::from(ErrorKind::WouldBlock));
        }
        if self.panics {
            panic!("the test write half panics on every write");
        }
        self.bytes.lock().unwrap().extend_from_slice(buf);
        if let Some(sender) = &self.written {
            let _ = sender.send(buf.to_vec());
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The write half of the test PTY: the shared recorder, registered
/// through a registration that never reports writable readiness, so a
/// refused write stays refused. A hangup is reported whatever the
/// interest, as the kernel reports one.
struct TestWriteHalf {
    writer: RecordingWriter,
    registration: corcovado::Registration,
    _readiness: corcovado::SetReadiness,
}

impl TestWriteHalf {
    fn interest(interest: Ready) -> Ready {
        #[cfg(unix)]
        let interest = interest | Ready::from(UnixReady::hup());
        interest
    }
}

impl Write for TestWriteHalf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writer.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl corcovado::event::Evented for TestWriteHalf {
    fn register(
        &self,
        poll: &corcovado::Poll,
        token: corcovado::Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        if self.writer.refuses_registration {
            return Err(io::Error::from(ErrorKind::Other));
        }
        poll.register(&self.registration, token, Self::interest(interest), opts)
    }
    fn reregister(
        &self,
        poll: &corcovado::Poll,
        token: corcovado::Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        poll.reregister(&self.registration, token, Self::interest(interest), opts)
    }
    fn deregister(&self, poll: &corcovado::Poll) -> io::Result<()> {
        poll.deregister(&self.registration)
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
        self.winsize_calls += 1;
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
        if let Some(exit) = &self.exit {
            poll.register(
                exit,
                self.child_event_token(),
                Ready::readable(),
                PollOpt::edge(),
            )?;
        }
        Ok(())
    }
    fn reregister(
        &mut self,
        _: &corcovado::Poll,
        _: Ready,
        _: PollOpt,
    ) -> io::Result<()> {
        Ok(())
    }
    fn deregister(&mut self, _: &corcovado::Poll) -> io::Result<()> {
        self.deregister_calls += 1;
        Ok(())
    }
}

impl EventedPty for TestPty {
    type WriteHalf = TestWriteHalf;

    fn write_half(&mut self) -> io::Result<TestWriteHalf> {
        if self.writer.no_half {
            return Err(io::Error::from(ErrorKind::Other));
        }
        let (registration, readiness) = corcovado::Registration::new2();
        *self.writer.half_readiness.lock().unwrap() = Some(readiness.clone());
        Ok(TestWriteHalf {
            writer: self.writer.clone(),
            registration,
            _readiness: readiness,
        })
    }

    fn shutdown(&mut self) -> io::Result<()> {
        self.shutdown_calls += 1;
        Ok(())
    }
    fn child_event_token(&self) -> corcovado::Token {
        2.into()
    }
    fn has_child(&self) -> bool {
        self.has_child
    }
    fn stop_reading(&mut self, _: &corcovado::Poll) -> io::Result<()> {
        if let Some(stopped) = &self.stopped {
            let _ = stopped.send(());
        }
        Ok(())
    }
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        Some(ChildEvent::Exited(Some(0)))
    }
}

fn test_pty(bytes: Vec<u8>, ending: Option<ErrorKind>) -> TestPty {
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
        registration_error: false,
        read_event: false,
        shutdown_calls: 0,
        deregister_calls: 0,
        winsize_calls: 0,
        raw_error: None,
        has_child: true,
        stopped: None,
        exit: None,
    }
}

/// A machine over `pty`; the writer thread takes its write half here,
/// so a test that wants to hear of writes sets that up before.
fn machine_with(pty: TestPty) -> Machine<TestPty, VoidListener> {
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
        pty,
        VoidListener,
        WindowId::from(0),
        0,
    )
    .unwrap()
}

fn machine(bytes: Vec<u8>, ending: Option<ErrorKind>) -> Machine<TestPty, VoidListener> {
    machine_with(test_pty(bytes, ending))
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
    match (raw_error, ending) {
        (None, Some(kind)) => assert_eq!(result.unwrap_err().kind(), kind),
        // EOF, and Linux's EIO for a closed other side, are both Closed.
        _ => assert_eq!(result.unwrap(), ReadOutcome::Closed),
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
    let mut bytes = vec![0; READ_BUDGET * 2 + 1];
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
    let mut bytes = vec![0; READ_BUDGET * 2 + 1];
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
    assert!(
        machine.writer.is_some(),
        "the writer was stopped and joined"
    );
}

/// The exit statuses a machine reports.
#[cfg(unix)]
#[derive(Clone)]
struct ExitRecorder(mpsc::Sender<Option<i32>>);

#[cfg(unix)]
impl EventListener for ExitRecorder {
    fn send_event(&self, event: RioEvent, _: WindowId) {
        if let RioEvent::ChildExited(_, status) = event {
            let _ = self.0.send(status);
        }
    }
}

/// `pty` reads to its end and then hangs up, with the read token armed.
#[cfg(unix)]
fn hangup_machine(
    mut pty: TestPty,
) -> (
    Machine<TestPty, ExitRecorder>,
    mpsc::Receiver<Option<i32>>,
    corcovado::SetReadiness,
) {
    let (registration, readiness) = corcovado::Registration::new2();
    pty.child = Some(registration);
    pty.read_event = true;
    readiness
        .set_readiness(Ready::readable() | Ready::from(UnixReady::hup()))
        .unwrap();
    let (sender, statuses) = mpsc::channel();
    let recorder = ExitRecorder(sender);
    let terminal = Arc::new(TerminalLock::new(Crosswords::new(
        CrosswordsSize::new(80, 24),
        CursorShape::Block,
        recorder.clone(),
        WindowId::from(0),
        0,
        0,
    )));
    let machine = Machine::new(terminal, pty, recorder, WindowId::from(0), 0).unwrap();
    (machine, statuses, readiness)
}

// A pty with no child of ours ends when its other side closes, after
// what was written before.
#[cfg(unix)]
#[test]
fn hangup_ends_a_pty_without_a_child() {
    let mut pty = test_pty(b"final output".to_vec(), None);
    pty.has_child = false;
    let (machine, statuses, _readiness) = hangup_machine(pty);
    let shutdown = machine.channel();
    let (completed, receiver) = mpsc::channel();
    let worker = machine.spawn();
    std::thread::spawn(move || completed.send(worker.join().unwrap()).unwrap());
    let result = receiver.recv_timeout(Duration::from_secs(5));
    if result.is_err() {
        shutdown.send(Msg::Shutdown).unwrap();
    }
    let (machine, _) = result.expect("a hangup must end a pty with no child");
    assert_eq!(first_line(&machine, 12), "final output");
    assert_eq!(statuses.try_iter().collect::<Vec<_>>(), [None]);
    assert_eq!(machine.pty.shutdown_calls, 1);
    assert_eq!(machine.pty.deregister_calls, 1);
}

// With a child of ours, the other side closing only stops the reading:
// the session ends when the child exits, and reports its status.
#[cfg(unix)]
#[test]
fn hangup_waits_for_the_child_and_its_status() {
    let (stopped, reading_stopped) = mpsc::channel();
    let (exit, exited) = corcovado::Registration::new2();
    let mut pty = test_pty(b"final output".to_vec(), None);
    pty.stopped = Some(stopped);
    pty.exit = Some(exit);
    let (machine, statuses, _readiness) = hangup_machine(pty);
    let shutdown = machine.channel();
    let worker = machine.spawn();

    reading_stopped
        .recv_timeout(Duration::from_secs(5))
        .expect("the hangup stops the reading");
    assert!(
        statuses.recv_timeout(Duration::from_millis(100)).is_err(),
        "the session lasts until the child exits"
    );
    exited.set_readiness(Ready::readable()).unwrap();
    let status = statuses.recv_timeout(Duration::from_secs(5));
    if status.is_err() {
        shutdown.send(Msg::Shutdown).unwrap();
    }
    assert_eq!(status.expect("the child's exit ends it"), Some(0));
    let (machine, _) = worker.join().unwrap();
    assert_eq!(first_line(&machine, 12), "final output");
    assert_eq!(machine.pty.shutdown_calls, 1);
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
    assert!(
        machine.writer.is_some(),
        "the writer was stopped and joined"
    );
}

/// A writer that cannot run leaves a terminal nothing can be typed
/// to, so the reader closes it the way it closes on a failure of its
/// own: the child is shut down and the owner hears the terminal is
/// gone, instead of the silent stop the owner's own shutdown gets.
#[test]
fn a_writer_failure_closes_the_terminal() {
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
    let mut pty = test_pty(Vec::new(), Some(ErrorKind::WouldBlock));
    pty.writer.refuses_registration = true;
    let machine =
        Machine::new(terminal.clone(), pty, recorder, WindowId::from(0), 0).unwrap();
    let (machine, _) = machine.spawn().join().unwrap();

    assert_eq!(machine.pty.shutdown_calls, 1);
    let events: Vec<_> = receiver.try_iter().collect();
    assert!(events.contains(&"exited"), "events: {events:?}");
    assert!(events.contains(&"closed"), "events: {events:?}");
}

/// A writer thread that panics is a failed writer: the terminal is
/// closed the same way, and the writer still comes back to be dropped
/// with the machine.
#[test]
fn a_writer_panic_closes_the_terminal() {
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
    let mut pty = test_pty(Vec::new(), Some(ErrorKind::WouldBlock));
    pty.writer.panics = true;
    let machine =
        Machine::new(terminal.clone(), pty, recorder, WindowId::from(0), 0).unwrap();
    let channel = machine.channel();
    let handle = machine.spawn();
    channel
        .send(Msg::Input(std::borrow::Cow::Borrowed(b"k")))
        .unwrap();
    // Joined with a bound, so a reader that never hears of the panic
    // fails this test instead of hanging it.
    let (completed, done) = mpsc::channel();
    std::thread::spawn(move || completed.send(handle.join().unwrap()).unwrap());
    let result = done.recv_timeout(Duration::from_secs(5));
    if result.is_err() {
        channel.send(Msg::Shutdown).unwrap();
    }
    let (machine, _) = result.expect("the reader must learn of the writer's panic");

    assert_eq!(machine.pty.shutdown_calls, 1);
    assert!(machine.writer.is_some(), "the writer came back");
    let events: Vec<_> = receiver.try_iter().collect();
    assert!(events.contains(&"exited"), "events: {events:?}");
    assert!(events.contains(&"closed"), "events: {events:?}");
}

/// A terminal whose machine could not be made has no writer to send
/// its replies to; they keep going to the owner as events.
#[test]
fn a_terminal_without_a_machine_keeps_answering_by_event() {
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
    let mut pty = test_pty(Vec::new(), Some(ErrorKind::WouldBlock));
    pty.writer.no_half = true;
    assert!(Machine::new(terminal.clone(), pty, recorder, WindowId::from(0), 0).is_err());

    let mut processor = handler::Processor::default();
    processor.advance(&mut *terminal.lock(), b"\x1b[c");
    let events: Vec<_> = receiver.try_iter().collect();
    assert_eq!(events, vec!["ptywrite"]);
}

/// What the owner queued ahead of the shutdown, its last command, is
/// written before the writer stops.
#[test]
fn input_queued_before_the_shutdown_is_written() {
    let machine = machine(Vec::new(), None);
    let channel = machine.channel();
    channel
        .send(Msg::Input(std::borrow::Cow::Borrowed(b"exit\r")))
        .unwrap();
    channel.send(Msg::Shutdown).unwrap();
    let (machine, _) = machine.spawn().join().unwrap();
    assert_eq!(machine.pty.writer.recorded(), b"exit\r".to_vec());
}

/// A resize goes to the owner's one channel and is passed on to the
/// reader, which owns the PTY it applies to.
#[test]
fn a_resize_reaches_the_pty() {
    let machine = machine(Vec::new(), None);
    let channel = machine.channel();
    channel
        .send(Msg::Resize(crate::event::WindowSize {
            rows: 30,
            cols: 100,
            width: 800,
            height: 600,
        }))
        .unwrap();
    channel.send(Msg::Shutdown).unwrap();
    let (machine, _) = machine.spawn().join().unwrap();
    assert_eq!(machine.pty.winsize_calls, 1);
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
    assert!(
        machine.writer.is_some(),
        "the writer was stopped and joined"
    );
}

/// The machine after a drain that followed the child's exit, over a
/// PTY with nothing to read at first: `before_read` gets the ordinal
/// of each read and what it sends is the next thing read.
#[cfg(windows)]
fn drain_fed_by(
    mut before_read: impl FnMut(usize, &mpsc::Sender<Vec<u8>>) + Send + 'static,
) -> Machine<TestPty, VoidListener> {
    let (feed_tx, feed) = mpsc::channel();
    let mut pty = test_pty(Vec::new(), Some(ErrorKind::WouldBlock));
    pty.feed = Some(feed);
    pty.before_read = Some(Box::new(move |read| before_read(read, &feed_tx)));
    let (registration, readiness) = corcovado::Registration::new2();
    pty.child = Some(registration);
    readiness.set_readiness(Ready::readable()).unwrap();
    let (machine, _) = machine_with(pty).spawn().join().unwrap();
    machine
}

/// On Windows the pump thread delivers the last output after the exit
/// event, so a drain that finds nothing to read asks again.
#[cfg(windows)]
#[test]
fn the_drain_waits_for_output_that_comes_after_an_empty_read() {
    let machine = drain_fed_by(|read, feed| {
        if read == 2 {
            feed.send(b"final output".to_vec()).unwrap();
        }
    });
    assert_eq!(first_line(&machine, 12), "final output");
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
// only reach it on the writer's wakeup that delivered it.
#[test]
fn input_is_written_on_the_wakeup_that_delivers_it() {
    let (sender, written) = mpsc::channel();
    let mut pty = test_pty(Vec::new(), None);
    pty.writer.written = Some(sender);
    let machine = machine_with(pty);
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
    assert_eq!(machine.pty.writer.recorded(), b"ls\r".to_vec());
}

// Bytes refused by a full PTY wait for the writable event. Messages
// arriving in between do not retry them, and a reply the parse
// produces meanwhile queues behind them.
#[test]
fn a_stuck_write_is_not_retried_when_more_arrives() {
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
            registration_error: false,
            read_event: true,
            shutdown_calls: 0,
            deregister_calls: 0,
            winsize_calls: 0,
            raw_error: None,
            has_child: true,
            stopped: None,
            exit: None,
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

    // The reply the read produced reached the writer; a retry of the
    // refused bytes on its account would show here.
    assert!(
        attempts.recv_timeout(Duration::from_millis(100)).is_err(),
        "the reply's arrival must not have retried the stuck bytes"
    );

    channel.send(Msg::Shutdown).unwrap();
    let (machine, _) = handle.join().unwrap();
    assert!(machine.pty.writer.recorded().is_empty());
}

/// Once the PTY hangs up no writable event is coming, so bytes that
/// were waiting for one, and bytes arriving after, are each tried on
/// arrival instead of waiting forever.
#[cfg(unix)]
#[test]
fn a_hangup_stops_the_wait_for_a_writable_pty() {
    let (attempts, attempted) = mpsc::channel();
    let mut pty = test_pty(Vec::new(), Some(ErrorKind::WouldBlock));
    pty.writer.blocked = true;
    pty.writer.attempts = Some(attempts);
    let mut machine = machine_with(pty);
    let channel = machine.channel();
    machine.spawn_writer();
    let timeout = Duration::from_secs(5);

    // The first write is refused, which arms the wait for writable.
    channel.send(Msg::Input(Cow::Borrowed(b"a"))).unwrap();
    attempted.recv_timeout(timeout).unwrap();
    channel.send(Msg::Input(Cow::Borrowed(b"b"))).unwrap();
    assert!(
        attempted.recv_timeout(Duration::from_millis(100)).is_err(),
        "a write was retried while waiting for writable"
    );

    let readiness = machine
        .pty
        .writer
        .half_readiness
        .lock()
        .unwrap()
        .clone()
        .unwrap();
    readiness
        .set_readiness(Ready::from(UnixReady::hup()))
        .unwrap();
    // The hangup itself prompts one attempt, and so does what follows.
    attempted.recv_timeout(timeout).unwrap();
    channel.send(Msg::Input(Cow::Borrowed(b"c"))).unwrap();
    attempted.recv_timeout(timeout).unwrap();

    machine.stop_writer();
}

// A query from the program is answered by the reader thread and handed
// to the writer thread, written to the PTY right after the parse that
// produced it, and never sent to the event loop.
#[test]
fn a_parser_reply_is_written_without_the_event_loop() {
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
            registration_error: false,
            read_event: true,
            shutdown_calls: 0,
            deregister_calls: 0,
            winsize_calls: 0,
            raw_error: None,
            has_child: true,
            stopped: None,
            exit: None,
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
    /// The chunk being parsed when it asked for the terminal, or None
    /// when it asked only after the pile, or when a handoff gave up at
    /// its bound, which only an overloaded machine makes it do.
    asked_during: Option<char>,
    /// From asking for the terminal to holding it.
    waited: Duration,
    /// The first line once the whole pile is parsed.
    first_line: String,
    /// Everything written to the PTY.
    written: Vec<u8>,
}

type SharedTerminal = Arc<TerminalLock<Crosswords<OnTitle>>>;
type ChunkStarts = Arc<std::sync::Mutex<Vec<(char, std::time::Instant)>>>;

/// Every chunk of a pile sets the title to its ordinal; this records
/// when the parser reached each, and on the chunk `summon_at` starts
/// an action on its own thread, that is while the reader holds the
/// terminal for that chunk.
#[derive(Clone)]
struct OnTitle {
    terminal: Arc<std::sync::OnceLock<SharedTerminal>>,
    starts: ChunkStarts,
    summon_at: char,
    action: Arc<dyn Fn(SharedTerminal) + Send + Sync>,
}

impl OnTitle {
    fn new(
        summon_at: char,
        action: impl Fn(SharedTerminal) + Send + Sync + 'static,
    ) -> OnTitle {
        OnTitle {
            terminal: Arc::new(std::sync::OnceLock::new()),
            starts: Arc::new(std::sync::Mutex::new(Vec::new())),
            summon_at,
            action: Arc::new(action),
        }
    }

    /// The terminal built with this listener, for the action to lock.
    fn watches(&self, terminal: &SharedTerminal) {
        self.terminal
            .set(Arc::clone(terminal))
            .ok()
            .expect("set once");
    }

    /// The chunk being parsed at `when`, if the pile had started.
    fn chunk_at(&self, when: std::time::Instant) -> Option<char> {
        self.starts
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(_, start)| *start <= when)
            .map(|(chunk, _)| *chunk)
    }
}

impl EventListener for OnTitle {
    fn send_event(&self, event: RioEvent, _: WindowId) {
        let RioEvent::Title(_, title) = event else {
            return;
        };
        let Some(chunk) = title.chars().last() else {
            return;
        };
        self.starts
            .lock()
            .unwrap()
            .push((chunk, std::time::Instant::now()));
        if chunk == self.summon_at {
            let terminal =
                Arc::clone(self.terminal.get().expect("set after construction"));
            let action = Arc::clone(&self.action);
            std::thread::spawn(move || action(terminal));
        }
    }
}

/// The ordinal character of chunk `n`: '1'..'9', then the ASCII
/// characters after '9' for more.
fn chunk_char(n: u8) -> char {
    (b'0' + n) as char
}

/// Whether `saw` is the chunk `asked_during` or the one after: the
/// waiter got the terminal when the reader let go of the chunk it was
/// parsing, or of the next if the waiter was still spinning rather
/// than parked at that release.
fn within_a_chunk(saw: char, asked_during: char) -> bool {
    saw == asked_during || saw == chunk_char(asked_during as u8 - b'0' + 1)
}

/// A pile of `chunks` reads: every chunk homes the cursor, writes its
/// ordinal and sets the title to it, padded with attribute resets and
/// then NULs (which the parser ignores) to exactly one read; the first
/// chunk also carries a DA query.
fn pile(chunks: u8) -> Vec<u8> {
    pile_with(chunks, None)
}

/// [`pile`] with `extra` bytes in the chunk of that number.
fn pile_with(chunks: u8, extra: Option<(u8, &[u8])>) -> Vec<u8> {
    let mut payload = Vec::new();
    for n in 1..=chunks {
        payload.extend_from_slice(b"\x1b[H");
        payload.push(b'0' + n);
        payload.extend_from_slice(b"\x1b]0;");
        payload.push(b'0' + n);
        payload.extend_from_slice(b"\x1b\\");
        if n == 1 {
            payload.extend_from_slice(b"\x1b[c");
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
/// takes the terminal and starts on the pile, and a waiter is started
/// while it parses the first chunk.
fn waiter_against_a_pile(chunks: u8) -> PileWaiter {
    // A pile that fills the reader's buffer would make the reader wait
    // for the lock before the read hook below releases the holder.
    assert!(chunks as usize <= READ_BUFFER_SIZE / READ_CHUNK);
    let payload = pile(chunks);
    let (written_tx, written) = mpsc::channel();
    let (saw_tx, saw) = mpsc::channel();
    let listener = OnTitle::new('1', move |terminal| {
        let asked = std::time::Instant::now();
        let guard = terminal.lock();
        let waited = asked.elapsed();
        saw_tx
            .send((guard.grid[Line(0)][Column(0)].c(), asked, waited))
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

    // The holder keeps the terminal busy, like a renderer that already
    // has it, until the read after the pile. It must not start before
    // the machine, whose constructor takes the terminal lock, is built.
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
            writer: RecordingWriter {
                written: Some(written_tx),
                ..RecordingWriter::default()
            },
            child: None,
            registration_error: false,
            read_event: false,
            shutdown_calls: 0,
            deregister_calls: 0,
            winsize_calls: 0,
            raw_error: None,
            has_child: true,
            stopped: None,
            exit: None,
        },
        listener.clone(),
        WindowId::from(0),
        0,
    )
    .unwrap();
    let mut state = State::default();
    // Room for the pile and one more read.
    let mut buf = vec![0; READ_BUFFER_SIZE + READ_CHUNK];
    // The reader runs here, on this thread; the writer runs as it would.
    machine.spawn_writer();

    let (held_tx, held) = mpsc::channel::<()>();
    let holder_terminal = Arc::clone(&terminal);
    let holder = std::thread::spawn(move || {
        let guard = holder_terminal.lock();
        held_tx.send(()).unwrap();
        release.recv().unwrap();
        drop(guard);
    });
    held.recv_timeout(Duration::from_secs(5)).unwrap();

    let outcome = machine.pty_read(&mut state, &mut buf).unwrap();
    assert_eq!(outcome, ReadOutcome::Closed, "the whole pile was read");
    holder.join().unwrap();
    // The chunk's reply went to the writer thread; wait for it to land.
    written
        .recv_timeout(Duration::from_secs(5))
        .expect("the writer wrote the reply");
    machine.stop_writer();
    let (saw, asked, waited) = saw.recv_timeout(Duration::from_secs(5)).unwrap();
    let asked_during = listener
        .chunk_at(asked)
        .filter(|c| *c != chunk_char(chunks) && terminal.bound_hits() == 0);
    PileWaiter {
        saw,
        asked_during,
        waited,
        first_line: first_line(&machine, 1),
        written: machine.pty.writer.recorded(),
    }
}

// Bytes that piled up while another thread held the terminal are
// parsed a chunk at a time once it is free, and the terminal is let go
// of between chunks: a thread that asks for it while a chunk is parsed
// gets it after that chunk, or the next if it was still spinning
// rather than parked when the reader let go, never after the whole
// pile. The reply of the first chunk reached the PTY as it was made. A waiter so slow to be scheduled that it asked
// only during the last chunk, or after the pile, proves nothing; such
// a run is repeated.
#[test]
fn a_waiting_thread_gets_the_terminal_within_a_chunk_of_asking() {
    for _ in 0..10 {
        let waiter = waiter_against_a_pile(4);
        assert_eq!(
            waiter.written,
            b"\x1b[?62;4;6;22;52c".to_vec(),
            "the reply was written without the event loop"
        );
        assert_eq!(waiter.first_line, "4", "the rest of the pile followed");
        let Some(asked_during) = waiter.asked_during else {
            continue;
        };
        assert!(
            within_a_chunk(waiter.saw, asked_during),
            "asked during chunk {asked_during}, got the terminal only after chunk {}",
            waiter.saw
        );
        return;
    }
    panic!("ten piles and the waiter never asked in time");
}

/// A spawned reader and writer over `payload`, with `listener` as the
/// terminal's owner and every PTY write announced on `written`. With
/// `piled`, a holder keeps the terminal until the read after that many
/// chunks, so those reads pile up.
struct SpawnedReader {
    channel: channel::Sender<Msg>,
    readiness: corcovado::SetReadiness,
    written: mpsc::Receiver<Vec<u8>>,
    terminal: Arc<TerminalLock<Crosswords<OnTitle>>>,
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
            registration_error: false,
            read_event: true,
            shutdown_calls: 0,
            deregister_calls: 0,
            winsize_calls: 0,
            raw_error: None,
            has_child: true,
            stopped: None,
            exit: None,
        },
        listener,
        WindowId::from(0),
        0,
    )
    .unwrap();
    let channel = machine.channel();
    if piled.is_some() {
        // The holder keeps the terminal busy, like a renderer that
        // already has it, until the read after the pile; it starts
        // after the machine, whose constructor takes the lock.
        let (held_tx, held) = mpsc::channel::<()>();
        let holder_terminal = Arc::clone(&terminal);
        std::thread::spawn(move || {
            let guard = holder_terminal.lock();
            held_tx.send(()).unwrap();
            release.recv().unwrap();
            drop(guard);
        });
        held.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    let handle = machine.spawn();
    SpawnedReader {
        channel,
        readiness,
        written,
        terminal,
        handle,
    }
}

// Writing is the writer thread's, so a keystroke is written while
// another thread holds the terminal and the reader waits for it, with
// no bound to wait out: the title in the chunk summons a holder that
// keeps the terminal until told to let go, and the keystroke lands on
// the PTY before that.
#[test]
fn input_is_written_while_another_thread_holds_the_terminal() {
    let payload = b"\x1b[H1\x1b]0;hold\x1b\\".to_vec();
    let (held_tx, held) = mpsc::channel::<()>();
    let (go_tx, go) = mpsc::channel::<()>();
    let go = std::sync::Mutex::new(Some(go));
    let listener = OnTitle::new('d', move |terminal| {
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
    reader
        .channel
        .send(Msg::Input(std::borrow::Cow::Borrowed(b"k")))
        .unwrap();
    assert_eq!(
        reader.written.recv_timeout(timeout).unwrap(),
        b"k".to_vec(),
        "written while the holder still held the terminal"
    );
    go_tx.send(()).unwrap();

    reader.channel.send(Msg::Shutdown).unwrap();
    let (machine, _) = reader.handle.join().unwrap();
    assert_eq!(machine.pty.writer.recorded(), b"k".to_vec());
}

// Everything written to the PTY goes through the writer's one inbox in
// the order it is produced, so a reply an embedder produces by parsing
// its own bytes under the lock during a handoff, as librio does, is
// written between the replies of the chunk before and the chunk after.
// An embedder scheduled so late that it asked only during chunk 2 or
// later proves nothing (asked in chunk 2's last microseconds it is
// rightly let in after chunk 3), and neither does a handoff that hit
// the bound, where the reader rightly moved on; such a run is repeated.
#[test]
fn an_injected_reply_during_a_handoff_keeps_its_place() {
    for _ in 0..10 {
        // DA in chunk 1 (always), DSR injected during the handoff after
        // chunk 1, DA again in chunk 3.
        let payload = pile_with(4, Some((3, b"\x1b[c")));
        let (asked_tx, asked) = mpsc::channel();
        let listener = OnTitle::new('1', move |terminal| {
            asked_tx.send(std::time::Instant::now()).unwrap();
            let mut guard = terminal.lock();
            let mut processor = handler::Processor::default();
            processor.advance(&mut *guard, b"\x1b[6n");
        });
        let reader = spawn_reader(payload, Some(4), listener.clone());
        let timeout = Duration::from_secs(5);

        reader.readiness.set_readiness(Ready::readable()).unwrap();
        let da = b"\x1b[?62;4;6;22;52c".to_vec();
        let dsr = b"\x1b[1;2R".to_vec();
        let mut got = Vec::new();
        while got.len() < 2 * da.len() + dsr.len() {
            got.extend(reader.written.recv_timeout(timeout).unwrap());
        }
        reader.channel.send(Msg::Shutdown).unwrap();
        let bound_hits = reader.terminal.bound_hits();
        let (machine, _) = reader.handle.join().unwrap();
        assert_eq!(first_line(&machine, 1), "4", "the whole pile was parsed");

        let asked = asked.recv_timeout(timeout).unwrap();
        if bound_hits > 0 || listener.chunk_at(asked).is_none_or(|chunk| chunk >= '2') {
            continue;
        }
        let mut expected = da.clone();
        expected.extend(&dsr);
        expected.extend(&da);
        assert_eq!(
            got, expected,
            "chunk 1's DA, the injected DSR, chunk 3's DA"
        );
        return;
    }
    panic!("ten piles and the embedder never asked in time");
}

/// Stands in for the owner of the terminal, as a frontend would: a
/// query only the owner can answer is answered into the writer's
/// inbox, and a reply the terminal routes through the owner is passed
/// back to the same inbox as an answer. It answers from inside the parser, on the
/// reader's thread, where a frontend answers later from its own; that
/// is the earliest an answer can arrive, the closest race with the
/// replies parsed right behind the query.
#[derive(Clone)]
struct Owner {
    channel: Arc<std::sync::OnceLock<channel::Sender<Msg>>>,
}

impl EventListener for Owner {
    fn send_event(&self, event: RioEvent, _: WindowId) {
        let channel = self.channel.get().expect("set after spawn");
        match event {
            RioEvent::TextAreaSizeRequest(_, format) => {
                let size = crate::event::WindowSize {
                    rows: 24,
                    cols: 80,
                    width: 640,
                    height: 480,
                };
                let _ = channel.send(Msg::Reply(std::borrow::Cow::Owned(
                    format(size).into_bytes(),
                )));
            }
            RioEvent::PtyWrite(_, text) => {
                let _ =
                    channel.send(Msg::Reply(std::borrow::Cow::Owned(text.into_bytes())));
            }
            _ => {}
        }
    }
}

// Replies reach the writer the moment the parser makes them, so a reply
// made before a query the owner answers is written before that answer
// even when both are in one read: a program that asks for the cursor
// position, the text area in pixels and device attributes in one write
// gets its answers in that order. Batched to the end of the chunk, the
// cursor report would have come last.
#[test]
fn replies_keep_their_place_before_an_owner_answer_in_the_same_read() {
    let channel_slot = Arc::new(std::sync::OnceLock::new());
    let owner = Owner {
        channel: Arc::clone(&channel_slot),
    };
    let (registration, readiness) = corcovado::Registration::new2();
    let (written_tx, written) = mpsc::channel();
    let terminal = Arc::new(TerminalLock::new(Crosswords::new(
        CrosswordsSize::new(80, 24),
        CursorShape::Block,
        owner.clone(),
        WindowId::from(0),
        0,
        0,
    )));
    let machine = Machine::new(
        Arc::clone(&terminal),
        TestPty {
            bytes: io::Cursor::new(b"\x1b[6n\x1b[14t\x1b[c".to_vec()),
            feed: None,
            idle: None,
            ending: Some(ErrorKind::WouldBlock),
            exhausted: None,
            before_read: None,
            reads: 0,
            writer: RecordingWriter {
                written: Some(written_tx),
                ..RecordingWriter::default()
            },
            child: Some(registration),
            registration_error: false,
            read_event: true,
            shutdown_calls: 0,
            deregister_calls: 0,
            winsize_calls: 0,
            raw_error: None,
            has_child: true,
            stopped: None,
            exit: None,
        },
        owner,
        WindowId::from(0),
        0,
    )
    .unwrap();
    let channel = machine.channel();
    assert!(channel_slot.set(channel.clone()).is_ok());
    let handle = machine.spawn();
    let timeout = Duration::from_secs(5);

    readiness.set_readiness(Ready::readable()).unwrap();
    let expected = b"\x1b[1;1R\x1b[4;480;640t\x1b[?62;4;6;22;52c".to_vec();
    let mut got = Vec::new();
    while got.len() < expected.len() {
        got.extend(written.recv_timeout(timeout).unwrap());
    }
    assert_eq!(got, expected, "cursor report, the owner's size answer, DA");

    channel.send(Msg::Shutdown).unwrap();
    handle.join().unwrap();
}

// How long a renderer waits when it asks for the terminal while a
// 1 MiB pile is parsed. Run with
// `cargo test -p rio-vt --release -- --ignored --nocapture pile_wait`.
#[test]
#[ignore]
fn pile_wait_latency() {
    let mut waits = Vec::new();
    while waits.len() < 20 {
        let waiter = waiter_against_a_pile(16);
        if let Some(asked_during) = waiter.asked_during {
            assert!(within_a_chunk(waiter.saw, asked_during));
            waits.push(waiter.waited);
        }
    }
    waits.sort();
    println!(
        "1 MiB pile, waiter wait: median {:?}, min {:?}, max {:?}",
        waits[waits.len() / 2],
        waits[0],
        waits[waits.len() - 1]
    );
}

// A thread that asks for the terminal mid burst, with nobody else
// holding it, gets it after the chunk being parsed, or the next if it
// was still spinning when the reader let go, not after the burst; the
// remaining chunks follow.
#[test]
fn a_waiting_thread_gets_the_terminal_after_the_current_chunk() {
    for _ in 0..10 {
        // Six chunks of one read each; the waiter is started while
        // chunk 2 is parsed.
        let payload = pile(6);
        let (saw_tx, saw) = mpsc::channel();
        let listener = OnTitle::new('2', move |terminal| {
            let asked = std::time::Instant::now();
            let guard = terminal.lock();
            saw_tx
                .send((guard.grid[Line(0)][Column(0)].c(), asked))
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
        let mut machine = Machine::new(
            Arc::clone(&terminal),
            TestPty {
                bytes: io::Cursor::new(payload),
                ending: None,
                exhausted: None,
                feed: None,
                idle: None,
                before_read: None,
                reads: 0,
                writer: RecordingWriter::default(),
                child: None,
                registration_error: false,
                read_event: false,
                shutdown_calls: 0,
                deregister_calls: 0,
                winsize_calls: 0,
                raw_error: None,
                has_child: true,
                stopped: None,
                exit: None,
            },
            listener.clone(),
            WindowId::from(0),
            0,
        )
        .unwrap();
        let mut state = State::default();
        let mut buf = vec![0; READ_BUFFER_SIZE];

        // One call parses up to the budget, four chunks; if the waiter
        // still holds the terminal when the reader comes back for the
        // next read, the reader reads ahead and parses the rest as a
        // pile in the same call. Either way the whole burst is parsed.
        let mut outcome = machine.pty_read(&mut state, &mut buf).unwrap();
        let (seen, asked) = saw.recv_timeout(Duration::from_secs(5)).unwrap();
        while outcome != ReadOutcome::Closed {
            outcome = machine.pty_read(&mut state, &mut buf).unwrap();
        }
        assert_eq!(first_line(&machine, 1), "6", "the whole burst was parsed");

        // The waiter counts only if it asked before the last chunk and
        // no handoff gave up at its bound.
        let Some(asked_during) = listener
            .chunk_at(asked)
            .filter(|c| *c != '6' && terminal.bound_hits() == 0)
        else {
            continue;
        };
        assert!(
            within_a_chunk(seen, asked_during),
            "asked during chunk {asked_during}, got the terminal only after chunk {seen}"
        );
        return;
    }
    panic!("ten bursts and the waiter never asked in time");
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

// A burst that scrolls the whole screen sends no render event of its
// own for the scroll: the damage event at the burst's end is the one
// wakeup the UI thread gets.
#[test]
fn a_scrolling_burst_wakes_the_ui_thread_once() {
    let (events, recorded) = mpsc::channel();
    let mut burst = Vec::new();
    for i in 0..40 {
        burst.extend_from_slice(format!("line {i}\r\n").as_bytes());
    }
    let mut terminal = Crosswords::new(
        CrosswordsSize::new(80, 24),
        CursorShape::Block,
        EventRecorder(events.clone()),
        WindowId::from(0),
        0,
        100,
    );
    // As after the renderer's first frame: nothing is damaged yet.
    terminal.reset_damage();
    let (registration, readiness) = corcovado::Registration::new2();
    let (exhausted_tx, exhausted) = mpsc::channel();
    let machine = Machine::new(
        Arc::new(TerminalLock::new(terminal)),
        TestPty {
            bytes: io::Cursor::new(burst),
            ending: None,
            feed: None,
            idle: None,
            exhausted: Some(exhausted_tx),
            before_read: None,
            reads: 0,
            writer: RecordingWriter::default(),
            child: Some(registration),
            registration_error: false,
            read_event: true,
            shutdown_calls: 0,
            deregister_calls: 0,
            winsize_calls: 0,
            raw_error: None,
            has_child: true,
            stopped: None,
            exit: None,
        },
        EventRecorder(events),
        WindowId::from(0),
        0,
    )
    .unwrap();
    let channel = machine.channel();
    let handle = machine.spawn();
    readiness.set_readiness(Ready::readable()).unwrap();
    exhausted.recv_timeout(Duration::from_secs(5)).unwrap();
    channel.send(Msg::Shutdown).unwrap();
    let (machine, _) = handle.join().unwrap();
    assert_eq!(machine.terminal.lock().history_size(), 17);
    let names: Vec<&str> = recorded.try_iter().collect();
    assert_eq!(names, vec!["damaged"]);
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
            registration_error: false,
            read_event: true,
            shutdown_calls: 0,
            deregister_calls: 0,
            winsize_calls: 0,
            raw_error: None,
            has_child: true,
            stopped: None,
            exit: None,
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

    // The owner answers: its answer is written. The fence stays up,
    // since the DA routed through the owner is still the owner's to
    // pass back, so a third write's DA takes the owner's queue too
    // instead of overtaking the second's.
    channel
        .send(Msg::Reply(std::borrow::Cow::Borrowed(b"\x1b[4;480;640t")))
        .unwrap();
    assert_eq!(written.recv_timeout(timeout).unwrap(), b"\x1b[4;480;640t");
    feed_tx.send(b"\x1b[c".to_vec()).unwrap();
    wake();
    next_named("ptywrite");
    burst_over();
    assert!(
        written.try_recv().is_err(),
        "a DA must not overtake the one the owner still holds"
    );

    // The owner passes both back, in order, and the fence lifts.
    let da = b"\x1b[?62;4;6;22;52c".to_vec();
    channel
        .send(Msg::Reply(std::borrow::Cow::Owned(da.clone())))
        .unwrap();
    channel
        .send(Msg::Reply(std::borrow::Cow::Owned(da.clone())))
        .unwrap();
    let mut passed_back = Vec::new();
    while passed_back.len() < 2 * da.len() {
        passed_back.extend(written.recv_timeout(timeout).unwrap());
    }

    // Fourth write: answered here again.
    feed_tx.send(b"\x1b[c".to_vec()).unwrap();
    wake();
    assert_eq!(written.recv_timeout(timeout).unwrap(), da);
    burst_over();

    channel.send(Msg::Shutdown).unwrap();
    let (machine, _) = handle.join().unwrap();
    let mut expected = b"\x1b[4;480;640t".to_vec();
    expected.extend(&da);
    expected.extend(&da);
    expected.extend(&da);
    assert_eq!(
        machine.pty.writer.recorded(),
        expected,
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
            registration_error: false,
            read_event: true,
            shutdown_calls: 0,
            deregister_calls: 0,
            winsize_calls: 0,
            raw_error: None,
            has_child: true,
            stopped: None,
            exit: None,
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
    // The writer thread lowers the fence as it takes the decline; a
    // marker written behind it shows it has, before the next read.
    channel
        .send(Msg::Input(std::borrow::Cow::Borrowed(b"m")))
        .unwrap();
    assert_eq!(written.recv_timeout(timeout).unwrap(), b"m".to_vec());

    feed_tx.send(b"\x1b[c".to_vec()).unwrap();
    readiness.set_readiness(Ready::empty()).unwrap();
    readiness.set_readiness(Ready::readable()).unwrap();
    assert_eq!(
        written.recv_timeout(timeout).unwrap(),
        b"\x1b[?62;4;6;22;52c".to_vec()
    );

    channel.send(Msg::Shutdown).unwrap();
    let (machine, _) = handle.join().unwrap();
    assert_eq!(
        machine.pty.writer.recorded(),
        b"m\x1b[?62;4;6;22;52c".to_vec()
    );
}

#[derive(Clone)]
struct EventRecorder(mpsc::Sender<&'static str>);

impl EventListener for EventRecorder {
    fn send_event(&self, event: RioEvent, _: WindowId) {
        let name = match event {
            RioEvent::TerminalDamaged(_) => "damaged",
            RioEvent::ChildExited(..) => "exited",
            RioEvent::RenderRoute(_) => "render",
            RioEvent::CloseTerminal(..) => "closed",
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
