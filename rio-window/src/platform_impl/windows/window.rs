#![cfg(windows_platform)]

use std::cell::Cell;
use std::ffi::c_void;
use std::mem::{self, MaybeUninit};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex, MutexGuard};
use std::{io, panic, ptr};

use windows_sys::Win32::Foundation::{
    BOOL, FALSE, HWND, LPARAM, OLE_E_WRONGCOMPOBJ, POINT, POINTS, RECT,
    RPC_E_CHANGED_MODE, S_OK, TRUE, WPARAM,
};
use windows_sys::Win32::Graphics::Dwm::{
    DwmEnableBlurBehindWindow, DwmSetWindowAttribute, DWMWA_BORDER_COLOR,
    DWMWA_CAPTION_COLOR, DWMWA_CLOAK, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_TEXT_COLOR,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWM_BB_BLURREGION, DWM_BB_ENABLE, DWM_BLURBEHIND,
    DWM_SYSTEMBACKDROP_TYPE, DWM_WINDOW_CORNER_PREFERENCE,
};
use windows_sys::Win32::Graphics::Gdi::{
    ChangeDisplaySettingsExW, ClientToScreen, CreateRectRgn, DeleteObject, InvalidateRgn,
    RedrawWindow, CDS_FULLSCREEN, DISP_CHANGE_BADFLAGS, DISP_CHANGE_BADMODE,
    DISP_CHANGE_BADPARAM, DISP_CHANGE_FAILED, DISP_CHANGE_SUCCESSFUL, RDW_INTERNALPAINT,
};
use windows_sys::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL,
    COINIT_APARTMENTTHREADED,
};
use windows_sys::Win32::System::Ole::{OleInitialize, RegisterDragDrop};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, GetActiveWindow, MapVirtualKeyW, ReleaseCapture, SendInput, ToUnicode,
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    MAPVK_VK_TO_VSC, VIRTUAL_KEY, VK_LMENU, VK_MENU, VK_SPACE,
};
use windows_sys::Win32::UI::Input::Touch::{RegisterTouchWindow, TWF_WANTPALM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, EnableMenuItem, FlashWindowEx, GetClientRect, GetCursorPos,
    GetForegroundWindow, GetSystemMenu, GetSystemMetrics, GetWindowPlacement,
    GetWindowTextLengthW, GetWindowTextW, IsWindowVisible, LoadCursorW, PeekMessageW,
    PostMessageW, RegisterClassExW, SetCursor, SetCursorPos, SetForegroundWindow,
    SetMenuDefaultItem, SetWindowDisplayAffinity, SetWindowPlacement, SetWindowPos,
    SetWindowTextW, TrackPopupMenu, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, FLASHWINFO,
    FLASHW_ALL, FLASHW_STOP, FLASHW_TIMERNOFG, FLASHW_TRAY, GWLP_HINSTANCE, HTBOTTOM,
    HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT,
    HTTOPRIGHT, MENU_ITEM_STATE, MFS_DISABLED, MFS_ENABLED, MF_BYCOMMAND, NID_READY,
    PM_NOREMOVE, SC_CLOSE, SC_MAXIMIZE, SC_MINIMIZE, SC_MOVE, SC_RESTORE, SC_SIZE,
    SM_DIGITIZER, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
    TPM_LEFTALIGN, TPM_RETURNCMD, WDA_EXCLUDEFROMCAPTURE, WDA_NONE, WM_NCLBUTTONDOWN,
    WM_SYSCOMMAND, WNDCLASSEXW,
};

use tracing::warn;

use crate::cursor::Cursor;
use crate::dpi::{PhysicalPosition, PhysicalSize, Position, Size};
use crate::error::{ExternalError, NotSupportedError, OsError as RootOsError};
use crate::icon::Icon;
use crate::platform::windows::{BackdropType, Color, CornerPreference};
use crate::platform_impl::platform::dark_mode::try_theme;
use crate::platform_impl::platform::definitions::{
    CLSID_TaskbarList, IID_ITaskbarList, IID_ITaskbarList2, ITaskbarList, ITaskbarList2,
};
use crate::platform_impl::platform::dpi::{
    dpi_to_scale_factor, enable_non_client_dpi_scaling, hwnd_dpi,
};
use crate::platform_impl::platform::drop_handler::FileDropHandler;
use crate::platform_impl::platform::event_loop::{self, ActiveEventLoop, DESTROY_MSG_ID};
use crate::platform_impl::platform::icon::{self, IconType, WinCursor};
use crate::platform_impl::platform::ime::ImeContext;
use crate::platform_impl::platform::keyboard::KeyEventBuilder;
use crate::platform_impl::platform::monitor::{self, MonitorHandle};
use crate::platform_impl::platform::window_state::{
    CursorFlags, SavedWindow, WindowFlags, WindowState,
};
use crate::platform_impl::platform::{util, Fullscreen, SelectedCursor, WindowId};
use crate::window::{
    CursorGrabMode, ImePurpose, ResizeDirection, Theme, UserAttentionType,
    WindowAttributes, WindowButtons, WindowLevel,
};

#[derive(Clone, Copy)]
#[repr(transparent)]
/// We need to pass the window handle to the event loop thread, which means it needs to be
/// Send+Sync.
struct SyncWindowHandle(HWND);

unsafe impl Send for SyncWindowHandle {}
unsafe impl Sync for SyncWindowHandle {}

impl SyncWindowHandle {
    fn hwnd(&self) -> HWND {
        self.0
    }
}

/// The Win32 implementation of the main `Window` object.
pub(crate) struct Window {
    /// Main handle for the window.
    window: SyncWindowHandle,

    /// The current window state.
    window_state: Arc<Mutex<WindowState>>,

    // The events loop proxy.
    thread_executor: event_loop::EventLoopThreadExecutor,
}

impl Window {
    pub(crate) fn new(
        event_loop: &ActiveEventLoop,
        w_attr: WindowAttributes,
    ) -> Result<Window, RootOsError> {
        // We dispatch an `init` function because of code style.
        // First person to remove the need for cloning here gets a cookie!
        //
        // done. you owe me -- ossi
        unsafe { init(w_attr, event_loop) }
    }

    pub(crate) fn maybe_queue_on_main(&self, f: impl FnOnce(&Self) + Send + 'static) {
        // TODO: Use `thread_executor` here
        f(self)
    }

    pub(crate) fn maybe_wait_on_main<R: Send>(
        &self,
        f: impl FnOnce(&Self) -> R + Send,
    ) -> R {
        // TODO: Use `thread_executor` here
        f(self)
    }

