#![allow(non_snake_case)]

mod runner;

use std::cell::Cell;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::marker::PhantomData;
use std::os::windows::io::{
    AsRawHandle as _, FromRawHandle as _, OwnedHandle, RawHandle,
};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use std::{mem, panic, ptr};

use crate::utils::Lazy;

use windows_sys::Win32::Foundation::{
    GetLastError, FALSE, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WAIT_FAILED, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromRect, MonitorFromWindow, RedrawWindow, ScreenToClient,
    ValidateRect, MONITORINFO, MONITOR_DEFAULTTONULL, RDW_INTERNALPAINT, SC_SCREENSAVE,
};
use windows_sys::Win32::System::Ole::RevokeDragDrop;
use windows_sys::Win32::System::Threading::{
    CreateWaitableTimerExW, GetCurrentThreadId, SetWaitableTimer,
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, INFINITE, TIMER_ALL_ACCESS,
};
use windows_sys::Win32::UI::Controls::{HOVER_DEFAULT, WM_MOUSELEAVE};
use windows_sys::Win32::UI::Input::Ime::{
    GCS_COMPSTR, GCS_RESULTSTR, ISC_SHOWUICOMPOSITIONWINDOW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows_sys::Win32::UI::Input::Pointer::{
    POINTER_FLAG_DOWN, POINTER_FLAG_UP, POINTER_FLAG_UPDATE,
};
use windows_sys::Win32::UI::Input::Touch::{
    CloseTouchInputHandle, GetTouchInputInfo, TOUCHEVENTF_DOWN, TOUCHEVENTF_MOVE,
    TOUCHEVENTF_UP, TOUCHINPUT,
};
use windows_sys::Win32::UI::Input::{
    MOUSE_MOVE_RELATIVE, RAWINPUT, RIM_TYPEKEYBOARD, RIM_TYPEMOUSE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
    GetCursorPos, GetMenu, LoadCursorW, MsgWaitForMultipleObjectsEx, PeekMessageW,
    PostMessageW, RegisterClassExW, RegisterWindowMessageA, SetCursor, SetWindowPos,
    TranslateMessage, CREATESTRUCTW, GIDC_ARRIVAL, GIDC_REMOVAL, GWL_STYLE, GWL_USERDATA,
    HTCAPTION, HTCLIENT, MINMAXINFO, MNC_CLOSE, MSG, MWMO_INPUTAVAILABLE,
    NCCALCSIZE_PARAMS, PM_REMOVE, PT_PEN, PT_TOUCH, QS_ALLINPUT, RI_MOUSE_HWHEEL,
    RI_MOUSE_WHEEL, SC_MINIMIZE, SC_RESTORE, SIZE_MAXIMIZED, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, WHEEL_DELTA, WINDOWPOS, WMSZ_BOTTOM, WMSZ_BOTTOMLEFT,
    WMSZ_BOTTOMRIGHT, WMSZ_LEFT, WMSZ_RIGHT, WMSZ_TOP, WMSZ_TOPLEFT, WMSZ_TOPRIGHT,
    WM_CAPTURECHANGED, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_DPICHANGED, WM_ENTERSIZEMOVE,
    WM_EXITSIZEMOVE, WM_GETMINMAXINFO, WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION,
    WM_IME_SETCONTEXT, WM_IME_STARTCOMPOSITION, WM_INPUT, WM_INPUT_DEVICE_CHANGE,
    WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
    WM_MBUTTONUP, WM_MENUCHAR, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    WM_NCACTIVATE, WM_NCCALCSIZE, WM_NCCREATE, WM_NCDESTROY, WM_NCLBUTTONDOWN, WM_PAINT,
    WM_POINTERDOWN, WM_POINTERUP, WM_POINTERUPDATE, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_SETCURSOR, WM_SETFOCUS, WM_SETTINGCHANGE, WM_SIZE, WM_SIZING, WM_SYSCOMMAND,
    WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TOUCH, WM_WINDOWPOSCHANGED, WM_WINDOWPOSCHANGING,
    WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_OVERLAPPED, WS_POPUP, WS_VISIBLE,
};

use crate::dpi::{PhysicalPosition, PhysicalSize};
use crate::error::EventLoopError;
use crate::event::{
    DeviceEvent, Event, Force, Ime, InnerSizeWriter, RawKeyEvent, Touch, TouchPhase,
    WindowEvent,
};
use crate::event_loop::{
    ActiveEventLoop as RootAEL, ControlFlow, DeviceEvents, EventLoopClosed,
};
use crate::keyboard::ModifiersState;
use crate::platform::pump_events::PumpStatus;
use crate::platform_impl::platform::dark_mode::try_theme;
use crate::platform_impl::platform::dpi::{become_dpi_aware, dpi_to_scale_factor};
use crate::platform_impl::platform::drop_handler::FileDropHandler;
use crate::platform_impl::platform::icon::WinCursor;
use crate::platform_impl::platform::ime::ImeContext;
use crate::platform_impl::platform::keyboard::KeyEventBuilder;
use crate::platform_impl::platform::keyboard_layout::LAYOUT_CACHE;
use crate::platform_impl::platform::monitor::{self, MonitorHandle};
use crate::platform_impl::platform::window::InitData;
use crate::platform_impl::platform::window_state::{
    CursorFlags, ImeState, WindowFlags, WindowState,
};
use crate::platform_impl::platform::{
    raw_input, util, wrap_device_id, Fullscreen, WindowId, DEVICE_ID,
};
use crate::window::{
    CustomCursor as RootCustomCursor, CustomCursorSource, Theme, WindowId as RootWindowId,
};
use runner::{EventLoopRunner, EventLoopRunnerShared};

use super::window::set_skip_taskbar;
use super::SelectedCursor;

/// some backends like macos uses an uninhabited `Never` type,
/// on windows, `UserEvent`s are also dispatched through the
/// WNDPROC callback, and due to the re-entrant nature of the
/// callback, recursively delivered events must be queued in a
/// buffer, the current implementation put this queue in
/// `EventLoopRunner`, which is shared between the event pumping
/// loop and the callback. because it's hard to decide from the
/// outside whether a event needs to be buffered, I decided not
/// use `Event<Never>` for the shared runner state, but use unit
/// as a placeholder so user events can be buffered as usual,
/// the real `UserEvent` is pulled from the mpsc channel directly
/// when the placeholder event is delivered to the event handler
pub(crate) struct UserEventPlaceholder;

// here below, the generic `EventLoopRunnerShared<T>` is replaced with
// `EventLoopRunnerShared<UserEventPlaceholder>` so we can get rid
// of the generic parameter T in types which don't depend on T.
// this is the approach which requires minimum changes to current
// backend implementation. it should be considered transitional
// and should be refactored and cleaned up eventually, I hope.

pub(crate) struct WindowData {
    pub window_state: Arc<Mutex<WindowState>>,
    pub event_loop_runner: EventLoopRunnerShared<UserEventPlaceholder>,
    pub key_event_builder: KeyEventBuilder,
    pub _file_drop_handler: Option<FileDropHandler>,
    pub userdata_removed: Cell<bool>,
    pub recurse_depth: Cell<u32>,
}

impl WindowData {
    fn send_event(&self, event: Event<UserEventPlaceholder>) {
        self.event_loop_runner.send_event(event);
    }

    fn window_state_lock(&self) -> MutexGuard<'_, WindowState> {
        self.window_state.lock().unwrap()
    }
}

struct ThreadMsgTargetData {
    event_loop_runner: EventLoopRunnerShared<UserEventPlaceholder>,
}

impl ThreadMsgTargetData {
    fn send_event(&self, event: Event<UserEventPlaceholder>) {
        self.event_loop_runner.send_event(event);
    }
}

/// The result of a subclass procedure (the message handling callback)
#[derive(Clone, Copy)]
pub(crate) enum ProcResult {
    DefWindowProc(WPARAM),
    Value(isize),
}

pub struct EventLoop<T: 'static> {
    user_event_sender: Sender<T>,
    user_event_receiver: Receiver<T>,
    window_target: RootAEL,
    msg_hook: Option<Box<dyn FnMut(*const c_void) -> bool + 'static>>,
    // It is a timer used on timed waits.
    // It is created lazily in case if we have `ControlFlow::WaitUntil`.
    // Keep it as a field to avoid recreating it on every `ControlFlow::WaitUntil`.
    high_resolution_timer: Option<OwnedHandle>,
}

pub(crate) struct PlatformSpecificEventLoopAttributes {
    pub(crate) any_thread: bool,
    pub(crate) dpi_aware: bool,
    pub(crate) msg_hook: Option<Box<dyn FnMut(*const c_void) -> bool + 'static>>,
}

impl Default for PlatformSpecificEventLoopAttributes {
    fn default() -> Self {
        Self {
            any_thread: false,
            dpi_aware: true,
            msg_hook: None,
        }
    }
}

pub struct ActiveEventLoop {
    thread_id: u32,
    thread_msg_target: HWND,
    pub(crate) runner_shared: EventLoopRunnerShared<UserEventPlaceholder>,
}

