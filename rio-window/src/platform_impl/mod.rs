use crate::monitor::{
    MonitorHandle as RootMonitorHandle, VideoModeHandle as RootVideoModeHandle,
};
use crate::window::Fullscreen as RootFullscreen;

#[cfg(windows_platform)]
#[path = "windows/mod.rs"]
mod platform;
#[cfg(any(x11_platform, wayland_platform))]
#[path = "linux/mod.rs"]
mod platform;
#[cfg(macos_platform)]
#[path = "macos/mod.rs"]
mod platform;
#[cfg(web_platform)]
#[path = "web/mod.rs"]
mod platform;
#[cfg(orbital_platform)]
#[path = "orbital/mod.rs"]
mod platform;

pub use self::platform::*;

/// Helper for converting between platform-specific and generic
/// [`VideoModeHandle`]/[`MonitorHandle`]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Fullscreen {
    Exclusive(VideoModeHandle),
    Borderless(Option<MonitorHandle>),
}

impl From<RootFullscreen> for Fullscreen {
    fn from(f: RootFullscreen) -> Self {
        match f {
            RootFullscreen::Exclusive(mode) => Self::Exclusive(mode.video_mode),
            RootFullscreen::Borderless(Some(handle)) => {
                Self::Borderless(Some(handle.inner))
            }
            RootFullscreen::Borderless(None) => Self::Borderless(None),
        }
    }
}

impl From<Fullscreen> for RootFullscreen {
    fn from(f: Fullscreen) -> Self {
        match f {
            Fullscreen::Exclusive(video_mode) => {
                Self::Exclusive(RootVideoModeHandle { video_mode })
            }
            Fullscreen::Borderless(Some(inner)) => {
                Self::Borderless(Some(RootMonitorHandle { inner }))
            }
            Fullscreen::Borderless(None) => Self::Borderless(None),
        }
    }
}

#[cfg(all(
    not(windows_platform),
    not(macos_platform),
    not(x11_platform),
    not(wayland_platform),
    not(web_platform),
    not(orbital_platform),
))]
compile_error!("The platform you're compiling for is not supported by winit");
