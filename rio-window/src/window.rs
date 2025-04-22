//! The [`Window`] struct and associated types.
use std::fmt;

use crate::dpi::{PhysicalPosition, PhysicalSize, Position, Size};
use crate::error::{ExternalError, NotSupportedError};
use crate::monitor::{MonitorHandle, VideoModeHandle};
use crate::platform_impl::{self, PlatformSpecificWindowAttributes};

pub use crate::cursor::{
    BadImage, Cursor, CustomCursor, CustomCursorSource, MAX_CURSOR_SIZE,
};
pub use crate::icon::{BadIcon, Icon};

#[doc(inline)]
pub use cursor_icon::{CursorIcon, ParseError as CursorIconParseError};

/// Represents a window.
///
/// The window is closed when dropped.
///
/// ## Threading
///
/// This is `Send + Sync`, meaning that it can be freely used from other
/// threads.
///
/// However, some platforms (macOS, Web and iOS) only allow user interface
/// interactions on the main thread, so on those platforms, if you use the
/// window from a thread other than the main, the code is scheduled to run on
/// the main thread, and your thread may be blocked until that completes.
///
/// ## Platform-specific
///
/// **Web:** The [`Window`], which is represented by a `HTMLElementCanvas`, can
/// not be closed by dropping the [`Window`].
pub struct Window {
    pub(crate) window: platform_impl::Window,
}

impl fmt::Debug for Window {
    fn fmt(&self, fmtr: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmtr.pad("Window { .. }")
    }
}

impl Drop for Window {
    /// This will close the [`Window`].
    ///
    /// See [`Window`] for more details.
    fn drop(&mut self) {
        self.window.maybe_wait_on_main(|w| {
            // If the window is in exclusive fullscreen, we must restore the desktop
            // video mode (generally this would be done on application exit, but
            // closing the window doesn't necessarily always mean application exit,
            // such as when there are multiple windows)
            if let Some(Fullscreen::Exclusive(_)) = w.fullscreen().map(|f| f.into()) {
                w.set_fullscreen(None);
            }
        })
    }
}

/// Identifier of a window. Unique for each window.
///
/// Can be obtained with [`window.id()`][`Window::id`].
///
/// Whenever you receive an event specific to a window, this event contains a `WindowId` which you
/// can then compare to the ids of your windows.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId(pub(crate) platform_impl::WindowId);

impl WindowId {
    /// Returns a dummy id, useful for unit testing.
    ///
    /// # Safety
    ///
    /// The only guarantee made about the return value of this function is that
    /// it will always be equal to itself and to future values returned by this function.
    /// No other guarantees are made. This may be equal to a real [`WindowId`].
    ///
    /// **Passing this into a winit function will result in undefined behavior.**
    pub const unsafe fn dummy() -> Self {
        #[allow(unused_unsafe)]
        WindowId(unsafe { platform_impl::WindowId::dummy() })
    }
}

impl fmt::Debug for WindowId {
    fn fmt(&self, fmtr: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(fmtr)
    }
}

impl From<WindowId> for u64 {
    fn from(window_id: WindowId) -> Self {
        window_id.0.into()
    }
}

impl From<u64> for WindowId {
    fn from(raw_id: u64) -> Self {
        Self(raw_id.into())
    }
}

/// Attributes used when creating a window.
#[derive(Debug, Clone)]
pub struct WindowAttributes {
    pub inner_size: Option<Size>,
    pub min_inner_size: Option<Size>,
    pub max_inner_size: Option<Size>,
    pub position: Option<Position>,
    pub resizable: bool,
    pub enabled_buttons: WindowButtons,
    pub title: String,
    pub maximized: bool,
    pub visible: bool,
    pub transparent: bool,
    pub blur: bool,
    pub decorations: bool,
    pub window_icon: Option<Icon>,
    pub preferred_theme: Option<Theme>,
    pub resize_increments: Option<Size>,
    pub content_protected: bool,
    pub window_level: WindowLevel,
    pub active: bool,
    pub cursor: Cursor,
    pub(crate) parent_window: Option<SendSyncRawWindowHandle>,
    pub fullscreen: Option<Fullscreen>,
    // Platform-specific configuration.
    #[allow(dead_code)]
    pub(crate) platform_specific: PlatformSpecificWindowAttributes,
}

impl Default for WindowAttributes {
    #[inline]
    fn default() -> WindowAttributes {
        WindowAttributes {
            inner_size: None,
            min_inner_size: None,
            max_inner_size: None,
            position: None,
            resizable: true,
            enabled_buttons: WindowButtons::all(),
            title: "winit window".to_owned(),
            maximized: false,
            fullscreen: None,
            visible: true,
            transparent: false,
            blur: false,
            decorations: true,
            window_level: Default::default(),
            window_icon: None,
            preferred_theme: None,
            resize_increments: None,
            content_protected: false,
            cursor: Cursor::default(),
            parent_window: None,
            active: true,
            platform_specific: Default::default(),
        }
    }
}

/// Wrapper for [`raw_window_handle::RawWindowHandle`] for [`WindowAttributes::parent_window`].
///
/// # Safety
///
/// The user has to account for that when using [`WindowAttributes::with_parent_window()`],
/// which is `unsafe`.
#[derive(Debug, Clone)]
pub(crate) struct SendSyncRawWindowHandle(pub(crate) raw_window_handle::RawWindowHandle);

unsafe impl Send for SendSyncRawWindowHandle {}
unsafe impl Sync for SendSyncRawWindowHandle {}

impl WindowAttributes {
    /// Initializes new attributes with default values.
    #[inline]
    #[deprecated = "use `Window::default_attributes` instead"]
    pub fn new() -> Self {
        Default::default()
    }
}

impl WindowAttributes {
    /// Get the parent window stored on the attributes.
    pub fn parent_window(&self) -> Option<&raw_window_handle::RawWindowHandle> {
        self.parent_window.as_ref().map(|handle| &handle.0)
    }

    /// Requests the window to be of specific dimensions.
    ///
    /// If this is not set, some platform-specific dimensions will be used.
    ///
    /// See [`Window::request_inner_size`] for details.
    #[inline]
    pub fn with_inner_size<S: Into<Size>>(mut self, size: S) -> Self {
        self.inner_size = Some(size.into());
        self
    }

    /// Sets the minimum dimensions a window can have.
    ///
    /// If this is not set, the window will have no minimum dimensions (aside
    /// from reserved).
    ///
    /// See [`Window::set_min_inner_size`] for details.
    #[inline]
    pub fn with_min_inner_size<S: Into<Size>>(mut self, min_size: S) -> Self {
        self.min_inner_size = Some(min_size.into());
        self
    }

    /// Sets the maximum dimensions a window can have.
    ///
    /// If this is not set, the window will have no maximum or will be set to
    /// the primary monitor's dimensions by the platform.
    ///
    /// See [`Window::set_max_inner_size`] for details.
    #[inline]
    pub fn with_max_inner_size<S: Into<Size>>(mut self, max_size: S) -> Self {
        self.max_inner_size = Some(max_size.into());
        self
    }

    /// Sets a desired initial position for the window.
    ///
    /// If this is not set, some platform-specific position will be chosen.
    ///
    /// See [`Window::set_outer_position`] for details.
    ///
    /// ## Platform-specific
    ///
    /// - **macOS:** The top left corner position of the window content, the window's "inner"
    ///   position. The window title bar will be placed above it. The window will be positioned such
    ///   that it fits on screen, maintaining set `inner_size` if any. If you need to precisely
    ///   position the top left corner of the whole window you have to use
    ///   [`Window::set_outer_position`] after creating the window.
    /// - **Windows:** The top left corner position of the window title bar, the window's "outer"
    ///   position. There may be a small gap between this position and the window due to the
    ///   specifics of the Window Manager.
    /// - **X11:** The top left corner of the window, the window's "outer" position.
    /// - **Others:** Ignored.
    #[inline]
    pub fn with_position<P: Into<Position>>(mut self, position: P) -> Self {
        self.position = Some(position.into());
        self
    }