impl<T: 'static> EventLoop<T> {
    pub(crate) fn new(
        attributes: &mut PlatformSpecificEventLoopAttributes,
    ) -> Result<Self, EventLoopError> {
        let thread_id = unsafe { GetCurrentThreadId() };

        if !attributes.any_thread && thread_id != main_thread_id() {
            panic!(
                "Initializing the event loop outside of the main thread is a significant \
                 cross-platform compatibility hazard. If you absolutely need to create an \
                 EventLoop on a different thread, you can use the \
                 `EventLoopBuilderExtWindows::any_thread` function."
            );
        }

        if attributes.dpi_aware {
            become_dpi_aware();
        }

        let thread_msg_target = create_event_target_window();

        let runner_shared = Rc::new(EventLoopRunner::new(thread_msg_target));

        let (user_event_sender, user_event_receiver) = mpsc::channel();
        insert_event_target_window_data(thread_msg_target, runner_shared.clone());
        raw_input::register_all_mice_and_keyboards_for_raw_input(
            thread_msg_target,
            Default::default(),
        );

        Ok(EventLoop {
            user_event_sender,
            user_event_receiver,
            window_target: RootAEL {
                p: ActiveEventLoop {
                    thread_id,
                    thread_msg_target,
                    runner_shared,
                },
                _marker: PhantomData,
            },
            msg_hook: attributes.msg_hook.take(),
            high_resolution_timer: None,
        })
    }

    pub fn window_target(&self) -> &RootAEL {
        &self.window_target
    }

    pub fn run<F>(mut self, event_handler: F) -> Result<(), EventLoopError>
    where
        F: FnMut(Event<T>, &RootAEL),
    {
        self.run_on_demand(event_handler)
    }

    pub fn run_on_demand<F>(&mut self, mut event_handler: F) -> Result<(), EventLoopError>
    where
        F: FnMut(Event<T>, &RootAEL),
    {
        {
            let runner = &self.window_target.p.runner_shared;

            let event_loop_windows_ref = &self.window_target;
            let user_event_receiver = &self.user_event_receiver;
            // # Safety
            // We make sure to call runner.clear_event_handler() before
            // returning
            unsafe {
                runner.set_event_handler(move |event| {
                    // the shared `EventLoopRunner` is not parameterized
                    // `EventLoopProxy::send_event()` calls `PostMessage`
                    // to wakeup and dispatch a placeholder `UserEvent`,
                    // when we received the placeholder event here, the
                    // real UserEvent(T) should already be put in the
                    // mpsc channel and ready to be pulled
                    let event = match event.map_nonuser_event() {
                        Ok(non_user_event) => non_user_event,
                        Err(_user_event_placeholder) => Event::UserEvent(
                            user_event_receiver
                                .try_recv()
                                .expect("user event signaled but not received"),
                        ),
                    };
                    event_handler(event, event_loop_windows_ref)
                });
            }
        }

        let exit_code = loop {
            self.wait_for_messages(None);
            // wait_for_messages calls user application before and after waiting
            // so it may have decided to exit.

            if let Some(code) = self.exit_code() {
                break code;
            }

            self.dispatch_peeked_messages();

            if let Some(code) = self.exit_code() {
                break code;
            }
        };

        let runner = &self.window_target.p.runner_shared;
        runner.loop_destroyed();

        // # Safety
        // We assume that this will effectively call `runner.clear_event_handler()`
        // to meet the safety requirements for calling `runner.set_event_handler()` above.
        runner.reset_runner();

        if exit_code == 0 {
            Ok(())
        } else {
            Err(EventLoopError::ExitFailure(exit_code))
        }
    }

    pub fn pump_events<F>(
        &mut self,
        timeout: Option<Duration>,
        mut event_handler: F,
    ) -> PumpStatus
    where
        F: FnMut(Event<T>, &RootAEL),
    {
        {
            let runner = &self.window_target.p.runner_shared;
            let event_loop_windows_ref = &self.window_target;
            let user_event_receiver = &self.user_event_receiver;

            // # Safety
            // We make sure to call runner.clear_event_handler() before
            // returning
            //
            // Note: we're currently assuming nothing can panic and unwind
            // to leave the runner in an unsound state with an associated
            // event handler.
            unsafe {
                runner.set_event_handler(move |event| {
                    let event = match event.map_nonuser_event() {
                        Ok(non_user_event) => non_user_event,
                        Err(_user_event_placeholder) => Event::UserEvent(
                            user_event_receiver
                                .recv()
                                .expect("user event signaled but not received"),
                        ),
                    };
                    event_handler(event, event_loop_windows_ref)
                });
                runner.wakeup();
            }
        }

        if self.exit_code().is_none() {
            self.wait_for_messages(timeout);
        }
        // wait_for_messages calls user application before and after waiting
        // so it may have decided to exit.

        if self.exit_code().is_none() {
            self.dispatch_peeked_messages();
        }

        let runner = &self.window_target.p.runner_shared;

        let status = if let Some(code) = runner.exit_code() {
            runner.loop_destroyed();

            // Immediately reset the internal state for the loop to allow
            // the loop to be run more than once.
            runner.reset_runner();
            PumpStatus::Exit(code)
        } else {
            runner.prepare_wait();
            PumpStatus::Continue
        };

        // We wait until we've checked for an exit status before clearing the
        // application callback, in case we need to dispatch a LoopExiting event
        //
        // # Safety
        // This pairs up with our call to `runner.set_event_handler` and ensures
        // the application's callback can't be held beyond its lifetime.
        runner.clear_event_handler();

        status
    }

    /// Waits until new event messages arrive to be peeked.
    /// Doesn't peek messages itself.
    ///
    /// Parameter timeout is optional. This method would wait for the smaller timeout
    /// between the argument and a timeout from control flow.
    fn wait_for_messages(&mut self, timeout: Option<Duration>) {
        let runner = &self.window_target.p.runner_shared;

        // We aim to be consistent with the MacOS backend which has a RunLoop
        // observer that will dispatch AboutToWait when about to wait for
        // events, and NewEvents after the RunLoop wakes up.
        //
        // We emulate similar behaviour by treating `MsgWaitForMultipleObjectsEx` as our wait
        // point and wake up point (when it returns) and we drain all other
        // pending messages via `PeekMessage` until we come back to "wait" via
        // `MsgWaitForMultipleObjectsEx`.
        //
        runner.prepare_wait();
        wait_for_messages_impl(
            &mut self.high_resolution_timer,
            runner.control_flow(),
            timeout,
        );
        // Before we potentially exit, make sure to consistently emit an event for the wake up
        runner.wakeup();
    }

    /// Dispatch all queued messages via `PeekMessageW`
    fn dispatch_peeked_messages(&mut self) {
        let runner = &self.window_target.p.runner_shared;

        // We generally want to continue dispatching all pending messages
        // but we also allow dispatching to be interrupted as a means to
        // ensure the `pump_events` won't indefinitely block an external
        // event loop if there are too many pending events. This interrupt
        // flag will be set after dispatching `RedrawRequested` events.
        runner.interrupt_msg_dispatch.set(false);

        // # Safety
        // The Windows API has no documented requirement for bitwise
        // initializing a `MSG` struct (it can be uninitialized memory for the C
        // API) and there's no API to construct or initialize a `MSG`. This
        // is the simplest way avoid uninitialized memory in Rust
        let mut msg: MSG = unsafe { mem::zeroed() };

        loop {
            unsafe {
                if PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE)
                    == false.into()
                {
                    break;
                }

                let handled = if let Some(callback) = self.msg_hook.as_deref_mut() {
                    callback(&mut msg as *mut _ as *mut _)
                } else {
                    false
                };
                if !handled {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }

            if let Err(payload) = runner.take_panic_error() {
                runner.reset_runner();
                panic::resume_unwind(payload);
            }

            if let Some(_code) = runner.exit_code() {
                break;
            }

            if runner.interrupt_msg_dispatch.get() {
                break;
            }
        }
    }

    pub fn create_proxy(&self) -> EventLoopProxy<T> {
        EventLoopProxy {
            target_window: self.window_target.p.thread_msg_target,
            event_send: self.user_event_sender.clone(),
        }
    }

    fn exit_code(&self) -> Option<i32> {
        self.window_target.p.exit_code()
    }
}

impl<T> Drop for EventLoop<T> {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.window_target.p.thread_msg_target);
        }
    }
}

impl ActiveEventLoop {
    #[inline(always)]
    pub(crate) fn create_thread_executor(&self) -> EventLoopThreadExecutor {
        EventLoopThreadExecutor {
            thread_id: self.thread_id,
            target_window: self.thread_msg_target,
        }
    }

    pub fn create_custom_cursor(&self, source: CustomCursorSource) -> RootCustomCursor {
        let inner = match WinCursor::new(&source.inner.0) {
            Ok(cursor) => cursor,
            Err(err) => {
                tracing::warn!("Failed to create custom cursor: {err}");
                WinCursor::Failed
            }
        };

        RootCustomCursor { inner }
    }

    // TODO: Investigate opportunities for caching
    pub fn available_monitors(&self) -> VecDeque<MonitorHandle> {
        monitor::available_monitors()
    }

    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        let monitor = monitor::primary_monitor();
        Some(monitor)
    }

    pub fn raw_display_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawDisplayHandle, raw_window_handle::HandleError> {
        Ok(raw_window_handle::RawDisplayHandle::Windows(
            raw_window_handle::WindowsDisplayHandle::new(),
        ))
    }

    pub fn listen_device_events(&self, allowed: DeviceEvents) {
        raw_input::register_all_mice_and_keyboards_for_raw_input(
            self.thread_msg_target,
            allowed,
        );
    }

    pub(crate) fn set_control_flow(&self, control_flow: ControlFlow) {
        self.runner_shared.set_control_flow(control_flow)
    }

    pub(crate) fn control_flow(&self) -> ControlFlow {
        self.runner_shared.control_flow()
    }

    pub(crate) fn exit(&self) {
        self.runner_shared.set_exit_code(0)
    }

    pub(crate) fn system_theme(&self) -> Option<Theme> {
        None
    }

    pub(crate) fn exiting(&self) -> bool {
        self.runner_shared.exit_code().is_some()
    }

    pub(crate) fn clear_exit(&self) {
        self.runner_shared.clear_exit();
    }

    pub(crate) fn owned_display_handle(&self) -> OwnedDisplayHandle {
        OwnedDisplayHandle
    }

    fn exit_code(&self) -> Option<i32> {
        self.runner_shared.exit_code()
    }
}

#[derive(Clone)]
pub(crate) struct OwnedDisplayHandle;

impl OwnedDisplayHandle {
    #[inline]
    pub fn raw_display_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawDisplayHandle, raw_window_handle::HandleError> {
        Ok(raw_window_handle::WindowsDisplayHandle::new().into())
    }
}

/// Returns the id of the main thread.
///
/// Windows has no real API to check if the current executing thread is the "main thread", unlike
/// macOS.
///
/// Windows will let us look up the current thread's id, but there's no API that lets us check what
/// the id of the main thread is. We would somehow need to get the main thread's id before a
/// developer could spin off any other threads inside of the main entrypoint in order to emulate the
/// capabilities of other platforms.
///
/// We can get the id of the main thread by using CRT initialization. CRT initialization can be used
/// to setup global state within a program. The OS will call a list of function pointers which
/// assign values to a static variable. To have get a hold of the main thread id, we need to place
/// our function pointer inside of the `.CRT$XCU` section so it is called before the main
/// entrypoint.
///
/// Full details of CRT initialization can be found here:
/// <https://docs.microsoft.com/en-us/cpp/c-runtime-library/crt-initialization?view=msvc-160>
fn main_thread_id() -> u32 {
    static mut MAIN_THREAD_ID: u32 = 0;

    // Function pointer used in CRT initialization section to set the above static field's value.
    // Mark as used so this is not removable.
    #[used]
    #[allow(non_upper_case_globals)]
    // Place the function pointer inside of CRT initialization section so it is loaded before
    // main entrypoint.
    //
    // See: https://doc.rust-lang.org/stable/reference/abi.html#the-link_section-attribute
    #[link_section = ".CRT$XCU"]
    static INIT_MAIN_THREAD_ID: unsafe fn() = {
        unsafe fn initer() {
            unsafe { MAIN_THREAD_ID = GetCurrentThreadId() };
        }
        initer
    };

    unsafe { MAIN_THREAD_ID }
}

/// Returns the minimum `Option<Duration>`, taking into account that `None`
/// equates to an infinite timeout, not a zero timeout (so can't just use
/// `Option::min`)
fn min_timeout(a: Option<Duration>, b: Option<Duration>) -> Option<Duration> {
    a.map_or(b, |a_timeout| {
        b.map_or(Some(a_timeout), |b_timeout| Some(a_timeout.min(b_timeout)))
    })
}

// Implementation taken from https://github.com/rust-lang/rust/blob/db5476571d9b27c862b95c1e64764b0ac8980e23/src/libstd/sys/windows/mod.rs
fn dur2timeout(dur: Duration) -> u32 {
    // Note that a duration is a (u64, u32) (seconds, nanoseconds) pair, and the
    // timeouts in windows APIs are typically u32 milliseconds. To translate, we
    // have two pieces to take care of:
    //
    // * Nanosecond precision is rounded up
    // * Greater than u32::MAX milliseconds (50 days) is rounded up to INFINITE (never time out).
    dur.as_secs()
        .checked_mul(1000)
        .and_then(|ms| ms.checked_add((dur.subsec_nanos() as u64) / 1_000_000))
        .and_then(|ms| {
            if dur.subsec_nanos() % 1_000_000 > 0 {
                ms.checked_add(1)
            } else {
                Some(ms)
            }
        })
        .map(|ms| {
            if ms > u32::MAX as u64 {
                INFINITE
            } else {
                ms as u32
            }
        })
        .unwrap_or(INFINITE)
}

/// Set upper limit for waiting time to avoid overflows.
/// I chose 50 days as a limit because it is used in dur2timeout.
const FIFTY_DAYS: Duration = Duration::from_secs(50_u64 * 24 * 60 * 60);
/// Waitable timers use 100 ns intervals to indicate due time.
/// <https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-setwaitabletimer#parameters>
/// And there is no point waiting using other ways for such small timings
/// because they are even less precise (can overshoot by few ms).
const MIN_WAIT: Duration = Duration::from_nanos(100);

fn create_high_resolution_timer() -> Option<OwnedHandle> {
    unsafe {
        let handle: HANDLE = CreateWaitableTimerExW(
            ptr::null(),
            ptr::null(),
            CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
            TIMER_ALL_ACCESS,
        );
        // CREATE_WAITABLE_TIMER_HIGH_RESOLUTION is supported only after
        // Win10 1803 but it is already default option for rustc
        // (std uses it to implement `std::thread::sleep`).
        if handle.is_null() {
            None
        } else {
            Some(OwnedHandle::from_raw_handle(handle))
        }
    }
}

