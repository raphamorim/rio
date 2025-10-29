#![cfg(free_unix)]

#[cfg(all(not(x11_platform), not(wayland_platform)))]
compile_error!("Please select a feature to build for unix: `x11`, `wayland`");

use std::collections::VecDeque;
use std::os::unix::io::{AsFd, AsRawFd, BorrowedFd, RawFd};
use std::sync::Arc;
use std::time::Duration;
use std::{env, fmt};
#[cfg(x11_platform)]
use std::{ffi::CStr, mem::MaybeUninit, os::raw::*, sync::Mutex};

#[cfg(x11_platform)]
use crate::utils::Lazy;
use smol_str::SmolStr;

#[cfg(x11_platform)]
use self::x11::{X11Error, XConnection, XError, XNotSupported};
use crate::dpi::{PhysicalPosition, PhysicalSize, Position, Size};
use crate::error::{
    EventLoopError, ExternalError, NotSupportedError, OsError as RootOsError,
};
use crate::event_loop::{
    ActiveEventLoop as RootELW, AsyncRequestSerial, ControlFlow, DeviceEvents,
    EventLoopClosed,
};
use crate::icon::Icon;
use crate::keyboard::Key;
use crate::platform::pump_events::PumpStatus;
#[cfg(x11_platform)]
use crate::platform::x11::{WindowType as XWindowType, XlibErrorHook};
use crate::window::{
    ActivationToken, Cursor, CursorGrabMode, CustomCursor, CustomCursorSource,
    ImePurpose, ResizeDirection, Theme, UserAttentionType, WindowAttributes,
    WindowButtons, WindowLevel,
};

pub(crate) use self::common::xkb::{physicalkey_to_scancode, scancode_to_physicalkey};
pub(crate) use crate::cursor::OnlyCursorImageSource as PlatformCustomCursorSource;
pub(crate) use crate::icon::RgbaIcon as PlatformIcon;
pub(crate) use crate::platform_impl::Fullscreen;

