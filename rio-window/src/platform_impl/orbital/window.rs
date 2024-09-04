use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::cursor::Cursor;
use crate::dpi::{PhysicalPosition, PhysicalSize, Position, Size};
use crate::platform_impl::Fullscreen;
use crate::window::ImePurpose;
use crate::{error, window};

use super::{
    ActiveEventLoop, MonitorHandle, OsError, RedoxSocket, TimeSocket, WindowId,
    WindowProperties,
};

// These values match the values uses in the `window_new` function in orbital:
// https://gitlab.redox-os.org/redox-os/orbital/-/blob/master/src/scheme.rs
const ORBITAL_FLAG_ASYNC: char = 'a';
const ORBITAL_FLAG_BACK: char = 'b';
const ORBITAL_FLAG_FRONT: char = 'f';
const ORBITAL_FLAG_HIDDEN: char = 'h';
const ORBITAL_FLAG_BORDERLESS: char = 'l';
const ORBITAL_FLAG_MAXIMIZED: char = 'm';
const ORBITAL_FLAG_RESIZABLE: char = 'r';
const ORBITAL_FLAG_TRANSPARENT: char = 't';

pub struct Window {
    window_socket: Arc<RedoxSocket>,
    redraws: Arc<Mutex<VecDeque<WindowId>>>,
    destroys: Arc<Mutex<VecDeque<WindowId>>>,
    wake_socket: Arc<TimeSocket>,
}

impl Window {
    pub(crate) fn new(
        el: &ActiveEventLoop,
        attrs: window::WindowAttributes,
    ) -> Result<Self, error::OsError> {
        let scale = MonitorHandle.scale_factor();

        let (x, y) = if let Some(pos) = attrs.position {
            pos.to_physical::<i32>(scale).into()
        } else {
            // These coordinates are a special value to center the window.
            (-1, -1)
        };

        let (w, h): (u32, u32) = if let Some(size) = attrs.inner_size {
            size.to_physical::<u32>(scale).into()
        } else {
            (1024, 768)
        };

        // TODO: min/max inner_size

        // Async by default.
        let mut flag_str = ORBITAL_FLAG_ASYNC.to_string();

        if attrs.maximized {
            flag_str.push(ORBITAL_FLAG_MAXIMIZED);
        }

        if attrs.resizable {
            flag_str.push(ORBITAL_FLAG_RESIZABLE);
        }

        // TODO: fullscreen

        if attrs.transparent {
            flag_str.push(ORBITAL_FLAG_TRANSPARENT);
        }

        if !attrs.decorations {
            flag_str.push(ORBITAL_FLAG_BORDERLESS);
        }

        if !attrs.visible {
            flag_str.push(ORBITAL_FLAG_HIDDEN);
        }

        match attrs.window_level {
            window::WindowLevel::AlwaysOnBottom => {
                flag_str.push(ORBITAL_FLAG_BACK);
            }
            window::WindowLevel::Normal => {}
            window::WindowLevel::AlwaysOnTop => {
                flag_str.push(ORBITAL_FLAG_FRONT);
            }
        }

        // TODO: window_icon

        // Open window.
        let window = RedoxSocket::orbital(&WindowProperties {
            flags: &flag_str,
            x,
            y,
            w,
            h,
            title: &attrs.title,
        })
        .expect("failed to open window");

        // Add to event socket.
        el.event_socket
            .write(&syscall::Event {
                id: window.fd,
                flags: syscall::EventFlags::EVENT_READ,
                data: window.fd,
            })
            .unwrap();

        let window_socket = Arc::new(window);

        // Notify event thread that this window was created, it will send some default events.
        {
            let mut creates = el.creates.lock().unwrap();
            creates.push_back(window_socket.clone());
        }

        el.wake_socket.wake().unwrap();

        Ok(Self {
            window_socket,
            redraws: el.redraws.clone(),
            destroys: el.destroys.clone(),
            wake_socket: el.wake_socket.clone(),
        })
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

    fn get_flag(&self, flag: char) -> Result<bool, error::ExternalError> {
        let mut buf: [u8; 4096] = [0; 4096];
        let path = self
            .window_socket
            .fpath(&mut buf)
            .map_err(|err| error::ExternalError::Os(os_error!(OsError::new(err))))?;
        let properties = WindowProperties::new(path);
        Ok(properties.flags.contains(flag))
    }

    fn set_flag(&self, flag: char, value: bool) -> Result<(), error::ExternalError> {
        self.window_socket
            .write(format!("F,{flag},{}", if value { 1 } else { 0 }).as_bytes())
            .map_err(|err| error::ExternalError::Os(os_error!(OsError::new(err))))?;
        Ok(())
    }

    #[inline]
    pub fn id(&self) -> WindowId {
        WindowId {
            fd: self.window_socket.fd as u64,
        }
    }

    #[inline]
    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        Some(MonitorHandle)
    }

