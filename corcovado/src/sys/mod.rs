#[cfg(all(unix, not(target_os = "fuchsia")))]
pub use self::unix::{Awakener, EventedFd, Events, Selector};
// pub use self::unix::{pipe, set_nonblock, Awakener, EventedFd, Events, Io, Selector};

#[cfg(all(unix, not(target_os = "fuchsia")))]
pub use self::unix::READY_ALL;

#[cfg(all(unix, not(target_os = "fuchsia")))]
pub mod unix;

#[cfg(windows)]
pub use self::windows::{Awakener, Binding, Events, Overlapped, Selector};

#[cfg(windows)]
mod windows;

#[cfg(target_os = "fuchsia")]
pub use self::fuchsia::{
    set_nonblock, Awakener, EventedHandle, Events, Selector, TcpListener, TcpStream,
    UdpSocket,
};

#[cfg(target_os = "fuchsia")]
pub mod fuchsia;

#[cfg(not(all(unix, not(target_os = "fuchsia"))))]
pub const READY_ALL: usize = 0;
