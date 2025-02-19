//! # Windows
//!
//! The supported OS version is Windows 7 or higher, though Windows 10 is
//! tested regularly.
use std::borrow::Borrow;
use std::ffi::c_void;
use std::path::Path;

use crate::dpi::PhysicalSize;
use crate::event::DeviceId;
use crate::event_loop::EventLoopBuilder;
use crate::monitor::MonitorHandle;
use crate::window::{BadIcon, Icon, Window, WindowAttributes};

/// Window Handle type used by Win32 API
pub type HWND = *mut c_void;
/// Menu Handle type used by Win32 API
pub type HMENU = *mut c_void;
/// Monitor Handle type used by Win32 API
pub type HMONITOR = *mut c_void;

/// Describes a system-drawn backdrop material of a window.
///
/// For a detailed explanation, see [`DWM_SYSTEMBACKDROP_TYPE docs`].
///
/// [`DWM_SYSTEMBACKDROP_TYPE docs`]: https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwm_systembackdrop_type
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackdropType {
    /// Corresponds to `DWMSBT_AUTO`.
    ///
    /// Usually draws a default backdrop effect on the title bar.
    #[default]
    Auto = 0,

    /// Corresponds to `DWMSBT_NONE`.
    None = 1,

    /// Corresponds to `DWMSBT_MAINWINDOW`.
    ///
    /// Draws the Mica backdrop material.
    MainWindow = 2,

    /// Corresponds to `DWMSBT_TRANSIENTWINDOW`.
    ///
    /// Draws the Background Acrylic backdrop material.
    TransientWindow = 3,

    /// Corresponds to `DWMSBT_TABBEDWINDOW`.
    ///
    /// Draws the Alt Mica backdrop material.
    TabbedWindow = 4,
}

/// Describes a color used by Windows
#[repr(transparent)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct Color(u32);

impl Color {
    // Special constant only valid for the window border and therefore modeled using Option<Color>
    // for user facing code
    const NONE: Color = Color(0xfffffffe);
    /// Use the system's default color
    pub const SYSTEM_DEFAULT: Color = Color(0xffffffff);

    /// Create a new color from the given RGB values
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self((r as u32) | ((g as u32) << 8) | ((b as u32) << 16))
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::SYSTEM_DEFAULT
    }
}

/// Describes how the corners of a window should look like.
///
/// For a detailed explanation, see [`DWM_WINDOW_CORNER_PREFERENCE docs`].
///
/// [`DWM_WINDOW_CORNER_PREFERENCE docs`]: https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwm_window_corner_preference
#[repr(i32)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CornerPreference {
    /// Corresponds to `DWMWCP_DEFAULT`.
    ///
    /// Let the system decide when to round window corners.
    #[default]
    Default = 0,

    /// Corresponds to `DWMWCP_DONOTROUND`.
    ///
    /// Never round window corners.
    DoNotRound = 1,

    /// Corresponds to `DWMWCP_ROUND`.
    ///
    /// Round the corners, if appropriate.
    Round = 2,

    /// Corresponds to `DWMWCP_ROUNDSMALL`.
    ///
    /// Round the corners if appropriate, with a small radius.
    RoundSmall = 3,
}

/// A wrapper around a [`Window`] that ignores thread-specific window handle limitations.
///
/// See [`WindowBorrowExtWindows::any_thread`] for more information.
#[derive(Debug)]
pub struct AnyThread<W>(W);

impl<W: Borrow<Window>> AnyThread<W> {
    /// Get a reference to the inner window.
    #[inline]
    pub fn get_ref(&self) -> &Window {
        self.0.borrow()
    }

    /// Get a reference to the inner object.
    #[inline]
    pub fn inner(&self) -> &W {
        &self.0
    }

    /// Unwrap and get the inner window.
    #[inline]
    pub fn into_inner(self) -> W {
        self.0
    }
}

impl<W: Borrow<Window>> AsRef<Window> for AnyThread<W> {
    fn as_ref(&self) -> &Window {
        self.get_ref()
    }
}

impl<W: Borrow<Window>> Borrow<Window> for AnyThread<W> {
    fn borrow(&self) -> &Window {
        self.get_ref()
    }
}

impl<W: Borrow<Window>> std::ops::Deref for AnyThread<W> {
    type Target = Window;