    /// Sets whether the window is resizable or not.
    ///
    /// The default is `true`.
    ///
    /// See [`Window::set_resizable`] for details.
    #[inline]
    pub fn with_resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Sets the enabled window buttons.
    ///
    /// The default is [`WindowButtons::all`]
    ///
    /// See [`Window::set_enabled_buttons`] for details.
    #[inline]
    pub fn with_enabled_buttons(mut self, buttons: WindowButtons) -> Self {
        self.enabled_buttons = buttons;
        self
    }

    /// Sets the initial title of the window in the title bar.
    ///
    /// The default is `"winit window"`.
    ///
    /// See [`Window::set_title`] for details.
    #[inline]
    pub fn with_title<T: Into<String>>(mut self, title: T) -> Self {
        self.title = title.into();
        self
    }

    /// Sets whether the window should be put into fullscreen upon creation.
    ///
    /// The default is `None`.
    ///
    /// See [`Window::set_fullscreen`] for details.
    #[inline]
    pub fn with_fullscreen(mut self, fullscreen: Option<Fullscreen>) -> Self {
        self.fullscreen = fullscreen;
        self
    }

    /// Request that the window is maximized upon creation.
    ///
    /// The default is `false`.
    ///
    /// See [`Window::set_maximized`] for details.
    #[inline]
    pub fn with_maximized(mut self, maximized: bool) -> Self {
        self.maximized = maximized;
        self
    }

    /// Sets whether the window will be initially visible or hidden.
    ///
    /// The default is to show the window.
    ///
    /// See [`Window::set_visible`] for details.
    #[inline]
    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Sets whether the background of the window should be transparent.
    ///
    /// If this is `true`, writing colors with alpha values different than
    /// `1.0` will produce a transparent window. On some platforms this
    /// is more of a hint for the system and you'd still have the alpha
    /// buffer. To control it see [`Window::set_transparent`].
    ///
    /// The default is `false`.
    #[inline]
    pub fn with_transparent(mut self, transparent: bool) -> Self {
        self.transparent = transparent;
        self
    }

    /// Sets whether the background of the window should be blurred by the system.
    ///
    /// The default is `false`.
    ///
    /// See [`Window::set_blur`] for details.
    #[inline]
    pub fn with_blur(mut self, blur: bool) -> Self {
        self.blur = blur;
        self
    }

    /// Get whether the window will support transparency.
    #[inline]
    pub fn transparent(&self) -> bool {
        self.transparent
    }

    /// Sets whether the window should have a border, a title bar, etc.
    ///
    /// The default is `true`.
    ///
    /// See [`Window::set_decorations`] for details.
    #[inline]
    pub fn with_decorations(mut self, decorations: bool) -> Self {
        self.decorations = decorations;
        self
    }

    /// Sets the window level.
    ///
    /// This is just a hint to the OS, and the system could ignore it.
    ///
    /// The default is [`WindowLevel::Normal`].
    ///
    /// See [`WindowLevel`] for details.
    #[inline]
    pub fn with_window_level(mut self, level: WindowLevel) -> Self {
        self.window_level = level;
        self
    }

    /// Sets the window icon.
    ///
    /// The default is `None`.
    ///
    /// See [`Window::set_window_icon`] for details.
    #[inline]
    pub fn with_window_icon(mut self, window_icon: Option<Icon>) -> Self {
        self.window_icon = window_icon;
        self
    }

    /// Sets a specific theme for the window.
    ///
    /// If `None` is provided, the window will use the system theme.
    ///
    /// The default is `None`.
    ///
    /// ## Platform-specific
    ///
    /// - **macOS:** This is an app-wide setting.
    /// - **Wayland:** This controls only CSD. When using `None` it'll try to use dbus to get the
    ///   system preference. When explicit theme is used, this will avoid dbus all together.
    /// - **x11:** Build window with `_GTK_THEME_VARIANT` hint set to `dark` or `light`.
    /// - **iOS / Android / Web / x11 / Orbital:** Ignored.
    #[inline]
    pub fn with_theme(mut self, theme: Option<Theme>) -> Self {
        self.preferred_theme = theme;
        self
    }

    /// Build window with resize increments hint.
    ///
    /// The default is `None`.
    ///
    /// See [`Window::set_resize_increments`] for details.
    #[inline]
    pub fn with_resize_increments<S: Into<Size>>(mut self, resize_increments: S) -> Self {
        self.resize_increments = Some(resize_increments.into());
        self
    }

    /// Prevents the window contents from being captured by other apps.
    ///
    /// The default is `false`.
    ///
    /// ## Platform-specific
    ///
    /// - **macOS**: if `false`, [`NSWindowSharingNone`] is used but doesn't completely
    ///   prevent all apps from reading the window content, for instance, QuickTime.
    /// - **iOS / Android / Web / x11 / Orbital:** Ignored.
    ///
    /// [`NSWindowSharingNone`]: https://developer.apple.com/documentation/appkit/nswindowsharingtype/nswindowsharingnone
    #[inline]
    pub fn with_content_protected(mut self, protected: bool) -> Self {
        self.content_protected = protected;
        self
    }

    /// Whether the window will be initially focused or not.
    ///
    /// The window should be assumed as not focused by default
    /// following by the [`WindowEvent::Focused`].
    ///
    /// ## Platform-specific:
    ///
    /// **Android / iOS / X11 / Wayland / Orbital:** Unsupported.
    ///
    /// [`WindowEvent::Focused`]: crate::event::WindowEvent::Focused.
    #[inline]
    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Modifies the cursor icon of the window.
    ///
    /// The default is [`CursorIcon::Default`].
    ///
    /// See [`Window::set_cursor()`] for more details.
    #[inline]
    pub fn with_cursor(mut self, cursor: impl Into<Cursor>) -> Self {
        self.cursor = cursor.into();
        self
    }

    #[inline]
    pub unsafe fn with_parent_window(
        mut self,
        parent_window: Option<raw_window_handle::RawWindowHandle>,
    ) -> Self {
        self.parent_window = parent_window.map(SendSyncRawWindowHandle);
        self
    }
}

/// Base Window functions.
impl Window {
    /// Create a new [`WindowAttributes`] which allows modifying the window's attributes before
    /// creation.
    #[inline]
    pub fn default_attributes() -> WindowAttributes {
        WindowAttributes::default()
    }

    /// Returns an identifier unique to the window.
    #[inline]
    pub fn id(&self) -> WindowId {
        let _span = tracing::debug_span!("rio_window::Window::id",).entered();

        self.window.maybe_wait_on_main(|w| WindowId(w.id()))
    }