    #[inline]
    pub fn available_monitors(&self) -> VecDeque<MonitorHandle> {
        let mut v = VecDeque::with_capacity(1);
        v.push_back(MonitorHandle);
        v
    }

    #[inline]
    pub fn current_monitor(&self) -> Option<MonitorHandle> {
        Some(MonitorHandle)
    }

    #[inline]
    pub fn scale_factor(&self) -> f64 {
        MonitorHandle.scale_factor()
    }

    #[inline]
    pub fn request_redraw(&self) {
        let window_id = self.id();
        let mut redraws = self.redraws.lock().unwrap();
        if !redraws.contains(&window_id) {
            redraws.push_back(window_id);

            self.wake_socket.wake().unwrap();
        }
    }

    #[inline]
    pub fn pre_present_notify(&self) {}

    #[inline]
    pub fn reset_dead_keys(&self) {
        // TODO?
    }

    #[inline]
    pub fn inner_position(
        &self,
    ) -> Result<PhysicalPosition<i32>, error::NotSupportedError> {
        let mut buf: [u8; 4096] = [0; 4096];
        let path = self
            .window_socket
            .fpath(&mut buf)
            .expect("failed to read properties");
        let properties = WindowProperties::new(path);
        Ok((properties.x, properties.y).into())
    }

    #[inline]
    pub fn outer_position(
        &self,
    ) -> Result<PhysicalPosition<i32>, error::NotSupportedError> {
        // TODO: adjust for window decorations
        self.inner_position()
    }

    #[inline]
    pub fn set_outer_position(&self, position: Position) {
        // TODO: adjust for window decorations
        let (x, y): (i32, i32) = position.to_physical::<i32>(self.scale_factor()).into();
        self.window_socket
            .write(format!("P,{x},{y}").as_bytes())
            .expect("failed to set position");
    }

    #[inline]
    pub fn inner_size(&self) -> PhysicalSize<u32> {
        let mut buf: [u8; 4096] = [0; 4096];
        let path = self
            .window_socket
            .fpath(&mut buf)
            .expect("failed to read properties");
        let properties = WindowProperties::new(path);
        (properties.w, properties.h).into()
    }

    #[inline]
    pub fn request_inner_size(&self, size: Size) -> Option<PhysicalSize<u32>> {
        let (w, h): (u32, u32) = size.to_physical::<u32>(self.scale_factor()).into();
        self.window_socket
            .write(format!("S,{w},{h}").as_bytes())
            .expect("failed to set size");
        None
    }

    #[inline]
    pub fn outer_size(&self) -> PhysicalSize<u32> {
        // TODO: adjust for window decorations
        self.inner_size()
    }

    #[inline]
    pub fn set_min_inner_size(&self, _: Option<Size>) {}

    #[inline]
    pub fn set_max_inner_size(&self, _: Option<Size>) {}

    #[inline]
    pub fn title(&self) -> String {
        let mut buf: [u8; 4096] = [0; 4096];
        let path = self
            .window_socket
            .fpath(&mut buf)
            .expect("failed to read properties");
        let properties = WindowProperties::new(path);
        properties.title.to_string()
    }

