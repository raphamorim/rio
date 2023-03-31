mod control;
pub mod handler;

use crate::crosswords::Crosswords;
use crate::event::sync::FairMutex;
use crate::event::EventListener;
use mio::unix::pipe::{Receiver, Sender};
use std::os::fd::AsRawFd;

use crate::event::{Msg, RioEvent};
use mio::{Events, Interest, Token};
use std::borrow::Cow;
use std::collections::VecDeque;

use std::io::{self, Read};
use std::sync::Arc;
use std::time::Instant;

use std::io::{ErrorKind, Write};

const READ_BUFFER_SIZE: usize = 0x10_0000;
/// Max bytes to read from the PTY while the terminal is locked.
const MAX_LOCKED_READ: usize = u16::MAX as usize;

pub struct Machine<T: teletypewriter::EventedPty, U: EventListener> {
    sender: Sender,
    receiver: Receiver,
    pty: T,
    stream: mio::net::TcpStream,
    poll: mio::Poll,
    terminal: Arc<FairMutex<Crosswords<U>>>,
    event_proxy: U,
}

#[derive(Default)]
pub struct State {
    write_list: VecDeque<Cow<'static, [u8]>>,
    writing: Option<Writing>,
    parser: handler::ParserProcessor,
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
    T: teletypewriter::EventedPty + Send + mio::event::Source + 'static,
    U: EventListener + Send + 'static,
{
    pub fn new(
        terminal: Arc<FairMutex<Crosswords<U>>>,
        pty: T,
        event_proxy: U,
    ) -> Result<Machine<T, U>, Box<dyn std::error::Error>> {
        let (sender, receiver) = mio::unix::pipe::new()?;
        let poll = mio::Poll::new()?;
        let addr: std::net::SocketAddr = "127.0.0.1:0".parse()?;
        let server = mio::net::TcpListener::bind(addr)?;
        let stream = mio::net::TcpStream::connect(server.local_addr()?)?;
        Ok(Machine {
            sender,
            receiver,
            poll,
            stream,
            pty,
            terminal,
            event_proxy,
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
                Ok(got) => {
                    unprocessed += got
                },
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
            for byte in &buf[..unprocessed] {
                state.parser.advance(&mut **terminal, *byte);
            }

            processed += unprocessed;
            unprocessed = 0;

            // Assure we're not blocking the terminal too long unnecessarily.
            if processed >= MAX_LOCKED_READ {
                break;
            }
        }

        // Queue terminal redraw unless all processed bytes were synchronized.
        if state.parser.sync_bytes_count() < processed && processed > 0 {
            self.event_proxy.send_event(RioEvent::Wakeup);
        }

        Ok(())
    }

    fn drain_recv_channel(&mut self, state: &mut State) -> bool {
        // while let Ok(msg) = self.receiver.try_recv() {
        //     match msg {
        //         Msg::Input(input) => state.write_list.push_back(input),
        //         // Msg::Resize(window_size) => self.pty.on_resize(window_size),
        //         Msg::Shutdown => return false,
        //     }
        // }

        // Read from the receiver using a direct libc call
        let mut buf = [0; 512];
        let n = self
            .receiver
            .try_io(|| {
                let buf_ptr = &mut buf as *mut _ as *mut _;
                let res =
                    unsafe { libc::read(self.receiver.as_raw_fd(), buf_ptr, buf.len()) };
                if res != -1 {
                    Ok(res as usize)
                } else {
                    // If EAGAIN or EWOULDBLOCK is set by libc::read, the closure
                    // should return `WouldBlock` error.
                    Err(io::Error::last_os_error())
                }
            })
            .unwrap_or(0);

        eprintln!("read {} bytes", n);

        true
    }

    /// Returns a `bool` indicating whether or not the event loop should continue running.
    #[inline]
    fn channel_event(&mut self, token: mio::Token, state: &mut State) -> bool {
        if !self.drain_recv_channel(state) {
            return false;
        }

        self.poll
            .registry()
            .reregister(&mut self.receiver, token, Interest::READABLE)
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

    pub fn channel(&self) -> i32 {
        self.sender.as_raw_fd()
    }

    pub fn spawn(mut self) {
        tokio::spawn(async move {
            let mut state = State::default();
            let mut buf = [0u8; READ_BUFFER_SIZE];

            let mut tokens = Token(0);
            let register = self
                .poll
                .registry()
                .register(&mut self.receiver, tokens, Interest::READABLE)
                .unwrap();

            // Register TTY through EventedRW interface.
            self.pty
                .register(&self.poll.registry(), tokens, Interest::READABLE)
                .unwrap();

            let mut events = Events::with_capacity(1024);
            let mut channel_token = 0;

            'event_loop: loop {
                // Wakeup the event loop when a synchronized update timeout was reached.
                let sync_timeout = state.parser.sync_timeout();
                let timeout =
                    sync_timeout.map(|st| st.saturating_duration_since(Instant::now()));

                if let Err(err) = self.poll.poll(&mut events, timeout) {
                    match err.kind() {
                        ErrorKind::Interrupted => continue,
                        _ => panic!("EventLoop polling error: {err:?}"),
                    }
                }

                // Handle synchronized update timeout.
                if events.is_empty() {
                    state.parser.stop_sync(&mut *self.terminal.lock());
                    self.event_proxy.send_event(RioEvent::Wakeup);
                    continue;
                }

                for event in events.iter() {
                    // println!(
                    //     "{:?} {:?}",
                    //     event,
                    //     event.token() == self.pty.child_event_token()
                    // );
                    match event.token() {
                        // token if token == mio::Token(channel_token) => {
                        //     if !self.channel_event(mio::Token(channel_token), &mut state)
                        //     {
                        //         println!("quebrou");
                        //         break 'event_loop;
                        //     }
                        // }
                        token if token == self.pty.child_event_token() => {
                            if let Some(teletypewriter::ChildEvent::Exited) =
                                self.pty.next_child_event()
                            {
                                self.pty_read(&mut state, &mut buf);
                                self.event_proxy.send_event(RioEvent::Wakeup);
                                break 'event_loop;
                            }
                        }

                        token
                            if token == self.pty.read_token()
                                || token == self.pty.write_token() =>
                        {
                            #[cfg(unix)]
                            // if UnixReady::from(event.readiness()).is_hup() {
                            //     // Don't try to do I/O on a dead PTY.
                            //     continue;
                            // }

                            if event.is_readable() {
                                if let Err(err) =
                                    self.pty_read(&mut state, &mut buf)
                                {
                                    // On Linux, a `read` on the master side of a PTY can fail
                                    // with `EIO` if the client side hangs up.  In that case,
                                    // just loop back round for the inevitable `Exited` event.
                                    // This sucks, but checking the process is either racy or
                                    // blocking.
                                    #[cfg(target_os = "linux")]
                                    if err.raw_os_error() == Some(libc::EIO) {
                                        continue;
                                    }

                                    println!(
                                        "Error reading from PTY in event loop: {}",
                                        err
                                    );
                                    break 'event_loop;
                                }
                            }

                            if event.is_writable() {
                                if let Err(err) = self.pty_write(&mut state) {
                                    println!("Error writing to PTY in event loop: {}", err);
                                    break 'event_loop;
                                }
                            }
                        }
                        _ => (),
                    }
                }

                // Register write interest if necessary.
                let mut interest = Interest::READABLE;
                if state.needs_write() {
                    interest.add(Interest::WRITABLE);
                }
                // Reregister with new interest.
                self.pty
                    .reregister(&self.poll.registry(), tokens, Interest::READABLE)
                    .unwrap();
            }

            // The evented instances are not dropped here so deregister them explicitly.
            let _ = self.poll.registry().deregister(&mut self.receiver);
            let _ = self.pty.deregister(&self.poll.registry());

            (self, state)
        });
    }
}