    /// Returns the scale factor that can be used to map logical pixels to physical pixels, and
    /// vice versa.
    ///
    /// Note that this value can change depending on user action (for example if the window is
    /// moved to another screen); as such, tracking [`WindowEvent::ScaleFactorChanged`] events is
    /// the most robust way to track the DPI you need to use to draw.
    ///
    /// This value may differ from [`MonitorHandle::scale_factor`].
    ///
    /// See the [`dpi`] crate for more information.
    ///
    /// ## Platform-specific
    ///
    /// The scale factor is calculated differently on different platforms:
    ///
    /// - **Windows:** On Windows 8 and 10, per-monitor scaling is readily configured by users from
    ///   the display settings. While users are free to select any option they want, they're only
    ///   given a selection of "nice" scale factors, i.e. 1.0, 1.25, 1.5... on Windows 7. The scale
    ///   factor is global and changing it requires logging out. See [this article][windows_1] for
    ///   technical details.
    /// - **macOS:** Recent macOS versions allow the user to change the scaling factor for specific
    ///   displays. When available, the user may pick a per-monitor scaling factor from a set of
    ///   pre-defined settings. All "retina displays" have a scaling factor above 1.0 by default,
    ///   but the specific value varies across devices.
    /// - **X11:** Many man-hours have been spent trying to figure out how to handle DPI in X11.
    ///   Winit currently uses a three-pronged approach:
    ///   + Use the value in the `WINIT_X11_SCALE_FACTOR` environment variable if present.
    ///   + If not present, use the value set in `Xft.dpi` in Xresources.
    ///   + Otherwise, calculate the scale factor based on the millimeter monitor dimensions
    ///     provided by XRandR.
    ///
    ///   If `WINIT_X11_SCALE_FACTOR` is set to `randr`, it'll ignore the `Xft.dpi` field and use
    ///   the   XRandR scaling method. Generally speaking, you should try to configure the
    ///   standard system   variables to do what you want before resorting to
    ///   `WINIT_X11_SCALE_FACTOR`.
    /// - **Wayland:** The scale factor is suggested by the compositor for each window individually
    ///   by using the wp-fractional-scale protocol if available. Falls back to integer-scale
    ///   factors otherwise.
    ///
    ///   The monitor scale factor may differ from the window scale factor.
    /// - **iOS:** Scale factors are set by Apple to the value that best suits the device, and range
    ///   from `1.0` to `3.0`. See [this article][apple_1] and [this article][apple_2] for more
    ///   information.
    ///
    ///   This uses the underlying `UIView`'s [`contentScaleFactor`].
    /// - **Android:** Scale factors are set by the manufacturer to the value that best suits the
    ///   device, and range from `1.0` to `4.0`. See [this article][android_1] for more information.
    ///
    ///   This is currently unimplemented, and this function always returns 1.0.
    /// - **Web:** The scale factor is the ratio between CSS pixels and the physical device pixels.
    ///   In other words, it is the value of [`window.devicePixelRatio`][web_1]. It is affected by
    ///   both the screen scaling and the browser zoom level and can go below `1.0`.
    /// - **Orbital:** This is currently unimplemented, and this function always returns 1.0.
    ///
    /// [`WindowEvent::ScaleFactorChanged`]: crate::event::WindowEvent::ScaleFactorChanged
    /// [windows_1]: https://docs.microsoft.com/en-us/windows/win32/hidpi/high-dpi-desktop-application-development-on-windows
    /// [apple_1]: https://developer.apple.com/library/archive/documentation/DeviceInformation/Reference/iOSDeviceCompatibility/Displays/Displays.html
    /// [apple_2]: https://developer.apple.com/design/human-interface-guidelines/macos/icons-and-images/image-size-and-resolution/
    /// [android_1]: https://developer.android.com/training/multiscreen/screendensities
    /// [web_1]: https://developer.mozilla.org/en-US/docs/Web/API/Window/devicePixelRatio
    /// [`contentScaleFactor`]: https://developer.apple.com/documentation/uikit/uiview/1622657-contentscalefactor?language=objc
    #[inline]
    pub fn scale_factor(&self) -> f64 {
        let _span = tracing::debug_span!("rio_window::Window::scale_factor",).entered();

        self.window.maybe_wait_on_main(|w| w.scale_factor())
    }

    /// Queues a [`WindowEvent::RedrawRequested`] event to be emitted that aligns with the windowing
    /// system drawing loop.
    ///
    /// This is the **strongly encouraged** method of redrawing windows, as it can integrate with
    /// OS-requested redraws (e.g. when a window gets resized). To improve the event delivery
    /// consider using [`Window::pre_present_notify`] as described in docs.
    ///
    /// Applications should always aim to redraw whenever they receive a `RedrawRequested` event.
    ///
    /// There are no strong guarantees about when exactly a `RedrawRequest` event will be emitted
    /// with respect to other events, since the requirements can vary significantly between
    /// windowing systems.
    ///
    /// However as the event aligns with the windowing system drawing loop, it may not arrive in
    /// same or even next event loop iteration.
    ///
    /// ## Platform-specific
    ///
    /// - **Windows** This API uses `RedrawWindow` to request a `WM_PAINT` message and
    ///   `RedrawRequested` is emitted in sync with any `WM_PAINT` messages.
    /// - **iOS:** Can only be called on the main thread.
    /// - **Wayland:** The events are aligned with the frame callbacks when
    ///   [`Window::pre_present_notify`] is used.
    /// - **Web:** [`WindowEvent::RedrawRequested`] will be aligned with the
    ///   `requestAnimationFrame`.
    ///
    /// [`WindowEvent::RedrawRequested`]: crate::event::WindowEvent::RedrawRequested
    #[inline]
    pub fn request_redraw(&self) {
        let _span = tracing::debug_span!("rio_window::Window::request_redraw",).entered();

        self.window.maybe_queue_on_main(|w| w.request_redraw())
    }

    /// Notify the windowing system before presenting to the window.
    ///
    /// You should call this event after your drawing operations, but before you submit
    /// the buffer to the display or commit your drawings. Doing so will help winit to properly
    /// schedule and make assumptions about its internal state. For example, it could properly
    /// throttle [`WindowEvent::RedrawRequested`].
    ///
    /// ## Example
    ///
    /// This example illustrates how it looks with OpenGL, but it applies to other graphics
    /// APIs and software rendering.
    ///
    /// ```no_run
    /// # use rio_window::window::Window;
    /// # fn swap_buffers() {}
    /// # fn scope(window: &Window) {
    /// // Do the actual drawing with OpenGL.
    ///
    /// // Notify winit that we're about to submit buffer to the windowing system.
    /// window.pre_present_notify();
    ///
    /// // Submit buffer to the windowing system.
    /// swap_buffers();
    /// # }
    /// ```
    ///
    /// ## Platform-specific
    ///
    /// **Wayland:** - schedules a frame callback to throttle [`WindowEvent::RedrawRequested`].
    ///
    /// [`WindowEvent::RedrawRequested`]: crate::event::WindowEvent::RedrawRequested
    #[inline]
    pub fn pre_present_notify(&self) {
        let _span =
            tracing::debug_span!("rio_window::Window::pre_present_notify",).entered();

        self.window.maybe_queue_on_main(|w| w.pre_present_notify());
    }

    /// Reset the dead key state of the keyboard.
    ///
    /// This is useful when a dead key is bound to trigger an action. Then
    /// this function can be called to reset the dead key state so that
    /// follow-up text input won't be affected by the dead key.
    ///
    /// ## Platform-specific
    /// - **Web, macOS:** Does nothing
    // ---------------------------
    // Developers' Note: If this cannot be implemented on every desktop platform
    // at least, then this function should be provided through a platform specific
    // extension trait
    pub fn reset_dead_keys(&self) {
        let _span =
            tracing::debug_span!("rio_window::Window::reset_dead_keys",).entered();

        self.window.maybe_queue_on_main(|w| w.reset_dead_keys())
    }
}

/// Position and size functions.
impl Window {
    /// Returns the position of the top-left hand corner of the window's client area relative to the
    /// top-left hand corner of the desktop.
    ///
    /// The same conditions that apply to [`Window::outer_position`] apply to this method.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS:** Can only be called on the main thread. Returns the top left coordinates of the
    ///   window's [safe area] in the screen space coordinate system.
    /// - **Web:** Returns the top-left coordinates relative to the viewport. _Note: this returns
    ///   the same value as [`Window::outer_position`]._
    /// - **Android / Wayland:** Always returns [`NotSupportedError`].
    ///
    /// [safe area]: https://developer.apple.com/documentation/uikit/uiview/2891103-safeareainsets?language=objc
    #[inline]
    pub fn inner_position(&self) -> Result<PhysicalPosition<i32>, NotSupportedError> {
        let _span = tracing::debug_span!("rio_window::Window::inner_position",).entered();

        self.window.maybe_wait_on_main(|w| w.inner_position())
    }

    /// Returns the position of the top-left hand corner of the window relative to the
    /// top-left hand corner of the desktop.
    ///
    /// Note that the top-left hand corner of the desktop is not necessarily the same as
    /// the screen. If the user uses a desktop with multiple monitors, the top-left hand corner
    /// of the desktop is the top-left hand corner of the monitor at the top-left of the desktop.
    ///
    /// The coordinates can be negative if the top-left hand corner of the window is outside
    /// of the visible screen region.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS:** Can only be called on the main thread. Returns the top left coordinates of the
    ///   window in the screen space coordinate system.
    /// - **Web:** Returns the top-left coordinates relative to the viewport.
    /// - **Android / Wayland:** Always returns [`NotSupportedError`].
    #[inline]
    pub fn outer_position(&self) -> Result<PhysicalPosition<i32>, NotSupportedError> {
        let _span = tracing::debug_span!("rio_window::Window::outer_position",).entered();

        self.window.maybe_wait_on_main(|w| w.outer_position())
    }