/// This function should not return error if parameters are valid
/// but there is no guarantee about that at MSDN docs
/// so we return result of GetLastError if fail.
///
/// ## Safety
///
/// timer must be a valid timer handle created by [create_high_resolution_timer].
/// timeout divided by 100 nanoseconds must be more than 0 and less than i64::MAX.
unsafe fn set_high_resolution_timer(
    timer: RawHandle,
    timeout: Duration,
) -> Result<(), u32> {
    const INTERVAL_NS: u32 = MIN_WAIT.subsec_nanos();
    const INTERVALS_IN_SEC: u64 =
        (Duration::from_secs(1).as_nanos() / INTERVAL_NS as u128) as u64;
    let intervals_to_wait: u64 = timeout.as_secs() * INTERVALS_IN_SEC
        + u64::from(timeout.subsec_nanos() / INTERVAL_NS);
    debug_assert!(
        intervals_to_wait < i64::MAX as u64,
        "Must be called with smaller duration",
    );
    // Use negative time to indicate relative time.
    let due_time: i64 = -(intervals_to_wait as i64);
    unsafe {
        let set_result =
            SetWaitableTimer(timer as HANDLE, &due_time, 0, None, ptr::null(), FALSE);
        if set_result != FALSE {
            Ok(())
        } else {
            Err(GetLastError())
        }
    }
}

/// Implementation detail of [EventLoop::wait_for_messages].
///
/// Does actual system-level waiting and doesn't process any messages itself,
/// including winits internal notifications about waiting and new messages arrival.
fn wait_for_messages_impl(
    high_resolution_timer: &mut Option<OwnedHandle>,
    control_flow: ControlFlow,
    timeout: Option<Duration>,
) {
    let timeout = {
        let control_flow_timeout = match control_flow {
            ControlFlow::Wait => None,
            ControlFlow::Poll => Some(Duration::ZERO),
            ControlFlow::WaitUntil(wait_deadline) => {
                let start = Instant::now();
                Some(wait_deadline.saturating_duration_since(start))
            }
        };
        let timeout = min_timeout(timeout, control_flow_timeout);
        if timeout == Some(Duration::ZERO) {
            // Do not wait if we don't have time.
            return;
        }
        // Now we decided to wait so need to do some clamping
        // to avoid problems with overflow and calling WinAPI with invalid parameters.
        timeout
            .map(|t| t.min(FIFTY_DAYS))
            // If timeout is less than minimally supported by Windows,
            // increase it to that minimum. Who want less than microsecond delays anyway?
            .map(|t| t.max(MIN_WAIT))
    };

    if timeout.is_some() && high_resolution_timer.is_none() {
        *high_resolution_timer = create_high_resolution_timer();
    }

    let high_resolution_timer: Option<RawHandle> = high_resolution_timer
        .as_ref()
        .map(OwnedHandle::as_raw_handle);

    let use_timer: bool;
    if let (Some(handle), Some(timeout)) = (high_resolution_timer, timeout) {
        let res = unsafe {
            // Safety: handle can be Some only if we succeeded in creating high resolution
            // timer. We properly clamped timeout so it can be used as argument
            // to timer.
            set_high_resolution_timer(handle, timeout)
        };
        if let Err(error_code) = res {
            // We successfully got timer but failed to set it?
            // Should be some bug in our code.
            tracing::trace!(
                "Failed to set high resolution timer: last error {}",
                error_code
            );
            use_timer = false;
        } else {
            use_timer = true;
        }
    } else {
        use_timer = false;
    }

    unsafe {
        // Either:
        //  1. User wants to wait indefinely if timeout is not set.
        //  2. We failed to get and set high resolution timer and we need something instead of it.
        let wait_duration_ms = timeout.map(dur2timeout).unwrap_or(INFINITE);

        let (num_handles, raw_handles) = if use_timer {
            (1, [high_resolution_timer.unwrap()])
        } else {
            (0, [ptr::null_mut()])
        };

        // We must use `QS_ALLINPUT` to wake on accessibility messages.
        let result = MsgWaitForMultipleObjectsEx(
            num_handles,
            raw_handles.as_ptr() as *const _,
            wait_duration_ms,
            QS_ALLINPUT,
            MWMO_INPUTAVAILABLE,
        );
        if result == WAIT_FAILED {
            // Well, nothing smart to do in such case.
            // Treat it as spurious wake up.
            tracing::warn!(
                "Failed to MsgWaitForMultipleObjectsEx: error code {}",
                GetLastError(),
            );
        }
    }
}

pub(crate) struct EventLoopThreadExecutor {
    thread_id: u32,
    target_window: HWND,
}

unsafe impl Send for EventLoopThreadExecutor {}
unsafe impl Sync for EventLoopThreadExecutor {}

impl EventLoopThreadExecutor {
    /// Check to see if we're in the parent event loop's thread.
    pub(super) fn in_event_loop_thread(&self) -> bool {
        let cur_thread_id = unsafe { GetCurrentThreadId() };
        self.thread_id == cur_thread_id
    }

    /// Executes a function in the event loop thread. If we're already in the event loop thread,
    /// we just call the function directly.
    ///
    /// The `Inserted` can be used to inject a `WindowState` for the callback to use. The state is
    /// removed automatically if the callback receives a `WM_CLOSE` message for the window.
    ///
    /// Note that if you are using this to change some property of a window and updating
    /// `WindowState` then you should call this within the lock of `WindowState`. Otherwise the
    /// events may be sent to the other thread in different order to the one in which you set
    /// `WindowState`, leaving them out of sync.
    ///
    /// Note that we use a FnMut instead of a FnOnce because we're too lazy to create an equivalent
    /// to the unstable FnBox.
    pub(super) fn execute_in_thread<F>(&self, mut function: F)
    where
        F: FnMut() + Send + 'static,
    {
        unsafe {
            if self.in_event_loop_thread() {
                function();
            } else {
                // We double-box because the first box is a fat pointer.
                let boxed2: ThreadExecFn = Box::new(Box::new(function));

                let raw = Box::into_raw(boxed2);

                let res =
                    PostMessageW(self.target_window, EXEC_MSG_ID.get(), raw as usize, 0);
                assert!(
                    res != false.into(),
                    "PostMessage failed; is the messages queue full?"
                );
            }
        }
    }
}

type ThreadExecFn = Box<Box<dyn FnMut()>>;

pub struct EventLoopProxy<T: 'static> {
    target_window: HWND,
    event_send: Sender<T>,
}
unsafe impl<T: Send + 'static> Send for EventLoopProxy<T> {}
unsafe impl<T: Sync + 'static> Sync for EventLoopProxy<T> {}

impl<T: 'static> Clone for EventLoopProxy<T> {
    fn clone(&self) -> Self {
        Self {
            target_window: self.target_window,
            event_send: self.event_send.clone(),
        }
    }
}

impl<T: 'static> EventLoopProxy<T> {
    pub fn send_event(&self, event: T) -> Result<(), EventLoopClosed<T>> {
        self.event_send
            .send(event)
            .inspect(|_result| {
                unsafe {
                    PostMessageW(self.target_window, USER_EVENT_MSG_ID.get(), 0, 0)
                };
            })
            .map_err(|e| EventLoopClosed(e.0))
    }
}

/// A lazily-initialized window message ID.
pub struct LazyMessageId {
    /// The ID.
    id: AtomicU32,

    /// The name of the message.
    name: &'static str,
}

/// An invalid custom window ID.
const INVALID_ID: u32 = 0x0;

impl LazyMessageId {
    /// Create a new `LazyId`.
    const fn new(name: &'static str) -> Self {
        Self {
            id: AtomicU32::new(INVALID_ID),
            name,
        }
    }

    /// Get the message ID.
    pub fn get(&self) -> u32 {
        // Load the ID.
        let id = self.id.load(Ordering::Relaxed);

        if id != INVALID_ID {
            return id;
        }

        // Register the message.
        // SAFETY: We are sure that the pointer is a valid C string ending with '\0'.
        assert!(self.name.ends_with('\0'));
        let new_id = unsafe { RegisterWindowMessageA(self.name.as_ptr()) };

        assert_ne!(
            new_id,
            0,
            "RegisterWindowMessageA returned zero for '{}': {}",
            self.name,
            std::io::Error::last_os_error()
        );

        // Store the new ID. Since `RegisterWindowMessageA` returns the same value for any given
        // string, the target value will always either be a). `INVALID_ID` or b). the
        // correct ID. Therefore a compare-and-swap operation here (or really any
        // consideration) is never necessary.
        self.id.store(new_id, Ordering::Relaxed);

        new_id
    }
}

// Message sent by the `EventLoopProxy` when we want to wake up the thread.
// WPARAM and LPARAM are unused.
static USER_EVENT_MSG_ID: LazyMessageId = LazyMessageId::new("Winit::WakeupMsg\0");
// Message sent when we want to execute a closure in the thread.
// WPARAM contains a Box<Box<dyn FnMut()>> that must be retrieved with `Box::from_raw`,
// and LPARAM is unused.
static EXEC_MSG_ID: LazyMessageId = LazyMessageId::new("Winit::ExecMsg\0");
// Message sent by a `Window` when it wants to be destroyed by the main thread.
// WPARAM and LPARAM are unused.
pub(crate) static DESTROY_MSG_ID: LazyMessageId =
    LazyMessageId::new("Winit::DestroyMsg\0");
// WPARAM is a bool specifying the `WindowFlags::MARKER_RETAIN_STATE_ON_SIZE` flag. See the
// documentation in the `window_state` module for more information.
pub(crate) static SET_RETAIN_STATE_ON_SIZE_MSG_ID: LazyMessageId =
    LazyMessageId::new("Winit::SetRetainMaximized\0");
static THREAD_EVENT_TARGET_WINDOW_CLASS: Lazy<Vec<u16>> =
    Lazy::new(|| util::encode_wide("Winit Thread Event Target"));
/// When the taskbar is created, it registers a message with the "TaskbarCreated" string and then
/// broadcasts this message to all top-level windows <https://docs.microsoft.com/en-us/windows/win32/shell/taskbar#taskbar-creation-notification>
pub(crate) static TASKBAR_CREATED: LazyMessageId = LazyMessageId::new("TaskbarCreated\0");

fn create_event_target_window() -> HWND {
    use windows_sys::Win32::UI::WindowsAndMessaging::{CS_HREDRAW, CS_VREDRAW};
    unsafe {
        let class = WNDCLASSEXW {
            cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(thread_event_target_callback),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: util::get_instance_handle(),
            hIcon: ptr::null_mut(),
            hCursor: ptr::null_mut(), // must be null in order for cursor state to work properly
            hbrBackground: ptr::null_mut(),
            lpszMenuName: ptr::null(),
            lpszClassName: THREAD_EVENT_TARGET_WINDOW_CLASS.as_ptr(),
            hIconSm: ptr::null_mut(),
        };

        RegisterClassExW(&class);
    }

    unsafe {
        // WS_EX_TOOLWINDOW prevents this window from ever showing up in the taskbar, which
        // we want to avoid. If you remove this style, this window won't show up in the
        // taskbar *initially*, but it can show up at some later point. This can sometimes
        // happen on its own after several hours have passed, although this has proven
        // difficult to reproduce. Alternatively, it can be manually triggered by killing
        // `explorer.exe` and then starting the process back up.
        // It is unclear why the bug is triggered by waiting for several hours.
        let window = CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
            THREAD_EVENT_TARGET_WINDOW_CLASS.as_ptr(),
            ptr::null(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            util::get_instance_handle(),
            ptr::null(),
        );

        super::set_window_long(
            window,
            GWL_STYLE,
            // The window technically has to be visible to receive WM_PAINT messages (which are
            // used for delivering events during resizes), but it isn't displayed to
            // the user because of the LAYERED style.
            (WS_VISIBLE | WS_POPUP) as isize,
        );
        window
    }
}

fn insert_event_target_window_data(
    thread_msg_target: HWND,
    event_loop_runner: EventLoopRunnerShared<UserEventPlaceholder>,
) {
    let userdata = ThreadMsgTargetData { event_loop_runner };
    let input_ptr = Box::into_raw(Box::new(userdata));

    unsafe {
        super::set_window_long(thread_msg_target, GWL_USERDATA, input_ptr as isize)
    };
}

/// Capture mouse input, allowing `window` to receive mouse events when the cursor is outside of
/// the window.
unsafe fn capture_mouse(window: HWND, window_state: &mut WindowState) {
    window_state.mouse.capture_count += 1;
    unsafe { SetCapture(window) };
}

