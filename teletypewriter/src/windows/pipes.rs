use crate::windows::spsc::*;
use corcovado::{
    event::Evented, Poll, PollOpt, Ready, Registration, SetReadiness, Token,
};
use miow::pipe::{AnonRead, AnonWrite};
use windows_sys::Win32::System::IO::CancelSynchronousIo;

use std::io;
use std::os::windows::io::AsRawHandle;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{channel, Receiver, TryRecvError},
    Arc,
};
use std::thread::{park, spawn, JoinHandle};

struct EventedAnonReadInner {
    registration: Registration,
    readiness: SetReadiness,
    done: AtomicBool,
}

/// Wraps an AnonRead pipe so that it can be read asynchronously using mio.
///
/// This is achieved by spawning a worker thread which continuously attempts
/// to read from the pipe into a buffer, which reads from the EventedAnonRead
/// object will be directed to.
///
/// This should only be considered if your application architecture requires
/// a synchronous anonymous pipe; an asynchronous NamedPipe will likely be
/// more performant.
pub struct EventedAnonRead {
    // Is an Option so it can be moved out and joined in the Drop impl.
    thread: Option<JoinHandle<()>>,
    consumer: SpscBufferReader,
    inner: Arc<EventedAnonReadInner>,
    error_receiver: Receiver<String>,
    // The worker's error, kept until the ring it filled is read
    ended: Option<String>,
}

/// Wakes a worker that `done` was set for, out of its park or out of
/// its read or write of the pipe, until it has ended. An unpark is kept
/// for a thread that is yet to park, but a cancel is not kept for one
/// that is only on its way to the pipe, so the cancel is repeated.
fn stop_worker(thread: &JoinHandle<()>) {
    thread.thread().unpark();
    while !thread.is_finished() {
        unsafe {
            CancelSynchronousIo(thread.as_raw_handle());
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

// Helper to send an error string from the worker threads
macro_rules! try_or_send {
    ($e:expr, $sender:ident) => {
        match $e {
            Ok(value) => value,
            Err(e) => {
                $sender
                    .send(format!("{}", e))
                    .expect("Could not send error");
                return;
            }
        }
    };
}

impl EventedAnonRead {
    pub fn new(mut pipe: AnonRead) -> Self {
        let (registration, readiness) = Registration::new2();

        let (mut producer, consumer) = spsc_buffer(65536);

        let done = AtomicBool::new(false);

        let (error_sender, error_receiver) = channel();

        let inner = Arc::new(EventedAnonReadInner {
            registration,
            readiness,
            done,
        });

        let thread = {
            let inner = inner.clone();
            spawn(move || {
                use std::io::Read;

                let mut tmp_buf = [0u8; 65535];

                loop {
                    if inner.done.load(Ordering::SeqCst) {
                        return;
                    }

                    // Read into temp buffer. The pipe ends when the host
                    // closes its last write end (a broken pipe, or a
                    // zero-length read), and the consumer only looks for
                    // an error once it is woken, so wake it.
                    let nbytes = match pipe.read(&mut tmp_buf[..]) {
                        Ok(0) => {
                            let _ = error_sender.send("pipe closed".to_string());
                            let _ = inner.readiness.set_readiness(Ready::readable());
                            return;
                        }
                        Ok(nbytes) => nbytes,
                        Err(err) => {
                            let _ = error_sender.send(format!("{err}"));
                            let _ = inner.readiness.set_readiness(Ready::readable());
                            return;
                        }
                    };

                    // Write from the temp buffer into the producer
                    let mut written = 0usize;
                    while written < nbytes {
                        // Wait for buffer to clear if need be. An
                        // unpark is kept until the next park, so a read
                        // that lands between the look at the ring and
                        // the park still wakes this thread.
                        while producer.is_full() {
                            if inner.done.load(Ordering::SeqCst) {
                                return;
                            }
                            park();
                        }

                        written += producer.write_from_slice(&tmp_buf[written..nbytes]);

                        if !inner.readiness.readiness().is_readable() {
                            try_or_send!(
                                inner.readiness.set_readiness(Ready::readable()),
                                error_sender
                            );
                        }
                    }
                }
            })
        };

        Self {
            thread: Some(thread),
            consumer,
            inner,
            error_receiver,
            ended: None,
        }
    }
}

impl EventedAnonRead {
    /// Takes note of the worker's end, which it reports once, after
    /// the last bytes it put in the ring.
    fn look_for_end(&mut self) {
        if self.ended.is_some() {
            return;
        }
        match self.error_receiver.try_recv() {
            Ok(err) => self.ended = Some(err),
            Err(TryRecvError::Disconnected) => self.ended = Some(String::new()),
            Err(TryRecvError::Empty) => {}
        }
    }
}

impl io::Read for EventedAnonRead {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.thread.is_none() {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, ""));
        }

        // Looked for before the ring is read: the worker ends after its
        // last write to the ring, so once the end is seen here the ring
        // gets no more, and what it holds is read before the end is
        // returned. Returned first, the end would discard the last of
        // the output.
        self.look_for_end();

        let nbytes = self.consumer.read_to_slice(buf);

        if self.consumer.is_empty() {
            if self.ended.is_some() {
                if nbytes == 0 {
                    // The worker is closing, or has closed
                    if let Some(thread) = self.thread.take() {
                        let _ = thread.join();
                    }
                    let err = self.ended.take().unwrap_or_default();
                    return Err(io::Error::new(io::ErrorKind::BrokenPipe, err));
                }
                // Left readable, for the read that returns the end.
                return Ok(nbytes);
            }

            self.inner.readiness.set_readiness(Ready::empty())?;

            // Possible race: the consumer may think the queue is empty but by the time
            // the readiness is set the producer thread may have written data, or
            // have ended, which it also says by a readiness that was just undone
            //
            // We avoid the race by re-checking the queue is empty like this, and undo the
            // readiness setting if necessary.
            self.look_for_end();
            if !self.consumer.is_empty() || self.ended.is_some() {
                self.inner.readiness.set_readiness(Ready::readable())?;
            }
        }

        if let Some(thread) = &self.thread {
            thread.thread().unpark();
        }
        Ok(nbytes)
    }
}