    fn window_state_lock(&self) -> MutexGuard<'_, WindowState> {
        self.window_state.lock().unwrap()
    }

    pub fn set_title(&self, text: &str) {
        let wide_text = util::encode_wide(text);
        unsafe {
            SetWindowTextW(self.hwnd(), wide_text.as_ptr());
        }
    }

    pub fn set_transparent(&self, transparent: bool) {
        let window = self.window;
        let window_state = Arc::clone(&self.window_state);
        self.thread_executor.execute_in_thread(move || {
            let _ = &window;
            WindowState::set_window_flags(
                window_state.lock().unwrap(),
                window.hwnd(),
                |f| f.set(WindowFlags::TRANSPARENT, transparent),
            );
        });
    }

    pub fn set_blur(&self, _blur: bool) {}

    #[inline]
    pub fn set_visible(&self, visible: bool) {
        let window = self.window;
        let window_state = Arc::clone(&self.window_state);
        self.thread_executor.execute_in_thread(move || {
            let _ = &window;
            WindowState::set_window_flags(
                window_state.lock().unwrap(),
                window.hwnd(),
                |f| f.set(WindowFlags::VISIBLE, visible),
            );
        });
    }

    #[inline]
    pub fn is_visible(&self) -> Option<bool> {
        Some(unsafe { IsWindowVisible(self.window.hwnd()) == 1 })
    }

    #[inline]
    pub fn request_redraw(&self) {
        // NOTE: mark that we requested a redraw to handle requests during `WM_PAINT` handling.
        self.window_state.lock().unwrap().redraw_requested = true;
        unsafe {
            RedrawWindow(self.hwnd(), ptr::null(), ptr::null_mut(), RDW_INTERNALPAINT);
        }
    }

    #[inline]
    pub fn pre_present_notify(&self) {}

    #[inline]
    pub fn outer_position(&self) -> Result<PhysicalPosition<i32>, NotSupportedError> {
        util::WindowArea::Outer
            .get_rect(self.hwnd())
            .map(|rect| Ok(PhysicalPosition::new(rect.left, rect.top)))
            .expect(
                "Unexpected GetWindowRect failure; please report this error to \
                 rust-windowing/winit",
            )
    }

    #[inline]
    pub fn inner_position(&self) -> Result<PhysicalPosition<i32>, NotSupportedError> {
        let mut position: POINT = unsafe { mem::zeroed() };
        if unsafe { ClientToScreen(self.hwnd(), &mut position) } == false.into() {
            panic!(
                "Unexpected ClientToScreen failure: please report this error to \
                 rust-windowing/winit"
            )
        }
        Ok(PhysicalPosition::new(position.x, position.y))
    }

    #[inline]
    pub fn set_outer_position(&self, position: Position) {
        let (x, y): (i32, i32) = position.to_physical::<i32>(self.scale_factor()).into();

        let window_state = Arc::clone(&self.window_state);
        let window = self.window;
        self.thread_executor.execute_in_thread(move || {
            let _ = &window;
            WindowState::set_window_flags(
                window_state.lock().unwrap(),
                window.hwnd(),
                |f| f.set(WindowFlags::MAXIMIZED, false),
            );
        });

        unsafe {
            SetWindowPos(
                self.hwnd(),
                ptr::null_mut(),
                x,
                y,
                0,
                0,
                SWP_ASYNCWINDOWPOS | SWP_NOZORDER | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            InvalidateRgn(self.hwnd(), ptr::null_mut(), false.into());
        }
    }

    #[inline]
    pub fn inner_size(&self) -> PhysicalSize<u32> {
        let mut rect: RECT = unsafe { mem::zeroed() };
        if unsafe { GetClientRect(self.hwnd(), &mut rect) } == false.into() {
            panic!(
                "Unexpected GetClientRect failure: please report this error to \
                 rust-windowing/winit"
            )
        }
        PhysicalSize::new(
            (rect.right - rect.left) as u32,
            (rect.bottom - rect.top) as u32,
        )
    }

    #[inline]
    pub fn outer_size(&self) -> PhysicalSize<u32> {
        util::WindowArea::Outer
            .get_rect(self.hwnd())
            .map(|rect| {
                PhysicalSize::new(
                    (rect.right - rect.left) as u32,
                    (rect.bottom - rect.top) as u32,
                )
            })
            .unwrap()
    }

    #[inline]
    pub fn request_inner_size(&self, size: Size) -> Option<PhysicalSize<u32>> {
        let scale_factor = self.scale_factor();
        let physical_size = size.to_physical::<u32>(scale_factor);

        let window_flags = self.window_state_lock().window_flags;
        window_flags.set_size(self.hwnd(), physical_size);

        if physical_size != self.inner_size() {
            let window_state = Arc::clone(&self.window_state);
            let window = self.window;
            self.thread_executor.execute_in_thread(move || {
                let _ = &window;
                WindowState::set_window_flags(
                    window_state.lock().unwrap(),
                    window.hwnd(),
                    |f| f.set(WindowFlags::MAXIMIZED, false),
                );
            });
        }

        None
    }

    #[inline]
    pub fn set_min_inner_size(&self, size: Option<Size>) {
        self.window_state_lock().min_size = size;
        // Make windows re-check the window size bounds.
        let size = self.inner_size();
        self.request_inner_size(size.into());
    }

    #[inline]
    pub fn set_max_inner_size(&self, size: Option<Size>) {
        self.window_state_lock().max_size = size;
        // Make windows re-check the window size bounds.
        let size = self.inner_size();
        self.request_inner_size(size.into());
    }

    #[inline]
    pub fn resize_increments(&self) -> Option<PhysicalSize<u32>> {
        let w = self.window_state_lock();
        let scale_factor = w.scale_factor;
        w.resize_increments
            .map(|size| size.to_physical(scale_factor))
    }

    #[inline]
    pub fn set_resize_increments(&self, increments: Option<Size>) {
        self.window_state_lock().resize_increments = increments;
    }

    #[inline]
    pub fn set_resizable(&self, resizable: bool) {
        let window = self.window;
        let window_state = Arc::clone(&self.window_state);

        self.thread_executor.execute_in_thread(move || {
            let _ = &window;
            WindowState::set_window_flags(
                window_state.lock().unwrap(),
                window.hwnd(),
                |f| f.set(WindowFlags::RESIZABLE, resizable),
            );
        });
    }

    #[inline]
    pub fn is_resizable(&self) -> bool {
        let window_state = self.window_state_lock();
        window_state.window_flags.contains(WindowFlags::RESIZABLE)
    }

    #[inline]
    pub fn set_enabled_buttons(&self, buttons: WindowButtons) {
        let window = self.window;
        let window_state = Arc::clone(&self.window_state);

        self.thread_executor.execute_in_thread(move || {
            let _ = &window;
            WindowState::set_window_flags(
                window_state.lock().unwrap(),
                window.hwnd(),
                |f| {
                    f.set(
                        WindowFlags::MINIMIZABLE,
                        buttons.contains(WindowButtons::MINIMIZE),
                    );
                    f.set(
                        WindowFlags::MAXIMIZABLE,
                        buttons.contains(WindowButtons::MAXIMIZE),
                    );
                    f.set(
                        WindowFlags::CLOSABLE,
                        buttons.contains(WindowButtons::CLOSE),
                    )
                },
            );
        });
    }

    pub fn enabled_buttons(&self) -> WindowButtons {
        let mut buttons = WindowButtons::empty();
        let window_state = self.window_state_lock();
        if window_state.window_flags.contains(WindowFlags::MINIMIZABLE) {
            buttons |= WindowButtons::MINIMIZE;
        }
        if window_state.window_flags.contains(WindowFlags::MAXIMIZABLE) {
            buttons |= WindowButtons::MAXIMIZE;
        }
        if window_state.window_flags.contains(WindowFlags::CLOSABLE) {
            buttons |= WindowButtons::CLOSE;
        }
        buttons
    }

    /// Returns the `hwnd` of this window.
    #[inline]
    pub fn hwnd(&self) -> HWND {
        self.window.hwnd()
    }

    #[inline]
    pub unsafe fn rwh_06_no_thread_check(
        &self,
    ) -> Result<raw_window_handle::RawWindowHandle, raw_window_handle::HandleError> {
        let mut window_handle = raw_window_handle::Win32WindowHandle::new(unsafe {
            // SAFETY: Handle will never be zero.
            std::num::NonZeroIsize::new_unchecked(self.window.hwnd() as isize)
        });
        let hinstance = unsafe { super::get_window_long(self.hwnd(), GWLP_HINSTANCE) };
        window_handle.hinstance = std::num::NonZeroIsize::new(hinstance);
        Ok(raw_window_handle::RawWindowHandle::Win32(window_handle))
    }

    #[inline]
    pub fn raw_window_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawWindowHandle, raw_window_handle::HandleError> {
        // TODO: Write a test once integration framework is ready to ensure that it holds.
        // If we aren't in the GUI thread, we can't return the window.
        if !self.thread_executor.in_event_loop_thread() {
            tracing::error!("tried to access window handle outside of the main thread");
            return Err(raw_window_handle::HandleError::Unavailable);
        }

        // SAFETY: We are on the correct thread.
        unsafe { self.rwh_06_no_thread_check() }
    }

    #[inline]
    pub fn raw_display_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawDisplayHandle, raw_window_handle::HandleError> {
        Ok(raw_window_handle::RawDisplayHandle::Windows(
            raw_window_handle::WindowsDisplayHandle::new(),
        ))
    }

    #[inline]
    pub fn set_cursor(&self, cursor: Cursor) {
        match cursor {
            Cursor::Icon(icon) => {
                self.window_state_lock().mouse.selected_cursor =
                    SelectedCursor::Named(icon);
                self.thread_executor.execute_in_thread(move || unsafe {
                    let cursor =
                        LoadCursorW(ptr::null_mut(), util::to_windows_cursor(icon));
                    SetCursor(cursor);
                });
            }
            Cursor::Custom(cursor) => {
                let new_cursor = match cursor.inner {
                    WinCursor::Cursor(cursor) => cursor,
                    WinCursor::Failed => {
                        warn!("Requested to apply failed cursor");
                        return;
                    }
                };
                self.window_state_lock().mouse.selected_cursor =
                    SelectedCursor::Custom(new_cursor.clone());
                self.thread_executor.execute_in_thread(move || unsafe {
                    SetCursor(new_cursor.as_raw_handle());
                });
            }
        }
    }

    #[inline]
    pub fn set_cursor_grab(&self, mode: CursorGrabMode) -> Result<(), ExternalError> {
        let confine = match mode {
            CursorGrabMode::None => false,
            CursorGrabMode::Confined => true,
            CursorGrabMode::Locked => {
                return Err(ExternalError::NotSupported(NotSupportedError::new()))
            }
        };

        let window = self.window;
        let window_state = Arc::clone(&self.window_state);
        let (tx, rx) = channel();

        self.thread_executor.execute_in_thread(move || {
            let _ = &window;
            let result = window_state
                .lock()
                .unwrap()
                .mouse
                .set_cursor_flags(window.hwnd(), |f| f.set(CursorFlags::GRABBED, confine))
                .map_err(|e| ExternalError::Os(os_error!(e)));
            let _ = tx.send(result);
        });
        rx.recv().unwrap()
    }

    #[inline]
    pub fn set_cursor_visible(&self, visible: bool) {
        let window = self.window;
        let window_state = Arc::clone(&self.window_state);
        let (tx, rx) = channel();

        self.thread_executor.execute_in_thread(move || {
            let _ = &window;
            let result = window_state
                .lock()
                .unwrap()
                .mouse
                .set_cursor_flags(window.hwnd(), |f| f.set(CursorFlags::HIDDEN, !visible))
                .map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
        rx.recv().unwrap().ok();
    }

    #[inline]
    pub fn scale_factor(&self) -> f64 {
        self.window_state_lock().scale_factor
    }

    #[inline]
    pub fn set_cursor_position(&self, position: Position) -> Result<(), ExternalError> {
        let scale_factor = self.scale_factor();
        let (x, y) = position.to_physical::<i32>(scale_factor).into();

        let mut point = POINT { x, y };
        unsafe {
            if ClientToScreen(self.hwnd(), &mut point) == false.into() {
                return Err(ExternalError::Os(os_error!(io::Error::last_os_error())));
            }
            if SetCursorPos(point.x, point.y) == false.into() {
                return Err(ExternalError::Os(os_error!(io::Error::last_os_error())));
            }
        }
        Ok(())
    }

    unsafe fn handle_os_dragging(&self, wparam: WPARAM) {
        let window = self.window;
        let window_state = self.window_state.clone();

        self.thread_executor.execute_in_thread(move || {
            {
                let mut guard = window_state.lock().unwrap();
                if !guard.dragging {
                    guard.dragging = true;
                } else {
                    return;
                }
            }

            let points = {
                let mut pos = unsafe { mem::zeroed() };
                unsafe { GetCursorPos(&mut pos) };
                pos
            };
            let points = POINTS {
                x: points.x as i16,
                y: points.y as i16,
            };

            // ReleaseCapture needs to execute on the main thread
            unsafe { ReleaseCapture() };

            unsafe {
                PostMessageW(
                    window.hwnd(),
                    WM_NCLBUTTONDOWN,
                    wparam,
                    &points as *const _ as LPARAM,
                )
            };
        });
    }

    #[inline]
    pub fn drag_window(&self) -> Result<(), ExternalError> {
        unsafe {
            self.handle_os_dragging(HTCAPTION as WPARAM);
        }

        Ok(())
    }

    #[inline]
    pub fn drag_resize_window(
        &self,
        direction: ResizeDirection,
    ) -> Result<(), ExternalError> {
        unsafe {
            self.handle_os_dragging(match direction {
                ResizeDirection::East => HTRIGHT,
                ResizeDirection::North => HTTOP,
                ResizeDirection::NorthEast => HTTOPRIGHT,
                ResizeDirection::NorthWest => HTTOPLEFT,
                ResizeDirection::South => HTBOTTOM,
                ResizeDirection::SouthEast => HTBOTTOMRIGHT,
                ResizeDirection::SouthWest => HTBOTTOMLEFT,
                ResizeDirection::West => HTLEFT,
            } as WPARAM);
        }

        Ok(())
    }

    unsafe fn handle_showing_window_menu(&self, position: Position) {
        unsafe {
            let point = {
                let mut point = POINT { x: 0, y: 0 };
                let scale_factor = self.scale_factor();
                let (x, y) = position.to_physical::<i32>(scale_factor).into();
                point.x = x;
                point.y = y;
                if ClientToScreen(self.hwnd(), &mut point) == false.into() {
                    warn!(
                        "Can't convert client-area coordinates to screen coordinates when showing \
                         window menu."
                    );
                    return;
                }
                point
            };

            // get the current system menu
            let h_menu = GetSystemMenu(self.hwnd(), 0);
            if h_menu.is_null() {
                warn!("The corresponding window doesn't have a system menu");
                // This situation should not be treated as an error so just return without showing
                // menu.
                return;
            }

            fn enable(b: bool) -> MENU_ITEM_STATE {
                if b {
                    MFS_ENABLED
                } else {
                    MFS_DISABLED
                }
            }

            // Change the menu items according to the current window status.

            let restore_btn = enable(self.is_maximized() && self.is_resizable());
            let size_btn = enable(!self.is_maximized() && self.is_resizable());
            let maximize_btn = enable(!self.is_maximized() && self.is_resizable());

            EnableMenuItem(h_menu, SC_RESTORE, MF_BYCOMMAND | restore_btn);
            EnableMenuItem(h_menu, SC_MOVE, MF_BYCOMMAND | enable(!self.is_maximized()));
            EnableMenuItem(h_menu, SC_SIZE, MF_BYCOMMAND | size_btn);
            EnableMenuItem(h_menu, SC_MINIMIZE, MF_BYCOMMAND | MFS_ENABLED);
            EnableMenuItem(h_menu, SC_MAXIMIZE, MF_BYCOMMAND | maximize_btn);
            EnableMenuItem(h_menu, SC_CLOSE, MF_BYCOMMAND | MFS_ENABLED);

            // Set the default menu item.
            SetMenuDefaultItem(h_menu, SC_CLOSE, 0);

            // Popup the system menu at the position.
            let result = TrackPopupMenu(
                h_menu,
                TPM_RETURNCMD | TPM_LEFTALIGN, /* for now im using LTR, but we have to use user
                                                * layout direction */
                point.x,
                point.y,
                0,
                self.hwnd(),
                std::ptr::null_mut(),
            );

            if result == 0 {
                // User canceled the menu, no need to continue.
                return;
            }

            // Send the command that the user select to the corresponding window.
            if PostMessageW(self.hwnd(), WM_SYSCOMMAND, result as _, 0) == 0 {
                warn!("Can't post the system menu message to the window.");
            }
        }
    }

    #[inline]
    pub fn show_window_menu(&self, position: Position) {
        unsafe {
            self.handle_showing_window_menu(position);
        }
    }

    #[inline]
    pub fn set_cursor_hittest(&self, hittest: bool) -> Result<(), ExternalError> {
        let window = self.window;
        let window_state = Arc::clone(&self.window_state);
        self.thread_executor.execute_in_thread(move || {
            WindowState::set_window_flags(
                window_state.lock().unwrap(),
                window.hwnd(),
                |f| f.set(WindowFlags::IGNORE_CURSOR_EVENT, !hittest),
            );
        });

        Ok(())
    }

    #[inline]
    pub fn id(&self) -> WindowId {
        WindowId(self.hwnd())
    }

    #[inline]
    pub fn set_minimized(&self, minimized: bool) {
        let window = self.window;
        let window_state = Arc::clone(&self.window_state);

        let is_minimized = util::is_minimized(self.hwnd());

        self.thread_executor.execute_in_thread(move || {
            let _ = &window;
            WindowState::set_window_flags_in_place(
                &mut window_state.lock().unwrap(),
                |f| f.set(WindowFlags::MINIMIZED, is_minimized),
            );
            WindowState::set_window_flags(
                window_state.lock().unwrap(),
                window.hwnd(),
                |f| f.set(WindowFlags::MINIMIZED, minimized),
            );
        });
    }

    #[inline]
    pub fn is_minimized(&self) -> Option<bool> {
        Some(util::is_minimized(self.hwnd()))
    }

    #[inline]
    pub fn set_maximized(&self, maximized: bool) {
        let window = self.window;
        let window_state = Arc::clone(&self.window_state);

        self.thread_executor.execute_in_thread(move || {
            let _ = &window;
            WindowState::set_window_flags(
                window_state.lock().unwrap(),
                window.hwnd(),
                |f| f.set(WindowFlags::MAXIMIZED, maximized),
            );
        });
    }

    #[inline]
    pub fn is_maximized(&self) -> bool {
        let window_state = self.window_state_lock();
        window_state.window_flags.contains(WindowFlags::MAXIMIZED)
    }

    #[inline]
    pub fn fullscreen(&self) -> Option<Fullscreen> {
        let window_state = self.window_state_lock();
        window_state.fullscreen.clone()
    }

    #[inline]
    pub fn set_fullscreen(&self, fullscreen: Option<Fullscreen>) {
        let window = self.window;
        let window_state = Arc::clone(&self.window_state);

        let mut window_state_lock = window_state.lock().unwrap();
        let old_fullscreen = window_state_lock.fullscreen.clone();

        match (&old_fullscreen, &fullscreen) {
            // Return if we already are in the same fullscreen mode
            _ if old_fullscreen == fullscreen => return,
            // Return if saved Borderless(monitor) is the same as current monitor when requested
            // fullscreen is Borderless(None)
            (
                Some(Fullscreen::Borderless(Some(monitor))),
                Some(Fullscreen::Borderless(None)),
            ) if *monitor == monitor::current_monitor(window.hwnd()) => return,
            _ => {}
        }

        window_state_lock.fullscreen.clone_from(&fullscreen);
        drop(window_state_lock);

        self.thread_executor.execute_in_thread(move || {
            let _ = &window;
            // Change video mode if we're transitioning to or from exclusive
            // fullscreen
            match (&old_fullscreen, &fullscreen) {
                (_, Some(Fullscreen::Exclusive(video_mode))) => {
                    let monitor = video_mode.monitor();
                    let monitor_info =
                        monitor::get_monitor_info(monitor.hmonitor()).unwrap();

                    let res = unsafe {
                        ChangeDisplaySettingsExW(
                            monitor_info.szDevice.as_ptr(),
                            &*video_mode.native_video_mode,
                            ptr::null_mut(),
                            CDS_FULLSCREEN,
                            ptr::null(),
                        )
                    };

                    debug_assert!(res != DISP_CHANGE_BADFLAGS);
                    debug_assert!(res != DISP_CHANGE_BADMODE);
                    debug_assert!(res != DISP_CHANGE_BADPARAM);
                    debug_assert!(res != DISP_CHANGE_FAILED);
                    assert_eq!(res, DISP_CHANGE_SUCCESSFUL);
                }
                (Some(Fullscreen::Exclusive(_)), _) => {
                    let res = unsafe {
                        ChangeDisplaySettingsExW(
                            ptr::null(),
                            ptr::null(),
                            ptr::null_mut(),
                            CDS_FULLSCREEN,
                            ptr::null(),
                        )
                    };

                    debug_assert!(res != DISP_CHANGE_BADFLAGS);
                    debug_assert!(res != DISP_CHANGE_BADMODE);
                    debug_assert!(res != DISP_CHANGE_BADPARAM);
                    debug_assert!(res != DISP_CHANGE_FAILED);
                    assert_eq!(res, DISP_CHANGE_SUCCESSFUL);
                }
                _ => (),
            }

            unsafe {
                // There are some scenarios where calling `ChangeDisplaySettingsExW` takes long
                // enough to execute that the DWM thinks our program has frozen and takes over
                // our program's window. When that happens, the `SetWindowPos` call below gets
                // eaten and the window doesn't get set to the proper fullscreen position.
                //
                // Calling `PeekMessageW` here notifies Windows that our process is still running
                // fine, taking control back from the DWM and ensuring that the `SetWindowPos` call
                // below goes through.
                let mut msg = mem::zeroed();
                PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_NOREMOVE);
            }

            // Update window style
            WindowState::set_window_flags(
                window_state.lock().unwrap(),
                window.hwnd(),
                |f| {
                    f.set(
                        WindowFlags::MARKER_EXCLUSIVE_FULLSCREEN,
                        matches!(fullscreen, Some(Fullscreen::Exclusive(_))),
                    );
                    f.set(
                        WindowFlags::MARKER_BORDERLESS_FULLSCREEN,
                        matches!(fullscreen, Some(Fullscreen::Borderless(_))),
                    );
                },
            );

            // Mark as fullscreen window wrt to z-order
            //
            // this needs to be called before the below fullscreen SetWindowPos as this itself
            // will generate WM_SIZE messages of the old window size that can race with what we set
            // below
            unsafe {
                taskbar_mark_fullscreen(window.hwnd(), fullscreen.is_some());
            }

            // Update window bounds
            match &fullscreen {
                Some(fullscreen) => {
                    // Save window bounds before entering fullscreen
                    let placement = unsafe {
                        let mut placement = mem::zeroed();
                        GetWindowPlacement(window.hwnd(), &mut placement);
                        placement
                    };

                    window_state.lock().unwrap().saved_window =
                        Some(SavedWindow { placement });

                    let monitor = match &fullscreen {
                        Fullscreen::Exclusive(video_mode) => video_mode.monitor(),
                        Fullscreen::Borderless(Some(monitor)) => monitor.clone(),
                        Fullscreen::Borderless(None) => {
                            monitor::current_monitor(window.hwnd())
                        }
                    };

                    let position: (i32, i32) = monitor.position().into();
                    let size: (u32, u32) = monitor.size().into();

                    unsafe {
                        SetWindowPos(
                            window.hwnd(),
                            ptr::null_mut(),
                            position.0,
                            position.1,
                            size.0 as i32,
                            size.1 as i32,
                            SWP_ASYNCWINDOWPOS | SWP_NOZORDER,
                        );
                        InvalidateRgn(window.hwnd(), ptr::null_mut(), false.into());
                    }
                }
                None => {
                    let mut window_state_lock = window_state.lock().unwrap();
                    if let Some(SavedWindow { placement }) =
                        window_state_lock.saved_window.take()
                    {
                        drop(window_state_lock);
                        unsafe {
                            SetWindowPlacement(window.hwnd(), &placement);
                            InvalidateRgn(window.hwnd(), ptr::null_mut(), false.into());
                        }
                    }
                }
            }
        });
    }

    #[inline]
    pub fn set_decorations(&self, decorations: bool) {
        let window = self.window;
        let window_state = Arc::clone(&self.window_state);

        self.thread_executor.execute_in_thread(move || {
            let _ = &window;
            WindowState::set_window_flags(
                window_state.lock().unwrap(),
                window.hwnd(),
                |f| f.set(WindowFlags::MARKER_DECORATIONS, decorations),
            );
        });
    }

    #[inline]
    pub fn is_decorated(&self) -> bool {
        let window_state = self.window_state_lock();
        window_state
            .window_flags
            .contains(WindowFlags::MARKER_DECORATIONS)
    }

    #[inline]
    pub fn set_window_level(&self, level: WindowLevel) {
        let window = self.window;
        let window_state = Arc::clone(&self.window_state);

        self.thread_executor.execute_in_thread(move || {
            let _ = &window;
            WindowState::set_window_flags(
                window_state.lock().unwrap(),
                window.hwnd(),
                |f| {
                    f.set(
                        WindowFlags::ALWAYS_ON_TOP,
                        level == WindowLevel::AlwaysOnTop,
                    );
                    f.set(
                        WindowFlags::ALWAYS_ON_BOTTOM,
                        level == WindowLevel::AlwaysOnBottom,
                    );
                },
            );
        });
    }

    #[inline]
    pub fn current_monitor(&self) -> Option<MonitorHandle> {
        Some(monitor::current_monitor(self.hwnd()))
    }

    #[inline]
    pub fn set_window_icon(&self, window_icon: Option<Icon>) {
        if let Some(ref window_icon) = window_icon {
            window_icon
                .inner
                .set_for_window(self.hwnd(), IconType::Small);
        } else {
            icon::unset_for_window(self.hwnd(), IconType::Small);
        }
        self.window_state_lock().window_icon = window_icon;
    }

    #[inline]
    pub fn set_enable(&self, enabled: bool) {
        unsafe { EnableWindow(self.hwnd(), enabled.into()) };
    }

    #[inline]
    pub fn set_taskbar_icon(&self, taskbar_icon: Option<Icon>) {
        if let Some(ref taskbar_icon) = taskbar_icon {
            taskbar_icon
                .inner
                .set_for_window(self.hwnd(), IconType::Big);
        } else {
            icon::unset_for_window(self.hwnd(), IconType::Big);
        }
        self.window_state_lock().taskbar_icon = taskbar_icon;
    }

    #[inline]
    pub fn set_ime_cursor_area(&self, spot: Position, size: Size) {
        let window = self.window;
        let state = self.window_state.clone();
        self.thread_executor.execute_in_thread(move || unsafe {
            let scale_factor = state.lock().unwrap().scale_factor;
            ImeContext::current(window.hwnd()).set_ime_cursor_area(
                spot,
                size,
                scale_factor,
            );
        });
    }

    #[inline]
    pub fn set_ime_allowed(&self, allowed: bool) {
        let window = self.window;
        let state = self.window_state.clone();
        self.thread_executor.execute_in_thread(move || unsafe {
            state.lock().unwrap().ime_allowed = allowed;
            ImeContext::set_ime_allowed(window.hwnd(), allowed);
        })
    }

    #[inline]
    pub fn set_ime_purpose(&self, _purpose: ImePurpose) {}

    #[inline]
    pub fn request_user_attention(&self, request_type: Option<UserAttentionType>) {
        let window = self.window;
        let active_window_handle = unsafe { GetActiveWindow() };
        if window.hwnd() == active_window_handle {
            return;
        }

        self.thread_executor.execute_in_thread(move || unsafe {
            let (flags, count) = request_type
                .map(|ty| match ty {
                    UserAttentionType::Critical => {
                        (FLASHW_ALL | FLASHW_TIMERNOFG, u32::MAX)
                    }
                    UserAttentionType::Informational => {
                        (FLASHW_TRAY | FLASHW_TIMERNOFG, 0)
                    }
                })
                .unwrap_or((FLASHW_STOP, 0));

            let flash_info = FLASHWINFO {
                cbSize: mem::size_of::<FLASHWINFO>() as u32,
                hwnd: window.hwnd(),
                dwFlags: flags,
                uCount: count,
                dwTimeout: 0,
            };
            FlashWindowEx(&flash_info);
        });
    }

    #[inline]
    pub fn set_theme(&self, theme: Option<Theme>) {
        try_theme(self.window.hwnd(), theme);
    }

    #[inline]
    pub fn theme(&self) -> Option<Theme> {
        Some(self.window_state_lock().current_theme)
    }

    #[inline]
    pub fn has_focus(&self) -> bool {
        let window_state = self.window_state.lock().unwrap();
        window_state.has_active_focus()
    }

    pub fn title(&self) -> String {
        let len = unsafe { GetWindowTextLengthW(self.window.hwnd()) } + 1;
        let mut buf = vec![0; len as usize];
        unsafe { GetWindowTextW(self.window.hwnd(), buf.as_mut_ptr(), len) };
        util::decode_wide(&buf).to_string_lossy().to_string()
    }

    #[inline]
    pub fn set_skip_taskbar(&self, skip: bool) {
        self.window_state_lock().skip_taskbar = skip;
        unsafe { set_skip_taskbar(self.hwnd(), skip) };
    }

    #[inline]
    pub fn set_undecorated_shadow(&self, shadow: bool) {
        let window = self.window;
        let window_state = Arc::clone(&self.window_state);

        self.thread_executor.execute_in_thread(move || {
            let _ = &window;
            WindowState::set_window_flags(
                window_state.lock().unwrap(),
                window.hwnd(),
                |f| f.set(WindowFlags::MARKER_UNDECORATED_SHADOW, shadow),
            );
        });
    }

    #[inline]
    pub fn set_system_backdrop(&self, backdrop_type: BackdropType) {
        unsafe {
            DwmSetWindowAttribute(
                self.hwnd(),
                DWMWA_SYSTEMBACKDROP_TYPE as u32,
                &(backdrop_type as i32) as *const _ as _,
                mem::size_of::<DWM_SYSTEMBACKDROP_TYPE>() as _,
            );
        }
    }

    #[inline]
    pub fn focus_window(&self) {
        let window_flags = self.window_state_lock().window_flags();

        let is_visible = window_flags.contains(WindowFlags::VISIBLE);
        let is_minimized = util::is_minimized(self.hwnd());
        let is_foreground = self.window.hwnd() == unsafe { GetForegroundWindow() };

        if is_visible && !is_minimized && !is_foreground {
            unsafe { force_window_active(self.window.hwnd()) };
        }
    }

    #[inline]
    pub fn set_content_protected(&self, protected: bool) {
        unsafe {
            SetWindowDisplayAffinity(
                self.hwnd(),
                if protected {
                    WDA_EXCLUDEFROMCAPTURE
                } else {
                    WDA_NONE
                },
            )
        };
    }

    #[inline]
    pub fn reset_dead_keys(&self) {
        // `ToUnicode` consumes the dead-key by default, so we are constructing a fake (but valid)
        // key input which we can call `ToUnicode` with.
        unsafe {
            let vk = VK_SPACE as VIRTUAL_KEY;
            let scancode = MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC);
            let kbd_state = [0; 256];
            let mut char_buff = [MaybeUninit::uninit(); 8];
            ToUnicode(
                vk as u32,
                scancode,
                kbd_state.as_ptr(),
                char_buff[0].as_mut_ptr(),
                char_buff.len() as i32,
                0,
            );
        }
    }

    #[inline]
    pub fn set_border_color(&self, color: Color) {
        unsafe {
            DwmSetWindowAttribute(
                self.hwnd(),
                DWMWA_BORDER_COLOR as u32,
                &color as *const _ as _,
                mem::size_of::<Color>() as _,
            );
        }
    }

    #[inline]
    pub fn set_title_background_color(&self, color: Color) {
        unsafe {
            DwmSetWindowAttribute(
                self.hwnd(),
                DWMWA_CAPTION_COLOR as u32,
                &color as *const _ as _,
                mem::size_of::<Color>() as _,
            );
        }
    }

    #[inline]
    pub fn set_title_text_color(&self, color: Color) {
        unsafe {
            DwmSetWindowAttribute(
                self.hwnd(),
                DWMWA_TEXT_COLOR as u32,
                &color as *const _ as _,
                mem::size_of::<Color>() as _,
            );
        }
    }

    #[inline]
    pub fn set_corner_preference(&self, preference: CornerPreference) {
        unsafe {
            DwmSetWindowAttribute(
                self.hwnd(),
                DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                &(preference as DWM_WINDOW_CORNER_PREFERENCE) as *const _ as _,
                mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as _,
            );
        }
    }

    #[inline]
    pub fn set_cloaked(&self, cloaked: bool) {
        let cloaked = if cloaked { TRUE } else { FALSE };
        unsafe {
            DwmSetWindowAttribute(
                self.hwnd(),
                DWMWA_CLOAK as u32,
                &cloaked as *const BOOL as *const _,
                mem::size_of::<BOOL>() as u32,
            );
        }
    }
}