/// Release mouse input, stopping windows on this thread from receiving mouse input when the cursor
/// is outside the window.
unsafe fn release_mouse(mut window_state: MutexGuard<'_, WindowState>) {
    window_state.mouse.capture_count = window_state.mouse.capture_count.saturating_sub(1);
    if window_state.mouse.capture_count == 0 {
        // ReleaseCapture() causes a WM_CAPTURECHANGED where we lock the window_state.
        drop(window_state);
        unsafe { ReleaseCapture() };
    }
}

fn normalize_pointer_pressure(pressure: u32) -> Option<Force> {
    match pressure {
        1..=1024 => Some(Force::Normalized(pressure as f64 / 1024.0)),
        _ => None,
    }
}

/// Emit a `ModifiersChanged` event whenever modifiers have changed.
/// Returns the current modifier state
fn update_modifiers(window: HWND, userdata: &WindowData) {
    use crate::event::WindowEvent::ModifiersChanged;

    let modifiers = {
        let mut layouts = LAYOUT_CACHE.lock().unwrap();
        layouts.get_agnostic_mods()
    };

    let mut window_state = userdata.window_state.lock().unwrap();
    if window_state.modifiers_state != modifiers {
        window_state.modifiers_state = modifiers;

        // Drop lock
        drop(window_state);

        userdata.send_event(Event::WindowEvent {
            window_id: RootWindowId(WindowId(window)),
            event: ModifiersChanged(modifiers.into()),
        });
    }
}

unsafe fn gain_active_focus(window: HWND, userdata: &WindowData) {
    use crate::event::WindowEvent::Focused;

    update_modifiers(window, userdata);

    userdata.send_event(Event::WindowEvent {
        window_id: RootWindowId(WindowId(window)),
        event: Focused(true),
    });
}

unsafe fn lose_active_focus(window: HWND, userdata: &WindowData) {
    use crate::event::WindowEvent::{Focused, ModifiersChanged};

    userdata.window_state_lock().modifiers_state = ModifiersState::empty();
    userdata.send_event(Event::WindowEvent {
        window_id: RootWindowId(WindowId(window)),
        event: ModifiersChanged(ModifiersState::empty().into()),
    });

    userdata.send_event(Event::WindowEvent {
        window_id: RootWindowId(WindowId(window)),
        event: Focused(false),
    });
}

