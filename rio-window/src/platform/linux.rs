//! Linux-specific functionality.

#[cfg(any(x11_platform, wayland_platform))]
#[doc(inline)]
pub use crate::platform_impl::common::theme_monitor;

#[cfg(any(x11_platform, wayland_platform))]
#[doc(inline)]
pub use crate::platform_impl::common::xdg_desktop_portal;