impl Drop for Window {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            // The window must be destroyed from the same thread that created it, so we send a
            // custom message to be handled by our callback to do the actual work.
            PostMessageW(self.hwnd(), DESTROY_MSG_ID.get(), 0, 0);
        }
    }
}

pub(super) struct InitData<'a> {
    // inputs
    pub event_loop: &'a ActiveEventLoop,
    pub attributes: WindowAttributes,
    pub window_flags: WindowFlags,
    // outputs
    pub window: Option<Window>,
}

impl InitData<'_> {
    unsafe fn create_window(&self, window: HWND) -> Window {
        // Register for touch events if applicable
        {
            let digitizer = unsafe { GetSystemMetrics(SM_DIGITIZER) as u32 };
            if digitizer & NID_READY != 0 {
                unsafe { RegisterTouchWindow(window, TWF_WANTPALM) };
            }
        }

        let dpi = unsafe { hwnd_dpi(window) };
        let scale_factor = dpi_to_scale_factor(dpi);

        // If the system theme is dark, we need to set the window theme now
        // before we update the window flags (and possibly show the
        // window for the first time).
        let current_theme = try_theme(window, self.attributes.preferred_theme);

        let window_state = {
            let window_state = WindowState::new(
                &self.attributes,
                scale_factor,
                current_theme,
                self.attributes.preferred_theme,
            );
            let window_state = Arc::new(Mutex::new(window_state));
            WindowState::set_window_flags(window_state.lock().unwrap(), window, |f| {
                *f = self.window_flags
            });
            window_state
        };

        enable_non_client_dpi_scaling(window);

        unsafe { ImeContext::set_ime_allowed(window, false) };

        Window {
            window: SyncWindowHandle(window),
            window_state,
            thread_executor: self.event_loop.create_thread_executor(),
        }
    }

    unsafe fn create_window_data(&self, win: &Window) -> event_loop::WindowData {
        let file_drop_handler = if self.attributes.platform_specific.drag_and_drop {
            let ole_init_result = unsafe { OleInitialize(ptr::null_mut()) };
            // It is ok if the initialize result is `S_FALSE` because it might happen that
            // multiple windows are created on the same thread.
            if ole_init_result == OLE_E_WRONGCOMPOBJ {
                panic!("OleInitialize failed! Result was: `OLE_E_WRONGCOMPOBJ`");
            } else if ole_init_result == RPC_E_CHANGED_MODE {
                panic!(
                    "OleInitialize failed! Result was: `RPC_E_CHANGED_MODE`. Make sure other \
                     crates are not using multithreaded COM library on the same thread or disable \
                     drag and drop support."
                );
            }

            let file_drop_runner = self.event_loop.runner_shared.clone();
            let file_drop_handler = FileDropHandler::new(
                win.window.hwnd(),
                Box::new(move |event| {
                    if let Ok(e) = event.map_nonuser_event() {
                        file_drop_runner.send_event(e)
                    }
                }),
            );

            let handler_interface_ptr = unsafe {
                &mut (*file_drop_handler.data).interface as *mut _ as *mut c_void
            };

            assert_eq!(
                unsafe { RegisterDragDrop(win.window.hwnd(), handler_interface_ptr) },
                S_OK
            );
            Some(file_drop_handler)
        } else {
            None
        };

        event_loop::WindowData {
            window_state: win.window_state.clone(),
            event_loop_runner: self.event_loop.runner_shared.clone(),
            key_event_builder: KeyEventBuilder::default(),
            _file_drop_handler: file_drop_handler,
            userdata_removed: Cell::new(false),
            recurse_depth: Cell::new(0),
        }
    }

    // Returns a pointer to window user data on success.
    // The user data will be registered for the window and can be accessed within the window event
    // callback.
    pub unsafe fn on_nccreate(&mut self, window: HWND) -> Option<isize> {
        let runner = self.event_loop.runner_shared.clone();
        let result = runner.catch_unwind(|| {
            let window = unsafe { self.create_window(window) };
            let window_data = unsafe { self.create_window_data(&window) };
            (window, window_data)
        });

        result.map(|(win, userdata)| {
            self.window = Some(win);
            let userdata = Box::into_raw(Box::new(userdata));
            userdata as _
        })
    }

    pub unsafe fn on_create(&mut self) {
        let win = self.window.as_mut().expect("failed window creation");

        // making the window transparent
        if self.attributes.transparent
            && !self.attributes.platform_specific.no_redirection_bitmap
        {
            // Empty region for the blur effect, so the window is fully transparent
            let region = unsafe { CreateRectRgn(0, 0, -1, -1) };

            let bb = DWM_BLURBEHIND {
                dwFlags: DWM_BB_ENABLE | DWM_BB_BLURREGION,
                fEnable: true.into(),
                hRgnBlur: region,
                fTransitionOnMaximized: false.into(),
            };
            let hr = unsafe { DwmEnableBlurBehindWindow(win.hwnd(), &bb) };
            if hr < 0 {
                warn!(
                    "Setting transparent window is failed. HRESULT Code: 0x{:X}",
                    hr
                );
            }
            unsafe { DeleteObject(region) };
        }

        win.set_skip_taskbar(self.attributes.platform_specific.skip_taskbar);
        win.set_window_icon(self.attributes.window_icon.clone());
        win.set_taskbar_icon(self.attributes.platform_specific.taskbar_icon.clone());

        let attributes = self.attributes.clone();

        if attributes.content_protected {
            win.set_content_protected(true);
        }

        win.set_cursor(attributes.cursor);

        // Set visible before setting the size to ensure the
        // attribute is correctly applied.
        win.set_visible(attributes.visible);

        win.set_enabled_buttons(attributes.enabled_buttons);

        let size = attributes
            .inner_size
            .unwrap_or_else(|| PhysicalSize::new(800, 600).into());
        let max_size = attributes
            .max_inner_size
            .unwrap_or_else(|| PhysicalSize::new(f64::MAX, f64::MAX).into());
        let min_size = attributes
            .min_inner_size
            .unwrap_or_else(|| PhysicalSize::new(0, 0).into());
        let clamped_size = Size::clamp(size, min_size, max_size, win.scale_factor());
        win.request_inner_size(clamped_size);

        // let margins = MARGINS {
        //     cxLeftWidth: 1,
        //     cxRightWidth: 1,
        //     cyTopHeight: 1,
        //     cyBottomHeight: 1,
        // };
        // dbg!(DwmExtendFrameIntoClientArea(win.hwnd(), &margins as *const _));

        if let Some(position) = attributes.position {
            win.set_outer_position(position);
        }

        win.set_system_backdrop(self.attributes.platform_specific.backdrop_type);

        if let Some(color) = self.attributes.platform_specific.border_color {
            win.set_border_color(color);
        }
        if let Some(color) = self.attributes.platform_specific.title_background_color {
            win.set_title_background_color(color);
        }
        if let Some(color) = self.attributes.platform_specific.title_text_color {
            win.set_title_text_color(color);
        }
        if let Some(corner) = self.attributes.platform_specific.corner_preference {
            win.set_corner_preference(corner);
        }
        win.set_cloaked(self.attributes.platform_specific.cloaked);
    }
}
unsafe fn init(
    attributes: WindowAttributes,
    event_loop: &ActiveEventLoop,
) -> Result<Window, RootOsError> {
    let title = util::encode_wide(&attributes.title);

    let class_name = util::encode_wide(&attributes.platform_specific.class_name);
    unsafe { register_window_class(&class_name) };

    let mut window_flags = WindowFlags::empty();
    window_flags.set(WindowFlags::MARKER_DECORATIONS, attributes.decorations);
    window_flags.set(
        WindowFlags::MARKER_UNDECORATED_SHADOW,
        attributes.platform_specific.decoration_shadow,
    );
    window_flags.set(
        WindowFlags::ALWAYS_ON_TOP,
        attributes.window_level == WindowLevel::AlwaysOnTop,
    );
    window_flags.set(
        WindowFlags::ALWAYS_ON_BOTTOM,
        attributes.window_level == WindowLevel::AlwaysOnBottom,
    );
    window_flags.set(
        WindowFlags::NO_BACK_BUFFER,
        attributes.platform_specific.no_redirection_bitmap,
    );
    window_flags.set(WindowFlags::MARKER_ACTIVATE, attributes.active);
    window_flags.set(WindowFlags::TRANSPARENT, attributes.transparent);
    // WindowFlags::VISIBLE and MAXIMIZED are set down below after the window has been configured.
    window_flags.set(WindowFlags::RESIZABLE, attributes.resizable);
    // Will be changed later using `window.set_enabled_buttons` but we need to set a default here
    // so the diffing later can work.
    window_flags.set(WindowFlags::CLOSABLE, true);
    window_flags.set(
        WindowFlags::CLIP_CHILDREN,
        attributes.platform_specific.clip_children,
    );

    let mut fallback_parent = || match attributes.platform_specific.owner {
        Some(parent) => {
            window_flags.set(WindowFlags::POPUP, true);
            Some(parent)
        }
        None => {
            window_flags.set(WindowFlags::ON_TASKBAR, true);
            None
        }
    };

    let parent = match attributes.parent_window.as_ref().map(|handle| handle.0) {
        Some(raw_window_handle::RawWindowHandle::Win32(handle)) => {
            window_flags.set(WindowFlags::CHILD, true);
            if attributes.platform_specific.menu.is_some() {
                warn!("Setting a menu on a child window is unsupported");
            }
            Some(handle.hwnd.get() as HWND)
        }
        Some(raw) => unreachable!("Invalid raw window handle {raw:?} on Windows"),
        None => fallback_parent(),
    };

    let menu = attributes.platform_specific.menu;
    let fullscreen = attributes.fullscreen.clone();
    let maximized = attributes.maximized;
    let mut initdata = InitData {
        event_loop,
        attributes,
        window_flags,
        window: None,
    };

    let (style, ex_style) = window_flags.to_window_styles();
    let handle = unsafe {
        CreateWindowExW(
            ex_style,
            class_name.as_ptr(),
            title.as_ptr(),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            parent.unwrap_or(ptr::null_mut()),
            menu.unwrap_or(ptr::null_mut()),
            util::get_instance_handle(),
            &mut initdata as *mut _ as *mut _,
        )
    };

    // If the window creation in `InitData` panicked, then should resume panicking here
    if let Err(panic_error) = event_loop.runner_shared.take_panic_error() {
        panic::resume_unwind(panic_error)
    }

    if handle.is_null() {
        return Err(os_error!(io::Error::last_os_error()));
    }

    // If the handle is non-null, then window creation must have succeeded, which means
    // that we *must* have populated the `InitData.window` field.
    let win = initdata.window.unwrap();

    // Need to set FULLSCREEN or MAXIMIZED after CreateWindowEx
    // This is because if the size is changed in WM_CREATE, the restored size will be stored in that
    // size.
    if fullscreen.is_some() {
        win.set_fullscreen(fullscreen.map(Into::into));
        unsafe { force_window_active(win.window.hwnd()) };
    } else if maximized {
        win.set_maximized(true);
    }

    Ok(win)
}

