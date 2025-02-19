use std::ffi::{c_void, OsStr, OsString};
use std::iter::once;
use std::ops::BitAnd;
use std::os::windows::prelude::{OsStrExt, OsStringExt};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{io, mem, ptr};

use crate::utils::Lazy;
use windows_sys::core::{HRESULT, PCWSTR};
use windows_sys::Win32::Foundation::{BOOL, HANDLE, HMODULE, HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{ClientToScreen, HMONITOR};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
use windows_sys::Win32::System::SystemServices::IMAGE_DOS_HEADER;
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT, MONITOR_DPI_TYPE, PROCESS_DPI_AWARENESS,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
use windows_sys::Win32::UI::Input::Pointer::{
    POINTER_INFO, POINTER_PEN_INFO, POINTER_TOUCH_INFO,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    ClipCursor, GetClientRect, GetClipCursor, GetSystemMetrics, GetWindowPlacement,
    GetWindowRect, IsIconic, ShowCursor, IDC_APPSTARTING, IDC_ARROW, IDC_CROSS, IDC_HAND,
    IDC_HELP, IDC_IBEAM, IDC_NO, IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE,
    IDC_SIZEWE, IDC_WAIT, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SW_MAXIMIZE, WINDOWPLACEMENT,
};

use crate::window::CursorIcon;

pub fn encode_wide(string: impl AsRef<OsStr>) -> Vec<u16> {
    string.as_ref().encode_wide().chain(once(0)).collect()
}

pub fn decode_wide(mut wide_c_string: &[u16]) -> OsString {
    if let Some(null_pos) = wide_c_string.iter().position(|c| *c == 0) {
        wide_c_string = &wide_c_string[..null_pos];
    }

    OsString::from_wide(wide_c_string)
}

pub fn has_flag<T>(bitset: T, flag: T) -> bool
where
    T: Copy + PartialEq + BitAnd<T, Output = T>,
{
    bitset & flag == flag
}

pub(crate) fn win_to_err(result: BOOL) -> Result<(), io::Error> {
    if result != false.into() {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

pub enum WindowArea {
    Outer,
    Inner,
}

impl WindowArea {
    pub fn get_rect(self, hwnd: HWND) -> Result<RECT, io::Error> {
        let mut rect = unsafe { mem::zeroed() };

        match self {
            WindowArea::Outer => {
                win_to_err(unsafe { GetWindowRect(hwnd, &mut rect) })?;
            }
            WindowArea::Inner => unsafe {
                let mut top_left = mem::zeroed();

                win_to_err(ClientToScreen(hwnd, &mut top_left))?;
                win_to_err(GetClientRect(hwnd, &mut rect))?;
                rect.left += top_left.x;
                rect.top += top_left.y;
                rect.right += top_left.x;
                rect.bottom += top_left.y;
            },
        }

        Ok(rect)
    }
}

pub fn is_maximized(window: HWND) -> bool {
    unsafe {
        let mut placement: WINDOWPLACEMENT = mem::zeroed();
        placement.length = mem::size_of::<WINDOWPLACEMENT>() as u32;
        GetWindowPlacement(window, &mut placement);
        placement.showCmd == SW_MAXIMIZE as u32
    }
}

pub fn set_cursor_hidden(hidden: bool) {
    static HIDDEN: AtomicBool = AtomicBool::new(false);
    let changed = HIDDEN.swap(hidden, Ordering::SeqCst) ^ hidden;
    if changed {
        unsafe { ShowCursor(BOOL::from(!hidden)) };
    }
}

pub fn get_cursor_clip() -> Result<RECT, io::Error> {
    unsafe {
        let mut rect: RECT = mem::zeroed();
        win_to_err(GetClipCursor(&mut rect)).map(|_| rect)
    }
}

/// Sets the cursor's clip rect.
///
/// Note that calling this will automatically dispatch a `WM_MOUSEMOVE` event.
pub fn set_cursor_clip(rect: Option<RECT>) -> Result<(), io::Error> {
    unsafe {
        let rect_ptr = rect
            .as_ref()
            .map(|r| r as *const RECT)
            .unwrap_or(ptr::null());
        win_to_err(ClipCursor(rect_ptr))
    }
}

pub fn get_desktop_rect() -> RECT {
    unsafe {
        let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        RECT {
            left,
            top,
            right: left + GetSystemMetrics(SM_CXVIRTUALSCREEN),
            bottom: top + GetSystemMetrics(SM_CYVIRTUALSCREEN),
        }
    }
}

pub fn is_focused(window: HWND) -> bool {
    window == unsafe { GetActiveWindow() }
}

pub fn is_minimized(window: HWND) -> bool {
    unsafe { IsIconic(window) != false.into() }
}

pub fn get_instance_handle() -> HMODULE {
    // Gets the instance handle by taking the address of the
    // pseudo-variable created by the microsoft linker:
    // https://devblogs.microsoft.com/oldnewthing/20041025-00/?p=37483

    // This is preferred over GetModuleHandle(NULL) because it also works in DLLs:
    // https://stackoverflow.com/questions/21718027/getmodulehandlenull-vs-hinstance

    extern "C" {
        static __ImageBase: IMAGE_DOS_HEADER;
    }

    unsafe { &__ImageBase as *const _ as _ }
}

pub(crate) fn to_windows_cursor(cursor: CursorIcon) -> PCWSTR {
    match cursor {
        CursorIcon::Default => IDC_ARROW,
        CursorIcon::Pointer => IDC_HAND,
        CursorIcon::Crosshair => IDC_CROSS,
        CursorIcon::Text | CursorIcon::VerticalText => IDC_IBEAM,
        CursorIcon::NotAllowed | CursorIcon::NoDrop => IDC_NO,
        CursorIcon::Grab
        | CursorIcon::Grabbing
        | CursorIcon::Move
        | CursorIcon::AllScroll => IDC_SIZEALL,
        CursorIcon::EResize
        | CursorIcon::WResize
        | CursorIcon::EwResize
        | CursorIcon::ColResize => IDC_SIZEWE,
        CursorIcon::NResize
        | CursorIcon::SResize
        | CursorIcon::NsResize
        | CursorIcon::RowResize => IDC_SIZENS,
        CursorIcon::NeResize | CursorIcon::SwResize | CursorIcon::NeswResize => {
            IDC_SIZENESW
        }
        CursorIcon::NwResize | CursorIcon::SeResize | CursorIcon::NwseResize => {
            IDC_SIZENWSE
        }
        CursorIcon::Wait => IDC_WAIT,
        CursorIcon::Progress => IDC_APPSTARTING,
        CursorIcon::Help => IDC_HELP,
        _ => IDC_ARROW, // use arrow for the missing cases.
    }
}

// Helper function to dynamically load function pointer as some functions
// may not be available on all Windows platforms supported by winit.
//
// `library` and `function` must be zero-terminated.
pub(super) fn get_function_impl(library: &str, function: &str) -> Option<*const c_void> {
    assert_eq!(library.chars().last(), Some('\0'));
    assert_eq!(function.chars().last(), Some('\0'));

    // Library names we will use are ASCII so we can use the A version to avoid string conversion.
    let module = unsafe { LoadLibraryA(library.as_ptr()) };
    if module.is_null() {
        return None;
    }

    unsafe { GetProcAddress(module, function.as_ptr()) }
        .map(|function_ptr| function_ptr as _)
}

macro_rules! get_function {
    ($lib:expr, $func:ident) => {
        crate::platform_impl::platform::util::get_function_impl(
            concat!($lib, '\0'),
            concat!(stringify!($func), '\0'),
        )
        .map(|f| unsafe { std::mem::transmute::<*const _, $func>(f) })
    };
}

pub type SetProcessDPIAware = unsafe extern "system" fn() -> BOOL;
pub type SetProcessDpiAwareness =
    unsafe extern "system" fn(value: PROCESS_DPI_AWARENESS) -> HRESULT;
pub type SetProcessDpiAwarenessContext =
    unsafe extern "system" fn(value: DPI_AWARENESS_CONTEXT) -> BOOL;
pub type GetDpiForWindow = unsafe extern "system" fn(hwnd: HWND) -> u32;
pub type GetDpiForMonitor = unsafe extern "system" fn(
    hmonitor: HMONITOR,
    dpi_type: MONITOR_DPI_TYPE,
    dpi_x: *mut u32,
    dpi_y: *mut u32,
) -> HRESULT;
pub type EnableNonClientDpiScaling = unsafe extern "system" fn(hwnd: HWND) -> BOOL;
pub type AdjustWindowRectExForDpi = unsafe extern "system" fn(
    rect: *mut RECT,
    dwStyle: u32,
    bMenu: BOOL,
    dwExStyle: u32,
    dpi: u32,
) -> BOOL;

pub type GetPointerFrameInfoHistory = unsafe extern "system" fn(
    pointerId: u32,
    entriesCount: *mut u32,
    pointerCount: *mut u32,
    pointerInfo: *mut POINTER_INFO,
) -> BOOL;

pub type SkipPointerFrameMessages = unsafe extern "system" fn(pointerId: u32) -> BOOL;
pub type GetPointerDeviceRects = unsafe extern "system" fn(
    device: HANDLE,
    pointerDeviceRect: *mut RECT,
    displayRect: *mut RECT,
) -> BOOL;

pub type GetPointerTouchInfo =
    unsafe extern "system" fn(pointerId: u32, touchInfo: *mut POINTER_TOUCH_INFO) -> BOOL;

pub type GetPointerPenInfo =
    unsafe extern "system" fn(pointId: u32, penInfo: *mut POINTER_PEN_INFO) -> BOOL;

pub(crate) static GET_DPI_FOR_WINDOW: Lazy<Option<GetDpiForWindow>> =
    Lazy::new(|| get_function!("user32.dll", GetDpiForWindow));
pub(crate) static ADJUST_WINDOW_RECT_EX_FOR_DPI: Lazy<Option<AdjustWindowRectExForDpi>> =
    Lazy::new(|| get_function!("user32.dll", AdjustWindowRectExForDpi));
pub(crate) static GET_DPI_FOR_MONITOR: Lazy<Option<GetDpiForMonitor>> =
    Lazy::new(|| get_function!("shcore.dll", GetDpiForMonitor));
pub(crate) static ENABLE_NON_CLIENT_DPI_SCALING: Lazy<Option<EnableNonClientDpiScaling>> =
    Lazy::new(|| get_function!("user32.dll", EnableNonClientDpiScaling));
pub(crate) static SET_PROCESS_DPI_AWARENESS_CONTEXT: Lazy<
    Option<SetProcessDpiAwarenessContext>,
> = Lazy::new(|| get_function!("user32.dll", SetProcessDpiAwarenessContext));
pub(crate) static SET_PROCESS_DPI_AWARENESS: Lazy<Option<SetProcessDpiAwareness>> =
    Lazy::new(|| get_function!("shcore.dll", SetProcessDpiAwareness));
pub(crate) static SET_PROCESS_DPI_AWARE: Lazy<Option<SetProcessDPIAware>> =
    Lazy::new(|| get_function!("user32.dll", SetProcessDPIAware));
pub(crate) static GET_POINTER_FRAME_INFO_HISTORY: Lazy<
    Option<GetPointerFrameInfoHistory>,
> = Lazy::new(|| get_function!("user32.dll", GetPointerFrameInfoHistory));
pub(crate) static SKIP_POINTER_FRAME_MESSAGES: Lazy<Option<SkipPointerFrameMessages>> =
    Lazy::new(|| get_function!("user32.dll", SkipPointerFrameMessages));
pub(crate) static GET_POINTER_DEVICE_RECTS: Lazy<Option<GetPointerDeviceRects>> =
    Lazy::new(|| get_function!("user32.dll", GetPointerDeviceRects));
pub(crate) static GET_POINTER_TOUCH_INFO: Lazy<Option<GetPointerTouchInfo>> =
    Lazy::new(|| get_function!("user32.dll", GetPointerTouchInfo));
pub(crate) static GET_POINTER_PEN_INFO: Lazy<Option<GetPointerPenInfo>> =
    Lazy::new(|| get_function!("user32.dll", GetPointerPenInfo));
