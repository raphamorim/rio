//! # Wayland
//!
//! **Note:** Windows don't appear on Wayland until you draw/present to them.
//!
//! By default, Winit loads system libraries using `dlopen`. This can be
//! disabled by disabling the `"wayland-dlopen"` cargo feature.
//!
//! ## Client-side decorations
//!
//! Winit provides client-side decorations by default, but the behaviour can
//! be controlled with the following feature flags:
//!
//! * `wayland-csd-adwaita` (default).
//! * `wayland-csd-adwaita-crossfont`.
//! * `wayland-csd-adwaita-notitle`.
use crate::event_loop::{ActiveEventLoop, EventLoopBuilder};
use crate::monitor::MonitorHandle;
use crate::window::{Window, WindowAttributes};

pub use crate::platform_impl::common::app_menu::AppMenuItem;
pub use crate::window::Theme;

/// Additional methods on [`ActiveEventLoop`] that are specific to Wayland.
pub trait ActiveEventLoopExtWayland {
    /// True if the [`ActiveEventLoop`] uses Wayland.
    fn is_wayland(&self) -> bool;
}

impl ActiveEventLoopExtWayland for ActiveEventLoop {
    #[inline]
    fn is_wayland(&self) -> bool {
        self.p.is_wayland()
    }
}

/// Additional methods on [`EventLoopBuilder`] that are specific to Wayland.
pub trait EventLoopBuilderExtWayland {
    /// Force using Wayland.
    fn with_wayland(&mut self) -> &mut Self;

    /// Whether to allow the event loop to be created off of the main thread.
    ///
    /// By default, the window is only allowed to be created on the main
    /// thread, to make platform compatibility easier.
    fn with_any_thread(&mut self, any_thread: bool) -> &mut Self;
}

impl<T> EventLoopBuilderExtWayland for EventLoopBuilder<T> {
    #[inline]
    fn with_wayland(&mut self) -> &mut Self {
        self.platform_specific.forced_backend =
            Some(crate::platform_impl::Backend::Wayland);
        self
    }

    #[inline]
    fn with_any_thread(&mut self, any_thread: bool) -> &mut Self {
        self.platform_specific.any_thread = any_thread;
        self
    }
}

/// Additional methods on [`Window`] that are specific to Wayland.
pub trait WindowExtWayland {
    /// Whether the compositor applied the unified title bar asked for with
    /// [`WindowAttributesExtWayland::with_unified_titlebar`]: it draws only
    /// the window control buttons, over the leading end of the top of the
    /// window, and the application draws the rest of that row and moves the
    /// window from it.
    ///
    /// `false` on X11 and on compositors without such a title bar, where
    /// the window keeps its regular decorations.
    fn unified_titlebar(&self) -> bool;

    /// Export `items` as the application menu of the window, for a desktop
    /// with a global menu bar, replacing the menu exported before.
    ///
    /// The menu is served on the session bus as `com.canonical.dbusmenu`
    /// and announced with the KDE application menu protocol. `handler` gets
    /// the [`AppMenuItem::id`] of an activated entry and is called on the
    /// thread that serves the menu, not on the event loop's.
    ///
    /// Returns `false`, exporting nothing, on X11 and on compositors
    /// without that protocol.
    fn set_app_menu(
        &self,
        items: Vec<AppMenuItem>,
        handler: impl Fn(u32) + Send + Sync + 'static,
    ) -> bool;

    /// Stop exporting the application menu of the window.
    fn remove_app_menu(&self);
}

impl WindowExtWayland for Window {
    #[inline]
    fn unified_titlebar(&self) -> bool {
        match &self.window {
            crate::platform_impl::Window::Wayland(window) => window.unified_titlebar(),
            #[cfg(x11_platform)]
            crate::platform_impl::Window::X(_) => false,
        }
    }

    #[inline]
    fn set_app_menu(
        &self,
        items: Vec<AppMenuItem>,
        handler: impl Fn(u32) + Send + Sync + 'static,
    ) -> bool {
        match &self.window {
            crate::platform_impl::Window::Wayland(window) => {
                window.set_app_menu(items, std::sync::Arc::new(handler))
            }
            #[cfg(x11_platform)]
            crate::platform_impl::Window::X(_) => false,
        }
    }

    #[inline]
    fn remove_app_menu(&self) {
        match &self.window {
            crate::platform_impl::Window::Wayland(window) => window.remove_app_menu(),
            #[cfg(x11_platform)]
            crate::platform_impl::Window::X(_) => (),
        }
    }
}

/// Additional methods on [`WindowAttributes`] that are specific to Wayland.
pub trait WindowAttributesExtWayland {
    /// Build window with the given name.
    ///
    /// The `general` name sets an application ID, which should match the `.desktop`
    /// file distributed with your program. The `instance` is a `no-op`.
    ///
    /// For details about application ID conventions, see the
    /// [Desktop Entry Spec](https://specifications.freedesktop.org/desktop-entry-spec/desktop-entry-spec-latest.html#desktop-file-id)
    fn with_name(self, general: impl Into<String>, instance: impl Into<String>) -> Self;

    /// Ask the compositor for a unified title bar: it draws only the window
    /// control buttons and leaves the row they lie in to the application.
    ///
    /// Only a compositor with the `cupertino_app_effects_v1` protocol has
    /// one; the others keep their regular decorations, as does a window
    /// built without decorations. [`WindowExtWayland::unified_titlebar`]
    /// tells which it was.
    fn with_unified_titlebar(self, unified_titlebar: bool) -> Self;
}

impl WindowAttributesExtWayland for WindowAttributes {
    #[inline]
    fn with_name(
        mut self,
        general: impl Into<String>,
        instance: impl Into<String>,
    ) -> Self {
        self.platform_specific.name = Some(crate::platform_impl::ApplicationName::new(
            general.into(),
            instance.into(),
        ));
        self
    }

    #[inline]
    fn with_unified_titlebar(mut self, unified_titlebar: bool) -> Self {
        self.platform_specific.unified_titlebar = unified_titlebar;
        self
    }
}

/// Additional methods on `MonitorHandle` that are specific to Wayland.
pub trait MonitorHandleExtWayland {
    /// Returns the inner identifier of the monitor.
    fn native_id(&self) -> u32;
}

impl MonitorHandleExtWayland for MonitorHandle {
    #[inline]
    fn native_id(&self) -> u32 {
        self.inner.native_identifier()
    }
}
