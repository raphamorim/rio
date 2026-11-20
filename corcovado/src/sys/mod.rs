#[cfg(unix)]
pub use self::unix::{Awakener, EventedFd, Events, Selector};

#[cfg(unix)]
pub use self::unix::READY_ALL;

#[cfg(unix)]
pub mod unix;

#[cfg(windows)]
pub use self::windows::{Awakener, Binding, Events, Overlapped, Selector};

#[cfg(windows)]
mod windows;

#[cfg(not(unix))]
pub const READY_ALL: usize = 0;