    fn deref(&self) -> &Self::Target {
        self.get_ref()
    }
}

impl<W: Borrow<Window>> raw_window_handle::HasWindowHandle for AnyThread<W> {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        // SAFETY: The top level user has asserted this is only used safely.
        unsafe { self.get_ref().window_handle_any_thread() }
    }
}

/// Additional methods on `EventLoop` that are specific to Windows.
pub trait EventLoopBuilderExtWindows {
    /// Whether to allow the event loop to be created off of the main thread.
    ///
    /// By default, the window is only allowed to be created on the main
    /// thread, to make platform compatibility easier.
    ///
    /// # `Window` caveats
    ///
    /// Note that any `Window` created on the new thread will be destroyed when the thread
    /// terminates. Attempting to use a `Window` after its parent thread terminates has
    /// unspecified, although explicitly not undefined, behavior.
    fn with_any_thread(&mut self, any_thread: bool) -> &mut Self;

    /// Whether to enable process-wide DPI awareness.
    ///
    /// By default, `winit` will attempt to enable process-wide DPI awareness. If
    /// that's undesirable, you can disable it with this function.
    ///
    /// # Example
    ///
    /// Disable process-wide DPI awareness.
    ///
    /// ```
    /// use rio_window::event_loop::EventLoopBuilder;
    /// #[cfg(target_os = "windows")]
    /// use rio_window::platform::windows::EventLoopBuilderExtWindows;
    ///
    /// let mut builder = EventLoopBuilder::new();
    /// #[cfg(target_os = "windows")]
    /// builder.with_dpi_aware(false);
    /// # if false { // We can't test this part
    /// let event_loop = builder.build();
    /// # }
    /// ```
    fn with_dpi_aware(&mut self, dpi_aware: bool) -> &mut Self;

    /// A callback to be executed before dispatching a win32 message to the window procedure.
    /// Return true to disable winit's internal message dispatching.
    ///
    /// # Example
    ///
    /// ```
    /// # use windows_sys::Win32::UI::WindowsAndMessaging::{ACCEL, CreateAcceleratorTableW, TranslateAcceleratorW, DispatchMessageW, TranslateMessage, MSG};
    /// use rio_window::event_loop::EventLoopBuilder;
    /// #[cfg(target_os = "windows")]
    /// use rio_window::platform::windows::EventLoopBuilderExtWindows;
    ///
    /// let mut builder = EventLoopBuilder::new();
    /// #[cfg(target_os = "windows")]
    /// builder.with_msg_hook(|msg|{
    ///     let msg = msg as *const MSG;
    /// #   let accels: Vec<ACCEL> = Vec::new();
    ///     let translated = unsafe {
    ///         TranslateAcceleratorW(
    ///             (*msg).hwnd,
    ///             CreateAcceleratorTableW(accels.as_ptr() as _, 1),
    ///             msg,
    ///         ) == 1
    ///     };
    ///     translated
    /// });
    /// ```
    fn with_msg_hook<F>(&mut self, callback: F) -> &mut Self
    where
        F: FnMut(*const c_void) -> bool + 'static;
}

impl<T> EventLoopBuilderExtWindows for EventLoopBuilder<T> {
    #[inline]
    fn with_any_thread(&mut self, any_thread: bool) -> &mut Self {
        self.platform_specific.any_thread = any_thread;
        self
    }

    #[inline]
    fn with_dpi_aware(&mut self, dpi_aware: bool) -> &mut Self {
        self.platform_specific.dpi_aware = dpi_aware;
        self
    }

    #[inline]
    fn with_msg_hook<F>(&mut self, callback: F) -> &mut Self
    where
        F: FnMut(*const c_void) -> bool + 'static,
    {
        self.platform_specific.msg_hook = Some(Box::new(callback));
        self
    }
}

/// Additional methods on `Window` that are specific to Windows.
pub trait WindowExtWindows {
    /// Enables or disables mouse and keyboard input to the specified window.
    ///
    /// A window must be enabled before it can be activated.
    /// If an application has create a modal dialog box by disabling its owner window
    /// (as described in [`WindowAttributesExtWindows::with_owner_window`]), the application must
    /// enable the owner window before destroying the dialog box.
    /// Otherwise, another window will receive the keyboard focus and be activated.
    ///
    /// If a child window is disabled, it is ignored when the system tries to determine which
    /// window should receive mouse messages.
    ///
    /// For more information, see <https://docs.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enablewindow#remarks>
    /// and <https://docs.microsoft.com/en-us/windows/win32/winmsg/window-features#disabled-windows>
    fn set_enable(&self, enabled: bool);

