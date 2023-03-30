use crate::crosswords::Crosswords;
use crate::event::sync::FairMutex;
use crate::event::EventListener;
use crate::event::EventP;
use crate::event::EventProxy;
use crate::event::Msg;
use mio::{Events, Interest, Token};
use std::borrow::Cow;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Read};
use std::sync::Arc;
use std::time::Instant;
use vte::Parser;
use winit::event_loop::EventLoopProxy;

use std::io::{ErrorKind, Write};

const READ_BUFFER_SIZE: usize = 0x10_0000;
/// Max bytes to read from the PTY while the terminal is locked.
const MAX_LOCKED_READ: usize = u16::MAX as usize;

pub struct Machine<T: teletypewriter::ProcessReadWrite, U: EventListener> {
    // handler: Performer,
    // parser: Parser,
    pty: T,
    stream: mio::net::TcpStream,
    events: mio::Events,
    poll: mio::Poll,
    terminal: Arc<FairMutex<Crosswords<U>>>,
    event_proxy: U,
}

#[derive(Default)]
pub struct State {
    write_list: VecDeque<Cow<'static, [u8]>>,
    writing: Option<Writing>,
    parser: Parser,
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
    T: teletypewriter::ProcessReadWrite + Send + mio::event::Source + 'static,
    U: EventListener + Send + 'static,
{
    pub fn new(
        terminal: Arc<FairMutex<Crosswords<U>>>,
        pty: T,
        event_proxy: U,
    ) -> Result<Machine<T, U>, Box<dyn std::error::Error>> {
        let poll = mio::Poll::new()?;
        let addr: std::net::SocketAddr = "127.0.0.1:0".parse()?;
        let server = mio::net::TcpListener::bind(addr)?;
        let events = mio::Events::with_capacity(1024);
        let stream = mio::net::TcpStream::connect(server.local_addr()?)?;
        Ok(Machine {
            poll,
            events,
            stream,
            pty,
            terminal,
            event_proxy,
        })
    }

    // #[inline]
    // pub fn sync_timeout(&self) -> Option<&Instant> {
    //     self.state.sync_state.timeout.as_ref()
    // }

    // /// Number of bytes in the synchronization buffer.
    // #[inline]
    // pub fn sync_bytes_count(&self) -> usize {
    //     self.state.sync_state.buffer.len()
    // }

    // pub fn process(&mut self, process: Pty) {
    //     let reader = BufReader::new(process);
    //     let handler = Performer::new(visible_rows_arc, columns, rows);
    //     let parser = Parser::new();
    //     for byte in reader.bytes() {
    //         parser
    //             .advance(&mut handler, *byte.as_ref().unwrap());
    //     }
    // }

    #[inline]
    fn pty_read<X>(
        &mut self,
        state: &mut State,
        buf: &mut [u8],
        mut writer: Option<&mut X>,
    ) -> io::Result<()>
    where
        X: Write,
    {
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

            // Write a copy of the bytes to the ref test file.
            if let Some(writer) = &mut writer {
                writer.write_all(&buf[..unprocessed]).unwrap();
            }

            // Parse the incoming bytes.
            for byte in &buf[..unprocessed] {
                // state.parser.advance(&mut **terminal, *byte);
            }

            processed += unprocessed;
            unprocessed = 0;

            // Assure we're not blocking the terminal too long unnecessarily.
            if processed >= MAX_LOCKED_READ {
                break;
            }
        }

        // Queue terminal redraw unless all processed bytes were synchronized.
        // if state.parser.sync_bytes_count() < processed && processed > 0 {
        // self.event_proxy.send_event(Event::Wakeup);
        // }

        Ok(())
    }

