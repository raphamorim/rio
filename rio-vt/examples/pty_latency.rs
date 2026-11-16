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
//! Run with `cargo run -p rio-vt --release --example pty_latency [rounds]`.
//!
//! `pty_latency flood [MiB] [plain|sgr|emoji]` measures the other
//! side: a child floods the PTY with text (`cat` of a prepared file,
//! as fast as the PTY delivers) while a thread stands in for the
//! renderer, taking the terminal lock every 8 ms and holding it for
//! 200 µs as a frame snapshot would, and the time each acquisition
//! waited is recorded, along with the drain throughput.
//!
//! Unix only: it spawns the child on a pseudo terminal.

#[cfg(unix)]
mod unix {
    use std::io::{self, Read, Write};
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::{Duration, Instant};

    use rio_vt::ansi::CursorShape;
    use rio_vt::corcovado;
    use rio_vt::crosswords::{Crosswords, CrosswordsSize};
    use rio_vt::event::sync::TerminalLock;
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

    /// The write half, every write announced like the whole PTY's.
    struct TimedWriteHalf {
        inner: <teletypewriter::Pty as EventedPty>::WriteHalf,
        writes: mpsc::Sender<(Instant, Vec<u8>)>,
    }

    impl Write for TimedWriteHalf {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let n = self.inner.write(buf)?;
            let _ = self.writes.send((Instant::now(), buf[..n].to_vec()));
            Ok(n)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.inner.flush()
        }
    }

    impl corcovado::event::Evented for TimedWriteHalf {
        fn register(
            &self,
            poll: &corcovado::Poll,
            token: corcovado::Token,
            interest: corcovado::Ready,
            opts: corcovado::PollOpt,
        ) -> io::Result<()> {
            self.inner.register(poll, token, interest, opts)
        }
        fn reregister(
            &self,
            poll: &corcovado::Poll,
            token: corcovado::Token,
            interest: corcovado::Ready,
            opts: corcovado::PollOpt,
        ) -> io::Result<()> {
            self.inner.reregister(poll, token, interest, opts)
        }
        fn deregister(&self, poll: &corcovado::Poll) -> io::Result<()> {
            self.inner.deregister(poll)
        }
    }

    impl EventedPty for Timed {
        type WriteHalf = TimedWriteHalf;
        fn write_half(&mut self) -> io::Result<TimedWriteHalf> {
            Ok(TimedWriteHalf {
                inner: self.inner.write_half()?,
                writes: self.writes.clone(),
            })
        }
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
            "{name:<28} median {:>9.1?}  p90 {:>9.1?}  max {:>9.1?}  min {:>9.1?}  n={}",
            at(0.5),
            at(0.9),
            samples[samples.len() - 1],
            samples[0],
            samples.len()
        );
    }

    /// A child floods the PTY; a renderer stand-in asks for the
    /// terminal every 8 ms and holds it 200 µs; how long it waits.
    fn flood(mib: usize, kind: &str) {
        let line = match kind {
            "sgr" => "\x1b[31mword\x1b[0m \x1b[1;32mbold\x1b[0m \x1b[4munder\x1b[0m \x1b[38;5;208mcolor\x1b[0m \x1b[38;2;10;200;90mtrue\x1b[0m \x1b[3mital\x1b[0m end\r\n",
            "emoji" => "status: \u{2705} ok \u{1F680} deploy \u{1F44D}\u{1F3FD} \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467} \u{1F1E7}\u{1F1F7} done \u{2764}\u{FE0F} \u{1F600}\u{1F601}\u{1F602}\r\n",
            _ => "the quick brown fox jumps over the lazy dog 0123456789 lorem ipsum dolor sit amet\r\n",
        };
        let bytes = mib * 1024 * 1024;
        let mut payload = line.repeat(bytes / line.len() + 1).into_bytes();
        payload.truncate(bytes);
        // `cat` of a file is as fast as the PTY delivers; a generator
        // in the child would be the bottleneck instead.
        let path = std::env::temp_dir()
            .join(format!("rio-pty-flood-{}.txt", std::process::id()));
        std::fs::write(&path, &payload).expect("flood file");
        let command = format!("stty raw -echo; exec cat {}", path.display());

        let (writes_tx, _writes) = mpsc::channel();
        let (damaged_tx, _damaged) = mpsc::channel();
        let listener = Listener {
            damaged: damaged_tx,
            to_reader: Arc::new(Mutex::new(None)),
        };
        let pty = teletypewriter::create_pty_with_spawn(
            Some("/bin/sh"),
            vec!["-c".into(), command],
            &None,
            None,
            200,
            50,
            2000,
            1200,
        )
        .expect("pty");
        let terminal = Arc::new(TerminalLock::new(Crosswords::new(
            CrosswordsSize::new(200, 50),
            CursorShape::Block,
            listener.clone(),
            WindowId::from(0),
            0,
            10_000,
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

        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let renderer = {
            let terminal = Arc::clone(&terminal);
            let done = Arc::clone(&done);
            std::thread::spawn(move || {
                let mut waits = Vec::new();
                while !done.load(std::sync::atomic::Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(8));
                    let asked = Instant::now();
                    let guard = terminal.lock();
                    waits.push(asked.elapsed());
                    // The snapshot: a frame's worth of reading the grid.
                    let hold = Instant::now();
                    while hold.elapsed() < Duration::from_micros(200) {
                        std::hint::spin_loop();
                    }
                    drop(guard);
                }
                waits
            })
        };

        let started = Instant::now();
        let reader = machine.spawn();
        let (_machine, _state) = reader.join().expect("reader thread");
        let elapsed = started.elapsed();
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        let waits = renderer.join().expect("renderer thread");
        let _ = std::fs::remove_file(&path);

        println!(
            "flood {mib} MiB {kind}: drained in {:.0?}, {:.0} MiB/s",
            elapsed,
            mib as f64 / elapsed.as_secs_f64()
        );
        stats("renderer lock wait", waits);
    }

    pub fn run() {
        let mut args = std::env::args().skip(1);
        let first = args.next();
        if first.as_deref() == Some("flood") {
            let mib: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(32);
            let kind = args.next().unwrap_or_else(|| "plain".into());
            flood(mib, &kind);
            return;
        }
        let rounds: usize = first.and_then(|n| n.parse().ok()).unwrap_or(300);

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
        let terminal = Arc::new(TerminalLock::new(Crosswords::new(
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