    /// This sets `ICON_BIG`. A good ceiling here is 256x256.
    fn set_taskbar_icon(&self, taskbar_icon: Option<Icon>);

    /// Whether to show or hide the window icon in the taskbar.
    fn set_skip_taskbar(&self, skip: bool);

    /// Shows or hides the background drop shadow for undecorated windows.
    ///
    /// Enabling the shadow causes a thin 1px line to appear on the top of the window.
    fn set_undecorated_shadow(&self, shadow: bool);

    /// Sets system-drawn backdrop type.
    ///
    /// Requires Windows 11 build 22523+.
    fn set_system_backdrop(&self, backdrop_type: BackdropType);

    /// Sets the color of the window border.
    ///
    /// Supported starting with Windows 11 Build 22000.
    fn set_border_color(&self, color: Option<Color>);

    /// Sets the background color of the title bar.
    ///
    /// Supported starting with Windows 11 Build 22000.
    fn set_title_background_color(&self, color: Option<Color>);

    /// Sets the color of the window title.
    ///
    /// Supported starting with Windows 11 Build 22000.
    fn set_title_text_color(&self, color: Color);

    /// Sets the preferred style of the window corners.
    ///
    /// Supported starting with Windows 11 Build 22000.
    fn set_corner_preference(&self, preference: CornerPreference);

    /// Cloaks the window such that it is not visible to the user. The window is still composed by DWM.
    ///
    /// A cloaked window will not appear on Taskbar. Cloaking is particularly useful when you want a
    /// window to complete its UI layout, sizing, etc. without flickering in front of the user.
    /// Not supported on Windows 7 and earlier.
    fn set_cloaked(&self, cloaked: bool);

    /// Get the raw window handle for this [`Window`] without checking for thread affinity.
    ///
    /// Window handles in Win32 have a property called "thread affinity" that ties them to their
    /// origin thread. Some operations can only happen on the window's origin thread, while others
    /// can be called from any thread. For example, [`SetWindowSubclass`] is not thread safe while
    /// [`GetDC`] is thread safe.
    ///
    /// In Rust terms, the window handle is `Send` sometimes but `!Send` other times.
    ///
    /// Therefore, in order to avoid confusing threading errors, [`Window`] only returns the
    /// window handle when the [`window_handle`] function is called from the thread that created
    /// the window. In other cases, it returns an [`Unavailable`] error.
    ///
    /// However in some cases you may already know that you are using the window handle for
    /// operations that are guaranteed to be thread-safe. In which case this function aims
    /// to provide an escape hatch so these functions are still accessible from other threads.
    ///
    /// # Safety
    ///
    /// It is the responsibility of the user to only pass the window handle into thread-safe
    /// Win32 APIs.
    ///
    /// [`SetWindowSubclass`]: https://learn.microsoft.com/en-us/windows/win32/api/commctrl/nf-commctrl-setwindowsubclass
    /// [`GetDC`]: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getdc
    /// [`Window`]: crate::window::Window
    /// [`window_handle`]: https://docs.rs/raw-window-handle/latest/raw_window_handle/trait.HasWindowHandle.html#tymethod.window_handle
    /// [`Unavailable`]: https://docs.rs/raw-window-handle/latest/raw_window_handle/enum.HandleError.html#variant.Unavailable
    ///
    /// ## Example
    ///
    /// ```no_run
    /// # use rio_window::window::Window;
    /// # fn scope(window: Window) {
    /// use std::thread;
    /// use rio_window::platform::windows::WindowExtWindows;
    /// use rio_window::raw_window_handle::HasWindowHandle;
    ///
    /// // We can get the window handle on the current thread.
    /// let handle = window.window_handle().unwrap();
    ///
    /// // However, on another thread, we can't!
    /// thread::spawn(move || {
    ///     assert!(window.window_handle().is_err());
    ///
    ///     // We can use this function as an escape hatch.
    ///     let handle = unsafe { window.window_handle_any_thread().unwrap() };
    /// });
    /// # }
    /// ```
    unsafe fn window_handle_any_thread(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError>;
}

impl WindowExtWindows for Window {
    #[inline]
    fn set_enable(&self, enabled: bool) {
        self.window.set_enable(enabled)
    }

