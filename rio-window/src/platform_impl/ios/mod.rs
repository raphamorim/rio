#![cfg(ios_platform)]
#![allow(clippy::let_unit_value)]

mod app_delegate;
mod app_state;
mod event_loop;
mod monitor;
mod view;
mod view_controller;
mod window;

use std::fmt;

use crate::event::DeviceId as RootDeviceId;

pub(crate) use self::event_loop::{
    ActiveEventLoop, EventLoop, EventLoopProxy, OwnedDisplayHandle,
    PlatformSpecificEventLoopAttributes,
};
pub(crate) use self::monitor::{MonitorHandle, VideoModeHandle};
pub(crate) use self::window::{PlatformSpecificWindowAttributes, Window, WindowId};
pub(crate) use crate::cursor::{
    NoCustomCursor as PlatformCustomCursor, NoCustomCursor as PlatformCustomCursorSource,
};
pub(crate) use crate::icon::NoIcon as PlatformIcon;
pub(crate) use crate::platform_impl::Fullscreen;

/// There is no way to detect which device that performed a certain event in
/// UIKit (i.e. you can't differentiate between different external keyboards,
/// or whether it was the main touchscreen, assistive technologies, or some
/// other pointer device that caused a touch event).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId;

impl DeviceId {
    pub const unsafe fn dummy() -> Self {
        DeviceId
    }
}

pub(crate) const DEVICE_ID: RootDeviceId = RootDeviceId(DeviceId);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyEventExtra {}

#[derive(Debug)]
pub enum OsError {}

impl fmt::Display for OsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "os error")
    }
}
