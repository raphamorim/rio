#![cfg(target_os = "redox")]

use std::fmt::{self, Display, Formatter};
use std::str;
use std::sync::Arc;

use smol_str::SmolStr;

use crate::dpi::{PhysicalPosition, PhysicalSize};
use crate::keyboard::Key;

pub(crate) use self::event_loop::{
    ActiveEventLoop, EventLoop, EventLoopProxy, OwnedDisplayHandle,
};
mod event_loop;

pub use self::window::Window;
mod window;

struct RedoxSocket {
    fd: usize,
}

impl RedoxSocket {
    fn event() -> syscall::Result<Self> {
        Self::open_raw("event:")
    }

    fn orbital(properties: &WindowProperties<'_>) -> syscall::Result<Self> {
        Self::open_raw(&format!("{properties}"))
    }

    // Paths should be checked to ensure they are actually sockets and not normal files. If a
    // non-socket path is used, it could cause read and write to not function as expected. For
    // example, the seek would change in a potentially unpredictable way if either read or write
    // were called at the same time by multiple threads.
    fn open_raw(path: &str) -> syscall::Result<Self> {
        let fd = syscall::open(path, syscall::O_RDWR | syscall::O_CLOEXEC)?;
        Ok(Self { fd })
    }

    fn read(&self, buf: &mut [u8]) -> syscall::Result<()> {
        let count = syscall::read(self.fd, buf)?;
        if count == buf.len() {
            Ok(())
        } else {
            Err(syscall::Error::new(syscall::EINVAL))
        }
    }

    fn write(&self, buf: &[u8]) -> syscall::Result<()> {
        let count = syscall::write(self.fd, buf)?;
        if count == buf.len() {
            Ok(())
        } else {
            Err(syscall::Error::new(syscall::EINVAL))
        }
    }

    fn fpath<'a>(&self, buf: &'a mut [u8]) -> syscall::Result<&'a str> {
        let count = syscall::fpath(self.fd, buf)?;
        str::from_utf8(&buf[..count]).map_err(|_err| syscall::Error::new(syscall::EINVAL))
    }
}

impl Drop for RedoxSocket {
    fn drop(&mut self) {
        let _ = syscall::close(self.fd);
    }
}

pub struct TimeSocket(RedoxSocket);

impl TimeSocket {
    fn open() -> syscall::Result<Self> {
        RedoxSocket::open_raw("time:4").map(Self)
    }

    // Read current time.
    fn current_time(&self) -> syscall::Result<syscall::TimeSpec> {
        let mut timespec = syscall::TimeSpec::default();
        self.0.read(&mut timespec)?;
        Ok(timespec)
    }

    // Write a timeout.
    fn timeout(&self, timespec: &syscall::TimeSpec) -> syscall::Result<()> {
        self.0.write(timespec)
    }

    // Wake immediately.
    fn wake(&self) -> syscall::Result<()> {
        // Writing a default TimeSpec will always trigger a time event.
        self.timeout(&syscall::TimeSpec::default())
    }
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PlatformSpecificEventLoopAttributes {}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WindowId {
    fd: u64,
}

impl WindowId {
    pub const fn dummy() -> Self {
        WindowId {
            fd: u64::max_value(),
        }
    }
}

impl From<WindowId> for u64 {
    fn from(id: WindowId) -> Self {
        id.fd
    }
}

impl From<u64> for WindowId {
    fn from(fd: u64) -> Self {
        Self { fd }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeviceId;

impl DeviceId {
    pub const fn dummy() -> Self {
        DeviceId
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlatformSpecificWindowAttributes;

struct WindowProperties<'a> {
    flags: &'a str,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    title: &'a str,
}

impl<'a> WindowProperties<'a> {
    fn new(path: &'a str) -> Self {
        // orbital:flags/x/y/w/h/t
        let mut parts = path.splitn(6, '/');
        let flags = parts.next().unwrap_or("");
        let x = parts
            .next()
            .map_or(0, |part| part.parse::<i32>().unwrap_or(0));
        let y = parts
            .next()
            .map_or(0, |part| part.parse::<i32>().unwrap_or(0));
        let w = parts
            .next()
            .map_or(0, |part| part.parse::<u32>().unwrap_or(0));
        let h = parts
            .next()
            .map_or(0, |part| part.parse::<u32>().unwrap_or(0));
        let title = parts.next().unwrap_or("");
        Self {
            flags,
            x,
            y,
            w,
            h,
            title,
        }
    }
}

impl<'a> fmt::Display for WindowProperties<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "orbital:{}/{}/{}/{}/{}/{}",
            self.flags, self.x, self.y, self.w, self.h, self.title
        )
    }
}

#[derive(Clone, Debug)]
pub struct OsError(Arc<syscall::Error>);

impl OsError {
    fn new(error: syscall::Error) -> Self {
        Self(Arc::new(error))
    }
}

impl Display for OsError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        self.0.fmt(fmt)
    }
}

pub(crate) use crate::cursor::{
    NoCustomCursor as PlatformCustomCursor, NoCustomCursor as PlatformCustomCursorSource,
};
pub(crate) use crate::icon::NoIcon as PlatformIcon;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MonitorHandle;

impl MonitorHandle {
    pub fn name(&self) -> Option<String> {
        Some("Redox Device".to_owned())
    }

    pub fn size(&self) -> PhysicalSize<u32> {
        PhysicalSize::new(0, 0) // TODO
    }

    pub fn position(&self) -> PhysicalPosition<i32> {
        (0, 0).into()
    }

    pub fn scale_factor(&self) -> f64 {
        1.0 // TODO
    }

    pub fn refresh_rate_millihertz(&self) -> Option<u32> {
        // FIXME no way to get real refresh rate for now.
        None
    }

    pub fn video_modes(&self) -> impl Iterator<Item = VideoModeHandle> {
        let size = self.size().into();
        // FIXME this is not the real refresh rate
        // (it is guaranteed to support 32 bit color though)
        std::iter::once(VideoModeHandle {
            size,
            bit_depth: 32,
            refresh_rate_millihertz: 60000,
            monitor: self.clone(),
        })
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct VideoModeHandle {
    size: (u32, u32),
    bit_depth: u16,
    refresh_rate_millihertz: u32,
    monitor: MonitorHandle,
}

impl VideoModeHandle {
    pub fn size(&self) -> PhysicalSize<u32> {
        self.size.into()
    }

    pub fn bit_depth(&self) -> u16 {
        self.bit_depth
    }

    pub fn refresh_rate_millihertz(&self) -> u32 {
        self.refresh_rate_millihertz
    }

    pub fn monitor(&self) -> MonitorHandle {
        self.monitor.clone()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct KeyEventExtra {
    pub key_without_modifiers: Key,
    pub text_with_all_modifiers: Option<SmolStr>,
}
