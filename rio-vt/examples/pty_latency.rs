//! Reader thread latency on a real PTY.
//!
//! Spawns `cat` on a raw pseudo terminal and drives it through the same
//! `Machine` the frontends use, with the PTY wrapped so every write is
//! timestamped. Two paths are measured:
//!
//! - input: the time from handing the reader thread a keystroke to the
//!   byte reaching the PTY;
//! - reply: the time from the reader thread writing a primary DA query
//!   (which `cat` echoes back to the parser) to it writing the answer.
//!   That includes one trip through `cat`, the same on every build.
//!
//! A `PtyWrite` event, the old route for replies, is forwarded to the
//! reader thread at once, with none of the frame work a UI thread does
//! in between, so an old build measures as its best case.
//!
//! Run with `cargo run -p rio-vt --release --example pty_latency`.
//! Unix only: it spawns `cat` on a pseudo terminal.

#[cfg(unix)]
mod unix {
    use std::io::{self, Read, Write};
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::{Duration, Instant};

    use rio_vt::ansi::CursorShape;
    use rio_vt::corcovado;
    use rio_vt::crosswords::{Crosswords, CrosswordsSize};
    use rio_vt::event::sync::FairMutex;
    use rio_vt::event::{EventListener, Msg, RioEvent, WindowId};
    use rio_vt::performer::Machine;
    use rio_vt::teletypewriter::{
        self, ChildEvent, EventedPty, ProcessReadWrite, WinsizeBuilder,
    };

    const DA: &[u8] = b"\x1b[c";
    const DA_REPLY: &[u8] = b"\x1b[?62";

    /// The real PTY, with every write announced.
    struct Timed {
        inner: teletypewriter::Pty,
        writes: mpsc::Sender<(Instant, Vec<u8>)>,
    }