/// Any window whose callback is configured to this function will have its events propagated
/// through the events loop of the thread the window was created in.
// This is the callback that is called by `DispatchMessage` in the events loop.
//
// Returning 0 tells the Win32 API that the message has been processed.
// FIXME: detect WM_DWMCOMPOSITIONCHANGED and call DwmEnableBlurBehindWindow if necessary
pub(super) unsafe extern "system" fn public_window_callback(
    window: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let userdata = unsafe { super::get_window_long(window, GWL_USERDATA) };

    let userdata_ptr = match (userdata, msg) {
        (0, WM_NCCREATE) => {
            let createstruct = unsafe { &mut *(lparam as *mut CREATESTRUCTW) };
            let initdata =
                unsafe { &mut *(createstruct.lpCreateParams as *mut InitData<'_>) };

            let result = match unsafe { initdata.on_nccreate(window) } {
                Some(userdata) => unsafe {
                    super::set_window_long(window, GWL_USERDATA, userdata as _);
                    DefWindowProcW(window, msg, wparam, lparam)
                },
                None => -1, // failed to create the window
            };

            return result;
        }
        // Getting here should quite frankly be impossible,
        // but we'll make window creation fail here just in case.
        (0, WM_CREATE) => return -1,
        (_, WM_CREATE) => unsafe {
            let createstruct = &mut *(lparam as *mut CREATESTRUCTW);
            let initdata = createstruct.lpCreateParams;
            let initdata = &mut *(initdata as *mut InitData<'_>);

            initdata.on_create();
            return DefWindowProcW(window, msg, wparam, lparam);
        },
        (0, _) => return unsafe { DefWindowProcW(window, msg, wparam, lparam) },
        _ => userdata as *mut WindowData,
    };

    let (result, userdata_removed, recurse_depth) = {
        let userdata = unsafe { &*(userdata_ptr) };

        userdata.recurse_depth.set(userdata.recurse_depth.get() + 1);

        let result = unsafe {
            public_window_callback_inner(window, msg, wparam, lparam, userdata)
        };

        let userdata_removed = userdata.userdata_removed.get();
        let recurse_depth = userdata.recurse_depth.get() - 1;
        userdata.recurse_depth.set(recurse_depth);

        (result, userdata_removed, recurse_depth)
    };

    if userdata_removed && recurse_depth == 0 {
        drop(unsafe { Box::from_raw(userdata_ptr) });
    }

    result
}

unsafe fn public_window_callback_inner(
    window: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    userdata: &WindowData,
) -> LRESULT {
    let mut result = ProcResult::DefWindowProc(wparam);

    // Send new modifiers before sending key events.
    let mods_changed_callback = || match msg {
        WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP => {
            update_modifiers(window, userdata);
            result = ProcResult::Value(0);
        }
        _ => (),
    };
    userdata
        .event_loop_runner
        .catch_unwind(mods_changed_callback)
        .unwrap_or_else(|| result = ProcResult::Value(-1));

    let keyboard_callback = || {
        use crate::event::WindowEvent::KeyboardInput;
        let events = userdata.key_event_builder.process_message(
            window,
            msg,
            wparam,
            lparam,
            &mut result,
        );
        for event in events {
            userdata.send_event(Event::WindowEvent {
                window_id: RootWindowId(WindowId(window)),
                event: KeyboardInput {
                    device_id: DEVICE_ID,
                    event: event.event,
                    is_synthetic: event.is_synthetic,
                },
            });
        }
    };
    userdata
        .event_loop_runner
        .catch_unwind(keyboard_callback)
        .unwrap_or_else(|| result = ProcResult::Value(-1));

    // I decided to bind the closure to `callback` and pass it to catch_unwind rather than passing
    // the closure to catch_unwind directly so that the match body indentation wouldn't change and
    // the git blame and history would be preserved.
    let callback = || match msg {
        WM_NCCALCSIZE => {
            let window_flags = userdata.window_state_lock().window_flags;
            if wparam == 0 || window_flags.contains(WindowFlags::MARKER_DECORATIONS) {
                result = ProcResult::DefWindowProc(wparam);
                return;
            }

            let params = unsafe { &mut *(lparam as *mut NCCALCSIZE_PARAMS) };

            if util::is_maximized(window) {
                // Limit the window size when maximized to the current monitor.
                // Otherwise it would include the non-existent decorations.
                //
                // Use `MonitorFromRect` instead of `MonitorFromWindow` to select
                // the correct monitor here.
                // See https://github.com/MicrosoftEdge/WebView2Feedback/issues/2549
                let monitor =
                    unsafe { MonitorFromRect(&params.rgrc[0], MONITOR_DEFAULTTONULL) };
                if let Ok(monitor_info) = monitor::get_monitor_info(monitor) {
                    params.rgrc[0] = monitor_info.monitorInfo.rcWork;
                }
            } else if window_flags.contains(WindowFlags::MARKER_UNDECORATED_SHADOW) {
                // Extend the client area to cover the whole non-client area.
                // https://docs.microsoft.com/en-us/windows/win32/winmsg/wm-nccalcsize#remarks
                //
                // HACK(msiglreith): To add the drop shadow we slightly tweak the non-client area.
                // This leads to a small black 1px border on the top. Adding a margin manually
                // on all 4 borders would result in the caption getting drawn by the DWM.
                //
                // Another option would be to allow the DWM to paint inside the client area.
                // Unfortunately this results in janky resize behavior, where the compositor is
                // ahead of the window surface. Currently, there seems no option to achieve this
                // with the Windows API.
                params.rgrc[0].top += 1;
                params.rgrc[0].bottom += 1;
            }

            result = ProcResult::Value(0);
        }

        WM_ENTERSIZEMOVE => {
            userdata.window_state_lock().set_window_flags_in_place(|f| {
                f.insert(WindowFlags::MARKER_IN_SIZE_MOVE)
            });
            result = ProcResult::Value(0);
        }

        WM_EXITSIZEMOVE => {
            let mut state = userdata.window_state_lock();
            if state.dragging {
                state.dragging = false;
                unsafe { PostMessageW(window, WM_LBUTTONUP, 0, lparam) };
            }

            state.set_window_flags_in_place(|f| {
                f.remove(WindowFlags::MARKER_IN_SIZE_MOVE)
            });
            result = ProcResult::Value(0);
        }

        WM_NCLBUTTONDOWN => {
            if wparam == HTCAPTION as _ {
                unsafe { PostMessageW(window, WM_MOUSEMOVE, 0, lparam) };
            }
            result = ProcResult::DefWindowProc(wparam);
        }

        WM_CLOSE => {
            use crate::event::WindowEvent::CloseRequested;
            userdata.send_event(Event::WindowEvent {
                window_id: RootWindowId(WindowId(window)),
                event: CloseRequested,
            });
            result = ProcResult::Value(0);
        }

        WM_DESTROY => {
            use crate::event::WindowEvent::Destroyed;
            unsafe { RevokeDragDrop(window) };
            userdata.send_event(Event::WindowEvent {
                window_id: RootWindowId(WindowId(window)),
                event: Destroyed,
            });
            result = ProcResult::Value(0);
        }

        WM_NCDESTROY => {
            unsafe { super::set_window_long(window, GWL_USERDATA, 0) };
            userdata.userdata_removed.set(true);
            result = ProcResult::Value(0);
        }

        WM_PAINT => {
            userdata.window_state_lock().redraw_requested =
                userdata.event_loop_runner.should_buffer();

            // We'll buffer only in response to `UpdateWindow`, if win32 decides to redraw the
            // window outside the normal flow of the event loop. This way mark event as handled
            // and request a normal redraw with `RedrawWindow`.
            if !userdata.event_loop_runner.should_buffer() {
                userdata.send_event(Event::WindowEvent {
                    window_id: RootWindowId(WindowId(window)),
                    event: WindowEvent::RedrawRequested,
                });
            }

            // NOTE: calling `RedrawWindow` during `WM_PAINT` does nothing, since to mark
            // `WM_PAINT` as handled we should call the `DefWindowProcW`. Call it and check whether
            // user asked for redraw during `RedrawRequested` event handling and request it again
            // after marking `WM_PAINT` as handled.
            result =
                ProcResult::Value(unsafe { DefWindowProcW(window, msg, wparam, lparam) });
            if std::mem::take(&mut userdata.window_state_lock().redraw_requested) {
                unsafe {
                    RedrawWindow(window, ptr::null(), ptr::null_mut(), RDW_INTERNALPAINT)
                };
            }
        }
        WM_WINDOWPOSCHANGING => {
            let mut window_state = userdata.window_state_lock();
            if let Some(ref mut fullscreen) = window_state.fullscreen {
                let window_pos = unsafe { &mut *(lparam as *mut WINDOWPOS) };
                let new_rect = RECT {
                    left: window_pos.x,
                    top: window_pos.y,
                    right: window_pos.x + window_pos.cx,
                    bottom: window_pos.y + window_pos.cy,
                };

                const NOMOVE_OR_NOSIZE: u32 = SWP_NOMOVE | SWP_NOSIZE;

                let new_rect = if window_pos.flags & NOMOVE_OR_NOSIZE != 0 {
                    let cur_rect = util::WindowArea::Outer.get_rect(window).expect(
                        "Unexpected GetWindowRect failure; please report this error to \
                         rust-windowing/winit",
                    );

                    match window_pos.flags & NOMOVE_OR_NOSIZE {
                        NOMOVE_OR_NOSIZE => None,

                        SWP_NOMOVE => Some(RECT {
                            left: cur_rect.left,
                            top: cur_rect.top,
                            right: cur_rect.left + window_pos.cx,
                            bottom: cur_rect.top + window_pos.cy,
                        }),

                        SWP_NOSIZE => Some(RECT {
                            left: window_pos.x,
                            top: window_pos.y,
                            right: window_pos.x - cur_rect.left + cur_rect.right,
                            bottom: window_pos.y - cur_rect.top + cur_rect.bottom,
                        }),

                        _ => unreachable!(),
                    }
                } else {
                    Some(new_rect)
                };

                if let Some(new_rect) = new_rect {
                    let new_monitor =
                        unsafe { MonitorFromRect(&new_rect, MONITOR_DEFAULTTONULL) };
                    match fullscreen {
                        Fullscreen::Borderless(ref mut fullscreen_monitor) => {
                            if !new_monitor.is_null()
                                && fullscreen_monitor
                                    .as_ref()
                                    .map(|monitor| new_monitor != monitor.hmonitor())
                                    .unwrap_or(true)
                            {
                                if let Ok(new_monitor_info) =
                                    monitor::get_monitor_info(new_monitor)
                                {
                                    let new_monitor_rect =
                                        new_monitor_info.monitorInfo.rcMonitor;
                                    window_pos.x = new_monitor_rect.left;
                                    window_pos.y = new_monitor_rect.top;
                                    window_pos.cx =
                                        new_monitor_rect.right - new_monitor_rect.left;
                                    window_pos.cy =
                                        new_monitor_rect.bottom - new_monitor_rect.top;
                                }
                                *fullscreen_monitor =
                                    Some(MonitorHandle::new(new_monitor));
                            }
                        }
                        Fullscreen::Exclusive(ref video_mode) => {
                            let old_monitor = video_mode.monitor.hmonitor();
                            if let Ok(old_monitor_info) =
                                monitor::get_monitor_info(old_monitor)
                            {
                                let old_monitor_rect =
                                    old_monitor_info.monitorInfo.rcMonitor;
                                window_pos.x = old_monitor_rect.left;
                                window_pos.y = old_monitor_rect.top;
                                window_pos.cx =
                                    old_monitor_rect.right - old_monitor_rect.left;
                                window_pos.cy =
                                    old_monitor_rect.bottom - old_monitor_rect.top;
                            }
                        }
                    }
                }
            }

            result = ProcResult::Value(0);
        }

        // WM_MOVE supplies client area positions, so we send Moved here instead.
        WM_WINDOWPOSCHANGED => {
            use crate::event::WindowEvent::Moved;

            let windowpos = lparam as *const WINDOWPOS;
            if unsafe { (*windowpos).flags & SWP_NOMOVE != SWP_NOMOVE } {
                let physical_position =
                    unsafe { PhysicalPosition::new((*windowpos).x, (*windowpos).y) };
                userdata.send_event(Event::WindowEvent {
                    window_id: RootWindowId(WindowId(window)),
                    event: Moved(physical_position),
                });
            }

            // This is necessary for us to still get sent WM_SIZE.
            result = ProcResult::DefWindowProc(wparam);
        }

        WM_SIZE => {
            use crate::event::WindowEvent::Resized;
            let w = super::loword(lparam as u32) as u32;
            let h = super::hiword(lparam as u32) as u32;

            let physical_size = PhysicalSize::new(w, h);
            let event = Event::WindowEvent {
                window_id: RootWindowId(WindowId(window)),
                event: Resized(physical_size),
            };

            {
                let mut w = userdata.window_state_lock();
                // See WindowFlags::MARKER_RETAIN_STATE_ON_SIZE docs for info on why this `if` check
                // exists.
                if !w
                    .window_flags()
                    .contains(WindowFlags::MARKER_RETAIN_STATE_ON_SIZE)
                {
                    let maximized = wparam == SIZE_MAXIMIZED as usize;
                    w.set_window_flags_in_place(|f| {
                        f.set(WindowFlags::MAXIMIZED, maximized)
                    });
                }
            }
            userdata.send_event(event);
            result = ProcResult::Value(0);
        }

        WM_SIZING => {
            /// Calculate the amount to add to round `value` to the nearest multiple of `increment`.
            fn snap_to_nearest_increment_delta(value: i32, increment: i32) -> i32 {
                let half_one = increment / 2;
                let half_two = increment - half_one;
                half_one - (value - half_two) % increment
            }

            let scale_factor = userdata.window_state_lock().scale_factor;
            let Some(inc) = userdata
                .window_state_lock()
                .resize_increments
                .map(|inc| inc.to_physical(scale_factor))
                .filter(|inc| inc.width > 0 && inc.height > 0)
            else {
                result = ProcResult::Value(0);
                return;
            };

            let side = wparam as u32;
            // The desired new size of the window, decorations included.
            let rect = unsafe { &mut *(lparam as *mut RECT) };

            // We need to calculate the dimensions of the window decorations to get the true
            // size of the window's contents
            let adj_rect = userdata
                .window_state_lock()
                .window_flags
                .adjust_rect(window, *rect)
                .unwrap_or(*rect);
            let deco_width = rect.left - adj_rect.left + adj_rect.right - rect.right;
            let deco_height = rect.top - adj_rect.top + adj_rect.bottom - rect.bottom;

            let width = rect.right - rect.left - deco_width;
            let height = rect.bottom - rect.top - deco_height;

            let mut width_delta = snap_to_nearest_increment_delta(width, inc.width);
            let mut height_delta = snap_to_nearest_increment_delta(height, inc.height);

            // Windows won't bound check the value of `rect` after we're done here, so we
            // have to check manually. If the width/height we snap to would go out of bounds, just
            // set it equal to the min/max bound.
            let min_size = userdata
                .window_state_lock()
                .min_size
                .map(|size| size.to_physical(scale_factor));
            let max_size = userdata
                .window_state_lock()
                .max_size
                .map(|size| size.to_physical(scale_factor));
            let final_width = width + width_delta;
            let final_height = height + height_delta;
            if let Some(min_size) = min_size {
                if final_width < min_size.width {
                    width_delta += min_size.width - final_width;
                }
                if final_height < min_size.height {
                    height_delta += min_size.height - final_height;
                }
            }
            if let Some(max_size) = max_size {
                if final_width > max_size.width {
                    width_delta -= final_width - max_size.width;
                }
                if final_height > max_size.height {
                    height_delta -= final_height - max_size.height;
                }
            }

            match side {
                WMSZ_LEFT | WMSZ_BOTTOMLEFT | WMSZ_TOPLEFT => {
                    rect.left -= width_delta;
                }
                WMSZ_RIGHT | WMSZ_BOTTOMRIGHT | WMSZ_TOPRIGHT => {
                    rect.right += width_delta;
                }
                _ => {}
            }

            match side {
                WMSZ_TOP | WMSZ_TOPLEFT | WMSZ_TOPRIGHT => {
                    rect.top -= height_delta;
                }
                WMSZ_BOTTOM | WMSZ_BOTTOMLEFT | WMSZ_BOTTOMRIGHT => {
                    rect.bottom += height_delta;
                }
                _ => {}
            }

            result = ProcResult::DefWindowProc(wparam);
        }

        WM_MENUCHAR => {
            result = ProcResult::Value((MNC_CLOSE << 16) as isize);
        }

        WM_IME_STARTCOMPOSITION => {
            let ime_allowed = userdata.window_state_lock().ime_allowed;
            if ime_allowed {
                userdata.window_state_lock().ime_state = ImeState::Enabled;

                userdata.send_event(Event::WindowEvent {
                    window_id: RootWindowId(WindowId(window)),
                    event: WindowEvent::Ime(Ime::Enabled),
                });
            }

            result = ProcResult::DefWindowProc(wparam);
        }

        WM_IME_COMPOSITION => {
            let ime_allowed_and_composing = {
                let w = userdata.window_state_lock();
                w.ime_allowed && w.ime_state != ImeState::Disabled
            };
            // Windows Hangul IME sends WM_IME_COMPOSITION after WM_IME_ENDCOMPOSITION, so
            // check whether composing.
            if ime_allowed_and_composing {
                let ime_context = unsafe { ImeContext::current(window) };

                if lparam == 0 {
                    userdata.send_event(Event::WindowEvent {
                        window_id: RootWindowId(WindowId(window)),
                        event: WindowEvent::Ime(Ime::Preedit(String::new(), None)),
                    });
                }

                // Google Japanese Input and ATOK have both flags, so
                // first, receive composing result if exist.
                if (lparam as u32 & GCS_RESULTSTR) != 0 {
                    if let Some(text) = unsafe { ime_context.get_composed_text() } {
                        userdata.window_state_lock().ime_state = ImeState::Enabled;

                        userdata.send_event(Event::WindowEvent {
                            window_id: RootWindowId(WindowId(window)),
                            event: WindowEvent::Ime(Ime::Preedit(String::new(), None)),
                        });
                        userdata.send_event(Event::WindowEvent {
                            window_id: RootWindowId(WindowId(window)),
                            event: WindowEvent::Ime(Ime::Commit(text)),
                        });
                    }
                }

                // Next, receive preedit range for next composing if exist.
                if (lparam as u32 & GCS_COMPSTR) != 0 {
                    if let Some((text, first, last)) =
                        unsafe { ime_context.get_composing_text_and_cursor() }
                    {
                        userdata.window_state_lock().ime_state = ImeState::Preedit;
                        let cursor_range = first.map(|f| (f, last.unwrap_or(f)));

                        userdata.send_event(Event::WindowEvent {
                            window_id: RootWindowId(WindowId(window)),
                            event: WindowEvent::Ime(Ime::Preedit(text, cursor_range)),
                        });
                    }
                }
            }

            // Not calling DefWindowProc to hide composing text drawn by IME.
            result = ProcResult::Value(0);
        }

        WM_IME_ENDCOMPOSITION => {
            let ime_allowed_or_composing = {
                let w = userdata.window_state_lock();
                w.ime_allowed || w.ime_state != ImeState::Disabled
            };
            if ime_allowed_or_composing {
                if userdata.window_state_lock().ime_state == ImeState::Preedit {
                    // Windows Hangul IME sends WM_IME_COMPOSITION after WM_IME_ENDCOMPOSITION, so
                    // trying receiving composing result and commit if exists.
                    let ime_context = unsafe { ImeContext::current(window) };
                    if let Some(text) = unsafe { ime_context.get_composed_text() } {
                        userdata.send_event(Event::WindowEvent {
                            window_id: RootWindowId(WindowId(window)),
                            event: WindowEvent::Ime(Ime::Preedit(String::new(), None)),
                        });
                        userdata.send_event(Event::WindowEvent {
                            window_id: RootWindowId(WindowId(window)),
                            event: WindowEvent::Ime(Ime::Commit(text)),
                        });
                    }
                }

                userdata.window_state_lock().ime_state = ImeState::Disabled;

                userdata.send_event(Event::WindowEvent {
                    window_id: RootWindowId(WindowId(window)),
                    event: WindowEvent::Ime(Ime::Disabled),
                });
            }

            result = ProcResult::DefWindowProc(wparam);
        }

        WM_IME_SETCONTEXT => {
            // Hide composing text drawn by IME.
            let wparam = wparam & (!ISC_SHOWUICOMPOSITIONWINDOW as usize);
            result = ProcResult::DefWindowProc(wparam);
        }

        // this is necessary for us to maintain minimize/restore state
        WM_SYSCOMMAND => {
            if wparam == SC_RESTORE as usize {
                let mut w = userdata.window_state_lock();
                w.set_window_flags_in_place(|f| f.set(WindowFlags::MINIMIZED, false));
            }
            if wparam == SC_MINIMIZE as usize {
                let mut w = userdata.window_state_lock();
                w.set_window_flags_in_place(|f| f.set(WindowFlags::MINIMIZED, true));
            }
            // Send `WindowEvent::Minimized` here if we decide to implement one

            if wparam == SC_SCREENSAVE as usize {
                let window_state = userdata.window_state_lock();
                if window_state.fullscreen.is_some() {
                    result = ProcResult::Value(0);
                    return;
                }
            }

            result = ProcResult::DefWindowProc(wparam);
        }

        WM_MOUSEMOVE => {
            use crate::event::WindowEvent::{CursorEntered, CursorLeft, CursorMoved};

            let x = super::get_x_lparam(lparam as u32) as i32;
            let y = super::get_y_lparam(lparam as u32) as i32;
            let position = PhysicalPosition::new(x as f64, y as f64);

            let cursor_moved;
            {
                let mut w = userdata.window_state_lock();
                let mouse_was_inside_window =
                    w.mouse.cursor_flags().contains(CursorFlags::IN_WINDOW);

                match get_pointer_move_kind(window, mouse_was_inside_window, x, y) {
                    PointerMoveKind::Enter => {
                        w.mouse
                            .set_cursor_flags(window, |f| {
                                f.set(CursorFlags::IN_WINDOW, true)
                            })
                            .ok();

                        drop(w);
                        userdata.send_event(Event::WindowEvent {
                            window_id: RootWindowId(WindowId(window)),
                            event: CursorEntered {
                                device_id: DEVICE_ID,
                            },
                        });

                        // Calling TrackMouseEvent in order to receive mouse leave events.
                        unsafe {
                            TrackMouseEvent(&mut TRACKMOUSEEVENT {
                                cbSize: mem::size_of::<TRACKMOUSEEVENT>() as u32,
                                dwFlags: TME_LEAVE,
                                hwndTrack: window,
                                dwHoverTime: HOVER_DEFAULT,
                            })
                        };
                    }
                    PointerMoveKind::Leave => {
                        w.mouse
                            .set_cursor_flags(window, |f| {
                                f.set(CursorFlags::IN_WINDOW, false)
                            })
                            .ok();

                        drop(w);
                        userdata.send_event(Event::WindowEvent {
                            window_id: RootWindowId(WindowId(window)),
                            event: CursorLeft {
                                device_id: DEVICE_ID,
                            },
                        });
                    }
                    PointerMoveKind::None => drop(w),
                }

                // handle spurious WM_MOUSEMOVE messages
                // see https://devblogs.microsoft.com/oldnewthing/20031001-00/?p=42343
                // and http://debugandconquer.blogspot.com/2015/08/the-cause-of-spurious-mouse-move.html
                let mut w = userdata.window_state_lock();
                cursor_moved = w.mouse.last_position != Some(position);
                w.mouse.last_position = Some(position);
            }

            if cursor_moved {
                update_modifiers(window, userdata);

                userdata.send_event(Event::WindowEvent {
                    window_id: RootWindowId(WindowId(window)),
                    event: CursorMoved {
                        device_id: DEVICE_ID,
                        position,
                    },
                });
            }

            result = ProcResult::Value(0);
        }

        WM_MOUSELEAVE => {
            use crate::event::WindowEvent::CursorLeft;
            {
                let mut w = userdata.window_state_lock();
                w.mouse
                    .set_cursor_flags(window, |f| f.set(CursorFlags::IN_WINDOW, false))
                    .ok();
            }

            userdata.send_event(Event::WindowEvent {
                window_id: RootWindowId(WindowId(window)),
                event: CursorLeft {
                    device_id: DEVICE_ID,
                },
            });

            result = ProcResult::Value(0);
        }

        WM_MOUSEWHEEL => {
            use crate::event::MouseScrollDelta::LineDelta;

            let value = (wparam >> 16) as i16;
            let value = value as f32 / WHEEL_DELTA as f32;

            update_modifiers(window, userdata);

            userdata.send_event(Event::WindowEvent {
                window_id: RootWindowId(WindowId(window)),
                event: WindowEvent::MouseWheel {
                    device_id: DEVICE_ID,
                    delta: LineDelta(0.0, value),
                    phase: TouchPhase::Moved,
                },
            });

            result = ProcResult::Value(0);
        }

        WM_MOUSEHWHEEL => {
            use crate::event::MouseScrollDelta::LineDelta;

            let value = (wparam >> 16) as i16;
            let value = -value as f32 / WHEEL_DELTA as f32; // NOTE: inverted! See https://github.com/rust-windowing/winit/pull/2105/

            update_modifiers(window, userdata);

            userdata.send_event(Event::WindowEvent {
                window_id: RootWindowId(WindowId(window)),
                event: WindowEvent::MouseWheel {
                    device_id: DEVICE_ID,
                    delta: LineDelta(value, 0.0),
                    phase: TouchPhase::Moved,
                },
            });

            result = ProcResult::Value(0);
        }

        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if msg == WM_SYSKEYDOWN {
                result = ProcResult::DefWindowProc(wparam);
            }
        }

        WM_KEYUP | WM_SYSKEYUP => {
            if msg == WM_SYSKEYUP && unsafe { !GetMenu(window).is_null() } {
                // let Windows handle event if the window has a native menu, a modal event loop
                // is started here on Alt key up.
                result = ProcResult::DefWindowProc(wparam);
            }
        }

        WM_LBUTTONDOWN => {
            use crate::event::ElementState::Pressed;
            use crate::event::MouseButton::Left;
            use crate::event::WindowEvent::MouseInput;

            unsafe { capture_mouse(window, &mut userdata.window_state_lock()) };

            update_modifiers(window, userdata);

            userdata.send_event(Event::WindowEvent {
                window_id: RootWindowId(WindowId(window)),
                event: MouseInput {
                    device_id: DEVICE_ID,
                    state: Pressed,
                    button: Left,
                },
            });
            result = ProcResult::Value(0);
        }

        WM_LBUTTONUP => {
            use crate::event::ElementState::Released;
            use crate::event::MouseButton::Left;
            use crate::event::WindowEvent::MouseInput;

            unsafe { release_mouse(userdata.window_state_lock()) };

            update_modifiers(window, userdata);

            userdata.send_event(Event::WindowEvent {
                window_id: RootWindowId(WindowId(window)),
                event: MouseInput {
                    device_id: DEVICE_ID,
                    state: Released,
                    button: Left,
                },
            });
            result = ProcResult::Value(0);
        }

        WM_RBUTTONDOWN => {
            use crate::event::ElementState::Pressed;
            use crate::event::MouseButton::Right;
            use crate::event::WindowEvent::MouseInput;

            unsafe { capture_mouse(window, &mut userdata.window_state_lock()) };

            update_modifiers(window, userdata);

            userdata.send_event(Event::WindowEvent {
                window_id: RootWindowId(WindowId(window)),
                event: MouseInput {
                    device_id: DEVICE_ID,
                    state: Pressed,
                    button: Right,
                },
            });
            result = ProcResult::Value(0);
        }

        WM_RBUTTONUP => {
            use crate::event::ElementState::Released;
            use crate::event::MouseButton::Right;
            use crate::event::WindowEvent::MouseInput;

            unsafe { release_mouse(userdata.window_state_lock()) };

            update_modifiers(window, userdata);

            userdata.send_event(Event::WindowEvent {
                window_id: RootWindowId(WindowId(window)),
                event: MouseInput {
                    device_id: DEVICE_ID,
                    state: Released,
                    button: Right,
                },
            });
            result = ProcResult::Value(0);
        }

        WM_MBUTTONDOWN => {
            use crate::event::ElementState::Pressed;
            use crate::event::MouseButton::Middle;
            use crate::event::WindowEvent::MouseInput;

            unsafe { capture_mouse(window, &mut userdata.window_state_lock()) };

            update_modifiers(window, userdata);

            userdata.send_event(Event::WindowEvent {
                window_id: RootWindowId(WindowId(window)),
                event: MouseInput {
                    device_id: DEVICE_ID,
                    state: Pressed,
                    button: Middle,
                },
            });
            result = ProcResult::Value(0);
        }

        WM_MBUTTONUP => {
            use crate::event::ElementState::Released;
            use crate::event::MouseButton::Middle;
            use crate::event::WindowEvent::MouseInput;

            unsafe { release_mouse(userdata.window_state_lock()) };

            update_modifiers(window, userdata);

            userdata.send_event(Event::WindowEvent {
                window_id: RootWindowId(WindowId(window)),
                event: MouseInput {
                    device_id: DEVICE_ID,
                    state: Released,
                    button: Middle,
                },
            });
            result = ProcResult::Value(0);
        }

        WM_XBUTTONDOWN => {
            use crate::event::ElementState::Pressed;
            use crate::event::MouseButton::{Back, Forward, Other};
            use crate::event::WindowEvent::MouseInput;
            let xbutton = super::get_xbutton_wparam(wparam as u32);

            unsafe { capture_mouse(window, &mut userdata.window_state_lock()) };

            update_modifiers(window, userdata);

            userdata.send_event(Event::WindowEvent {
                window_id: RootWindowId(WindowId(window)),
                event: MouseInput {
                    device_id: DEVICE_ID,
                    state: Pressed,
                    button: match xbutton {
                        1 => Back,
                        2 => Forward,
                        _ => Other(xbutton),
                    },
                },
            });
            result = ProcResult::Value(0);
        }

        WM_XBUTTONUP => {
            use crate::event::ElementState::Released;
            use crate::event::MouseButton::{Back, Forward, Other};
            use crate::event::WindowEvent::MouseInput;
            let xbutton = super::get_xbutton_wparam(wparam as u32);

            unsafe { release_mouse(userdata.window_state_lock()) };

            update_modifiers(window, userdata);

            userdata.send_event(Event::WindowEvent {
                window_id: RootWindowId(WindowId(window)),
                event: MouseInput {
                    device_id: DEVICE_ID,
                    state: Released,
                    button: match xbutton {
                        1 => Back,
                        2 => Forward,
                        _ => Other(xbutton),
                    },
                },
            });
            result = ProcResult::Value(0);
        }

        WM_CAPTURECHANGED => {
            // lparam here is a handle to the window which is gaining mouse capture.
            // If it is the same as our window, then we're essentially retaining the capture. This
            // can happen if `SetCapture` is called on our window when it already has the mouse
            // capture.
            if lparam != window as isize {
                userdata.window_state_lock().mouse.capture_count = 0;
            }
            result = ProcResult::Value(0);
        }

        WM_TOUCH => {
            let pcount = super::loword(wparam as u32) as usize;
            let mut inputs = Vec::with_capacity(pcount);
            let htouch = lparam as *mut _;
            if unsafe {
                GetTouchInputInfo(
                    htouch,
                    pcount as u32,
                    inputs.as_mut_ptr(),
                    mem::size_of::<TOUCHINPUT>() as i32,
                ) > 0
            } {
                unsafe { inputs.set_len(pcount) };
                for input in &inputs {
                    let mut location = POINT {
                        x: input.x / 100,
                        y: input.y / 100,
                    };

                    if unsafe { ScreenToClient(window, &mut location) } == false.into() {
                        continue;
                    }

                    let x = location.x as f64 + (input.x % 100) as f64 / 100f64;
                    let y = location.y as f64 + (input.y % 100) as f64 / 100f64;
                    let location = PhysicalPosition::new(x, y);
                    userdata.send_event(Event::WindowEvent {
                        window_id: RootWindowId(WindowId(window)),
                        event: WindowEvent::Touch(Touch {
                            phase: if util::has_flag(input.dwFlags, TOUCHEVENTF_DOWN) {
                                TouchPhase::Started
                            } else if util::has_flag(input.dwFlags, TOUCHEVENTF_UP) {
                                TouchPhase::Ended
                            } else if util::has_flag(input.dwFlags, TOUCHEVENTF_MOVE) {
                                TouchPhase::Moved
                            } else {
                                continue;
                            },
                            location,
                            force: None, // WM_TOUCH doesn't support pressure information
                            id: input.dwID as u64,
                            device_id: DEVICE_ID,
                        }),
                    });
                }
            }
            unsafe { CloseTouchInputHandle(htouch) };
            result = ProcResult::Value(0);
        }

        WM_POINTERDOWN | WM_POINTERUPDATE | WM_POINTERUP => {
            if let (
                Some(GetPointerFrameInfoHistory),
                Some(SkipPointerFrameMessages),
                Some(GetPointerDeviceRects),
            ) = (
                *util::GET_POINTER_FRAME_INFO_HISTORY,
                *util::SKIP_POINTER_FRAME_MESSAGES,
                *util::GET_POINTER_DEVICE_RECTS,
            ) {
                let pointer_id = super::loword(wparam as u32) as u32;
                let mut entries_count = 0u32;
                let mut pointers_count = 0u32;
                if unsafe {
                    GetPointerFrameInfoHistory(
                        pointer_id,
                        &mut entries_count,
                        &mut pointers_count,
                        ptr::null_mut(),
                    )
                } == false.into()
                {
                    result = ProcResult::Value(0);
                    return;
                }

                let pointer_info_count = (entries_count * pointers_count) as usize;
                let mut pointer_infos = Vec::with_capacity(pointer_info_count);
                if unsafe {
                    GetPointerFrameInfoHistory(
                        pointer_id,
                        &mut entries_count,
                        &mut pointers_count,
                        pointer_infos.as_mut_ptr(),
                    )
                } == false.into()
                {
                    result = ProcResult::Value(0);
                    return;
                }
                unsafe { pointer_infos.set_len(pointer_info_count) };

                // https://docs.microsoft.com/en-us/windows/desktop/api/winuser/nf-winuser-getpointerframeinfohistory
                // The information retrieved appears in reverse chronological order, with the most
                // recent entry in the first row of the returned array
                for pointer_info in pointer_infos.iter().rev() {
                    let mut device_rect = mem::MaybeUninit::uninit();
                    let mut display_rect = mem::MaybeUninit::uninit();

                    if unsafe {
                        GetPointerDeviceRects(
                            pointer_info.sourceDevice,
                            device_rect.as_mut_ptr(),
                            display_rect.as_mut_ptr(),
                        )
                    } == false.into()
                    {
                        continue;
                    }

                    let device_rect = unsafe { device_rect.assume_init() };
                    let display_rect = unsafe { display_rect.assume_init() };

                    // For the most precise himetric to pixel conversion we calculate the ratio
                    // between the resolution of the display device (pixel) and
                    // the touch device (himetric).
                    let himetric_to_pixel_ratio_x =
                        (display_rect.right - display_rect.left) as f64
                            / (device_rect.right - device_rect.left) as f64;
                    let himetric_to_pixel_ratio_y =
                        (display_rect.bottom - display_rect.top) as f64
                            / (device_rect.bottom - device_rect.top) as f64;

                    // ptHimetricLocation's origin is 0,0 even on multi-monitor setups.
                    // On multi-monitor setups we need to translate the himetric location to the
                    // rect of the display device it's attached to.
                    let x = display_rect.left as f64
                        + pointer_info.ptHimetricLocation.x as f64
                            * himetric_to_pixel_ratio_x;
                    let y = display_rect.top as f64
                        + pointer_info.ptHimetricLocation.y as f64
                            * himetric_to_pixel_ratio_y;

                    let mut location = POINT {
                        x: x.floor() as i32,
                        y: y.floor() as i32,
                    };

                    if unsafe { ScreenToClient(window, &mut location) } == false.into() {
                        continue;
                    }

                    let force = match pointer_info.pointerType {
                        PT_TOUCH => {
                            let mut touch_info = mem::MaybeUninit::uninit();
                            util::GET_POINTER_TOUCH_INFO.and_then(|GetPointerTouchInfo| {
                                match unsafe {
                                    GetPointerTouchInfo(
                                        pointer_info.pointerId,
                                        touch_info.as_mut_ptr(),
                                    )
                                } {
                                    0 => None,
                                    _ => normalize_pointer_pressure(unsafe {
                                        touch_info.assume_init().pressure
                                    }),
                                }
                            })
                        }
                        PT_PEN => {
                            let mut pen_info = mem::MaybeUninit::uninit();
                            util::GET_POINTER_PEN_INFO.and_then(
                                |GetPointerPenInfo| match unsafe {
                                    GetPointerPenInfo(
                                        pointer_info.pointerId,
                                        pen_info.as_mut_ptr(),
                                    )
                                } {
                                    0 => None,
                                    _ => normalize_pointer_pressure(unsafe {
                                        pen_info.assume_init().pressure
                                    }),
                                },
                            )
                        }
                        _ => None,
                    };

                    let x = location.x as f64 + x.fract();
                    let y = location.y as f64 + y.fract();
                    let location = PhysicalPosition::new(x, y);
                    userdata.send_event(Event::WindowEvent {
                        window_id: RootWindowId(WindowId(window)),
                        event: WindowEvent::Touch(Touch {
                            phase: if util::has_flag(
                                pointer_info.pointerFlags,
                                POINTER_FLAG_DOWN,
                            ) {
                                TouchPhase::Started
                            } else if util::has_flag(
                                pointer_info.pointerFlags,
                                POINTER_FLAG_UP,
                            ) {
                                TouchPhase::Ended
                            } else if util::has_flag(
                                pointer_info.pointerFlags,
                                POINTER_FLAG_UPDATE,
                            ) {
                                TouchPhase::Moved
                            } else {
                                continue;
                            },
                            location,
                            force,
                            id: pointer_info.pointerId as u64,
                            device_id: DEVICE_ID,
                        }),
                    });
                }

                unsafe { SkipPointerFrameMessages(pointer_id) };
            }
            result = ProcResult::Value(0);
        }

        WM_NCACTIVATE => {
            let is_active = wparam != false.into();
            let active_focus_changed = userdata.window_state_lock().set_active(is_active);
            if active_focus_changed {
                if is_active {
                    unsafe { gain_active_focus(window, userdata) };
                } else {
                    unsafe { lose_active_focus(window, userdata) };
                }
            }
            result = ProcResult::DefWindowProc(wparam);
        }

        WM_SETFOCUS => {
            let active_focus_changed = userdata.window_state_lock().set_focused(true);
            if active_focus_changed {
                unsafe { gain_active_focus(window, userdata) };
            }
            result = ProcResult::Value(0);
        }

        WM_KILLFOCUS => {
            let active_focus_changed = userdata.window_state_lock().set_focused(false);
            if active_focus_changed {
                unsafe { lose_active_focus(window, userdata) };
            }
            result = ProcResult::Value(0);
        }

        WM_SETCURSOR => {
            let set_cursor_to = {
                let window_state = userdata.window_state_lock();
                // The return value for the preceding `WM_NCHITTEST` message is conveniently
                // provided through the low-order word of lParam. We use that here since
                // `WM_MOUSEMOVE` seems to come after `WM_SETCURSOR` for a given cursor movement.
                let in_client_area = super::loword(lparam as u32) as u32 == HTCLIENT;
                if in_client_area {
                    Some(window_state.mouse.selected_cursor.clone())
                } else {
                    None
                }
            };

            match set_cursor_to {
                Some(selected_cursor) => {
                    let hcursor = match selected_cursor {
                        SelectedCursor::Named(cursor_icon) => unsafe {
                            LoadCursorW(
                                ptr::null_mut(),
                                util::to_windows_cursor(cursor_icon),
                            )
                        },
                        SelectedCursor::Custom(cursor) => cursor.as_raw_handle(),
                    };
                    unsafe { SetCursor(hcursor) };
                    result = ProcResult::Value(0);
                }
                None => result = ProcResult::DefWindowProc(wparam),
            }
        }

        WM_GETMINMAXINFO => {
            let mmi = lparam as *mut MINMAXINFO;

            let window_state = userdata.window_state_lock();
            let window_flags = window_state.window_flags;

            if window_state.min_size.is_some() || window_state.max_size.is_some() {
                if let Some(min_size) = window_state.min_size {
                    let min_size = min_size.to_physical(window_state.scale_factor);
                    let (width, height): (u32, u32) =
                        window_flags.adjust_size(window, min_size).into();
                    unsafe {
                        (*mmi).ptMinTrackSize = POINT {
                            x: width as i32,
                            y: height as i32,
                        }
                    };
                }
                if let Some(max_size) = window_state.max_size {
                    let max_size = max_size.to_physical(window_state.scale_factor);
                    let (width, height): (u32, u32) =
                        window_flags.adjust_size(window, max_size).into();
                    unsafe {
                        (*mmi).ptMaxTrackSize = POINT {
                            x: width as i32,
                            y: height as i32,
                        }
                    };
                }
            }

            result = ProcResult::Value(0);
        }

        // Only sent on Windows 8.1 or newer. On Windows 7 and older user has to log out to change
        // DPI, therefore all applications are closed while DPI is changing.
        WM_DPICHANGED => {
            use crate::event::WindowEvent::ScaleFactorChanged;

            // This message actually provides two DPI values - x and y. However MSDN says that
            // "you only need to use either the X-axis or the Y-axis value when scaling your
            // application since they are the same".
            // https://msdn.microsoft.com/en-us/library/windows/desktop/dn312083(v=vs.85).aspx
            let new_dpi_x = super::loword(wparam as u32) as u32;
            let new_scale_factor = dpi_to_scale_factor(new_dpi_x);
            let old_scale_factor: f64;

            let (allow_resize, window_flags) = {
                let mut window_state = userdata.window_state_lock();
                old_scale_factor = window_state.scale_factor;
                window_state.scale_factor = new_scale_factor;

                if new_scale_factor == old_scale_factor {
                    result = ProcResult::Value(0);
                    return;
                }

                let allow_resize = window_state.fullscreen.is_none()
                    && !window_state.window_flags().contains(WindowFlags::MAXIMIZED);

                (allow_resize, window_state.window_flags)
            };

            // New size as suggested by Windows.
            let suggested_rect = unsafe { *(lparam as *const RECT) };

            // The window rect provided is the window's outer size, not it's inner size. However,
            // win32 doesn't provide an `UnadjustWindowRectEx` function to get the client rect from
            // the outer rect, so we instead adjust the window rect to get the decoration margins
            // and remove them from the outer size.
            let margin_left: i32;
            let margin_top: i32;
            // let margin_right: i32;
            // let margin_bottom: i32;
            {
                let adjusted_rect = window_flags
                    .adjust_rect(window, suggested_rect)
                    .unwrap_or(suggested_rect);
                margin_left = suggested_rect.left - adjusted_rect.left;
                margin_top = suggested_rect.top - adjusted_rect.top;
                // margin_right = adjusted_rect.right - suggested_rect.right;
                // margin_bottom = adjusted_rect.bottom - suggested_rect.bottom;
            }

            let old_physical_inner_rect = util::WindowArea::Inner
                .get_rect(window)
                .expect("failed to query (old) inner window area");
            let old_physical_inner_size = PhysicalSize::new(
                (old_physical_inner_rect.right - old_physical_inner_rect.left) as u32,
                (old_physical_inner_rect.bottom - old_physical_inner_rect.top) as u32,
            );

            // `allow_resize` prevents us from re-applying DPI adjustment to the restored size after
            // exiting fullscreen (the restored size is already DPI adjusted).
            let new_physical_inner_size = match allow_resize {
                // We calculate our own size because the default suggested rect doesn't do a great
                // job of preserving the window's logical size.
                true => old_physical_inner_size
                    .to_logical::<f64>(old_scale_factor)
                    .to_physical::<u32>(new_scale_factor),
                false => old_physical_inner_size,
            };

            let new_inner_size = Arc::new(Mutex::new(new_physical_inner_size));
            userdata.send_event(Event::WindowEvent {
                window_id: RootWindowId(WindowId(window)),
                event: ScaleFactorChanged {
                    scale_factor: new_scale_factor,
                    inner_size_writer: InnerSizeWriter::new(Arc::downgrade(
                        &new_inner_size,
                    )),
                },
            });

            let new_physical_inner_size = *new_inner_size.lock().unwrap();
            drop(new_inner_size);

            let dragging_window: bool;

            {
                let window_state = userdata.window_state_lock();
                dragging_window = window_state
                    .window_flags()
                    .contains(WindowFlags::MARKER_IN_SIZE_MOVE);
                // Unset maximized if we're changing the window's size.
                if new_physical_inner_size != old_physical_inner_size {
                    WindowState::set_window_flags(window_state, window, |f| {
                        f.set(WindowFlags::MAXIMIZED, false)
                    });
                }
            }

            let new_outer_rect: RECT;
            {
                let suggested_ul = (
                    suggested_rect.left + margin_left,
                    suggested_rect.top + margin_top,
                );

                let mut conservative_rect = RECT {
                    left: suggested_ul.0,
                    top: suggested_ul.1,
                    right: suggested_ul.0 + new_physical_inner_size.width as i32,
                    bottom: suggested_ul.1 + new_physical_inner_size.height as i32,
                };

                conservative_rect = window_flags
                    .adjust_rect(window, conservative_rect)
                    .unwrap_or(conservative_rect);

                // If we're dragging the window, offset the window so that the cursor's
                // relative horizontal position in the title bar is preserved.
                if dragging_window {
                    let bias = {
                        let cursor_pos = {
                            let mut pos = unsafe { mem::zeroed() };
                            unsafe { GetCursorPos(&mut pos) };
                            pos
                        };
                        let suggested_cursor_horizontal_ratio =
                            (cursor_pos.x - suggested_rect.left) as f64
                                / (suggested_rect.right - suggested_rect.left) as f64;

                        (cursor_pos.x
                            - (suggested_cursor_horizontal_ratio
                                * (conservative_rect.right - conservative_rect.left)
                                    as f64) as i32)
                            - conservative_rect.left
                    };
                    conservative_rect.left += bias;
                    conservative_rect.right += bias;
                }

                // Check to see if the new window rect is on the monitor with the new DPI factor.
                // If it isn't, offset the window so that it is.
                let new_dpi_monitor =
                    unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONULL) };
                let conservative_rect_monitor =
                    unsafe { MonitorFromRect(&conservative_rect, MONITOR_DEFAULTTONULL) };
                new_outer_rect = if conservative_rect_monitor == new_dpi_monitor {
                    conservative_rect
                } else {
                    let get_monitor_rect = |monitor| {
                        let mut monitor_info = MONITORINFO {
                            cbSize: mem::size_of::<MONITORINFO>() as _,
                            ..unsafe { mem::zeroed() }
                        };
                        unsafe { GetMonitorInfoW(monitor, &mut monitor_info) };
                        monitor_info.rcMonitor
                    };
                    let wrong_monitor = conservative_rect_monitor;
                    let wrong_monitor_rect = get_monitor_rect(wrong_monitor);
                    let new_monitor_rect = get_monitor_rect(new_dpi_monitor);

                    // The direction to nudge the window in to get the window onto the monitor with
                    // the new DPI factor. We calculate this by seeing which monitor edges are
                    // shared and nudging away from the wrong monitor based on those.
                    #[allow(clippy::bool_to_int_with_if)]
                    let delta_nudge_to_dpi_monitor = (
                        if wrong_monitor_rect.left == new_monitor_rect.right {
                            -1
                        } else if wrong_monitor_rect.right == new_monitor_rect.left {
                            1
                        } else {
                            0
                        },
                        if wrong_monitor_rect.bottom == new_monitor_rect.top {
                            1
                        } else if wrong_monitor_rect.top == new_monitor_rect.bottom {
                            -1
                        } else {
                            0
                        },
                    );

                    let abort_after_iterations = new_monitor_rect.right
                        - new_monitor_rect.left
                        + new_monitor_rect.bottom
                        - new_monitor_rect.top;
                    for _ in 0..abort_after_iterations {
                        conservative_rect.left += delta_nudge_to_dpi_monitor.0;
                        conservative_rect.right += delta_nudge_to_dpi_monitor.0;
                        conservative_rect.top += delta_nudge_to_dpi_monitor.1;
                        conservative_rect.bottom += delta_nudge_to_dpi_monitor.1;

                        if unsafe {
                            MonitorFromRect(&conservative_rect, MONITOR_DEFAULTTONULL)
                        } == new_dpi_monitor
                        {
                            break;
                        }
                    }

                    conservative_rect
                };
            }

            unsafe {
                SetWindowPos(
                    window,
                    ptr::null_mut(),
                    new_outer_rect.left,
                    new_outer_rect.top,
                    new_outer_rect.right - new_outer_rect.left,
                    new_outer_rect.bottom - new_outer_rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                )
            };

            result = ProcResult::Value(0);
        }

        WM_SETTINGCHANGE => {
            use crate::event::WindowEvent::ThemeChanged;

            let preferred_theme = userdata.window_state_lock().preferred_theme;

            if preferred_theme.is_none() {
                let new_theme = try_theme(window, preferred_theme);
                let mut window_state = userdata.window_state_lock();

                if window_state.current_theme != new_theme {
                    window_state.current_theme = new_theme;
                    drop(window_state);
                    userdata.send_event(Event::WindowEvent {
                        window_id: RootWindowId(WindowId(window)),
                        event: ThemeChanged(new_theme),
                    });
                }
            }
            result = ProcResult::DefWindowProc(wparam);
        }

        _ => {
            if msg == DESTROY_MSG_ID.get() {
                unsafe { DestroyWindow(window) };
                result = ProcResult::Value(0);
            } else if msg == SET_RETAIN_STATE_ON_SIZE_MSG_ID.get() {
                let mut window_state = userdata.window_state_lock();
                window_state.set_window_flags_in_place(|f| {
                    f.set(WindowFlags::MARKER_RETAIN_STATE_ON_SIZE, wparam != 0)
                });
                result = ProcResult::Value(0);
            } else if msg == TASKBAR_CREATED.get() {
                let window_state = userdata.window_state_lock();
                unsafe { set_skip_taskbar(window, window_state.skip_taskbar) };
                result = ProcResult::DefWindowProc(wparam);
            } else {
                result = ProcResult::DefWindowProc(wparam);
            }
        }
    };

    userdata
        .event_loop_runner
        .catch_unwind(callback)
        .unwrap_or_else(|| result = ProcResult::Value(-1));

    match result {
        ProcResult::DefWindowProc(wparam) => unsafe {
            DefWindowProcW(window, msg, wparam, lparam)
        },
        ProcResult::Value(val) => val,
    }
}

