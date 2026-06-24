#[cfg(any(x11_platform, wayland_platform))]
use crate::event_loop::ActiveEventLoop;

#[cfg(any(x11_platform, wayland_platform))]
pub trait ActiveEventLoopExtLinux {
    /// Start following the system color-scheme (dark/light) via the XDG Desktop
    /// Portal.
    ///
    /// While active, the event loop reports the current preference through
    /// [`ActiveEventLoop::system_theme`] and emits
    /// [`WindowEvent::ThemeChanged`](crate::event::WindowEvent::ThemeChanged) for
    /// every window whenever it changes.
    ///
    /// Call this once, only when the application wants to follow the system
    /// theme; the watcher then runs for the lifetime of the process.
    fn start_system_theme_monitor(&self);
}

#[cfg(any(x11_platform, wayland_platform))]
impl ActiveEventLoopExtLinux for ActiveEventLoop {
    #[inline]
    fn start_system_theme_monitor(&self) {
        self.p.start_system_theme_monitor();
    }
}
