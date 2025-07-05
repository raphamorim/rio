#[macro_use]
mod util;

mod app;
mod app_delegate;
mod cursor;
mod dispatcher;
mod display_link;
mod event;
mod event_handler;
mod event_loop;
mod ffi;
mod menu;
mod monitor;
mod observer;
mod view;
mod window;
mod window_delegate;

use std::fmt;

pub(crate) use self::event::{
    physicalkey_to_scancode, scancode_to_physicalkey, KeyEventExtra,
};
pub(crate) use self::event_loop::{
    ActiveEventLoop, EventLoop, EventLoopProxy, OwnedDisplayHandle,
    PlatformSpecificEventLoopAttributes,
};
pub(crate) use self::monitor::{MonitorHandle, VideoModeHandle};
pub(crate) use self::window::WindowId;
pub(crate) use self::window_delegate::PlatformSpecificWindowAttributes;
use crate::event::DeviceId as RootDeviceId;

pub(crate) use self::cursor::CustomCursor as PlatformCustomCursor;
pub(crate) use self::window::Window;
pub(crate) use crate::cursor::OnlyCursorImageSource as PlatformCustomCursorSource;
pub(crate) use crate::icon::NoIcon as PlatformIcon;
pub(crate) use crate::platform_impl::Fullscreen;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId;

impl DeviceId {
    pub const unsafe fn dummy() -> Self {
        DeviceId
    }
}

// Constant device ID; to be removed when if backend is updated to report real device IDs.
pub(crate) const DEVICE_ID: RootDeviceId = RootDeviceId(DeviceId);

#[derive(Debug)]
pub enum OsError {
    CGError(core_graphics::base::CGError),
    CreationError(&'static str),
}

impl fmt::Display for OsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OsError::CGError(e) => f.pad(&format!("CGError {e}")),
            OsError::CreationError(e) => f.pad(e),
        }
    }
}
