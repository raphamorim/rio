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
    ending: Option<ErrorKind>,
    exhausted: Option<mpsc::Sender<()>>,
    writer: Vec<u8>,
    child: Option<corcovado::Registration>,
    registration_error: bool,
    read_event: bool,
    shutdown_calls: usize,
    deregister_calls: usize,
    raw_error: Option<i32>,
}

impl Read for TestPty {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let got = self.bytes.read(buf)?;
        if got != 0 {
            return Ok(got);
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

impl ProcessReadWrite for TestPty {
    type Reader = Self;
    type Writer = Vec<u8>;
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
        Arc::new(FairMutex::new(terminal)),
        TestPty {
            bytes: io::Cursor::new(bytes),
            ending,
            exhausted: None,
            writer: Vec::new(),
            child: None,
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

fn first_line(machine: &Machine<TestPty, VoidListener>, len: usize) -> String {
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
    terminal: Arc<std::sync::Mutex<std::sync::Weak<FairMutex<Crosswords<Self>>>>>,
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
    let terminal = Arc::new(FairMutex::new(Crosswords::new(
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

#[test]
fn explicit_shutdown_finalizes_once() {
    let machine = machine(Vec::new(), None);
    machine.channel().send(Msg::Shutdown).unwrap();
    let (machine, _) = machine.spawn().join().unwrap();
    assert_eq!(machine.pty.shutdown_calls, 1);
    assert_eq!(machine.pty.deregister_calls, 1);
}

#[derive(Clone)]
struct EventRecorder(mpsc::Sender<&'static str>);

impl EventListener for EventRecorder {
    fn send_event(&self, event: RioEvent, _: WindowId) {
        let name = match event {
            RioEvent::TerminalDamaged(_) => "damaged",
            RioEvent::ChildExited(..) => "exited",
            RioEvent::TmuxUpdate(..) => "tmux update",
            RioEvent::TmuxStatus(..) => "tmux status",
            RioEvent::TmuxEnded(..) => "tmux ended",
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
    let terminal = Arc::new(FairMutex::new(Crosswords::new(
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
    Arc<FairMutex<Crosswords<EventRecorder>>>,
);

fn recording_machine(bytes: Vec<u8>, ending: Option<ErrorKind>) -> RecordingMachine {
    let (sender, receiver) = mpsc::channel();
    let recorder = EventRecorder(sender);
    let terminal = Arc::new(FairMutex::new(Crosswords::new(
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

fn tmux_state() -> State {
    let mut state = State::default();
    state.parser.set_tmux_control_allowed(true);
    state
}

fn queued(state: &State) -> Vec<String> {
    state
        .write_list
        .iter()
        .map(|line| String::from_utf8_lossy(line).into_owned())
        .collect()
}

/// The hosting terminal sees what surrounds the session and nothing of it.
#[test]
fn tmux_session_leaves_the_terminal_untouched_and_hands_it_back() {
    let (mut machine, events, _) = recording_machine(
        b"before\x1bP1000p%begin 1 1 0\r\n%end 1 1 0\r\n\
          %output %0 \xe2\x9c\x9c not for this grid\r\n\
          %exit\r\n\x1b\\after"
            .to_vec(),
        Some(ErrorKind::WouldBlock),
    );
    let mut state = tmux_state();
    machine
        .pty_read(&mut state, &mut vec![0; READ_BUFFER_SIZE])
        .unwrap();

    assert!(state.tmux.is_none());
    let terminal = machine.terminal.lock();
    let line: String = (0..11)
        .map(|col| terminal.grid[Line(0)][Column(col)].c())
        .collect();
    assert_eq!(line, "beforeafter");
    drop(terminal);
    assert!(events.try_iter().any(|event| event == "tmux ended"));
}

#[test]
fn tmux_session_starts_mid_buffer_and_spans_reads() {
    let mut machine = machine(
        b"\x1bP1000p%begin 1 1 0\r".to_vec(),
        Some(ErrorKind::WouldBlock),
    );
    let mut state = tmux_state();
    let mut buf = vec![0; READ_BUFFER_SIZE];
    machine.pty_read(&mut state, &mut buf).unwrap();
    assert!(state.tmux.is_some());
    assert!(queued(&state).is_empty(), "no complete line yet");

    machine.pty.bytes = io::Cursor::new(b"\n%end 1 1 0\r\n".to_vec());
    machine.pty_read(&mut state, &mut buf).unwrap();
    assert_eq!(queued(&state).len(), 1);
    assert!(queued(&state)[0].starts_with("list-windows "));
}

/// `ssh` dying takes the client with it, with no `%exit` and the shell
/// talking on the same PTY.
#[test]
fn tmux_session_lost_to_the_shell_keeps_what_the_shell_said() {
    let mut machine = machine(
        b"\x1bP1000p%begin 1 1 0\r\n%end 1 1 0\r\nConnection closed".to_vec(),
        Some(ErrorKind::WouldBlock),
    );
    let mut state = tmux_state();
    machine
        .pty_read(&mut state, &mut vec![0; READ_BUFFER_SIZE])
        .unwrap();
    assert!(state.tmux.is_none());
    assert_eq!(first_line(&machine, 17), "Connection closed");
}

/// The hidden tab must be back before the exit closes it.
#[test]
fn tmux_session_ends_before_the_child_exit_is_reported() {
    let (mut machine, events, _) = recording_machine(
        b"\x1bP1000p%begin 1 1 0\r\n%end 1 1 0\r\n".to_vec(),
        Some(ErrorKind::WouldBlock),
    );
    machine.set_tmux_control(true);
    let (registration, readiness) = corcovado::Registration::new2();
    machine.pty.child = Some(registration);
    readiness.set_readiness(Ready::readable()).unwrap();
    machine.spawn().join().unwrap();

    let events: Vec<_> = events.try_iter().collect();
    let ended = events.iter().position(|event| *event == "tmux ended");
    let exited = events.iter().position(|event| *event == "exited");
    assert!(ended.is_some(), "events: {events:?}");
    assert!(ended < exited, "events: {events:?}");
}

#[test]
fn tmux_commands_after_the_session_never_reach_the_shell() {
    let mut machine = machine(Vec::new(), Some(ErrorKind::WouldBlock));
    let mut state = tmux_state();
    machine
        .channel()
        .send(Msg::TmuxCommand("send-keys -t %0 -H 72 6d".into()))
        .unwrap();
    assert!(machine.drain_recv_channel(&mut state));
    assert!(queued(&state).is_empty());
}

#[test]
fn dcs_1000p_is_ignored_unless_enabled() {
    let mut machine = machine(
        b"\x1bP1000p%begin 1 1 0\r\n\x1b\\shown".to_vec(),
        Some(ErrorKind::WouldBlock),
    );
    let mut state = State::default();
    machine
        .pty_read(&mut state, &mut vec![0; READ_BUFFER_SIZE])
        .unwrap();
    assert!(state.tmux.is_none());
    assert_eq!(first_line(&machine, 5), "shown");
}

#[test]
fn other_dcs_p_sequences_do_not_start_tmux() {
    let mut machine = machine(
        b"\x1bP999p\x1b\\shown".to_vec(),
        Some(ErrorKind::WouldBlock),
    );
    let mut state = tmux_state();
    machine
        .pty_read(&mut state, &mut vec![0; READ_BUFFER_SIZE])
        .unwrap();
    assert!(state.tmux.is_none());
    assert_eq!(first_line(&machine, 5), "shown");
}