    #[inline]
    fn set_taskbar_icon(&self, taskbar_icon: Option<Icon>) {
        self.window.set_taskbar_icon(taskbar_icon)
    }

    #[inline]
    fn set_skip_taskbar(&self, skip: bool) {
        self.window.set_skip_taskbar(skip)
    }

    #[inline]
    fn set_undecorated_shadow(&self, shadow: bool) {
        self.window.set_undecorated_shadow(shadow)
    }

    #[inline]
    fn set_system_backdrop(&self, backdrop_type: BackdropType) {
        self.window.set_system_backdrop(backdrop_type)
    }

    #[inline]
    fn set_border_color(&self, color: Option<Color>) {
        self.window.set_border_color(color.unwrap_or(Color::NONE))
    }

    #[inline]
    fn set_title_background_color(&self, color: Option<Color>) {
        // The windows docs don't mention NONE as a valid options but it works in practice and is
        // useful to circumvent the Windows option "Show accent color on title bars and
        // window borders"
        self.window
            .set_title_background_color(color.unwrap_or(Color::NONE))
    }

    #[inline]
    fn set_title_text_color(&self, color: Color) {
        self.window.set_title_text_color(color)
    }

    #[inline]
    fn set_corner_preference(&self, preference: CornerPreference) {
        self.window.set_corner_preference(preference)
    }

    #[inline]
    fn set_cloaked(&self, cloaked: bool) {
        self.window.set_cloaked(cloaked)
    }

    unsafe fn window_handle_any_thread(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        unsafe {
            let handle = self.window.rwh_06_no_thread_check()?;

            // SAFETY: The handle is valid in this context.
            Ok(raw_window_handle::WindowHandle::borrow_raw(handle))
        }
    }
}

/// Additional methods for anything that dereference to [`Window`].
///
/// [`Window`]: crate::window::Window
pub trait WindowBorrowExtWindows: Borrow<Window> + Sized {
    /// Create an object that allows accessing the inner window handle in a thread-unsafe way.
    ///
    /// It is possible to call [`window_handle_any_thread`] to get around Windows's thread
    /// affinity limitations. However, it may be desired to pass the [`Window`] into something
    /// that requires the [`HasWindowHandle`] trait, while ignoring thread affinity limitations.
    ///
    /// This function wraps anything that implements `Borrow<Window>` into a structure that
    /// uses the inner window handle as a mean of implementing [`HasWindowHandle`]. It wraps
    /// `Window`, `&Window`, `Arc<Window>`, and other reference types.
    ///
    /// # Safety
    ///
    /// It is the responsibility of the user to only pass the window handle into thread-safe
    /// Win32 APIs.
    ///
    /// [`Window`]: crate::window::Window
    unsafe fn any_thread(self) -> AnyThread<Self> {
        AnyThread(self)
    }
}

impl<W: Borrow<Window> + Sized> WindowBorrowExtWindows for W {}

/// Additional methods on `WindowAttributes` that are specific to Windows.
#[allow(rustdoc::broken_intra_doc_links)]
pub trait WindowAttributesExtWindows {
    /// Set an owner to the window to be created. Can be used to create a dialog box, for example.
    /// This only works when [`WindowAttributes::with_parent_window`] isn't called or set to `None`.
    /// Can be used in combination with
    /// [`WindowExtWindows::set_enable(false)`][WindowExtWindows::set_enable] on the owner
    /// window to create a modal dialog box.
    ///
    /// From MSDN:
    /// - An owned window is always above its owner in the z-order.
    /// - The system automatically destroys an owned window when its owner is destroyed.
    /// - An owned window is hidden when its owner is minimized.
    ///
    /// For more information, see <https://docs.microsoft.com/en-us/windows/win32/winmsg/window-features#owned-windows>
    fn with_owner_window(self, parent: HWND) -> Self;