unsafe extern "system" fn thread_event_target_callback(
    window: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let userdata_ptr = unsafe { super::get_window_long(window, GWL_USERDATA) }
        as *mut ThreadMsgTargetData;
    if userdata_ptr.is_null() {
        // `userdata_ptr` will always be null for the first `WM_GETMINMAXINFO`, as well as
        // `WM_NCCREATE` and `WM_CREATE`.
        return unsafe { DefWindowProcW(window, msg, wparam, lparam) };
    }
    let userdata = unsafe { Box::from_raw(userdata_ptr) };

    let mut userdata_removed = false;

    // I decided to bind the closure to `callback` and pass it to catch_unwind rather than passing
    // the closure to catch_unwind directly so that the match body indentation wouldn't change and
    // the git blame and history would be preserved.
    let callback = || match msg {
        WM_NCDESTROY => {
            unsafe {
                RedrawWindow(window, ptr::null(), ptr::null_mut(), RDW_INTERNALPAINT)
            };
            unsafe { super::set_window_long(window, GWL_USERDATA, 0) };
            userdata_removed = true;
            0
        }
        WM_PAINT => unsafe {
            ValidateRect(window, ptr::null());
            // Default WM_PAINT behaviour. This makes sure modals and popups are shown immediately
            // when opening them.
            DefWindowProcW(window, msg, wparam, lparam)
        },

        WM_INPUT_DEVICE_CHANGE => {
            unsafe {
                RedrawWindow(window, ptr::null(), ptr::null_mut(), RDW_INTERNALPAINT)
            };
            let event = match wparam as u32 {
                GIDC_ARRIVAL => DeviceEvent::Added,
                GIDC_REMOVAL => DeviceEvent::Removed,
                _ => unreachable!(),
            };
            unsafe {
                RedrawWindow(window, ptr::null(), ptr::null_mut(), RDW_INTERNALPAINT)
            };

            userdata.send_event(Event::DeviceEvent {
                device_id: wrap_device_id(lparam as u32),
                event,
            });

            0
        }

        WM_INPUT => {
            if let Some(data) = raw_input::get_raw_input_data(lparam as _) {
                unsafe { handle_raw_input(&userdata, data) };
            }

            unsafe { DefWindowProcW(window, msg, wparam, lparam) }
        }

        _ if msg == USER_EVENT_MSG_ID.get() => {
            unsafe {
                RedrawWindow(window, ptr::null(), ptr::null_mut(), RDW_INTERNALPAINT)
            };
            // synthesis a placeholder UserEvent, so that if the callback is
            // re-entered it can be buffered for later delivery. the real
            // user event is still in the mpsc channel and will be pulled
            // once the placeholder event is delivered to the wrapper
            // `event_handler`
            userdata.send_event(Event::UserEvent(UserEventPlaceholder));
            0
        }
        _ if msg == EXEC_MSG_ID.get() => {
            unsafe {
                RedrawWindow(window, ptr::null(), ptr::null_mut(), RDW_INTERNALPAINT)
            };
            let mut function: ThreadExecFn = unsafe { Box::from_raw(wparam as *mut _) };
            function();
            0
        }
        _ => unsafe { DefWindowProcW(window, msg, wparam, lparam) },
    };

    let result = userdata
        .event_loop_runner
        .catch_unwind(callback)
        .unwrap_or(-1);
    if userdata_removed {
        drop(userdata);
    } else {
        Box::leak(userdata);
    }
    result
}