    /// Modifies the position of the window.
    ///
    /// See [`Window::outer_position`] for more information about the coordinates.
    /// This automatically un-maximizes the window if it's maximized.
    ///
    /// ```no_run
    /// # use rio_window::dpi::{LogicalPosition, PhysicalPosition};
    /// # use rio_window::window::Window;
    /// # fn scope(window: &Window) {
    /// // Specify the position in logical dimensions like this:
    /// window.set_outer_position(LogicalPosition::new(400.0, 200.0));
    ///
    /// // Or specify the position in physical dimensions like this:
    /// window.set_outer_position(PhysicalPosition::new(400, 200));
    /// # }
    /// ```
    ///
    /// ## Platform-specific
    ///
    /// - **iOS:** Can only be called on the main thread. Sets the top left coordinates of the
    ///   window in the screen space coordinate system.
    /// - **Web:** Sets the top-left coordinates relative to the viewport. Doesn't account for CSS
    ///   [`transform`].
    /// - **Android / Wayland:** Unsupported.
    ///
    /// [`transform`]: https://developer.mozilla.org/en-US/docs/Web/CSS/transform
    #[inline]
    pub fn set_outer_position<P: Into<Position>>(&self, position: P) {
        let position = position.into();
        let _span = tracing::debug_span!(
            "rio_window::Window::set_outer_position",
            position = ?position
        )
        .entered();

        self.window
            .maybe_queue_on_main(move |w| w.set_outer_position(position))
    }

    /// Returns the physical size of the window's client area.
    ///
    /// The client area is the content of the window, excluding the title bar and borders.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS:** Can only be called on the main thread. Returns the `PhysicalSize` of the window's
    ///   [safe area] in screen space coordinates.
    /// - **Web:** Returns the size of the canvas element. Doesn't account for CSS [`transform`].
    ///
    /// [safe area]: https://developer.apple.com/documentation/uikit/uiview/2891103-safeareainsets?language=objc
    /// [`transform`]: https://developer.mozilla.org/en-US/docs/Web/CSS/transform
    #[inline]
    pub fn inner_size(&self) -> PhysicalSize<u32> {
        let _span = tracing::debug_span!("rio_window::Window::inner_size",).entered();

        self.window.maybe_wait_on_main(|w| w.inner_size())
    }

    /// Request the new size for the window.
    ///
    /// On platforms where the size is entirely controlled by the user the
    /// applied size will be returned immediately, resize event in such case
    /// may not be generated.
    ///
    /// On platforms where resizing is disallowed by the windowing system, the current
    /// inner size is returned immediately, and the user one is ignored.
    ///
    /// When `None` is returned, it means that the request went to the display system,
    /// and the actual size will be delivered later with the [`WindowEvent::Resized`].
    ///
    /// See [`Window::inner_size`] for more information about the values.
    ///
    /// The request could automatically un-maximize the window if it's maximized.
    ///
    /// ```no_run
    /// # use rio_window::dpi::{LogicalSize, PhysicalSize};
    /// # use rio_window::window::Window;
    /// # fn scope(window: &Window) {
    /// // Specify the size in logical dimensions like this:
    /// let _ = window.request_inner_size(LogicalSize::new(400.0, 200.0));
    ///
    /// // Or specify the size in physical dimensions like this:
    /// let _ = window.request_inner_size(PhysicalSize::new(400, 200));
    /// # }
    /// ```
    ///
    /// ## Platform-specific
    ///
    /// - **Web:** Sets the size of the canvas element. Doesn't account for CSS [`transform`].
    ///
    /// [`WindowEvent::Resized`]: crate::event::WindowEvent::Resized
    /// [`transform`]: https://developer.mozilla.org/en-US/docs/Web/CSS/transform
    #[inline]
    #[must_use]
    pub fn request_inner_size<S: Into<Size>>(
        &self,
        size: S,
    ) -> Option<PhysicalSize<u32>> {
        let size = size.into();
        let _span = tracing::debug_span!(
            "rio_window::Window::request_inner_size",
            size = ?size
        )
        .entered();
        self.window
            .maybe_wait_on_main(|w| w.request_inner_size(size))
    }

    /// Returns the physical size of the entire window.
    ///
    /// These dimensions include the title bar and borders. If you don't want that (and you usually
    /// don't), use [`Window::inner_size`] instead.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS:** Can only be called on the main thread. Returns the [`PhysicalSize`] of the window
    ///   in screen space coordinates.
    /// - **Web:** Returns the size of the canvas element. _Note: this returns the same value as
    ///   [`Window::inner_size`]._
    #[inline]
    pub fn outer_size(&self) -> PhysicalSize<u32> {
        let _span = tracing::debug_span!("rio_window::Window::outer_size",).entered();
        self.window.maybe_wait_on_main(|w| w.outer_size())
    }

    /// Sets a minimum dimension size for the window.
    ///
    /// ```no_run
    /// # use rio_window::dpi::{LogicalSize, PhysicalSize};
    /// # use rio_window::window::Window;
    /// # fn scope(window: &Window) {
    /// // Specify the size in logical dimensions like this:
    /// window.set_min_inner_size(Some(LogicalSize::new(400.0, 200.0)));
    ///
    /// // Or specify the size in physical dimensions like this:
    /// window.set_min_inner_size(Some(PhysicalSize::new(400, 200)));
    /// # }
    /// ```
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / Orbital:** Unsupported.
    #[inline]
    pub fn set_min_inner_size<S: Into<Size>>(&self, min_size: Option<S>) {
        let min_size = min_size.map(|s| s.into());
        let _span = tracing::debug_span!(
            "rio_window::Window::set_min_inner_size",
            min_size = ?min_size
        )
        .entered();
        self.window
            .maybe_queue_on_main(move |w| w.set_min_inner_size(min_size))
    }

    /// Sets a maximum dimension size for the window.
    ///
    /// ```no_run
    /// # use rio_window::dpi::{LogicalSize, PhysicalSize};
    /// # use rio_window::window::Window;
    /// # fn scope(window: &Window) {
    /// // Specify the size in logical dimensions like this:
    /// window.set_max_inner_size(Some(LogicalSize::new(400.0, 200.0)));
    ///
    /// // Or specify the size in physical dimensions like this:
    /// window.set_max_inner_size(Some(PhysicalSize::new(400, 200)));
    /// # }
    /// ```
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / Orbital:** Unsupported.
    #[inline]
    pub fn set_max_inner_size<S: Into<Size>>(&self, max_size: Option<S>) {
        let max_size = max_size.map(|s| s.into());
        let _span = tracing::debug_span!(
            "rio_window::Window::max_size",
            max_size = ?max_size
        )
        .entered();
        self.window
            .maybe_queue_on_main(move |w| w.set_max_inner_size(max_size))
    }

    /// Returns window resize increments if any were set.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / Web / Wayland / Orbital:** Always returns [`None`].
    #[inline]
    pub fn resize_increments(&self) -> Option<PhysicalSize<u32>> {
        let _span =
            tracing::debug_span!("rio_window::Window::resize_increments",).entered();
        self.window.maybe_wait_on_main(|w| w.resize_increments())
    }

    /// Sets window resize increments.
    ///
    /// This is a niche constraint hint usually employed by terminal emulators
    /// and other apps that need "blocky" resizes.
    ///
    /// ## Platform-specific
    ///
    /// - **macOS:** Increments are converted to logical size and then macOS rounds them to whole
    ///   numbers.
    /// - **Wayland:** Not implemented.
    /// - **iOS / Android / Web / Orbital:** Unsupported.
    #[inline]
    pub fn set_resize_increments<S: Into<Size>>(&self, increments: Option<S>) {
        let increments = increments.map(Into::into);
        let _span = tracing::debug_span!(
            "rio_window::Window::set_resize_increments",
            increments = ?increments
        )
        .entered();
        self.window
            .maybe_queue_on_main(move |w| w.set_resize_increments(increments))
    }
}

/// Misc. attribute functions.
impl Window {
    /// Modifies the title of the window.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android:** Unsupported.
    #[inline]
    pub fn set_title(&self, title: &str) {
        let _span =
            tracing::debug_span!("rio_window::Window::set_title", title).entered();
        self.window.maybe_wait_on_main(|w| w.set_title(title))
    }