    /// Sets a menu on the window to be created.
    ///
    /// Parent and menu are mutually exclusive; a child window cannot have a menu!
    ///
    /// The menu must have been manually created beforehand with [`CreateMenu`] or similar.
    ///
    /// Note: Dark mode cannot be supported for win32 menus, it's simply not possible to change how
    /// the menus look. If you use this, it is recommended that you combine it with
    /// `with_theme(Some(Theme::Light))` to avoid a jarring effect.
    #[cfg_attr(
        windows_platform,
        doc = "[`CreateMenu`]: windows_sys::Win32::UI::WindowsAndMessaging::CreateMenu"
    )]
    #[cfg_attr(
        not(windows_platform),
        doc = "[`CreateMenu`]: #only-available-on-windows"
    )]
    fn with_menu(self, menu: HMENU) -> Self;

    /// This sets `ICON_BIG`. A good ceiling here is 256x256.
    fn with_taskbar_icon(self, taskbar_icon: Option<Icon>) -> Self;

    /// This sets `WS_EX_NOREDIRECTIONBITMAP`.
    fn with_no_redirection_bitmap(self, flag: bool) -> Self;

    /// Enables or disables drag and drop support (enabled by default). Will interfere with other
    /// crates that use multi-threaded COM API (`CoInitializeEx` with `COINIT_MULTITHREADED`
    /// instead of `COINIT_APARTMENTTHREADED`) on the same thread. Note that winit may still
    /// attempt to initialize COM API regardless of this option. Currently only fullscreen mode
    /// does that, but there may be more in the future. If you need COM API with
    /// `COINIT_MULTITHREADED` you must initialize it before calling any winit functions. See <https://docs.microsoft.com/en-us/windows/win32/api/objbase/nf-objbase-coinitialize#remarks> for more information.
    fn with_drag_and_drop(self, flag: bool) -> Self;

    /// Whether show or hide the window icon in the taskbar.
    fn with_skip_taskbar(self, skip: bool) -> Self;

    /// Customize the window class name.
    fn with_class_name<S: Into<String>>(self, class_name: S) -> Self;

    /// Shows or hides the background drop shadow for undecorated windows.
    ///
    /// The shadow is hidden by default.
    /// Enabling the shadow causes a thin 1px line to appear on the top of the window.
    fn with_undecorated_shadow(self, shadow: bool) -> Self;

    /// Sets system-drawn backdrop type.
    ///
    /// Requires Windows 11 build 22523+.
    fn with_system_backdrop(self, backdrop_type: BackdropType) -> Self;

    /// This sets or removes `WS_CLIPCHILDREN` style.
    fn with_clip_children(self, flag: bool) -> Self;

    /// Sets the color of the window border.
    ///
    /// Supported starting with Windows 11 Build 22000.
    fn with_border_color(self, color: Option<Color>) -> Self;

    /// Sets the background color of the title bar.
    ///
    /// Supported starting with Windows 11 Build 22000.
    fn with_title_background_color(self, color: Option<Color>) -> Self;

    /// Sets the color of the window title.
    ///
    /// Supported starting with Windows 11 Build 22000.
    fn with_title_text_color(self, color: Color) -> Self;

    /// Sets the preferred style of the window corners.
    ///
    /// Supported starting with Windows 11 Build 22000.
    fn with_corner_preference(self, corners: CornerPreference) -> Self;

    /// Cloaks the window such that it is not visible to the user. The window is still composed by DWM.
    ///
    /// Not supported on Windows 7 and earlier.
    fn with_cloaked(self, cloaked: bool) -> Self;
}

impl WindowAttributesExtWindows for WindowAttributes {
    #[inline]
    fn with_owner_window(mut self, parent: HWND) -> Self {
        self.platform_specific.owner = Some(parent);
        self
    }

    #[inline]
    fn with_menu(mut self, menu: HMENU) -> Self {
        self.platform_specific.menu = Some(menu);
        self
    }

    #[inline]
    fn with_taskbar_icon(mut self, taskbar_icon: Option<Icon>) -> Self {
        self.platform_specific.taskbar_icon = taskbar_icon;
        self
    }

    #[inline]
    fn with_no_redirection_bitmap(mut self, flag: bool) -> Self {
        self.platform_specific.no_redirection_bitmap = flag;
        self
    }

    #[inline]
    fn with_drag_and_drop(mut self, flag: bool) -> Self {
        self.platform_specific.drag_and_drop = flag;
        self
    }

    #[inline]
    fn with_skip_taskbar(mut self, skip: bool) -> Self {
        self.platform_specific.skip_taskbar = skip;
        self
    }