    impl Read for Timed {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.inner.reader().read(buf)
        }
    }

    impl Write for Timed {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let n = self.inner.writer().write(buf)?;
            let _ = self.writes.send((Instant::now(), buf[..n].to_vec()));
            Ok(n)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.inner.writer().flush()
        }
    }

    impl ProcessReadWrite for Timed {
        type Reader = Self;
        type Writer = Self;
        fn reader(&mut self) -> &mut Self {
            self
        }
        fn writer(&mut self) -> &mut Self {
            self
        }
        fn read_token(&self) -> corcovado::Token {
            self.inner.read_token()
        }
        fn write_token(&self) -> corcovado::Token {
            self.inner.write_token()
        }
        fn set_winsize(&mut self, size: WinsizeBuilder) -> io::Result<()> {
            self.inner.set_winsize(size)
        }
        fn register(
            &mut self,
            poll: &corcovado::Poll,
            tokens: &mut dyn Iterator<Item = corcovado::Token>,
            interest: corcovado::Ready,
            opts: corcovado::PollOpt,
        ) -> io::Result<()> {
            self.inner.register(poll, tokens, interest, opts)
        }
        fn reregister(
            &mut self,
            poll: &corcovado::Poll,
            interest: corcovado::Ready,
            opts: corcovado::PollOpt,
        ) -> io::Result<()> {
            self.inner.reregister(poll, interest, opts)
        }
        fn deregister(&mut self, poll: &corcovado::Poll) -> io::Result<()> {
            self.inner.deregister(poll)
        }
    }

    impl EventedPty for Timed {
        fn shutdown(&mut self) -> io::Result<()> {
            self.inner.shutdown()
        }
        fn child_event_token(&self) -> corcovado::Token {
            self.inner.child_event_token()
        }
        fn next_child_event(&mut self) -> Option<ChildEvent> {
            self.inner.next_child_event()
        }
    }

    /// Stands in for the UI thread: forwards a `PtyWrite` to the reader
    /// thread immediately and timestamps damage notices.
    #[derive(Clone)]
    struct Listener {
        damaged: mpsc::Sender<Instant>,
        to_reader: Arc<Mutex<Option<corcovado::channel::Sender<Msg>>>>,
    }

    impl EventListener for Listener {
        fn send_event(&self, event: RioEvent, _: WindowId) {
            match event {
                RioEvent::TerminalDamaged(_) => {
                    let _ = self.damaged.send(Instant::now());
                }
                RioEvent::PtyWrite(_, text) => {
                    if let Some(channel) = self.to_reader.lock().unwrap().as_ref() {
                        let _ = channel.send(Msg::Input(text.into_bytes().into()));
                    }
                }
                _ => {}
            }
        }
    }

    fn stats(name: &str, mut samples: Vec<Duration>) {
        samples.sort();
        let at = |q: f64| samples[((samples.len() - 1) as f64 * q) as usize];
        println!(
            "{name:<28} median {:>9.1?}  p90 {:>9.1?}  min {:>9.1?}  n={}",
            at(0.5),
            at(0.9),
            samples[0],
            samples.len()
        );
    }

    pub fn run() {
        let rounds: usize = std::env::args()
            .nth(1)
            .and_then(|n| n.parse().ok())
            .unwrap_or(300);

        let (writes_tx, writes) = mpsc::channel();
        let (damaged_tx, damaged) = mpsc::channel();
        let to_reader = Arc::new(Mutex::new(None));
        let listener = Listener {
            damaged: damaged_tx,
            to_reader: Arc::clone(&to_reader),
        };

        // `cat` on a raw PTY echoes exactly what it is given, escapes too.
        let pty = teletypewriter::create_pty_with_spawn(
            Some("/bin/sh"),
            vec!["-c".into(), "stty raw -echo; exec cat".into()],
            &None,
            None,
            80,
            24,
            800,
            600,
        )
        .expect("pty");
        let terminal = Arc::new(FairMutex::new(Crosswords::new(
            CrosswordsSize::new(80, 24),
            CursorShape::Block,
            listener.clone(),
            WindowId::from(0),
            0,
            0,
        )));
        let machine = Machine::new(
            Arc::clone(&terminal),
            Timed {
                inner: pty,
                writes: writes_tx,
            },
            listener,
            WindowId::from(0),
            0,
        )
        .expect("machine");
        let channel = machine.channel();
        *to_reader.lock().unwrap() = Some(channel.clone());
        let _reader = machine.spawn();

        let wait = Duration::from_secs(5);
        // Until `cat` echoes, the shell is still setting the line up.
        channel.send(Msg::Input(b"warm".to_vec().into())).unwrap();
        damaged.recv_timeout(wait).expect("cat is up");
        let rearm = || {
            terminal.lock().damage_event_in_flight = false;
            while damaged.try_recv().is_ok() {}
            while writes.try_recv().is_ok() {}
        };

        let mut input = Vec::with_capacity(rounds);
        let mut echo = Vec::with_capacity(rounds);
        for _ in 0..rounds {
            rearm();
            let sent = Instant::now();
            channel.send(Msg::Input(b"x".to_vec().into())).unwrap();
            let (written, _) = writes.recv_timeout(wait).expect("keystroke written");
            input.push(written - sent);
            let seen = damaged.recv_timeout(wait).expect("echo parsed");
            echo.push(seen - sent);
            std::thread::sleep(Duration::from_millis(1));
        }

        let mut reply = Vec::with_capacity(rounds);
        for _ in 0..rounds {
            rearm();
            channel.send(Msg::Input(DA.to_vec().into())).unwrap();
            let (query_written, bytes) =
                writes.recv_timeout(wait).expect("query written");
            assert_eq!(bytes, DA);
            let (replied, bytes) = writes.recv_timeout(wait).expect("reply written");
            assert!(bytes.starts_with(DA_REPLY), "{bytes:?}");
            reply.push(replied - query_written);
            std::thread::sleep(Duration::from_millis(1));
        }

        stats("keystroke to pty write", input);
        stats("keystroke to echo parsed", echo);
        stats("DA query to reply written", reply);
        channel.send(Msg::Shutdown).unwrap();
    }
}

#[cfg(unix)]
fn main() {
    unix::run();
}

#[cfg(not(unix))]
fn main() {
    eprintln!("this example spawns cat on a Unix pseudo terminal");
}
