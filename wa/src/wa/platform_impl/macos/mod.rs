// WA is a fork of https://github.com/rust-windowing/winit/
// Winit is is licensed under Apache 2.0 license https://github.com/rust-windowing/winit/blob/master/LICENSE

#[macro_use]
mod util;

mod app;
mod app_delegate;
mod app_state;
mod appkit;
mod event;
mod event_loop;
mod ffi;
mod menu;
mod monitor;
mod observer;
mod view;
mod window;
mod window_delegate;

use std::fmt;

pub(crate) use self::{
    event::KeyEventExtra,
    event_loop::{
        EventLoop, EventLoopProxy, EventLoopWindowTarget,
        PlatformSpecificEventLoopAttributes,
    },
    monitor::{MonitorHandle, VideoMode},
    window::{PlatformSpecificWindowBuilderAttributes, WindowId},
};
use crate::event::DeviceId as RootDeviceId;

pub(crate) use self::window::Window;
pub(crate) use crate::wa::icon::NoIcon as PlatformIcon;
pub(crate) use crate::wa::platform_impl::Fullscreen;

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