impl Evented for EventedAnonRead {
    fn register(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        poll.register(&self.inner.registration, token, interest, opts)
    }

    fn reregister(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        poll.reregister(&self.inner.registration, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        poll.deregister(&self.inner.registration)
    }
}

impl Drop for EventedAnonRead {
    fn drop(&mut self) {
        self.inner.done.store(true, Ordering::SeqCst);

        // A read that returned the worker's error has already joined
        // it: the pipe ended, as it does when the console host exits.
        let Some(thread) = self.thread.take() else {
            return;
        };

        // Stop reader thread waiting for room in the ring, or for pipe
        // contents
        stop_worker(&thread);

        thread
            .join()
            .expect("Could not close EventedAnonRead worker");
    }
}

struct EventedAnonWriteInner {
    registration: Registration,
    readiness: SetReadiness,
    done: AtomicBool,
}

/// Wraps an AnonWrite pipe so that it can be written asynchronously using mio.
///
/// This is achieved by spawning a worker thread which continuously attempts
/// to write to the pipe from a buffer, which writes to the EventedAnonWrite
/// object will be directed to.
///
/// This should only be considered if your application architecture requires
/// a synchronous anonymous pipe; an asynchronous NamedPipe will likely be
/// more performant.
pub struct EventedAnonWrite {
    // Is an Option so it can be moved out and joined in the Drop impl
    thread: Option<JoinHandle<()>>,
    producer: SpscBufferWriter,
    inner: Arc<EventedAnonWriteInner>,
    error_receiver: Receiver<String>,
}

impl EventedAnonWrite {
    pub fn new(mut pipe: AnonWrite) -> Self {
        let (registration, readiness) = Registration::new2();

        let (producer, mut consumer) = spsc_buffer(65536);

        let done = AtomicBool::new(false);

        let inner = Arc::new(EventedAnonWriteInner {
            registration,
            readiness,
            done,
        });

        let (error_sender, error_receiver) = channel();

        let thread = {
            let inner = inner.clone();
            spawn(move || {
                use std::io::Write;
                let mut tmp_buf = [0u8; 65535];

                try_or_send!(
                    inner.readiness.set_readiness(Ready::writable()),
                    error_sender
                );

                loop {
                    if inner.done.load(Ordering::SeqCst) {
                        return;
                    }

                    // Read into temp buffer
                    let nbytes = {
                        // Wait for buffer to have contents. An unpark
                        // is kept until the next park, so a write that
                        // lands between the look at the ring and the
                        // park still wakes this thread. Were it lost,
                        // its bytes would stay in the ring until the
                        // next write: a key pressed and not delivered
                        // until another one was.
                        while consumer.is_empty() {
                            if inner.done.load(Ordering::SeqCst) {
                                return;
                            }
                            park();
                        }

                        let nbytes = consumer.read_to_slice(&mut tmp_buf);

                        if !inner.readiness.readiness().is_writable() {
                            try_or_send!(
                                inner.readiness.set_readiness(Ready::writable()),
                                error_sender
                            );
                        }

                        nbytes
                    };

                    let mut written = 0usize;
                    while written < nbytes {
                        written += try_or_send!(
                            pipe.write(&tmp_buf[written..nbytes]),
                            error_sender
                        );
                    }
                }
            })
        };

        Self {
            thread: Some(thread),
            producer,
            inner,
            error_receiver,
        }
    }
}

impl io::Write for EventedAnonWrite {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.thread.is_none() {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, ""));
        }

        match self.error_receiver.try_recv() {
            Ok(err) => {
                // Other thread will be closing
                self.thread.take().unwrap().join().unwrap();
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, err));
            }
            Err(TryRecvError::Disconnected) => {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, ""))
            }
            Err(TryRecvError::Empty) => {}
        }

        let nbytes = self.producer.write_from_slice(buf);
        if self.producer.is_full() {
            self.inner.readiness.set_readiness(Ready::empty())?;

            // Possible race: the producer may think the buffer is full but by the time
            // the readiness is set the consumer thread may have read data
            //
            // It is sufficient to re-check the buffer is empty, and undo the readiness
            // setting to work around this.
            if !self.producer.is_full() {
                self.inner.readiness.set_readiness(Ready::writable())?;
            }
        }

        if let Some(thread) = &self.thread {
            thread.thread().unpark();
        }
        Ok(nbytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Evented for EventedAnonWrite {
    fn register(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        poll.register(&self.inner.registration, token, interest, opts)
    }

    fn reregister(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        poll.reregister(&self.inner.registration, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        poll.deregister(&self.inner.registration)
    }
}

impl Drop for EventedAnonWrite {
    fn drop(&mut self) {
        self.inner.done.store(true, Ordering::SeqCst);

        // A write that returned the worker's error has already joined
        // it.
        let Some(thread) = self.thread.take() else {
            return;
        };

        // Stop the writer thread, which waits either for contents or in
        // a write to a full pipe that nobody reads, which never
        // returns.
        stop_worker(&thread);

        thread
            .join()
            .expect("Could not close EventedAnonWrite worker");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::mpsc;
    use std::time::Duration;

    /// Each write reaches the pipe on its own. The next one is made as
    /// soon as the last arrived, when the worker is on its way to sleep
    /// on an empty ring; a write whose wakeup is lost there would stay
    /// in the ring, since no other write follows it.
    #[test]
    fn a_write_reaches_the_pipe_without_a_second_one() {
        let (mut pipe, conin) = miow::pipe::anonymous(0).unwrap();
        let mut writer = EventedAnonWrite::new(conin);

        let (tx, rx) = mpsc::channel();
        let reader = spawn(move || {
            let mut byte = [0u8; 1];
            while let Ok(1) = pipe.read(&mut byte) {
                if tx.send(byte[0]).is_err() {
                    return;
                }
            }
        });

        for i in 0..20_000u32 {
            let byte = i as u8;
            assert_eq!(writer.write(&[byte]).unwrap(), 1);
            let got = rx
                .recv_timeout(Duration::from_secs(10))
                .expect("the write was left in the ring");
            assert_eq!(got, byte);
        }

        // Dropping the writer joins its worker and closes the pipe,
        // which ends the reader.
        drop(writer);
        reader.join().unwrap();
    }

    /// A reader whose pipe ended, and that said so from `read`, has
    /// joined its worker by then, and is dropped without a panic.
    #[test]
    fn a_reader_is_dropped_after_its_pipe_ended() {
        let (conout, host) = miow::pipe::anonymous(0).unwrap();
        let mut reader = EventedAnonRead::new(conout);
        drop(host);

        let mut buf = [0u8; 16];
        let mut tries = 0;
        let err = loop {
            match reader.read(&mut buf) {
                Ok(_nbytes) => {
                    tries += 1;
                    assert!(tries < 10_000, "the end of the pipe was not reported");
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(err) => break err,
            }
        };
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);

        drop(reader);
    }

    /// What was written to the pipe before it ended is read before
    /// the end is, though the worker reported the end long before.
    #[test]
    fn a_reader_returns_the_last_bytes_before_the_end_of_its_pipe() {
        let (conout, mut host) = miow::pipe::anonymous(0).unwrap();
        let mut reader = EventedAnonRead::new(conout);
        host.write_all(b"last").unwrap();
        drop(host);
        std::thread::sleep(Duration::from_millis(200));

        let mut got = Vec::new();
        let mut buf = [0u8; 3];
        let mut tries = 0;
        let err = loop {
            match reader.read(&mut buf) {
                Ok(nbytes) => {
                    got.extend_from_slice(&buf[..nbytes]);
                    tries += 1;
                    assert!(tries < 10_000, "the end of the pipe was not reported");
                    if nbytes == 0 {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
                Err(err) => break err,
            }
        };
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(got, b"last");
    }

    /// A reader is dropped at once, when its worker may not have got
    /// to its read yet, with a pipe nothing is written to: a single
    /// cancel that came before the read would leave it there.
    #[test]
    fn a_reader_is_dropped_before_its_worker_reads() {
        for _ in 0..200 {
            let (conout, host) = miow::pipe::anonymous(0).unwrap();

            // The thread that drops it is started first, so the drop
            // comes as soon after the worker's start as it can.
            let (to_dropper, reader) = mpsc::channel::<EventedAnonRead>();
            let (tx, rx) = mpsc::channel();
            let dropper = spawn(move || {
                drop(reader.recv().unwrap());
                let _ = tx.send(());
            });
            to_dropper.send(EventedAnonRead::new(conout)).unwrap();

            rx.recv_timeout(Duration::from_secs(10))
                .expect("the drop waited for a read of an idle pipe");
            dropper.join().unwrap();

            // Only now does the pipe lose its writer.
            drop(host);
        }
    }

    /// A writer whose pipe has no reader any more, and that said so
    /// from `write`, is dropped without a panic as well.
    #[test]
    fn a_writer_is_dropped_after_its_pipe_ended() {
        let (host, conin) = miow::pipe::anonymous(0).unwrap();
        let mut writer = EventedAnonWrite::new(conin);
        drop(host);

        let mut tries = 0;
        let err = loop {
            match writer.write(&[0]) {
                Ok(_nbytes) => {
                    tries += 1;
                    assert!(tries < 10_000, "the end of the pipe was not reported");
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(err) => break err,
            }
        };
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);

        drop(writer);
    }

    /// A writer whose worker is in a write that nobody reads is still
    /// dropped: the write is cancelled, not waited for.
    #[test]
    fn a_writer_is_dropped_while_its_pipe_is_full() {
        let (host, conin) = miow::pipe::anonymous(0).unwrap();
        let mut writer = EventedAnonWrite::new(conin);

        // Written until the ring stays full after the worker took from
        // it, which it only does once the worker takes no more: it is
        // then in a write the pipe has no room for.
        let chunk = [0u8; 65536];
        let mut sent = 0usize;
        let mut waits = 0;
        let mut full_for = 0;
        while full_for < 200 {
            sent += writer.write(&chunk).unwrap();
            assert!(sent < 32 << 20, "the pipe took all that was written");
            if !writer.producer.is_full() {
                full_for = 0;
                continue;
            }
            // The first 65536 bytes fill the ring with no help from
            // the worker; more than that were sent only if it took
            // some.
            if sent > 65536 {
                full_for += 1;
            }
            waits += 1;
            assert!(waits < 20_000, "the worker took nothing from the ring");
            std::thread::sleep(Duration::from_millis(1));
        }

        let (tx, rx) = mpsc::channel();
        let dropper = spawn(move || {
            drop(writer);
            let _ = tx.send(());
        });
        rx.recv_timeout(Duration::from_secs(10))
            .expect("the drop waited for a write nobody reads");
        dropper.join().unwrap();

        // Only now does the pipe lose its reader.
        drop(host);
    }
}
