//! A low-level readiness-based IO library focused on what Rio needs from its
//! PTY event loop: event notification backed by epoll, kqueue, and IOCP, plus
//! a pollable cross-thread channel and user-space readiness (`Registration` /
//! `SetReadiness`).
//!
//! Corcovado is a trimmed-down maintained fork of mio 0.6.x. Networking types,
//! the timer, and the Fuchsia backend from the original crate were removed;
//! `std` types (or `EventedFd` on unix) cover those needs instead.
//!
//! # Usage
//!
//! Create a [`Poll`], register [`event::Evented`] sources with it, and read
//! events from the OS into [`Events`]. For more detail, see [`Poll`].
//!
//! # Platforms
//!
//! Linux, macOS, Windows, FreeBSD, NetBSD, Android, and iOS.
//!
//! [`Poll`]: struct.Poll.html
//! [`Events`]: struct.Events.html
//! [`event::Evented`]: event/trait.Evented.html

mod event_imp;
mod io;
mod lazycell;
mod poll;
#[cfg(unix)]
mod socket;
mod sys;
mod token;

pub mod channel;
#[cfg(unix)]
pub mod stream;

pub use event_imp::{PollOpt, Ready};
pub use poll::{Poll, Registration, SetReadiness};
pub use token::Token;

pub mod event {
    //! Readiness event types and utilities.

    pub use super::event_imp::{Event, Evented};
    pub use super::poll::{Events, Iter};
}

pub use event::Events;

#[cfg(unix)]
pub mod unix {
    //! Unix only extensions
    pub use crate::sys::unix::UnixReady;
    pub use crate::sys::EventedFd;
}

/// Windows-only extensions to the mio crate.
///
/// Mio on windows is currently implemented with IOCP for a high-performance
/// implementation of asynchronous I/O. Mio then provides TCP and UDP as sample
/// bindings for the system to connect networking types to asynchronous I/O. On
/// Unix this scheme is then also extensible to all other file descriptors with
/// the `EventedFd` type, but on Windows no such analog is available. The
/// purpose of this module, however, is to similarly provide a mechanism for
/// foreign I/O types to get hooked up into the IOCP event loop.
///
/// This module provides two types for interfacing with a custom IOCP handle:
///
/// * `Binding` - this type is intended to govern binding with mio's `Poll`
///   type. Each I/O object should contain an instance of `Binding` that's
///   interfaced with for the implementation of the `Evented` trait. The
///   `register`, `reregister`, and `deregister` methods for the `Evented` trait
///   all have rough analogs with `Binding`.
///
///   Note that this type **does not handle readiness**. That is, this type does
///   not handle whether sockets are readable/writable/etc. It's intended that
///   IOCP types will internally manage this state with a `SetReadiness` type
///   from the `poll` module. The `SetReadiness` is typically lazily created on
///   the first time that `Evented::register` is called and then stored in the
///   I/O object.
///
///   Also note that for types which represent streams of bytes the mio
///   interface of *readiness* doesn't map directly to the Windows model of
///   *completion*. This means that types will have to perform internal
///   buffering to ensure that a readiness interface can be provided. For a
///   sample implementation see the anonymous pipe wrappers in
///   teletypewriter's Windows backend.
///
/// * `Overlapped` - this type is intended to be used as the concrete instances
///   of the `OVERLAPPED` type that most win32 methods expect. It's crucial, for
///   safety, that all asynchronous operations are initiated with an instance of
///   `Overlapped` and not another instantiation of `OVERLAPPED`.
///
///   Mio's `Overlapped` type is created with a function pointer that receives
///   a `OVERLAPPED_ENTRY` type when called. This `OVERLAPPED_ENTRY` type is
///   defined in the `winapi` crate. Whenever a completion is posted to an IOCP
///   object the `OVERLAPPED` that was signaled will be interpreted as
///   `Overlapped` in the mio crate and this function pointer will be invoked.
///   Through this function pointer, and through the `OVERLAPPED` pointer,
///   implementations can handle management of I/O events.
///
/// When put together these two types enable custom Windows handles to be
/// registered with mio's event loops. The `Binding` type is used to associate
/// handles and the `Overlapped` type is used to execute I/O operations. When
/// the I/O operations are completed a custom function pointer is called which
/// typically modifies a `SetReadiness` set by `Evented` methods which will get
/// later hooked into the mio event loop.
#[cfg(windows)]
pub mod windows {

    pub use crate::sys::{Binding, Overlapped};
}