    #[inline]
    pub fn set_title(&self, title: &str) {
        self.window_socket
            .write(format!("T,{title}").as_bytes())
            .expect("failed to set title");
    }

    #[inline]
    pub fn set_transparent(&self, transparent: bool) {
        let _ = self.set_flag(ORBITAL_FLAG_TRANSPARENT, transparent);
    }

    #[inline]
    pub fn set_blur(&self, _blur: bool) {}

    #[inline]
    pub fn set_visible(&self, visible: bool) {
        let _ = self.set_flag(ORBITAL_FLAG_HIDDEN, !visible);
    }

    #[inline]
    pub fn is_visible(&self) -> Option<bool> {
        Some(!self.get_flag(ORBITAL_FLAG_HIDDEN).unwrap_or(false))
    }

    #[inline]
    pub fn resize_increments(&self) -> Option<PhysicalSize<u32>> {
        None
    }

    #[inline]
    pub fn set_resize_increments(&self, _increments: Option<Size>) {}

    #[inline]
    pub fn set_resizable(&self, resizeable: bool) {
        let _ = self.set_flag(ORBITAL_FLAG_RESIZABLE, resizeable);
    }

    #[inline]
    pub fn is_resizable(&self) -> bool {
        self.get_flag(ORBITAL_FLAG_RESIZABLE).unwrap_or(false)
    }

    #[inline]
    pub fn set_minimized(&self, _minimized: bool) {}

    #[inline]
    pub fn is_minimized(&self) -> Option<bool> {
        None
    }

    #[inline]
    pub fn set_maximized(&self, maximized: bool) {
        let _ = self.set_flag(ORBITAL_FLAG_MAXIMIZED, maximized);
    }

    #[inline]
    pub fn is_maximized(&self) -> bool {
        self.get_flag(ORBITAL_FLAG_MAXIMIZED).unwrap_or(false)
    }

    #[inline]
    pub(crate) fn set_fullscreen(&self, _monitor: Option<Fullscreen>) {}

    #[inline]
    pub(crate) fn fullscreen(&self) -> Option<Fullscreen> {
        None
    }

    #[inline]
    pub fn set_decorations(&self, decorations: bool) {
        let _ = self.set_flag(ORBITAL_FLAG_BORDERLESS, !decorations);
    }

    #[inline]
    pub fn is_decorated(&self) -> bool {
        !self.get_flag(ORBITAL_FLAG_BORDERLESS).unwrap_or(false)
    }

    #[inline]
    pub fn set_window_level(&self, level: window::WindowLevel) {
        match level {
            window::WindowLevel::AlwaysOnBottom => {
                let _ = self.set_flag(ORBITAL_FLAG_BACK, true);
            }
            window::WindowLevel::Normal => {
                let _ = self.set_flag(ORBITAL_FLAG_BACK, false);
                let _ = self.set_flag(ORBITAL_FLAG_FRONT, false);
            }
            window::WindowLevel::AlwaysOnTop => {
                let _ = self.set_flag(ORBITAL_FLAG_FRONT, true);
            }
        }
    }

    #[inline]
    pub fn set_window_icon(&self, _window_icon: Option<crate::icon::Icon>) {}

    #[inline]
    pub fn set_ime_cursor_area(&self, _position: Position, _size: Size) {}

    #[inline]
    pub fn set_ime_allowed(&self, _allowed: bool) {}

    #[inline]
    pub fn set_ime_purpose(&self, _purpose: ImePurpose) {}

    #[inline]
    pub fn focus_window(&self) {}

    #[inline]
    pub fn request_user_attention(
        &self,
        _request_type: Option<window::UserAttentionType>,
    ) {
    }

    #[inline]
    pub fn set_cursor(&self, _: Cursor) {}