    #[inline]
    #[cfg(target_os = "macos")]
    pub fn set_subtitle(&self, title: &str) {
        let _span =
            tracing::debug_span!("rio_window::Window::set_title", title).entered();
        self.window.maybe_wait_on_main(|w| w.set_subtitle(title))
    }

    /// Change the window transparency state.
    ///
    /// This is just a hint that may not change anything about
    /// the window transparency, however doing a mismatch between
    /// the content of your window and this hint may result in
    /// visual artifacts.
    ///
    /// The default value follows the [`WindowAttributes::with_transparent`].
    ///
    /// ## Platform-specific
    ///
    /// - **macOS:** If you're not drawing to the window yourself, you might have to set the
    ///   background color of the window to enable transparency.
    /// - **Web / iOS / Android:** Unsupported.
    /// - **X11:** Can only be set while building the window, with
    ///   [`WindowAttributes::with_transparent`].
    #[inline]
    pub fn set_transparent(&self, transparent: bool) {
        let _span =
            tracing::debug_span!("rio_window::Window::set_transparent", transparent)
                .entered();
        self.window
            .maybe_queue_on_main(move |w| w.set_transparent(transparent))
    }

    /// Change the window blur state.
    ///
    /// If `true`, this will make the transparent window background blurry.
    ///
    /// ## Platform-specific
    ///
    /// - **Android / iOS / X11 / Web / Windows:** Unsupported.
    /// - **Wayland:** Only works with org_kde_kwin_blur_manager protocol.
    #[inline]
    pub fn set_blur(&self, blur: bool) {
        let _span = tracing::debug_span!("rio_window::Window::set_blur", blur).entered();
        self.window.maybe_queue_on_main(move |w| w.set_blur(blur))
    }

    /// Modifies the window's visibility.
    ///
    /// If `false`, this will hide the window. If `true`, this will show the window.
    ///
    /// ## Platform-specific
    ///
    /// - **Android / Wayland / Web:** Unsupported.
    /// - **iOS:** Can only be called on the main thread.
    #[inline]
    pub fn set_visible(&self, visible: bool) {
        let _span =
            tracing::debug_span!("rio_window::Window::set_visible", visible).entered();
        self.window
            .maybe_queue_on_main(move |w| w.set_visible(visible))
    }

    /// Gets the window's current visibility state.
    ///
    /// `None` means it couldn't be determined, so it is not recommended to use this to drive your
    /// rendering backend.
    ///
    /// ## Platform-specific
    ///
    /// - **X11:** Not implemented.
    /// - **Wayland / iOS / Android / Web:** Unsupported.
    #[inline]
    pub fn is_visible(&self) -> Option<bool> {
        let _span = tracing::debug_span!("rio_window::Window::is_visible",).entered();
        self.window.maybe_wait_on_main(|w| w.is_visible())
    }

    /// Sets whether the window is resizable or not.
    ///
    /// Note that making the window unresizable doesn't exempt you from handling
    /// [`WindowEvent::Resized`], as that event can still be triggered by DPI scaling, entering
    /// fullscreen mode, etc. Also, the window could still be resized by calling
    /// [`Window::request_inner_size`].
    ///
    /// ## Platform-specific
    ///
    /// This only has an effect on desktop platforms.
    ///
    /// - **X11:** Due to a bug in XFCE, this has no effect on Xfwm.
    /// - **iOS / Android / Web:** Unsupported.
    ///
    /// [`WindowEvent::Resized`]: crate::event::WindowEvent::Resized
    #[inline]
    pub fn set_resizable(&self, resizable: bool) {
        let _span = tracing::debug_span!("rio_window::Window::set_resizable", resizable)
            .entered();
        self.window
            .maybe_queue_on_main(move |w| w.set_resizable(resizable))
    }

    /// Gets the window's current resizable state.
    ///
    /// ## Platform-specific
    ///
    /// - **X11:** Not implemented.
    /// - **iOS / Android / Web:** Unsupported.
    #[inline]
    pub fn is_resizable(&self) -> bool {
        let _span = tracing::debug_span!("rio_window::Window::is_resizable",).entered();
        self.window.maybe_wait_on_main(|w| w.is_resizable())
    }

    /// Sets the enabled window buttons.
    ///
    /// ## Platform-specific
    ///
    /// - **Wayland / X11 / Orbital:** Not implemented.
    /// - **Web / iOS / Android:** Unsupported.
    pub fn set_enabled_buttons(&self, buttons: WindowButtons) {
        let _span = tracing::debug_span!(
            "rio_window::Window::set_enabled_buttons",
            buttons = ?buttons
        )
        .entered();
        self.window
            .maybe_queue_on_main(move |w| w.set_enabled_buttons(buttons))
    }

    /// Gets the enabled window buttons.
    ///
    /// ## Platform-specific
    ///
    /// - **Wayland / X11 / Orbital:** Not implemented. Always returns [`WindowButtons::all`].
    /// - **Web / iOS / Android:** Unsupported. Always returns [`WindowButtons::all`].
    pub fn enabled_buttons(&self) -> WindowButtons {
        let _span =
            tracing::debug_span!("rio_window::Window::enabled_buttons",).entered();
        self.window.maybe_wait_on_main(|w| w.enabled_buttons())
    }

    /// Sets the window to minimized or back
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / Web / Orbital:** Unsupported.
    /// - **Wayland:** Un-minimize is unsupported.
    #[inline]
    pub fn set_minimized(&self, minimized: bool) {
        let _span = tracing::debug_span!("rio_window::Window::set_minimized", minimized)
            .entered();
        self.window
            .maybe_queue_on_main(move |w| w.set_minimized(minimized))
    }

    /// Gets the window's current minimized state.
    ///
    /// `None` will be returned, if the minimized state couldn't be determined.
    ///
    /// ## Note
    ///
    /// - You shouldn't stop rendering for minimized windows, however you could lower the fps.
    ///
    /// ## Platform-specific
    ///
    /// - **Wayland**: always `None`.
    /// - **iOS / Android / Web / Orbital:** Unsupported.
    #[inline]
    pub fn is_minimized(&self) -> Option<bool> {
        let _span = tracing::debug_span!("rio_window::Window::is_minimized",).entered();
        self.window.maybe_wait_on_main(|w| w.is_minimized())
    }

    /// Sets the window to maximized or back.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / Web:** Unsupported.
    #[inline]
    pub fn set_maximized(&self, maximized: bool) {
        let _span = tracing::debug_span!("rio_window::Window::set_maximized", maximized)
            .entered();
        self.window
            .maybe_queue_on_main(move |w| w.set_maximized(maximized))
    }

    /// Gets the window's current maximized state.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / Web:** Unsupported.
    #[inline]
    pub fn is_maximized(&self) -> bool {
        let _span = tracing::debug_span!("rio_window::Window::is_maximized",).entered();
        self.window.maybe_wait_on_main(|w| w.is_maximized())
    }

    /// Sets the window to fullscreen or back.
    ///
    /// ## Platform-specific
    ///
    /// - **macOS:** [`Fullscreen::Exclusive`] provides true exclusive mode with a video mode
    ///   change. *Caveat!* macOS doesn't provide task switching (or spaces!) while in exclusive
    ///   fullscreen mode. This mode should be used when a video mode change is desired, but for a
    ///   better user experience, borderless fullscreen might be preferred.
    ///
    ///   [`Fullscreen::Borderless`] provides a borderless fullscreen window on a
    ///   separate space. This is the idiomatic way for fullscreen games to work
    ///   on macOS. See `WindowExtMacOs::set_simple_fullscreen` if
    ///   separate spaces are not preferred.
    ///
    ///   The dock and the menu bar are disabled in exclusive fullscreen mode.
    /// - **iOS:** Can only be called on the main thread.
    /// - **Wayland:** Does not support exclusive fullscreen mode and will no-op a request.
    /// - **Windows:** Screen saver is disabled in fullscreen mode.
    /// - **Android / Orbital:** Unsupported.
    /// - **Web:** Does nothing without a [transient activation].
    ///
    /// [transient activation]: https://developer.mozilla.org/en-US/docs/Glossary/Transient_activation
    #[inline]
    pub fn set_fullscreen(&self, fullscreen: Option<Fullscreen>) {
        let _span = tracing::debug_span!(
            "rio_window::Window::set_fullscreen",
            fullscreen = ?fullscreen
        )
        .entered();
        self.window
            .maybe_queue_on_main(move |w| w.set_fullscreen(fullscreen.map(|f| f.into())))
    }