unsafe fn register_window_class(class_name: &[u16]) {
    let class = WNDCLASSEXW {
        cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(super::event_loop::public_window_callback),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: util::get_instance_handle(),
        hIcon: ptr::null_mut(),
        hCursor: ptr::null_mut(), // must be null in order for cursor state to work properly
        hbrBackground: ptr::null_mut(),
        lpszMenuName: ptr::null(),
        lpszClassName: class_name.as_ptr(),
        hIconSm: ptr::null_mut(),
    };

    // We ignore errors because registering the same window class twice would trigger
    //  an error, and because errors here are detected during CreateWindowEx anyway.
    // Also since there is no weird element in the struct, there is no reason for this
    //  call to fail.
    unsafe { RegisterClassExW(&class) };
}

struct ComInitialized(#[allow(dead_code)] *mut ());
impl Drop for ComInitialized {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

thread_local! {
    static COM_INITIALIZED: ComInitialized = {
        unsafe {
            CoInitializeEx(ptr::null(), COINIT_APARTMENTTHREADED as u32);
            ComInitialized(ptr::null_mut())
        }
    };

    static TASKBAR_LIST: Cell<*mut ITaskbarList> = const { Cell::new(ptr::null_mut()) };
    static TASKBAR_LIST2: Cell<*mut ITaskbarList2> = const { Cell::new(ptr::null_mut()) };
}

pub fn com_initialized() {
    COM_INITIALIZED.with(|_| {});
}

// Reference Implementation:
// https://github.com/chromium/chromium/blob/f18e79d901f56154f80eea1e2218544285e62623/ui/views/win/fullscreen_handler.cc
//
// As per MSDN marking the window as fullscreen should ensure that the
// taskbar is moved to the bottom of the Z-order when the fullscreen window
// is activated. If the window is not fullscreen, the Shell falls back to
// heuristics to determine how the window should be treated, which means
// that it could still consider the window as fullscreen. :(
unsafe fn taskbar_mark_fullscreen(handle: HWND, fullscreen: bool) {
    com_initialized();

    TASKBAR_LIST2.with(|task_bar_list2_ptr| {
        let mut task_bar_list2 = task_bar_list2_ptr.get();

        if task_bar_list2.is_null() {
            let hr = unsafe {
                CoCreateInstance(
                    &CLSID_TaskbarList,
                    ptr::null_mut(),
                    CLSCTX_ALL,
                    &IID_ITaskbarList2,
                    &mut task_bar_list2 as *mut _ as *mut _,
                )
            };
            if hr != S_OK {
                // In visual studio retrieving the taskbar list fails
                return;
            }

            let hr_init = unsafe { (*(*task_bar_list2).lpVtbl).parent.HrInit };
            if unsafe { hr_init(task_bar_list2.cast()) } != S_OK {
                // In some old windows, the taskbar object could not be created, we just ignore it
                return;
            }
            task_bar_list2_ptr.set(task_bar_list2)
        }

        task_bar_list2 = task_bar_list2_ptr.get();
        let mark_fullscreen_window =
            unsafe { (*(*task_bar_list2).lpVtbl).MarkFullscreenWindow };
        unsafe { mark_fullscreen_window(task_bar_list2, handle, fullscreen.into()) };
    })
}

pub(crate) unsafe fn set_skip_taskbar(hwnd: HWND, skip: bool) {
    com_initialized();
    TASKBAR_LIST.with(|task_bar_list_ptr| {
        let mut task_bar_list = task_bar_list_ptr.get();

        if task_bar_list.is_null() {
            let hr = unsafe {
                CoCreateInstance(
                    &CLSID_TaskbarList,
                    ptr::null_mut(),
                    CLSCTX_ALL,
                    &IID_ITaskbarList,
                    &mut task_bar_list as *mut _ as *mut _,
                )
            };
            if hr != S_OK {
                // In visual studio retrieving the taskbar list fails
                return;
            }

            let hr_init = unsafe { (*(*task_bar_list).lpVtbl).HrInit };
            if unsafe { hr_init(task_bar_list.cast()) } != S_OK {
                // In some old windows, the taskbar object could not be created, we just ignore it
                return;
            }
            task_bar_list_ptr.set(task_bar_list)
        }

        task_bar_list = task_bar_list_ptr.get();
        if skip {
            let delete_tab = unsafe { (*(*task_bar_list).lpVtbl).DeleteTab };
            unsafe { delete_tab(task_bar_list, hwnd) };
        } else {
            let add_tab = unsafe { (*(*task_bar_list).lpVtbl).AddTab };
            unsafe { add_tab(task_bar_list, hwnd) };
        }
    });
}

unsafe fn force_window_active(handle: HWND) {
    // In some situation, calling SetForegroundWindow could not bring up the window,
    // This is a little hack which can "steal" the foreground window permission
    // We only call this function in the window creation, so it should be fine.
    // See : https://stackoverflow.com/questions/10740346/setforegroundwindow-only-working-while-visual-studio-is-open
    let alt_sc = unsafe { MapVirtualKeyW(VK_MENU as u32, MAPVK_VK_TO_VSC) };

    let inputs = [
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_LMENU,
                    wScan: alt_sc as u16,
                    dwFlags: KEYEVENTF_EXTENDEDKEY,
                    dwExtraInfo: 0,
                    time: 0,
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_LMENU,
                    wScan: alt_sc as u16,
                    dwFlags: KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP,
                    dwExtraInfo: 0,
                    time: 0,
                },
            },
        },
    ];

    // Simulate a key press and release
    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            mem::size_of::<INPUT>() as i32,
        )
    };

    unsafe { SetForegroundWindow(handle) };
}
