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
    let (machine, state) = machine.spawn().join().unwrap();
    assert_eq!(first_line(&machine, 12), "final output");
    assert_eq!(state.parser.sync_bytes_count(), 0);
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
    let (machine, state) = machine.spawn().join().unwrap();
    assert_eq!(machine.pty.shutdown_calls, 1);
    assert_eq!(machine.pty.deregister_calls, 1);
    assert_eq!(first_line(&machine, 12), "final output");
    assert_eq!(state.parser.sync_bytes_count(), 0);
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