    /// Gets the window's current fullscreen state.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS:** Can only be called on the main thread.
    /// - **Android / Orbital:** Will always return `None`.
    /// - **Wayland:** Can return `Borderless(None)` when there are no monitors.
    /// - **Web:** Can only return `None` or `Borderless(None)`.
    #[inline]
    pub fn fullscreen(&self) -> Option<Fullscreen> {
        let _span = tracing::debug_span!("rio_window::Window::fullscreen",).entered();
        self.window
            .maybe_wait_on_main(|w| w.fullscreen().map(|f| f.into()))
    }

    /// Turn window decorations on or off.
    ///
    /// Enable/disable window decorations provided by the server or Winit.
    /// By default this is enabled. Note that fullscreen windows and windows on
    /// mobile and web platforms naturally do not have decorations.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / Web:** No effect.
    #[inline]
    pub fn set_decorations(&self, decorations: bool) {
        let _span =
            tracing::debug_span!("rio_window::Window::set_decorations", decorations)
                .entered();
        self.window
            .maybe_queue_on_main(move |w| w.set_decorations(decorations))
    }

    /// Gets the window's current decorations state.
    ///
    /// Returns `true` when windows are decorated (server-side or by Winit).
    /// Also returns `true` when no decorations are required (mobile, web).
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / Web:** Always returns `true`.
    #[inline]
    pub fn is_decorated(&self) -> bool {
        let _span = tracing::debug_span!("rio_window::Window::is_decorated",).entered();
        self.window.maybe_wait_on_main(|w| w.is_decorated())
    }

    /// Change the window level.
    ///
    /// This is just a hint to the OS, and the system could ignore it.
    ///
    /// See [`WindowLevel`] for details.
    pub fn set_window_level(&self, level: WindowLevel) {
        let _span = tracing::debug_span!(
            "rio_window::Window::set_window_level",
            level = ?level
        )
        .entered();
        self.window
            .maybe_queue_on_main(move |w| w.set_window_level(level))
    }

    /// Sets the window icon.
    ///
    /// On Windows and X11, this is typically the small icon in the top-left
    /// corner of the titlebar.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / Web / Wayland / macOS / Orbital:** Unsupported.
    ///
    /// - **Windows:** Sets `ICON_SMALL`. The base size for a window icon is 16x16, but it's
    ///   recommended to account for screen scaling and pick a multiple of that, i.e. 32x32.
    ///
    /// - **X11:** Has no universal guidelines for icon sizes, so you're at the whims of the WM.
    ///   That said, it's usually in the same ballpark as on Windows.
    #[inline]
    pub fn set_window_icon(&self, window_icon: Option<Icon>) {
        let _span =
            tracing::debug_span!("rio_window::Window::set_window_icon",).entered();
        self.window
            .maybe_queue_on_main(move |w| w.set_window_icon(window_icon))
    }

    /// Set the IME cursor editing area, where the `position` is the top left corner of that area
    /// and `size` is the size of this area starting from the position. An example of such area
    /// could be a input field in the UI or line in the editor.
    ///
    /// The windowing system could place a candidate box close to that area, but try to not obscure
    /// the specified area, so the user input to it stays visible.
    ///
    /// The candidate box is the window / popup / overlay that allows you to select the desired
    /// characters. The look of this box may differ between input devices, even on the same
    /// platform.
    ///
    /// (Apple's official term is "candidate window", see their [chinese] and [japanese] guides).
    ///
    /// ## Example
    ///
    /// ```no_run
    /// # use rio_window::dpi::{LogicalPosition, PhysicalPosition, LogicalSize, PhysicalSize};
    /// # use rio_window::window::Window;
    /// # fn scope(window: &Window) {
    /// // Specify the position in logical dimensions like this:
    /// window.set_ime_cursor_area(LogicalPosition::new(400.0, 200.0), LogicalSize::new(100, 100));
    ///
    /// // Or specify the position in physical dimensions like this:
    /// window.set_ime_cursor_area(PhysicalPosition::new(400, 200), PhysicalSize::new(100, 100));
    /// # }
    /// ```
    ///
    /// ## Platform-specific
    ///
    /// - **X11:** - area is not supported, only position.
    /// - **iOS / Android / Web / Orbital:** Unsupported.
    ///
    /// [chinese]: https://support.apple.com/guide/chinese-input-method/use-the-candidate-window-cim12992/104/mac/12.0
    /// [japanese]: https://support.apple.com/guide/japanese-input-method/use-the-candidate-window-jpim10262/6.3/mac/12.0
    #[inline]
    pub fn set_ime_cursor_area<P: Into<Position>, S: Into<Size>>(
        &self,
        position: P,
        size: S,
    ) {
        let position = position.into();
        let size = size.into();
        let _span = tracing::debug_span!(
            "rio_window::Window::set_ime_cursor_area",
            position = ?position,
            size = ?size,
        )
        .entered();
        self.window
            .maybe_queue_on_main(move |w| w.set_ime_cursor_area(position, size))
    }

    /// Sets whether the window should get IME events
    ///
    /// When IME is allowed, the window will receive [`Ime`] events, and during the
    /// preedit phase the window will NOT get [`KeyboardInput`] events. The window
    /// should allow IME while it is expecting text input.
    ///
    /// When IME is not allowed, the window won't receive [`Ime`] events, and will
    /// receive [`KeyboardInput`] events for every keypress instead. Not allowing
    /// IME is useful for games for example.
    ///
    /// IME is **not** allowed by default.
    ///
    /// ## Platform-specific
    ///
    /// - **macOS:** IME must be enabled to receive text-input where dead-key sequences are
    ///   combined.
    /// - **iOS / Android / Web / Orbital:** Unsupported.
    /// - **X11**: Enabling IME will disable dead keys reporting during compose.
    ///
    /// [`Ime`]: crate::event::WindowEvent::Ime
    /// [`KeyboardInput`]: crate::event::WindowEvent::KeyboardInput
    #[inline]
    pub fn set_ime_allowed(&self, allowed: bool) {
        let _span = tracing::debug_span!("rio_window::Window::set_ime_allowed", allowed)
            .entered();
        self.window
            .maybe_queue_on_main(move |w| w.set_ime_allowed(allowed))
    }

    /// Sets the IME purpose for the window using [`ImePurpose`].
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / Web / Windows / X11 / macOS / Orbital:** Unsupported.
    #[inline]
    pub fn set_ime_purpose(&self, purpose: ImePurpose) {
        let _span = tracing::debug_span!(
            "rio_window::Window::set_ime_purpose",
            purpose = ?purpose
        )
        .entered();
        self.window
            .maybe_queue_on_main(move |w| w.set_ime_purpose(purpose))
    }

    /// Brings the window to the front and sets input focus. Has no effect if the window is
    /// already in focus, minimized, or not visible.
    ///
    /// This method steals input focus from other applications. Do not use this method unless
    /// you are certain that's what the user wants. Focus stealing can cause an extremely disruptive
    /// user experience.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / Wayland / Orbital:** Unsupported.
    #[inline]
    pub fn focus_window(&self) {
        let _span = tracing::debug_span!("rio_window::Window::focus_window",).entered();
        self.window.maybe_queue_on_main(|w| w.focus_window())
    }

    /// Gets whether the window has keyboard focus.
    ///
    /// This queries the same state information as [`WindowEvent::Focused`].
    ///
    /// [`WindowEvent::Focused`]: crate::event::WindowEvent::Focused
    #[inline]
    pub fn has_focus(&self) -> bool {
        let _span = tracing::debug_span!("rio_window::Window::has_focus",).entered();
        self.window.maybe_wait_on_main(|w| w.has_focus())
    }