    fn drain_recv_channel(&mut self, state: &mut State) -> bool {
        // while let Ok(msg) = self.stream.try_recv() {
        //     match msg {
        //         Msg::Input(input) => state.write_list.push_back(input),
        //         // Msg::Resize(window_size) => self.pty.on_resize(window_size),
        //         Msg::Shutdown => return false,
        //     }
        // }

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
            .reregister(&mut self.stream, token, Interest::READABLE)
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

    pub fn spawn(mut self) {
        tokio::spawn(async move {
            let mut state = State::default();
            let mut buf = [0u8; READ_BUFFER_SIZE];

            let mut tokens = Token(0);
            // let channel_token = tokens.next().unwrap();
            let register = self
                .poll
                .registry()
                .register(&mut self.stream, tokens, Interest::READABLE)
                .unwrap();

            // Register TTY through EventedRW interface.
            self.pty
                .register(&self.poll.registry(), tokens, Interest::READABLE)
                .unwrap();

            let mut events = Events::with_capacity(1024);

            // let mut pipe = None;

            'event_loop: loop {
                // Wakeup the event loop when a synchronized update timeout was reached.
                // let sync_timeout = state.parser.sync_timeout();
                // let timeout =
                // sync_timeout.map(|st| st.saturating_duration_since(Instant::now()));

                // if let Err(err) = self.poll.poll(&mut events, timeout) {
                //     match err.kind() {
                //         ErrorKind::Interrupted => continue,
                //         _ => panic!("EventLoop polling error: {err:?}"),
                //     }
                // }

                // Handle synchronized update timeout.
                if events.is_empty() {
                    // state.parser.stop_sync(&mut *self.terminal.lock());
                    // self.event_proxy.send_event(Event::Wakeup);
                    continue;
                }

                for event in events.iter() {
                    println!("{:?}", event);
                    break 'event_loop;
                    // match event.token() {
                    // token if token == channel_token => {
                    //     if !self.channel_event(channel_token, &mut state) {
                    //         break 'event_loop;
                    //     }
                    // }

                    // token if token == self.pty.child_event_token() => {
                    //     if let Some(tty::ChildEvent::Exited) =
                    //         self.pty.next_child_event()
                    //     {
                    //         if self.hold {
                    //             // With hold enabled, make sure the PTY is drained.
                    //             let _ = self.pty_read(
                    //                 &mut state,
                    //                 &mut buf,
                    //                 pipe.as_mut(),
                    //             );
                    //         } else {
                    //             // Without hold, shutdown the terminal.
                    //             self.terminal.lock().exit();
                    //         }

                    //         self.event_proxy.send_event(Event::Wakeup);
                    //         break 'event_loop;
                    //     }
                    // }

                    // token
                    //     if token == self.pty.read_token()
                    //         || token == self.pty.write_token() =>
                    // {
                    //     #[cfg(unix)]
                    //     if UnixReady::from(event.readiness()).is_hup() {
                    //         // Don't try to do I/O on a dead PTY.
                    //         continue;
                    //     }

                    //     if event.readiness().is_readable() {
                    //         if let Err(err) =
                    //             self.pty_read(&mut state, &mut buf, pipe.as_mut())
                    //         {
                    //             // On Linux, a `read` on the master side of a PTY can fail
                    //             // with `EIO` if the client side hangs up.  In that case,
                    //             // just loop back round for the inevitable `Exited` event.
                    //             // This sucks, but checking the process is either racy or
                    //             // blocking.
                    //             #[cfg(target_os = "linux")]
                    //             if err.raw_os_error() == Some(libc::EIO) {
                    //                 continue;
                    //             }

                    //             error!(
                    //                 "Error reading from PTY in event loop: {}",
                    //                 err
                    //             );
                    //             break 'event_loop;
                    //         }
                    //     }

                    //     if event.readiness().is_writable() {
                    //         if let Err(err) = self.pty_write(&mut state) {
                    //             error!("Error writing to PTY in event loop: {}", err);
                    //             break 'event_loop;
                    //         }
                    //     }
                    // }
                    //     _ => (),
                    // }
                }

                // Register write interest if necessary.
                let mut interest = Interest::READABLE;
                if state.needs_write() {
                    interest.add(Interest::WRITABLE);
                }
                // Reregister with new interest.
                // self.pty
                // .reregister(&self.poll, interest, poll_opts)
                // .unwrap();
            }

            // The evented instances are not dropped here so deregister them explicitly.
            let _ = self.poll.registry().deregister(&mut self.stream);
            let _ = self.pty.deregister(&self.poll.registry());

            (self, state)

            // let reader = BufReader::new(self.pty);
            // let handler = Performer::new(columns, rows);
            // let parser = Parser::new();
            // for byte in reader.bytes() {
            //     parser
            //         .advance(&mut handler, *byte.as_ref().unwrap());
            // }
        });
    }
}