    #[inline]
    fn with_class_name<S: Into<String>>(mut self, class_name: S) -> Self {
        self.platform_specific.class_name = class_name.into();
        self
    }

    #[inline]
    fn with_undecorated_shadow(mut self, shadow: bool) -> Self {
        self.platform_specific.decoration_shadow = shadow;
        self
    }

    #[inline]
    fn with_system_backdrop(mut self, backdrop_type: BackdropType) -> Self {
        self.platform_specific.backdrop_type = backdrop_type;
        self
    }

    #[inline]
    fn with_clip_children(mut self, flag: bool) -> Self {
        self.platform_specific.clip_children = flag;
        self
    }

    #[inline]
    fn with_border_color(mut self, color: Option<Color>) -> Self {
        self.platform_specific.border_color = Some(color.unwrap_or(Color::NONE));
        self
    }

    #[inline]
    fn with_title_background_color(mut self, color: Option<Color>) -> Self {
        self.platform_specific.title_background_color =
            Some(color.unwrap_or(Color::NONE));
        self
    }

    #[inline]
    fn with_title_text_color(mut self, color: Color) -> Self {
        self.platform_specific.title_text_color = Some(color);
        self
    }

    #[inline]
    fn with_corner_preference(mut self, corners: CornerPreference) -> Self {
        self.platform_specific.corner_preference = Some(corners);
        self
    }

    #[inline]
    fn with_cloaked(mut self, cloaked: bool) -> Self {
        self.platform_specific.cloaked = cloaked;
        self
    }
}

/// Additional methods on `MonitorHandle` that are specific to Windows.
pub trait MonitorHandleExtWindows {
    /// Returns the name of the monitor adapter specific to the Win32 API.
    fn native_id(&self) -> String;

    /// Returns the handle of the monitor - `HMONITOR`.
    fn hmonitor(&self) -> HMONITOR;
}

impl MonitorHandleExtWindows for MonitorHandle {
    #[inline]
    fn native_id(&self) -> String {
        self.inner.native_identifier()
    }

    #[inline]
    fn hmonitor(&self) -> HMONITOR {
        self.inner.hmonitor()
    }
}

/// Additional methods on `DeviceId` that are specific to Windows.
pub trait DeviceIdExtWindows {
    /// Returns an identifier that persistently refers to this specific device.
    ///
    /// Will return `None` if the device is no longer available.
    fn persistent_identifier(&self) -> Option<String>;
}

impl DeviceIdExtWindows for DeviceId {
    #[inline]
    fn persistent_identifier(&self) -> Option<String> {
        self.0.persistent_identifier()
    }
}

/// Additional methods on `Icon` that are specific to Windows.
pub trait IconExtWindows: Sized {
    /// Create an icon from a file path.
    ///
    /// Specify `size` to load a specific icon size from the file, or `None` to load the default
    /// icon size from the file.
    ///
    /// In cases where the specified size does not exist in the file, Windows may perform scaling
    /// to get an icon of the desired size.
    fn from_path<P: AsRef<Path>>(
        path: P,
        size: Option<PhysicalSize<u32>>,
    ) -> Result<Self, BadIcon>;

    /// Create an icon from a resource embedded in this executable or library.
    ///
    /// Specify `size` to load a specific icon size from the file, or `None` to load the default
    /// icon size from the file.
    ///
    /// In cases where the specified size does not exist in the file, Windows may perform scaling
    /// to get an icon of the desired size.
    fn from_resource(
        ordinal: u16,
        size: Option<PhysicalSize<u32>>,
    ) -> Result<Self, BadIcon>;
}

impl IconExtWindows for Icon {
    fn from_path<P: AsRef<Path>>(
        path: P,
        size: Option<PhysicalSize<u32>>,
    ) -> Result<Self, BadIcon> {
        let win_icon = crate::platform_impl::WinIcon::from_path(path, size)?;
        Ok(Icon { inner: win_icon })
    }

    fn from_resource(
        ordinal: u16,
        size: Option<PhysicalSize<u32>>,
    ) -> Result<Self, BadIcon> {
        let win_icon = crate::platform_impl::WinIcon::from_resource(ordinal, size)?;
        Ok(Icon { inner: win_icon })
    }
}