    /// Requests user attention to the window, this has no effect if the application
    /// is already focused. How requesting for user attention manifests is platform dependent,
    /// see [`UserAttentionType`] for details.
    ///
    /// Providing `None` will unset the request for user attention. Unsetting the request for
    /// user attention might not be done automatically by the WM when the window receives input.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / Web / Orbital:** Unsupported.
    /// - **macOS:** `None` has no effect.
    /// - **X11:** Requests for user attention must be manually cleared.
    /// - **Wayland:** Requires `xdg_activation_v1` protocol, `None` has no effect.
    #[inline]
    pub fn request_user_attention(&self, request_type: Option<UserAttentionType>) {
        let _span = tracing::debug_span!(
            "rio_window::Window::request_user_attention",
            request_type = ?request_type
        )
        .entered();
        self.window
            .maybe_queue_on_main(move |w| w.request_user_attention(request_type))
    }

    /// Sets the current window theme. Use `None` to fallback to system default.
    ///
    /// ## Platform-specific
    ///
    /// - **macOS:** This is an app-wide setting.
    /// - **Wayland:** Sets the theme for the client side decorations. Using `None` will use dbus to
    ///   get the system preference.
    /// - **X11:** Sets `_GTK_THEME_VARIANT` hint to `dark` or `light` and if `None` is used, it
    ///   will default to  [`Theme::Dark`].
    /// - **iOS / Android / Web / Orbital:** Unsupported.
    #[inline]
    pub fn set_theme(&self, theme: Option<Theme>) {
        let _span = tracing::debug_span!(
            "rio_window::Window::set_theme",
            theme = ?theme
        )
        .entered();
        self.window.maybe_queue_on_main(move |w| w.set_theme(theme))
    }

    /// Returns the current window theme.
    ///
    /// ## Platform-specific
    ///
    /// - **macOS:** This is an app-wide setting.
    /// - **iOS / Android / Wayland / x11 / Orbital:** Unsupported.
    #[inline]
    pub fn theme(&self) -> Option<Theme> {
        let _span = tracing::debug_span!("rio_window::Window::theme",).entered();
        self.window.maybe_wait_on_main(|w| w.theme())
    }

    /// Prevents the window contents from being captured by other apps.
    ///
    /// ## Platform-specific
    ///
    /// - **macOS**: if `false`, [`NSWindowSharingNone`] is used but doesn't completely
    ///   prevent all apps from reading the window content, for instance, QuickTime.
    /// - **iOS / Android / x11 / Wayland / Web / Orbital:** Unsupported.
    ///
    /// [`NSWindowSharingNone`]: https://developer.apple.com/documentation/appkit/nswindowsharingtype/nswindowsharingnone
    pub fn set_content_protected(&self, protected: bool) {
        let _span =
            tracing::debug_span!("rio_window::Window::set_content_protected", protected)
                .entered();
        self.window
            .maybe_queue_on_main(move |w| w.set_content_protected(protected))
    }

    /// Gets the current title of the window.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / x11 / Wayland / Web:** Unsupported. Always returns an empty string.
    #[inline]
    pub fn title(&self) -> String {
        let _span = tracing::debug_span!("rio_window::Window::title",).entered();
        self.window.maybe_wait_on_main(|w| w.title())
    }
}

/// Cursor functions.
impl Window {
    /// Modifies the cursor icon of the window.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / Orbital:** Unsupported.
    /// - **Web:** Custom cursors have to be loaded and decoded first, until then the previous
    ///   cursor is shown.
    #[inline]
    pub fn set_cursor(&self, cursor: impl Into<Cursor>) {
        let cursor = cursor.into();
        let _span = tracing::debug_span!("rio_window::Window::set_cursor",).entered();
        self.window
            .maybe_queue_on_main(move |w| w.set_cursor(cursor))
    }

    /// Deprecated! Use [`Window::set_cursor()`] instead.
    #[deprecated = "Renamed to `set_cursor`"]
    #[inline]
    pub fn set_cursor_icon(&self, icon: CursorIcon) {
        self.set_cursor(icon)
    }

    /// Changes the position of the cursor in window coordinates.
    ///
    /// ```no_run
    /// # use rio_window::dpi::{LogicalPosition, PhysicalPosition};
    /// # use rio_window::window::Window;
    /// # fn scope(window: &Window) {
    /// // Specify the position in logical dimensions like this:
    /// window.set_cursor_position(LogicalPosition::new(400.0, 200.0));
    ///
    /// // Or specify the position in physical dimensions like this:
    /// window.set_cursor_position(PhysicalPosition::new(400, 200));
    /// # }
    /// ```
    ///
    /// ## Platform-specific
    ///
    /// - **Wayland**: Cursor must be in [`CursorGrabMode::Locked`].
    /// - **iOS / Android / Web / Orbital:** Always returns an [`ExternalError::NotSupported`].
    #[inline]
    pub fn set_cursor_position<P: Into<Position>>(
        &self,
        position: P,
    ) -> Result<(), ExternalError> {
        let position = position.into();
        let _span = tracing::debug_span!(
            "rio_window::Window::set_cursor_position",
            position = ?position
        )
        .entered();
        self.window
            .maybe_wait_on_main(|w| w.set_cursor_position(position))
    }

    /// Set grabbing [mode][CursorGrabMode] on the cursor preventing it from leaving the window.
    ///
    /// # Example
    ///
    /// First try confining the cursor, and if that fails, try locking it instead.
    ///
    /// ```no_run
    /// # use rio_window::window::{CursorGrabMode, Window};
    /// # fn scope(window: &Window) {
    /// window
    ///     .set_cursor_grab(CursorGrabMode::Confined)
    ///     .or_else(|_e| window.set_cursor_grab(CursorGrabMode::Locked))
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn set_cursor_grab(&self, mode: CursorGrabMode) -> Result<(), ExternalError> {
        let _span = tracing::debug_span!(
            "rio_window::Window::set_cursor_grab",
            mode = ?mode
        )
        .entered();
        self.window.maybe_wait_on_main(|w| w.set_cursor_grab(mode))
    }

    /// Modifies the cursor's visibility.
    ///
    /// If `false`, this will hide the cursor. If `true`, this will show the cursor.
    ///
    /// ## Platform-specific
    ///
    /// - **Windows:** The cursor is only hidden within the confines of the window.
    /// - **X11:** The cursor is only hidden within the confines of the window.
    /// - **Wayland:** The cursor is only hidden within the confines of the window.
    /// - **macOS:** The cursor is hidden as long as the window has input focus, even if the cursor
    ///   is outside of the window.
    /// - **iOS / Android:** Unsupported.
    #[inline]
    pub fn set_cursor_visible(&self, visible: bool) {
        let _span =
            tracing::debug_span!("rio_window::Window::set_cursor_visible", visible)
                .entered();
        self.window
            .maybe_queue_on_main(move |w| w.set_cursor_visible(visible))
    }

    /// Moves the window with the left mouse button until the button is released.
    ///
    /// There's no guarantee that this will work unless the left mouse button was pressed
    /// immediately before this function is called.
    ///
    /// ## Platform-specific
    ///
    /// - **X11:** Un-grabs the cursor.
    /// - **Wayland:** Requires the cursor to be inside the window to be dragged.
    /// - **macOS:** May prevent the button release event to be triggered.
    /// - **iOS / Android / Web:** Always returns an [`ExternalError::NotSupported`].
    #[inline]
    pub fn drag_window(&self) -> Result<(), ExternalError> {
        let _span = tracing::debug_span!("rio_window::Window::drag_window",).entered();
        self.window.maybe_wait_on_main(|w| w.drag_window())
    }

    /// Resizes the window with the left mouse button until the button is released.
    ///
    /// There's no guarantee that this will work unless the left mouse button was pressed
    /// immediately before this function is called.
    ///
    /// ## Platform-specific
    ///
    /// - **macOS:** Always returns an [`ExternalError::NotSupported`]
    /// - **iOS / Android / Web:** Always returns an [`ExternalError::NotSupported`].
    #[inline]
    pub fn drag_resize_window(
        &self,
        direction: ResizeDirection,
    ) -> Result<(), ExternalError> {
        let _span = tracing::debug_span!(
            "rio_window::Window::drag_resize_window",
            direction = ?direction
        )
        .entered();
        self.window
            .maybe_wait_on_main(|w| w.drag_resize_window(direction))
    }

