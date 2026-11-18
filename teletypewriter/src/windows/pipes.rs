use crate::windows::spsc::*;
use corcovado::{
    event::Evented, Poll, PollOpt, Ready, Registration, SetReadiness, Token,
};
use miow::pipe::{AnonRead, AnonWrite};
use parking_lot::{Condvar, Mutex};
use windows_sys::Win32::System::IO::CancelSynchronousIo;

use std::io;
use std::os::windows::io::AsRawHandle;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{channel, Receiver, TryRecvError},
    Arc,
};
use std::thread::{spawn, JoinHandle};

struct WaitTag {}

struct EventedAnonReadInner {
    registration: Registration,
    readiness: SetReadiness,
    done: AtomicBool,
    sig_buffer_not_full: Condvar,
    wait_tag: Mutex<WaitTag>,
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

        let sig_buffer_not_full = Condvar::new();
        let wait_tag = Mutex::new(WaitTag {});

        let (error_sender, error_receiver) = channel();

        let inner = Arc::new(EventedAnonReadInner {
            registration,
            readiness,
            done,
            sig_buffer_not_full,
            wait_tag,
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
                        // Wait for buffer to clear if need be. The ring
                        // is looked at again under the lock, which the
                        // other side takes before it notifies: a read
                        // that lands after the look above is then either
                        // seen here or notifies once this thread waits,
                        // and is not lost between the two.
                        if producer.is_full() {
                            let mut wait_tag = inner.wait_tag.lock();
                            while producer.is_full() && !inner.done.load(Ordering::SeqCst)
                            {
                                inner.sig_buffer_not_full.wait(&mut wait_tag);
                            }
                            if inner.done.load(Ordering::SeqCst) {
                                return;
                            }
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
        }
    }
}

impl io::Read for EventedAnonRead {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
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

        let nbytes = self.consumer.read_to_slice(buf);

        if self.consumer.is_empty() {
            self.inner.readiness.set_readiness(Ready::empty())?;

            // Possible race: the consumer may think the queue is empty but by the time
            // the readiness is set the producer thread may have written data
            //
            // We avoid the race by re-checking the queue is empty like this, and undo the
            // readiness setting if necessary.
            if !self.consumer.is_empty() {
                self.inner.readiness.set_readiness(Ready::readable())?;
            }
        }

        // Through the lock, so the worker is either yet to look at the
        // ring or already waiting, and never between the two.
        drop(self.inner.wait_tag.lock());
        self.inner.sig_buffer_not_full.notify_one();
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

        drop(self.inner.wait_tag.lock());
        self.inner.sig_buffer_not_full.notify_one();

        // A read that returned the worker's error has already joined
        // it: the pipe ended, as it does when the child exits.
        let Some(thread) = self.thread.take() else {
            return;
        };

        // Stop reader thread waiting for pipe contents
        unsafe {
            CancelSynchronousIo(thread.as_raw_handle());
        }

        thread
            .join()
            .expect("Could not close EventedAnonRead worker");
    }
}

struct EventedAnonWriteInner {
    registration: Registration,
    readiness: SetReadiness,
    done: AtomicBool,
    sig_buffer_not_empty: Condvar,
    wait_tag: Mutex<WaitTag>,
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

        let sig_buffer_not_empty = Condvar::new();
        let wait_tag = Mutex::new(WaitTag {});

        let inner = Arc::new(EventedAnonWriteInner {
            registration,
            readiness,
            done,
            sig_buffer_not_empty,
            wait_tag,
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

                    // Read into temp buffer while holding the lock
                    let nbytes = {
                        // Wait for buffer to have contents. The ring is
                        // looked at again under the lock, which the
                        // other side takes before it notifies: a write
                        // that lands after the look above is then either
                        // seen here or notifies once this thread waits.
                        // Without that its bytes stayed in the ring
                        // until the next write, a key pressed and not
                        // delivered until another one was.
                        if consumer.is_empty() {
                            let mut wait_tag = inner.wait_tag.lock();
                            while consumer.is_empty()
                                && !inner.done.load(Ordering::SeqCst)
                            {
                                inner.sig_buffer_not_empty.wait(&mut wait_tag);
                            }
                            if inner.done.load(Ordering::SeqCst) {
                                return;
                            }
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

        // Through the lock, so the worker is either yet to look at the
        // ring or already waiting, and never between the two.
        drop(self.inner.wait_tag.lock());
        self.inner.sig_buffer_not_empty.notify_one();
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

        // Stop the writer thread waiting for contents
        drop(self.inner.wait_tag.lock());
        self.inner.sig_buffer_not_empty.notify_one();

        // A write that returned the worker's error has already joined
        // it.
        let Some(thread) = self.thread.take() else {
            return;
        };

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

    /// The same for a writer whose pipe has no reader any more.
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
}