pub(crate) mod common;
#[cfg(wayland_platform)]
pub(crate) mod wayland;
#[cfg(x11_platform)]
pub(crate) mod x11;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Backend {
    #[cfg(x11_platform)]
    X,
    #[cfg(wayland_platform)]
    Wayland,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PlatformSpecificEventLoopAttributes {
    pub(crate) forced_backend: Option<Backend>,
    pub(crate) any_thread: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationName {
    pub general: String,
    pub instance: String,
}

impl ApplicationName {
    pub fn new(general: String, instance: String) -> Self {
        Self { general, instance }
    }
}

#[derive(Clone, Debug)]
pub struct PlatformSpecificWindowAttributes {
    pub name: Option<ApplicationName>,
    pub activation_token: Option<ActivationToken>,
    #[cfg(x11_platform)]
    pub x11: X11WindowAttributes,
}

#[derive(Clone, Debug)]
#[cfg(x11_platform)]
pub struct X11WindowAttributes {
    pub visual_id: Option<x11rb::protocol::xproto::Visualid>,
    pub screen_id: Option<i32>,
    pub base_size: Option<Size>,
    pub override_redirect: bool,
    pub x11_window_types: Vec<XWindowType>,

    /// The parent window to embed this window into.
    pub embed_window: Option<x11rb::protocol::xproto::Window>,
}

#[cfg_attr(not(x11_platform), allow(clippy::derivable_impls))]
impl Default for PlatformSpecificWindowAttributes {
    fn default() -> Self {
        Self {
            name: None,
            activation_token: None,
            #[cfg(x11_platform)]
            x11: X11WindowAttributes {
                visual_id: None,
                screen_id: None,
                base_size: None,
                override_redirect: false,
                x11_window_types: vec![XWindowType::Normal],
                embed_window: None,
            },
        }
    }
}

#[cfg(x11_platform)]
pub(crate) static X11_BACKEND: Lazy<Mutex<Result<Arc<XConnection>, XNotSupported>>> =
    Lazy::new(|| Mutex::new(XConnection::new(Some(x_error_callback)).map(Arc::new)));

#[derive(Debug, Clone)]
pub enum OsError {
    Misc(&'static str),
    #[cfg(x11_platform)]
    XNotSupported(XNotSupported),
    #[cfg(x11_platform)]
    XError(Arc<X11Error>),
    #[cfg(wayland_platform)]
    WaylandError(Arc<wayland::WaylandError>),
}

impl fmt::Display for OsError {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match *self {
            OsError::Misc(e) => _f.pad(e),
            #[cfg(x11_platform)]
            OsError::XNotSupported(ref e) => fmt::Display::fmt(e, _f),
            #[cfg(x11_platform)]
            OsError::XError(ref e) => fmt::Display::fmt(e, _f),
            #[cfg(wayland_platform)]
            OsError::WaylandError(ref e) => fmt::Display::fmt(e, _f),
        }
    }
}

pub(crate) enum Window {
    #[cfg(x11_platform)]
    X(x11::Window),
    #[cfg(wayland_platform)]
    Wayland(wayland::Window),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId(u64);

impl From<WindowId> for u64 {
    fn from(window_id: WindowId) -> Self {
        window_id.0
    }
}

impl From<u64> for WindowId {
    fn from(raw_id: u64) -> Self {
        Self(raw_id)
    }
}

impl WindowId {
    pub const unsafe fn dummy() -> Self {
        Self(0)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeviceId {
    #[cfg(x11_platform)]
    X(x11::DeviceId),
    #[cfg(wayland_platform)]
    Wayland(wayland::DeviceId),
}

impl DeviceId {
    pub const unsafe fn dummy() -> Self {
        #[cfg(wayland_platform)]
        {
            DeviceId::Wayland(unsafe { wayland::DeviceId::dummy() })
        }
        #[cfg(all(not(wayland_platform), x11_platform))]
        {
            DeviceId::X(unsafe { x11::DeviceId::dummy() })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MonitorHandle {
    #[cfg(x11_platform)]
    X(x11::MonitorHandle),
    #[cfg(wayland_platform)]
    Wayland(wayland::MonitorHandle),
}

/// `x11_or_wayland!(match expr; Enum(foo) => foo.something())`
/// expands to the equivalent of
/// ```ignore
/// match self {
///    Enum::X(foo) => foo.something(),
///    Enum::Wayland(foo) => foo.something(),
/// }
/// ```
/// The result can be converted to another enum by adding `; as AnotherEnum`
macro_rules! x11_or_wayland {
    (match $what:expr; $enum:ident ( $($c1:tt)* ) => $x:expr; as $enum2:ident ) => {
        match $what {
            #[cfg(x11_platform)]
            $enum::X($($c1)*) => $enum2::X($x),
            #[cfg(wayland_platform)]
            $enum::Wayland($($c1)*) => $enum2::Wayland($x),
        }
    };
    (match $what:expr; $enum:ident ( $($c1:tt)* ) => $x:expr) => {
        match $what {
            #[cfg(x11_platform)]
            $enum::X($($c1)*) => $x,
            #[cfg(wayland_platform)]
            $enum::Wayland($($c1)*) => $x,
        }
    };
}

impl MonitorHandle {
    #[inline]
    pub fn name(&self) -> Option<String> {
        x11_or_wayland!(match self; MonitorHandle(m) => m.name())
    }

    #[inline]
    pub fn native_identifier(&self) -> u32 {
        x11_or_wayland!(match self; MonitorHandle(m) => m.native_identifier())
    }

    #[inline]
    pub fn size(&self) -> PhysicalSize<u32> {
        x11_or_wayland!(match self; MonitorHandle(m) => m.size())
    }

    #[inline]
    pub fn position(&self) -> PhysicalPosition<i32> {
        x11_or_wayland!(match self; MonitorHandle(m) => m.position())
    }

    #[inline]
    pub fn refresh_rate_millihertz(&self) -> Option<u32> {
        x11_or_wayland!(match self; MonitorHandle(m) => m.refresh_rate_millihertz())
    }

    #[inline]
    pub fn scale_factor(&self) -> f64 {
        x11_or_wayland!(match self; MonitorHandle(m) => m.scale_factor() as _)
    }

    #[inline]
    pub fn video_modes(&self) -> Box<dyn Iterator<Item = VideoModeHandle>> {
        x11_or_wayland!(match self; MonitorHandle(m) => Box::new(m.video_modes()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VideoModeHandle {
    #[cfg(x11_platform)]
    X(x11::VideoModeHandle),
    #[cfg(wayland_platform)]
    Wayland(wayland::VideoModeHandle),
}

impl VideoModeHandle {
    #[inline]
    pub fn size(&self) -> PhysicalSize<u32> {
        x11_or_wayland!(match self; VideoModeHandle(m) => m.size())
    }

    #[inline]
    pub fn bit_depth(&self) -> u16 {
        x11_or_wayland!(match self; VideoModeHandle(m) => m.bit_depth())
    }

    #[inline]
    pub fn refresh_rate_millihertz(&self) -> u32 {
        x11_or_wayland!(match self; VideoModeHandle(m) => m.refresh_rate_millihertz())
    }

    #[inline]
    pub fn monitor(&self) -> MonitorHandle {
        x11_or_wayland!(match self; VideoModeHandle(m) => m.monitor(); as MonitorHandle)
    }
}

impl Window {
    #[inline]
    pub(crate) fn new(
        window_target: &ActiveEventLoop,
        attribs: WindowAttributes,
    ) -> Result<Self, RootOsError> {
        match *window_target {
            #[cfg(wayland_platform)]
            ActiveEventLoop::Wayland(ref window_target) => {
                wayland::Window::new(window_target, attribs).map(Window::Wayland)
            }
            #[cfg(x11_platform)]
            ActiveEventLoop::X(ref window_target) => {
                x11::Window::new(window_target, attribs).map(Window::X)
            }
        }
    }

    pub(crate) fn maybe_queue_on_main(&self, f: impl FnOnce(&Self) + Send + 'static) {
        f(self)
    }

    pub(crate) fn maybe_wait_on_main<R: Send>(
        &self,
        f: impl FnOnce(&Self) -> R + Send,
    ) -> R {
        f(self)
    }

    #[inline]
    pub fn id(&self) -> WindowId {
        x11_or_wayland!(match self; Window(w) => w.id())
    }

    #[inline]
    pub fn set_title(&self, title: &str) {
        x11_or_wayland!(match self; Window(w) => w.set_title(title));
    }

    #[inline]
    pub fn set_transparent(&self, transparent: bool) {
        x11_or_wayland!(match self; Window(w) => w.set_transparent(transparent));
    }

    #[inline]
    pub fn set_blur(&self, blur: bool) {
        x11_or_wayland!(match self; Window(w) => w.set_blur(blur));
    }

    #[inline]
    pub fn set_visible(&self, visible: bool) {
        x11_or_wayland!(match self; Window(w) => w.set_visible(visible))
    }

    #[inline]
    pub fn is_visible(&self) -> Option<bool> {
        x11_or_wayland!(match self; Window(w) => w.is_visible())
    }

    #[inline]
    pub fn outer_position(&self) -> Result<PhysicalPosition<i32>, NotSupportedError> {
        x11_or_wayland!(match self; Window(w) => w.outer_position())
    }

    #[inline]
    pub fn inner_position(&self) -> Result<PhysicalPosition<i32>, NotSupportedError> {
        x11_or_wayland!(match self; Window(w) => w.inner_position())
    }

    #[inline]
    pub fn set_outer_position(&self, position: Position) {
        x11_or_wayland!(match self; Window(w) => w.set_outer_position(position))
    }

    #[inline]
    pub fn inner_size(&self) -> PhysicalSize<u32> {
        x11_or_wayland!(match self; Window(w) => w.inner_size())
    }

    #[inline]
    pub fn outer_size(&self) -> PhysicalSize<u32> {
        x11_or_wayland!(match self; Window(w) => w.outer_size())
    }

    #[inline]
    pub fn request_inner_size(&self, size: Size) -> Option<PhysicalSize<u32>> {
        x11_or_wayland!(match self; Window(w) => w.request_inner_size(size))
    }

    #[inline]
    pub(crate) fn request_activation_token(
        &self,
    ) -> Result<AsyncRequestSerial, NotSupportedError> {
        x11_or_wayland!(match self; Window(w) => w.request_activation_token())
    }

    #[inline]
    pub fn set_min_inner_size(&self, dimensions: Option<Size>) {
        x11_or_wayland!(match self; Window(w) => w.set_min_inner_size(dimensions))
    }

    #[inline]
    pub fn set_max_inner_size(&self, dimensions: Option<Size>) {
        x11_or_wayland!(match self; Window(w) => w.set_max_inner_size(dimensions))
    }

    #[inline]
    pub fn resize_increments(&self) -> Option<PhysicalSize<u32>> {
        x11_or_wayland!(match self; Window(w) => w.resize_increments())
    }

    #[inline]
    pub fn set_resize_increments(&self, increments: Option<Size>) {
        x11_or_wayland!(match self; Window(w) => w.set_resize_increments(increments))
    }

    #[inline]
    pub fn set_resizable(&self, resizable: bool) {
        x11_or_wayland!(match self; Window(w) => w.set_resizable(resizable))
    }

    #[inline]
    pub fn is_resizable(&self) -> bool {
        x11_or_wayland!(match self; Window(w) => w.is_resizable())
    }

    #[inline]
    pub fn set_enabled_buttons(&self, buttons: WindowButtons) {
        x11_or_wayland!(match self; Window(w) => w.set_enabled_buttons(buttons))
    }

    #[inline]
    pub fn enabled_buttons(&self) -> WindowButtons {
        x11_or_wayland!(match self; Window(w) => w.enabled_buttons())
    }

    #[inline]
    pub fn set_cursor(&self, cursor: Cursor) {
        x11_or_wayland!(match self; Window(w) => w.set_cursor(cursor))
    }

    #[inline]
    pub fn set_cursor_grab(&self, mode: CursorGrabMode) -> Result<(), ExternalError> {
        x11_or_wayland!(match self; Window(window) => window.set_cursor_grab(mode))
    }

    #[inline]
    pub fn set_cursor_visible(&self, visible: bool) {
        x11_or_wayland!(match self; Window(window) => window.set_cursor_visible(visible))
    }

    #[inline]
    pub fn drag_window(&self) -> Result<(), ExternalError> {
        x11_or_wayland!(match self; Window(window) => window.drag_window())
    }

    #[inline]
    pub fn drag_resize_window(
        &self,
        direction: ResizeDirection,
    ) -> Result<(), ExternalError> {
        x11_or_wayland!(match self; Window(window) => window.drag_resize_window(direction))
    }

    #[inline]
    pub fn show_window_menu(&self, position: Position) {
        x11_or_wayland!(match self; Window(w) => w.show_window_menu(position))
    }

    #[inline]
    pub fn set_cursor_hittest(&self, hittest: bool) -> Result<(), ExternalError> {
        x11_or_wayland!(match self; Window(w) => w.set_cursor_hittest(hittest))
    }

    #[inline]
    pub fn scale_factor(&self) -> f64 {
        x11_or_wayland!(match self; Window(w) => w.scale_factor())
    }

    #[inline]
    pub fn set_cursor_position(&self, position: Position) -> Result<(), ExternalError> {
        x11_or_wayland!(match self; Window(w) => w.set_cursor_position(position))
    }

    #[inline]
    pub fn set_maximized(&self, maximized: bool) {
        x11_or_wayland!(match self; Window(w) => w.set_maximized(maximized))
    }

    #[inline]
    pub fn is_maximized(&self) -> bool {
        x11_or_wayland!(match self; Window(w) => w.is_maximized())
    }

    #[inline]
    pub fn set_minimized(&self, minimized: bool) {
        x11_or_wayland!(match self; Window(w) => w.set_minimized(minimized))
    }

    #[inline]
    pub fn is_minimized(&self) -> Option<bool> {
        x11_or_wayland!(match self; Window(w) => w.is_minimized())
    }

    #[inline]
    pub(crate) fn fullscreen(&self) -> Option<Fullscreen> {
        x11_or_wayland!(match self; Window(w) => w.fullscreen())
    }

    #[inline]
    pub(crate) fn set_fullscreen(&self, monitor: Option<Fullscreen>) {
        x11_or_wayland!(match self; Window(w) => w.set_fullscreen(monitor))
    }

    #[inline]
    pub fn set_decorations(&self, decorations: bool) {
        x11_or_wayland!(match self; Window(w) => w.set_decorations(decorations))
    }

    #[inline]
    pub fn is_decorated(&self) -> bool {
        x11_or_wayland!(match self; Window(w) => w.is_decorated())
    }

    #[inline]
    pub fn set_window_level(&self, level: WindowLevel) {
        x11_or_wayland!(match self; Window(w) => w.set_window_level(level))
    }

    #[inline]
    pub fn set_window_icon(&self, window_icon: Option<Icon>) {
        x11_or_wayland!(match self; Window(w) => w.set_window_icon(window_icon.map(|icon| icon.inner)))
    }

    #[inline]
    pub fn set_ime_cursor_area(&self, position: Position, size: Size) {
        x11_or_wayland!(match self; Window(w) => w.set_ime_cursor_area(position, size))
    }

    #[inline]
    pub fn reset_dead_keys(&self) {
        common::xkb::reset_dead_keys()
    }

    #[inline]
    pub fn set_ime_allowed(&self, allowed: bool) {
        x11_or_wayland!(match self; Window(w) => w.set_ime_allowed(allowed))
    }

    #[inline]
    pub fn set_ime_purpose(&self, purpose: ImePurpose) {
        x11_or_wayland!(match self; Window(w) => w.set_ime_purpose(purpose))
    }

    #[inline]
    pub fn focus_window(&self) {
        x11_or_wayland!(match self; Window(w) => w.focus_window())
    }

    pub fn request_user_attention(&self, request_type: Option<UserAttentionType>) {
        x11_or_wayland!(match self; Window(w) => w.request_user_attention(request_type))
    }

    #[inline]
    pub fn request_redraw(&self) {
        x11_or_wayland!(match self; Window(w) => w.request_redraw())
    }

    #[inline]
    pub fn pre_present_notify(&self) {
        x11_or_wayland!(match self; Window(w) => w.pre_present_notify())
    }

    #[inline]
    pub fn current_monitor(&self) -> Option<MonitorHandle> {
        Some(
            x11_or_wayland!(match self; Window(w) => w.current_monitor()?; as MonitorHandle),
        )
    }

    #[inline]
    pub fn available_monitors(&self) -> VecDeque<MonitorHandle> {
        match self {
            #[cfg(x11_platform)]
            Window::X(ref window) => window
                .available_monitors()
                .into_iter()
                .map(MonitorHandle::X)
                .collect(),
            #[cfg(wayland_platform)]
            Window::Wayland(ref window) => window
                .available_monitors()
                .into_iter()
                .map(MonitorHandle::Wayland)
                .collect(),
        }
    }

    #[inline]
    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        Some(
            x11_or_wayland!(match self; Window(w) => w.primary_monitor()?; as MonitorHandle),
        )
    }

    #[inline]
    pub fn raw_window_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawWindowHandle, raw_window_handle::HandleError> {
        x11_or_wayland!(match self; Window(window) => window.raw_window_handle_raw_window_handle())
    }

    #[inline]
    pub fn raw_display_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawDisplayHandle, raw_window_handle::HandleError> {
        x11_or_wayland!(match self; Window(window) => window.raw_display_handle_raw_window_handle())
    }

    #[inline]
    pub fn set_theme(&self, theme: Option<Theme>) {
        x11_or_wayland!(match self; Window(window) => window.set_theme(theme))
    }

    #[inline]
    pub fn theme(&self) -> Option<Theme> {
        x11_or_wayland!(match self; Window(window) => window.theme())
    }

    pub fn set_content_protected(&self, protected: bool) {
        x11_or_wayland!(match self; Window(window) => window.set_content_protected(protected))
    }

    #[inline]
    pub fn has_focus(&self) -> bool {
        x11_or_wayland!(match self; Window(window) => window.has_focus())
    }

    pub fn title(&self) -> String {
        x11_or_wayland!(match self; Window(window) => window.title())
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct KeyEventExtra {
    pub text_with_all_modifiers: Option<SmolStr>,
    pub key_without_modifiers: Key,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PlatformCustomCursor {
    #[cfg(wayland_platform)]
    Wayland(wayland::CustomCursor),
    #[cfg(x11_platform)]
    X(x11::CustomCursor),
}

/// Hooks for X11 errors.
#[cfg(x11_platform)]
pub(crate) static XLIB_ERROR_HOOKS: Mutex<Vec<XlibErrorHook>> = Mutex::new(Vec::new());

#[cfg(x11_platform)]
unsafe extern "C" fn x_error_callback(
    display: *mut x11::ffi::Display,
    event: *mut x11::ffi::XErrorEvent,
) -> c_int {
    let xconn_lock = X11_BACKEND.lock().unwrap_or_else(|e| e.into_inner());
    if let Ok(ref xconn) = *xconn_lock {
        // Call all the hooks.
        let mut error_handled = false;
        for hook in XLIB_ERROR_HOOKS.lock().unwrap().iter() {
            error_handled |= hook(display as *mut _, event as *mut _);
        }

        // `assume_init` is safe here because the array consists of `MaybeUninit` values,
        // which do not require initialization.
        let mut buf: [MaybeUninit<c_char>; 1024] =
            unsafe { MaybeUninit::uninit().assume_init() };
        unsafe {
            (xconn.xlib.XGetErrorText)(
                display,
                (*event).error_code as c_int,
                buf.as_mut_ptr() as *mut c_char,
                buf.len() as c_int,
            )
        };
        let description =
            unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char) }.to_string_lossy();

        let error = unsafe {
            XError {
                description: description.into_owned(),
                error_code: (*event).error_code,
                request_code: (*event).request_code,
                minor_code: (*event).minor_code,
            }
        };

        // Don't log error.
        if !error_handled {
            tracing::error!("X11 error: {:#?}", error);
            // XXX only update the error, if it wasn't handled by any of the hooks.
            *xconn.latest_error.lock().unwrap() = Some(error);
        }
    }
    // Fun fact: this return value is completely ignored.
    0
}

pub enum EventLoop<T: 'static> {
    #[cfg(wayland_platform)]
    Wayland(Box<wayland::EventLoop<T>>),
    #[cfg(x11_platform)]
    X(Box<x11::EventLoop<T>>),
}

pub enum EventLoopProxy<T: 'static> {
    #[cfg(x11_platform)]
    X(x11::EventLoopProxy<T>),
    #[cfg(wayland_platform)]
    Wayland(wayland::EventLoopProxy<T>),
}

impl<T: 'static> Clone for EventLoopProxy<T> {
    fn clone(&self) -> Self {
        x11_or_wayland!(match self; EventLoopProxy(proxy) => proxy.clone(); as EventLoopProxy)
    }
}

impl<T: 'static> EventLoop<T> {
    pub(crate) fn new(
        attributes: &PlatformSpecificEventLoopAttributes,
    ) -> Result<Self, EventLoopError> {
        if !attributes.any_thread && !is_main_thread() {
            panic!(
                "Initializing the event loop outside of the main thread is a significant \
                 cross-platform compatibility hazard. If you absolutely need to create an \
                 EventLoop on a different thread, you can use the \
                 `EventLoopBuilderExtX11::any_thread` or `EventLoopBuilderExtWayland::any_thread` \
                 functions."
            );
        }

        // NOTE: Wayland first because of X11 could be present under Wayland as well. Empty
        // variables are also treated as not set.
        let backend = match (
            attributes.forced_backend,
            env::var("WAYLAND_DISPLAY")
                .ok()
                .filter(|var| !var.is_empty())
                .or_else(|| env::var("WAYLAND_SOCKET").ok())
                .filter(|var| !var.is_empty())
                .is_some(),
            env::var("DISPLAY")
                .map(|var| !var.is_empty())
                .unwrap_or(false),
        ) {
            // User is forcing a backend.
            (Some(backend), ..) => backend,
            // Wayland is present.
            #[cfg(wayland_platform)]
            (None, true, _) => Backend::Wayland,
            // X11 is present.
            #[cfg(x11_platform)]
            (None, _, true) => Backend::X,
            // No backend is present.
            (_, wayland_display, x11_display) => {
                let msg = if wayland_display && !cfg!(wayland_platform) {
                    "DISPLAY is not set; note: enable the `winit/wayland` feature to support \
                     Wayland"
                } else if x11_display && !cfg!(x11_platform) {
                    "neither WAYLAND_DISPLAY nor WAYLAND_SOCKET is set; note: enable the \
                     `winit/x11` feature to support X11"
                } else {
                    "neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set."
                };
                return Err(EventLoopError::Os(os_error!(OsError::Misc(msg))));
            }
        };

        // Create the display based on the backend.
        match backend {
            #[cfg(wayland_platform)]
            Backend::Wayland => EventLoop::new_wayland_any_thread(),
            #[cfg(x11_platform)]
            Backend::X => EventLoop::new_x11_any_thread(),
        }
    }

    #[cfg(wayland_platform)]
    fn new_wayland_any_thread() -> Result<EventLoop<T>, EventLoopError> {
        wayland::EventLoop::new().map(|evlp| EventLoop::Wayland(Box::new(evlp)))
    }

    #[cfg(x11_platform)]
    fn new_x11_any_thread() -> Result<EventLoop<T>, EventLoopError> {
        let xconn = match X11_BACKEND
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            Ok(xconn) => xconn.clone(),
            Err(err) => {
                return Err(EventLoopError::Os(os_error!(OsError::XNotSupported(
                    err.clone()
                ))))
            }
        };

        Ok(EventLoop::X(Box::new(x11::EventLoop::new(xconn))))
    }

    pub fn create_proxy(&self) -> EventLoopProxy<T> {
        x11_or_wayland!(match self; EventLoop(evlp) => evlp.create_proxy(); as EventLoopProxy)
    }

    pub fn run<F>(mut self, callback: F) -> Result<(), EventLoopError>
    where
        F: FnMut(crate::event::Event<T>, &RootELW),
    {
        self.run_on_demand(callback)
    }

    pub fn run_on_demand<F>(&mut self, callback: F) -> Result<(), EventLoopError>
    where
        F: FnMut(crate::event::Event<T>, &RootELW),
    {
        x11_or_wayland!(match self; EventLoop(evlp) => evlp.run_on_demand(callback))
    }

    pub fn pump_events<F>(&mut self, timeout: Option<Duration>, callback: F) -> PumpStatus
    where
        F: FnMut(crate::event::Event<T>, &RootELW),
    {
        x11_or_wayland!(match self; EventLoop(evlp) => evlp.pump_events(timeout, callback))
    }

    pub fn window_target(&self) -> &crate::event_loop::ActiveEventLoop {
        x11_or_wayland!(match self; EventLoop(evlp) => evlp.window_target())
    }
}

impl<T> AsFd for EventLoop<T> {
    fn as_fd(&self) -> BorrowedFd<'_> {
        x11_or_wayland!(match self; EventLoop(evlp) => evlp.as_fd())
    }
}

impl<T> AsRawFd for EventLoop<T> {
    fn as_raw_fd(&self) -> RawFd {
        x11_or_wayland!(match self; EventLoop(evlp) => evlp.as_raw_fd())
    }
}

impl<T: 'static> EventLoopProxy<T> {
    pub fn send_event(&self, event: T) -> Result<(), EventLoopClosed<T>> {
        x11_or_wayland!(match self; EventLoopProxy(proxy) => proxy.send_event(event))
    }
}

pub enum ActiveEventLoop {
    #[cfg(wayland_platform)]
    Wayland(Box<wayland::ActiveEventLoop>),
    #[cfg(x11_platform)]
    X(x11::ActiveEventLoop),
}

impl ActiveEventLoop {
    #[inline]
    pub fn is_wayland(&self) -> bool {
        match *self {
            #[cfg(wayland_platform)]
            ActiveEventLoop::Wayland(_) => true,
            #[cfg(x11_platform)]
            _ => false,
        }
    }

    pub fn create_custom_cursor(&self, cursor: CustomCursorSource) -> CustomCursor {
        x11_or_wayland!(match self; ActiveEventLoop(evlp) => evlp.create_custom_cursor(cursor))
    }

    #[inline]
    pub fn available_monitors(&self) -> VecDeque<MonitorHandle> {
        match *self {
            #[cfg(wayland_platform)]
            ActiveEventLoop::Wayland(ref evlp) => evlp
                .available_monitors()
                .map(MonitorHandle::Wayland)
                .collect(),
            #[cfg(x11_platform)]
            ActiveEventLoop::X(ref evlp) => {
                evlp.available_monitors().map(MonitorHandle::X).collect()
            }
        }
    }

    #[inline]
    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        Some(
            x11_or_wayland!(match self; ActiveEventLoop(evlp) => evlp.primary_monitor()?; as MonitorHandle),
        )
    }

    #[inline]
    pub fn listen_device_events(&self, allowed: DeviceEvents) {
        x11_or_wayland!(match self; Self(evlp) => evlp.listen_device_events(allowed))
    }

    #[inline]
    pub fn raw_display_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawDisplayHandle, raw_window_handle::HandleError> {
        x11_or_wayland!(match self; Self(evlp) => evlp.raw_display_handle_raw_window_handle())
    }

    pub(crate) fn set_control_flow(&self, control_flow: ControlFlow) {
        x11_or_wayland!(match self; Self(evlp) => evlp.set_control_flow(control_flow))
    }

    pub(crate) fn control_flow(&self) -> ControlFlow {
        x11_or_wayland!(match self; Self(evlp) => evlp.control_flow())
    }

    pub(crate) fn clear_exit(&self) {
        x11_or_wayland!(match self; Self(evlp) => evlp.clear_exit())
    }

    pub(crate) fn exit(&self) {
        x11_or_wayland!(match self; Self(evlp) => evlp.exit())
    }

    pub(crate) fn exiting(&self) -> bool {
        x11_or_wayland!(match self; Self(evlp) => evlp.exiting())
    }

    pub(crate) fn owned_display_handle(&self) -> OwnedDisplayHandle {
        match self {
            #[cfg(x11_platform)]
            Self::X(conn) => OwnedDisplayHandle::X(conn.x_connection().clone()),
            #[cfg(wayland_platform)]
            Self::Wayland(conn) => OwnedDisplayHandle::Wayland(conn.connection.clone()),
        }
    }

    #[allow(dead_code)]
    fn set_exit_code(&self, code: i32) {
        x11_or_wayland!(match self; Self(evlp) => evlp.set_exit_code(code))
    }

    #[allow(dead_code)]
    fn exit_code(&self) -> Option<i32> {
        x11_or_wayland!(match self; Self(evlp) => evlp.exit_code())
    }

    pub(crate) fn system_theme(&self) -> Option<Theme> {
        x11_or_wayland!(match self; Self(evlp) => evlp.system_theme())
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub(crate) enum OwnedDisplayHandle {
    #[cfg(x11_platform)]
    X(Arc<XConnection>),
    #[cfg(wayland_platform)]
    Wayland(wayland_client::Connection),
}

impl OwnedDisplayHandle {
    #[inline]
    pub fn raw_display_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawDisplayHandle, raw_window_handle::HandleError> {
        use std::ptr::NonNull;

        match self {
            #[cfg(x11_platform)]
            Self::X(xconn) => Ok(raw_window_handle::XlibDisplayHandle::new(
                NonNull::new(xconn.display.cast()),
                xconn.default_screen_index() as _,
            )
            .into()),

            #[cfg(wayland_platform)]
            Self::Wayland(conn) => {
                use sctk::reexports::client::Proxy;

                Ok(raw_window_handle::WaylandDisplayHandle::new(
                    NonNull::new(conn.display().id().as_ptr().cast()).unwrap(),
                )
                .into())
            }
        }
    }
}

/// Returns the minimum `Option<Duration>`, taking into account that `None`
/// equates to an infinite timeout, not a zero timeout (so can't just use
/// `Option::min`)
fn min_timeout(a: Option<Duration>, b: Option<Duration>) -> Option<Duration> {
    a.map_or(b, |a_timeout| {
        b.map_or(Some(a_timeout), |b_timeout| Some(a_timeout.min(b_timeout)))
    })
}

#[cfg(target_os = "linux")]
fn is_main_thread() -> bool {
    rustix::thread::gettid() == rustix::process::getpid()
}

#[cfg(any(target_os = "dragonfly", target_os = "freebsd", target_os = "openbsd"))]
fn is_main_thread() -> bool {
    use libc::pthread_main_np;

    unsafe { pthread_main_np() == 1 }
}

#[cfg(target_os = "netbsd")]
fn is_main_thread() -> bool {
    std::thread::current().name() == Some("main")
}