    /// Show [window menu] at a specified position .
    ///
    /// This is the context menu that is normally shown when interacting with
    /// the title bar. This is useful when implementing custom decorations.
    ///
    /// ## Platform-specific
    /// **Android / iOS / macOS / Orbital / Wayland / Web / X11:** Unsupported.
    ///
    /// [window menu]: https://en.wikipedia.org/wiki/Common_menus_in_Microsoft_Windows#System_menu
    pub fn show_window_menu(&self, position: impl Into<Position>) {
        let position = position.into();
        let _span = tracing::debug_span!(
            "rio_window::Window::show_window_menu",
            position = ?position
        )
        .entered();
        self.window
            .maybe_queue_on_main(move |w| w.show_window_menu(position))
    }

    /// Modifies whether the window catches cursor events.
    ///
    /// If `true`, the window will catch the cursor events. If `false`, events are passed through
    /// the window such that any other window behind it receives them. By default hittest is
    /// enabled.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / Web / Orbital:** Always returns an [`ExternalError::NotSupported`].
    #[inline]
    pub fn set_cursor_hittest(&self, hittest: bool) -> Result<(), ExternalError> {
        let _span =
            tracing::debug_span!("rio_window::Window::set_cursor_hittest", hittest)
                .entered();
        self.window
            .maybe_wait_on_main(|w| w.set_cursor_hittest(hittest))
    }
}

/// Monitor info functions.
impl Window {
    /// Returns the monitor on which the window currently resides.
    ///
    /// Returns `None` if current monitor can't be detected.
    #[inline]
    pub fn current_monitor(&self) -> Option<MonitorHandle> {
        let _span =
            tracing::debug_span!("rio_window::Window::current_monitor",).entered();
        self.window.maybe_wait_on_main(|w| {
            w.current_monitor().map(|inner| MonitorHandle { inner })
        })
    }

    /// Returns the list of all the monitors available on the system.
    ///
    /// This is the same as [`ActiveEventLoop::available_monitors`], and is provided for
    /// convenience.
    ///
    /// [`ActiveEventLoop::available_monitors`]: crate::event_loop::ActiveEventLoop::available_monitors
    #[inline]
    pub fn available_monitors(&self) -> impl Iterator<Item = MonitorHandle> {
        let _span =
            tracing::debug_span!("rio_window::Window::available_monitors",).entered();
        self.window.maybe_wait_on_main(|w| {
            w.available_monitors()
                .into_iter()
                .map(|inner| MonitorHandle { inner })
        })
    }

    /// Returns the primary monitor of the system.
    ///
    /// Returns `None` if it can't identify any monitor as a primary one.
    ///
    /// This is the same as [`ActiveEventLoop::primary_monitor`], and is provided for convenience.
    ///
    /// ## Platform-specific
    ///
    /// **Wayland / Web:** Always returns `None`.
    ///
    /// [`ActiveEventLoop::primary_monitor`]: crate::event_loop::ActiveEventLoop::primary_monitor
    #[inline]
    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        let _span =
            tracing::debug_span!("rio_window::Window::primary_monitor",).entered();
        self.window.maybe_wait_on_main(|w| {
            w.primary_monitor().map(|inner| MonitorHandle { inner })
        })
    }
}

impl raw_window_handle::HasWindowHandle for Window {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let raw = self.window.raw_window_handle_raw_window_handle()?;

        // SAFETY: The window handle will never be deallocated while the window is alive,
        // and the main thread safety requirements are upheld internally by each platform.
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(raw) })
    }
}

impl raw_window_handle::HasDisplayHandle for Window {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError>
    {
        let raw = self.window.raw_display_handle_raw_window_handle()?;

        // SAFETY: The window handle will never be deallocated while the window is alive,
        // and the main thread safety requirements are upheld internally by each platform.
        Ok(unsafe { raw_window_handle::DisplayHandle::borrow_raw(raw) })
    }
}

/// The behavior of cursor grabbing.
///
/// Use this enum with [`Window::set_cursor_grab`] to grab the cursor.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum CursorGrabMode {
    /// No grabbing of the cursor is performed.
    None,

    /// The cursor is confined to the window area.
    ///
    /// There's no guarantee that the cursor will be hidden. You should hide it by yourself if you
    /// want to do so.
    ///
    /// ## Platform-specific
    ///
    /// - **macOS:** Not implemented. Always returns [`ExternalError::NotSupported`] for now.
    /// - **iOS / Android / Web:** Always returns an [`ExternalError::NotSupported`].
    Confined,

    /// The cursor is locked inside the window area to the certain position.
    ///
    /// There's no guarantee that the cursor will be hidden. You should hide it by yourself if you
    /// want to do so.
    ///
    /// ## Platform-specific
    ///
    /// - **X11 / Windows:** Not implemented. Always returns [`ExternalError::NotSupported`] for
    ///   now.
    /// - **iOS / Android:** Always returns an [`ExternalError::NotSupported`].
    Locked,
}

/// Defines the orientation that a window resize will be performed.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ResizeDirection {
    East,
    North,
    NorthEast,
    NorthWest,
    South,
    SouthEast,
    SouthWest,
    West,
}

impl From<ResizeDirection> for CursorIcon {
    fn from(direction: ResizeDirection) -> Self {
        use ResizeDirection::*;
        match direction {
            East => CursorIcon::EResize,
            North => CursorIcon::NResize,
            NorthEast => CursorIcon::NeResize,
            NorthWest => CursorIcon::NwResize,
            South => CursorIcon::SResize,
            SouthEast => CursorIcon::SeResize,
            SouthWest => CursorIcon::SwResize,
            West => CursorIcon::WResize,
        }
    }
}

/// Fullscreen modes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fullscreen {
    Exclusive(VideoModeHandle),

    /// Providing `None` to `Borderless` will fullscreen on the current monitor.
    Borderless(Option<MonitorHandle>),
}

/// The theme variant to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    /// Use the light variant.
    Light,

    /// Use the dark variant.
    Dark,
}

/// ## Platform-specific
///
/// - **X11:** Sets the WM's `XUrgencyHint`. No distinction between [`Critical`] and
///   [`Informational`].
///
/// [`Critical`]: Self::Critical
/// [`Informational`]: Self::Informational
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UserAttentionType {
    /// ## Platform-specific
    ///
    /// - **macOS:** Bounces the dock icon until the application is in focus.
    /// - **Windows:** Flashes both the window and the taskbar button until the application is in
    ///   focus.
    Critical,

    /// ## Platform-specific
    ///
    /// - **macOS:** Bounces the dock icon once.
    /// - **Windows:** Flashes the taskbar button until the application is in focus.
    #[default]
    Informational,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct WindowButtons: u32 {
        const CLOSE  = 1 << 0;
        const MINIMIZE  = 1 << 1;
        const MAXIMIZE  = 1 << 2;
    }
}

/// A window level groups windows with respect to their z-position.
///
/// The relative ordering between windows in different window levels is fixed.
/// The z-order of a window within the same window level may change dynamically on user interaction.
///
/// ## Platform-specific
///
/// - **iOS / Android / Web / Wayland:** Unsupported.
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub enum WindowLevel {
    /// The window will always be below normal windows.
    ///
    /// This is useful for a widget-based app.
    AlwaysOnBottom,

    /// The default.
    #[default]
    Normal,

    /// The window will always be on top of normal windows.
    AlwaysOnTop,
}

/// Generic IME purposes for use in [`Window::set_ime_purpose`].
///
/// The purpose may improve UX by optimizing the IME for the specific use case,
/// if winit can express the purpose to the platform and the platform reacts accordingly.
///
/// ## Platform-specific
///
/// - **iOS / Android / Web / Windows / X11 / macOS / Orbital:** Unsupported.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
pub enum ImePurpose {
    /// No special hints for the IME (default).
    Normal,
    /// The IME is used for password input.
    Password,
    /// The IME is used to input into a terminal.
    ///
    /// For example, that could alter OSK on Wayland to show extra buttons.
    Terminal,
}

impl Default for ImePurpose {
    fn default() -> Self {
        Self::Normal
    }
}

/// An opaque token used to activate the [`Window`].
///
/// [`Window`]: crate::window::Window
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ActivationToken {
    pub(crate) _token: String,
}

impl ActivationToken {
    pub(crate) fn _new(_token: String) -> Self {
        Self { _token }
    }
}
