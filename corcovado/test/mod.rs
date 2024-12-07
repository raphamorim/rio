extern crate bytes;
extern crate corcovado;
extern crate env_logger;
extern crate iovec;
extern crate net2;
extern crate slab;
extern crate tempdir;
extern crate tracing;

#[cfg(target_os = "fuchsia")]
extern crate fuchsia_zircon as zircon;

pub use ports::localhost;

mod test_close_on_drop;
mod test_custom_evented;
mod test_double_register;
mod test_echo_server;
mod test_local_addr_ready;
mod test_multicast;
mod test_oneshot;
mod test_poll;
mod test_register_deregister;
mod test_register_multiple_event_loops;
mod test_reregister_without_poll;
mod test_smoke;
mod test_tcp;
mod test_tcp_level;
mod test_tcp_shutdown;
mod test_udp_level;
mod test_udp_socket;
mod test_write_then_drop;

#[cfg(target_os = "fuchsia")]
mod test_fuchsia_handles;

use bytes::{Buf, MutBuf};
use corcovado::event::Event;
use corcovado::{Events, Poll};
use std::io::{self, Read, Write};
use std::time::Duration;

pub trait TryRead {
    fn try_read_buf<B: MutBuf>(&mut self, buf: &mut B) -> io::Result<Option<usize>>
    where
        Self: Sized,
    {
        // Reads the length of the slice supplied by buf.mut_bytes into the buffer
        // This is not guaranteed to consume an entire datagram or segment.
        // If your protocol is msg based (instead of continuous stream) you should
        // ensure that your buffer is large enough to hold an entire segment (1532 bytes if not jumbo
        // frames)
        let res = self.try_read(unsafe { buf.mut_bytes() });

        if let Ok(Some(cnt)) = res {
            unsafe {
                buf.advance(cnt);
            }
        }

        res
    }

    fn try_read(&mut self, buf: &mut [u8]) -> io::Result<Option<usize>>;
}

pub trait TryWrite {
    fn try_write_buf<B: Buf>(&mut self, buf: &mut B) -> io::Result<Option<usize>>
    where
        Self: Sized,
    {
        let res = self.try_write(buf.bytes());

        if let Ok(Some(cnt)) = res {
            buf.advance(cnt);
        }

        res
    }

    fn try_write(&mut self, buf: &[u8]) -> io::Result<Option<usize>>;
}

impl<T: Read> TryRead for T {
    fn try_read(&mut self, dst: &mut [u8]) -> io::Result<Option<usize>> {
        self.read(dst).map_non_block()
    }
}

impl<T: Write> TryWrite for T {
    fn try_write(&mut self, src: &[u8]) -> io::Result<Option<usize>> {
        self.write(src).map_non_block()
    }
}

/*
 *
 * ===== Helpers =====
 *
 */

/// A helper trait to provide the map_non_block function on Results.
trait MapNonBlock<T> {
    /// Maps a `Result<T>` to a `Result<Option<T>>` by converting
    /// operation-would-block errors into `Ok(None)`.
    fn map_non_block(self) -> io::Result<Option<T>>;
}

impl<T> MapNonBlock<T> for io::Result<T> {
    fn map_non_block(self) -> io::Result<Option<T>> {
        use std::io::ErrorKind::WouldBlock;

        match self {
            Ok(value) => Ok(Some(value)),
            Err(err) => {
                if let WouldBlock = err.kind() {
                    Ok(None)
                } else {
                    Err(err)
                }
            }
        }
    }
}

mod ports {
    use std::net::SocketAddr;
    use std::str::FromStr;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering::SeqCst;

    // Helper for getting a unique port for the task run
    // TODO: Reuse ports to not spam the system
    static mut NEXT_PORT: AtomicUsize = AtomicUsize::new(0);
    const FIRST_PORT: usize = 18080;

    fn next_port() -> usize {
        unsafe {
            // If the atomic was never used, set it to the initial port
            #[allow(deprecated)]
            #[allow(static_mut_refs)]
            NEXT_PORT.compare_and_swap(0, FIRST_PORT, SeqCst);

            // Get and increment the port list
            #[allow(static_mut_refs)]
            NEXT_PORT.fetch_add(1, SeqCst)
        }
    }

    pub fn localhost() -> SocketAddr {
        let s = format!("127.0.0.1:{}", next_port());
        FromStr::from_str(&s).unwrap()
    }
}

pub fn sleep_ms(ms: u64) {
    use std::thread;
    thread::sleep(Duration::from_millis(ms));
}

pub fn expect_events(
    poll: &Poll,
    event_buffer: &mut Events,
    poll_try_count: usize,
    mut expected: Vec<Event>,
) {
    const MS: u64 = 1_000;

    for _ in 0..poll_try_count {
        poll.poll(event_buffer, Some(Duration::from_millis(MS)))
            .unwrap();
        for event in event_buffer.iter() {
            let pos_opt = expected.iter().position(|exp_event| {
                (event.token() == exp_event.token())
                    && event.readiness().contains(exp_event.readiness())
            });
            if let Some(pos) = pos_opt {
                expected.remove(pos);
            }
        }

        if expected.is_empty() {
            break;
        }
    }

    assert!(
        expected.is_empty(),
        "The following expected events were not found: {:?}",
        expected
    );
}
