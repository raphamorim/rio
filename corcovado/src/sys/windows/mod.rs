//! Implementation of mio for Windows using IOCP
//!
//! This module uses I/O Completion Ports (IOCP) on Windows to implement mio's
//! Unix epoll-like interface. Unfortunately these two I/O models are
//! fundamentally incompatible:
//!
//! * IOCP is a completion-based model where work is submitted to the kernel and
//!   a program is notified later when the work finished.
//! * epoll is a readiness-based model where the kernel is queried as to what
//!   work can be done, and afterwards the work is done.
//!
//! As a result, this implementation for Windows is much less "low level" than
//! the Unix implementation of mio. This design decision was intentional,
//! however.
//!
//! ## What is IOCP?
//!
//! The [official docs][docs] have a comprehensive explanation of what IOCP is,
//! but at a high level it requires the following operations to be executed to
//! perform some I/O:
//!
//! 1. A completion port is created
//! 2. An I/O handle and a token is registered with this completion port
//! 3. Some I/O is issued on the handle. This generally means that an API was
//!    invoked with a zeroed `OVERLAPPED` structure. The API will immediately
//!    return.
//! 4. After some time, the application queries the I/O port for completed
//!    events. The port will returned a pointer to the `OVERLAPPED` along with
//!    the token presented at registration time.
//!
//! Many I/O operations can be fired off before waiting on a port, and the port
//! will block execution of the calling thread until an I/O event has completed
//! (or a timeout has elapsed).
//!
//! Currently all of these low-level operations are housed in a separate `miow`
//! crate to provide a 0-cost abstraction over IOCP. This crate uses that to
//! implement all fiddly bits so there's very few actual Windows API calls or
//! `unsafe` blocks as a result.
//!
//! [docs]: https://msdn.microsoft.com/en-us/library/windows/desktop/aa365198%28v=vs.85%29.aspx
//!
//! ## Safety of IOCP
//!
//! Unfortunately for us, IOCP is pretty unsafe in terms of Rust lifetimes and
//! such. When an I/O operation is submitted to the kernel, it involves handing
//! the kernel a few pointers like a buffer to read/write, an `OVERLAPPED`
//! structure pointer, and perhaps some other buffers such as for socket
//! addresses. These pointers all have to remain valid **for the entire I/O
//! operation's duration**.
//!
//! There's no way to define a safe lifetime for these pointers/buffers over
//! the span of an I/O operation, so we're forced to add a layer of abstraction
//! (not 0-cost) to make these APIs safe. Currently this implementation
//! basically just boxes everything up on the heap to give it a stable address
//! and then keys off that most of the time.
//!
//! ## From completion to readiness
//!
//! Translating a completion-based model to a readiness-based model is no easy
//! task. This backend keeps only the plumbing needed for that translation:
//! the `Selector` owns a completion port, and consumers manage their own
//! readiness through `Registration`/`SetReadiness` from the `poll` module
//! (this is how the readiness queue also implements level and edge semantics
//! for user-space sources on Windows).
//!
//! Custom I/O objects bind a handle to the port with `Binding` and issue
//! operations through `Overlapped`. When a completion is dequeued, the
//! `Selector` assumes the `OVERLAPPED` pointer is the interior of a
//! `selector::Overlapped`, whose trailing function pointer is invoked with
//! the completion status; that callback is responsible for updating a
//! `SetReadiness` accordingly. Note that `PollOpt::level()` is not
//! implemented for IOCP-bound handles themselves; level semantics exist only
//! via the user-space readiness queue.

mod awakener;
mod selector;

pub use self::awakener::Awakener;
pub use self::selector::{Binding, Events, Overlapped, Selector};