unsafe fn handle_raw_input(userdata: &ThreadMsgTargetData, data: RAWINPUT) {
    use crate::event::DeviceEvent::{Button, Key, Motion, MouseMotion, MouseWheel};
    use crate::event::ElementState::{Pressed, Released};
    use crate::event::MouseScrollDelta::LineDelta;

    let device_id = wrap_device_id(data.header.hDevice as _);

    if data.header.dwType == RIM_TYPEMOUSE {
        let mouse = unsafe { data.data.mouse };

        if util::has_flag(mouse.usFlags, MOUSE_MOVE_RELATIVE) {
            let x = mouse.lLastX as f64;
            let y = mouse.lLastY as f64;

            if x != 0.0 {
                userdata.send_event(Event::DeviceEvent {
                    device_id,
                    event: Motion { axis: 0, value: x },
                });
            }

            if y != 0.0 {
                userdata.send_event(Event::DeviceEvent {
                    device_id,
                    event: Motion { axis: 1, value: y },
                });
            }

            if x != 0.0 || y != 0.0 {
                userdata.send_event(Event::DeviceEvent {
                    device_id,
                    event: MouseMotion { delta: (x, y) },
                });
            }
        }

        let button_flags = unsafe { mouse.Anonymous.Anonymous.usButtonFlags };
        if util::has_flag(button_flags as u32, RI_MOUSE_WHEEL) {
            let button_data = unsafe { mouse.Anonymous.Anonymous.usButtonData } as i16;
            let delta = button_data as f32 / WHEEL_DELTA as f32;
            userdata.send_event(Event::DeviceEvent {
                device_id,
                event: MouseWheel {
                    delta: LineDelta(0.0, delta),
                },
            });
        }
        if util::has_flag(button_flags as u32, RI_MOUSE_HWHEEL) {
            let button_data = unsafe { mouse.Anonymous.Anonymous.usButtonData } as i16;
            let delta = -button_data as f32 / WHEEL_DELTA as f32;
            userdata.send_event(Event::DeviceEvent {
                device_id,
                event: MouseWheel {
                    delta: LineDelta(delta, 0.0),
                },
            });
        }

        let button_state = raw_input::get_raw_mouse_button_state(button_flags as u32);
        for (button, state) in button_state.iter().enumerate() {
            if let Some(state) = *state {
                userdata.send_event(Event::DeviceEvent {
                    device_id,
                    event: Button {
                        button: button as _,
                        state,
                    },
                });
            }
        }
    } else if data.header.dwType == RIM_TYPEKEYBOARD {
        let keyboard = unsafe { data.data.keyboard };

        let pressed = keyboard.Message == WM_KEYDOWN || keyboard.Message == WM_SYSKEYDOWN;
        let released = keyboard.Message == WM_KEYUP || keyboard.Message == WM_SYSKEYUP;

        if !pressed && !released {
            return;
        }

        if let Some(physical_key) = raw_input::get_keyboard_physical_key(keyboard) {
            let state = if pressed { Pressed } else { Released };

            userdata.send_event(Event::DeviceEvent {
                device_id,
                event: Key(RawKeyEvent {
                    physical_key,
                    state,
                }),
            });
        }
    }
}

enum PointerMoveKind {
    /// Pointer enterd to the window.
    Enter,
    /// Pointer leaved the window client area.
    Leave,
    /// Pointer is inside the window or `GetClientRect` failed.
    None,
}

fn get_pointer_move_kind(
    window: HWND,
    mouse_was_inside_window: bool,
    x: i32,
    y: i32,
) -> PointerMoveKind {
    let rect: RECT = unsafe {
        let mut rect: RECT = mem::zeroed();
        if GetClientRect(window, &mut rect) == false.into() {
            return PointerMoveKind::None; // exit early if GetClientRect failed
        }
        rect
    };

    let x = (rect.left..rect.right).contains(&x);
    let y = (rect.top..rect.bottom).contains(&y);

    if !mouse_was_inside_window && x && y {
        PointerMoveKind::Enter
    } else if mouse_was_inside_window && !(x && y) {
        PointerMoveKind::Leave
    } else {
        PointerMoveKind::None
    }
}