    #[inline]
    pub fn set_cursor_position(&self, _: Position) -> Result<(), error::ExternalError> {
        Err(error::ExternalError::NotSupported(
            error::NotSupportedError::new(),
        ))
    }

    #[inline]
    pub fn set_cursor_grab(
        &self,
        mode: window::CursorGrabMode,
    ) -> Result<(), error::ExternalError> {
        let (grab, relative) = match mode {
            window::CursorGrabMode::None => (false, false),
            window::CursorGrabMode::Confined => (true, false),
            window::CursorGrabMode::Locked => (true, true),
        };
        self.window_socket
            .write(format!("M,G,{}", if grab { 1 } else { 0 }).as_bytes())
            .map_err(|err| error::ExternalError::Os(os_error!(OsError::new(err))))?;
        self.window_socket
            .write(format!("M,R,{}", if relative { 1 } else { 0 }).as_bytes())
            .map_err(|err| error::ExternalError::Os(os_error!(OsError::new(err))))?;
        Ok(())
    }

    #[inline]
    pub fn set_cursor_visible(&self, visible: bool) {
        let _ = self
            .window_socket
            .write(format!("M,C,{}", if visible { 1 } else { 0 }).as_bytes());
    }

    #[inline]
    pub fn drag_window(&self) -> Result<(), error::ExternalError> {
        self.window_socket
            .write(b"D")
            .map_err(|err| error::ExternalError::Os(os_error!(OsError::new(err))))?;
        Ok(())
    }

    #[inline]
    pub fn drag_resize_window(
        &self,
        direction: window::ResizeDirection,
    ) -> Result<(), error::ExternalError> {
        let arg = match direction {
            window::ResizeDirection::East => "R",
            window::ResizeDirection::North => "T",
            window::ResizeDirection::NorthEast => "T,R",
            window::ResizeDirection::NorthWest => "T,L",
            window::ResizeDirection::South => "B",
            window::ResizeDirection::SouthEast => "B,R",
            window::ResizeDirection::SouthWest => "B,L",
            window::ResizeDirection::West => "L",
        };
        self.window_socket
            .write(format!("D,{}", arg).as_bytes())
            .map_err(|err| error::ExternalError::Os(os_error!(OsError::new(err))))?;
        Ok(())
    }

    #[inline]
    pub fn show_window_menu(&self, _position: Position) {}

    #[inline]
    pub fn set_cursor_hittest(&self, _hittest: bool) -> Result<(), error::ExternalError> {
        Err(error::ExternalError::NotSupported(
            error::NotSupportedError::new(),
        ))
    }

    #[inline]
    pub fn raw_window_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawWindowHandle, raw_window_handle::HandleError> {
        let handle = raw_window_handle::OrbitalWindowHandle::new({
            let window = self.window_socket.fd as *mut _;
            std::ptr::NonNull::new(window).expect("orbital fd should never be null")
        });
        Ok(raw_window_handle::RawWindowHandle::Orbital(handle))
    }

    #[inline]
    pub fn raw_display_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawDisplayHandle, raw_window_handle::HandleError> {
        Ok(raw_window_handle::RawDisplayHandle::Orbital(
            raw_window_handle::OrbitalDisplayHandle::new(),
        ))
    }

    #[inline]
    pub fn set_enabled_buttons(&self, _buttons: window::WindowButtons) {}

    #[inline]
    pub fn enabled_buttons(&self) -> window::WindowButtons {
        window::WindowButtons::all()
    }

    #[inline]
    pub fn theme(&self) -> Option<window::Theme> {
        None
    }

    #[inline]
    pub fn has_focus(&self) -> bool {
        false
    }

    #[inline]
    pub fn set_theme(&self, _theme: Option<window::Theme>) {}

    pub fn set_content_protected(&self, _protected: bool) {}
}

impl Drop for Window {
    fn drop(&mut self) {
        {
            let mut destroys = self.destroys.lock().unwrap();
            destroys.push_back(self.id());
        }

        self.wake_socket.wake().unwrap();
    }
}
