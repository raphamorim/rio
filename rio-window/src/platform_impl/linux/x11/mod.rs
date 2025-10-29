use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::CStr;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::os::raw::*;
use std::os::unix::io::{AsFd, AsRawFd, BorrowedFd, RawFd};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};
use std::{fmt, ptr, slice, str};

use calloop::generic::Generic;
use calloop::ping::Ping;
use calloop::{EventLoop as Loop, Readiness};
use libc::{setlocale, LC_CTYPE};
use tracing::warn;

use x11rb::connection::RequestConnection;
use x11rb::errors::{ConnectError, ConnectionError, IdsExhausted, ReplyError};
use x11rb::protocol::xinput::{self, ConnectionExt as _};
use x11rb::protocol::xkb;
use x11rb::protocol::xproto::{self, ConnectionExt as _};
use x11rb::x11_utils::X11Error as LogicalError;
use x11rb::xcb_ffi::ReplyOrIdError;

use crate::error::{EventLoopError, OsError as RootOsError};
use crate::event::{Event, StartCause, WindowEvent};
use crate::event_loop::{
    ActiveEventLoop as RootAEL, ControlFlow, DeviceEvents, EventLoopClosed,
};
use crate::platform::pump_events::PumpStatus;
use crate::platform_impl::common::xkb::Context;
use crate::platform_impl::platform::{min_timeout, WindowId};
use crate::platform_impl::{
    ActiveEventLoop as PlatformActiveEventLoop, OsError, PlatformCustomCursor,
};
use crate::window::{
    CustomCursor as RootCustomCursor, CustomCursorSource, Theme, WindowAttributes,
};

mod activation;
mod atoms;
mod dnd;
mod event_processor;
pub mod ffi;
mod ime;
mod monitor;
mod util;
mod window;
mod xdisplay;
mod xsettings;

pub use util::CustomCursor;

use atoms::*;
use dnd::{Dnd, DndState};
use event_processor::{EventProcessor, MAX_MOD_REPLAY_LEN};
use ime::{Ime, ImeCreationError, ImeReceiver, ImeRequest, ImeSender};
pub(crate) use monitor::{MonitorHandle, VideoModeHandle};
use window::UnownedWindow;
pub(crate) use xdisplay::{XConnection, XError, XNotSupported};

// Xinput constants not defined in x11rb
const ALL_DEVICES: u16 = 0;
const ALL_MASTER_DEVICES: u16 = 1;
const ICONIC_STATE: u32 = 3;

/// The underlying x11rb connection that we are using.
type X11rbConnection = x11rb::xcb_ffi::XCBConnection;

type X11Source = Generic<BorrowedFd<'static>>;

struct WakeSender<T> {
    sender: Sender<T>,
    waker: Ping,
}

impl<T> Clone for WakeSender<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            waker: self.waker.clone(),
        }
    }
}

impl<T> WakeSender<T> {
    pub fn send(&self, t: T) -> Result<(), EventLoopClosed<T>> {
        let res = self.sender.send(t).map_err(|e| EventLoopClosed(e.0));
        if res.is_ok() {
            self.waker.ping();
        }
        res
    }
}

struct PeekableReceiver<T> {
    recv: Receiver<T>,
    first: Option<T>,
}

impl<T> PeekableReceiver<T> {
    pub fn from_recv(recv: Receiver<T>) -> Self {
        Self { recv, first: None }
    }

    pub fn has_incoming(&mut self) -> bool {
        if self.first.is_some() {
            return true;
        }

        match self.recv.try_recv() {
            Ok(v) => {
                self.first = Some(v);
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                warn!("Channel was disconnected when checking incoming");
                false
            }
        }
    }

    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        if let Some(first) = self.first.take() {
            return Ok(first);
        }
        self.recv.try_recv()
    }
}

pub struct ActiveEventLoop {
    xconn: Arc<XConnection>,
    wm_delete_window: xproto::Atom,
    net_wm_ping: xproto::Atom,
    ime_sender: ImeSender,
    control_flow: Cell<ControlFlow>,
    exit: Cell<Option<i32>>,
    root: xproto::Window,
    ime: Option<RefCell<Ime>>,
    windows: RefCell<HashMap<WindowId, Weak<UnownedWindow>>>,
    redraw_sender: WakeSender<WindowId>,
    activation_sender: WakeSender<ActivationToken>,
    device_events: Cell<DeviceEvents>,
}

pub struct EventLoop<T: 'static> {
    loop_running: bool,
    event_loop: Loop<'static, EventLoopState>,
    waker: calloop::ping::Ping,
    event_processor: EventProcessor,
    redraw_receiver: PeekableReceiver<WindowId>,
    user_receiver: PeekableReceiver<T>,
    activation_receiver: PeekableReceiver<ActivationToken>,
    user_sender: Sender<T>,

    /// The current state of the event loop.
    state: EventLoopState,
}

type ActivationToken = (WindowId, crate::event_loop::AsyncRequestSerial);

struct EventLoopState {
    /// The latest readiness state for the x11 file descriptor
    x11_readiness: Readiness,
}

pub struct EventLoopProxy<T: 'static> {
    user_sender: WakeSender<T>,
}

impl<T: 'static> Clone for EventLoopProxy<T> {
    fn clone(&self) -> Self {
        EventLoopProxy {
            user_sender: self.user_sender.clone(),
        }
    }
}

impl<T: 'static> EventLoop<T> {
    pub(crate) fn new(xconn: Arc<XConnection>) -> EventLoop<T> {
        let root = xconn.default_root().root;
        let atoms = xconn.atoms();

        let wm_delete_window = atoms[WM_DELETE_WINDOW];
        let net_wm_ping = atoms[_NET_WM_PING];

        let dnd = Dnd::new(Arc::clone(&xconn))
            .expect("Failed to call XInternAtoms when initializing drag and drop");

        let (ime_sender, ime_receiver) = mpsc::channel();
        let (ime_event_sender, ime_event_receiver) = mpsc::channel();
        // Input methods will open successfully without setting the locale, but it won't be
        // possible to actually commit pre-edit sequences.
        unsafe {
            // Remember default locale to restore it if target locale is unsupported
            // by Xlib
            let default_locale = setlocale(LC_CTYPE, ptr::null());
            setlocale(LC_CTYPE, c"".as_ptr() as *const _);

            // Check if set locale is supported by Xlib.
            // If not, calls to some Xlib functions like `XSetLocaleModifiers`
            // will fail.
            let locale_supported = (xconn.xlib.XSupportsLocale)() == 1;
            if !locale_supported {
                let unsupported_locale = setlocale(LC_CTYPE, ptr::null());
                warn!(
                    "Unsupported locale \"{}\". Restoring default locale \"{}\".",
                    CStr::from_ptr(unsupported_locale).to_string_lossy(),
                    CStr::from_ptr(default_locale).to_string_lossy()
                );
                // Restore default locale
                setlocale(LC_CTYPE, default_locale);
            }
        }

        let ime = Ime::new(Arc::clone(&xconn), ime_event_sender);
        if let Err(ImeCreationError::OpenFailure(state)) = ime.as_ref() {
            warn!("Failed to open input method: {state:#?}");
        } else if let Err(err) = ime.as_ref() {
            warn!("Failed to set input method destruction callback: {err:?}");
        }

        let ime = ime.ok().map(RefCell::new);

        let randr_event_offset = xconn
            .select_xrandr_input(root)
            .expect("Failed to query XRandR extension");

        let xi2ext = xconn
            .xcb_connection()
            .extension_information(xinput::X11_EXTENSION_NAME)
            .expect("Failed to query XInput extension")
            .expect("X server missing XInput extension");
        let xkbext = xconn
            .xcb_connection()
            .extension_information(xkb::X11_EXTENSION_NAME)
            .expect("Failed to query XKB extension")
            .expect("X server missing XKB extension");

        // Check for XInput2 support.
        xconn
            .xcb_connection()
            .xinput_xi_query_version(2, 3)
            .expect("Failed to send XInput2 query version request")
            .reply()
            .expect("Error while checking for XInput2 query version reply");

        xconn.update_cached_wm_info(root);

        // Create an event loop.
        let event_loop = Loop::<EventLoopState>::try_new()
            .expect("Failed to initialize the event loop");
        let handle = event_loop.handle();

        // Create the X11 event dispatcher.
        let source = X11Source::new(
            // SAFETY: xcb owns the FD and outlives the source.
            unsafe { BorrowedFd::borrow_raw(xconn.xcb_connection().as_raw_fd()) },
            calloop::Interest::READ,
            calloop::Mode::Level,
        );
        handle
            .insert_source(source, |readiness, _, state| {
                state.x11_readiness = readiness;
                Ok(calloop::PostAction::Continue)
            })
            .expect("Failed to register the X11 event dispatcher");

        let (waker, waker_source) =
            calloop::ping::make_ping().expect("Failed to create event loop waker");
        event_loop
            .handle()
            .insert_source(waker_source, move |_, _, _| {
                // No extra handling is required, we just need to wake-up.
            })
            .expect("Failed to register the event loop waker source");

        // Create a channel for handling redraw requests.
        let (redraw_sender, redraw_channel) = mpsc::channel();

        // Create a channel for sending activation tokens.
        let (activation_token_sender, activation_token_channel) = mpsc::channel();

        // Create a channel for sending user events.
        let (user_sender, user_channel) = mpsc::channel();

        let xkb_context =
            Context::from_x11_xkb(xconn.xcb_connection().get_raw_xcb_connection())
                .unwrap();

        let mut xmodmap = util::ModifierKeymap::new();
        xmodmap.reload_from_x_connection(&xconn);

        let window_target = ActiveEventLoop {
            ime,
            root,
            control_flow: Cell::new(ControlFlow::default()),
            exit: Cell::new(None),
            windows: Default::default(),
            ime_sender,
            xconn,
            wm_delete_window,
            net_wm_ping,
            redraw_sender: WakeSender {
                sender: redraw_sender, // not used again so no clone
                waker: waker.clone(),
            },
            activation_sender: WakeSender {
                sender: activation_token_sender, // not used again so no clone
                waker: waker.clone(),
            },
            device_events: Default::default(),
        };

        // Set initial device event filter.
        window_target.update_listen_device_events(true);

        let root_window_target = RootAEL {
            p: PlatformActiveEventLoop::X(window_target),
            _marker: PhantomData,
        };

        let event_processor = EventProcessor {
            target: root_window_target,
            dnd,
            devices: Default::default(),
            randr_event_offset,
            ime_receiver,
            ime_event_receiver,
            xi2ext,
            xfiltered_modifiers: VecDeque::with_capacity(MAX_MOD_REPLAY_LEN),
            xmodmap,
            xkbext,
            xkb_context,
            num_touch: 0,
            held_key_press: None,
            first_touch: None,
            active_window: None,
            modifiers: Default::default(),
            is_composing: false,
        };

        // Register for device hotplug events
        // (The request buffer is flushed during `init_device`)
        let xconn = &EventProcessor::window_target(&event_processor.target).xconn;

        xconn
            .select_xinput_events(
                root,
                ALL_DEVICES,
                x11rb::protocol::xinput::XIEventMask::HIERARCHY,
            )
            .expect_then_ignore_error(
                "Failed to register for XInput2 device hotplug events",
            );

        xconn
            .select_xkb_events(
                0x100, // Use the "core keyboard device"
                xkb::EventType::NEW_KEYBOARD_NOTIFY
                    | xkb::EventType::MAP_NOTIFY
                    | xkb::EventType::STATE_NOTIFY,
            )
            .unwrap();

        event_processor.init_device(ALL_DEVICES);

        EventLoop {
            loop_running: false,
            event_loop,
            waker,
            event_processor,
            redraw_receiver: PeekableReceiver::from_recv(redraw_channel),
            activation_receiver: PeekableReceiver::from_recv(activation_token_channel),
            user_receiver: PeekableReceiver::from_recv(user_channel),
            user_sender,
            state: EventLoopState {
                x11_readiness: Readiness::EMPTY,
            },
        }
    }

    pub fn create_proxy(&self) -> EventLoopProxy<T> {
        EventLoopProxy {
            user_sender: WakeSender {
                sender: self.user_sender.clone(),
                waker: self.waker.clone(),
            },
        }
    }

    pub(crate) fn window_target(&self) -> &RootAEL {
        &self.event_processor.target
    }

    pub fn run_on_demand<F>(&mut self, mut event_handler: F) -> Result<(), EventLoopError>
    where
        F: FnMut(Event<T>, &RootAEL),
    {
        let exit = loop {
            match self.pump_events(None, &mut event_handler) {
                PumpStatus::Exit(0) => {
                    break Ok(());
                }
                PumpStatus::Exit(code) => {
                    break Err(EventLoopError::ExitFailure(code));
                }
                _ => {
                    continue;
                }
            }
        };

        // Applications aren't allowed to carry windows between separate
        // `run_on_demand` calls but if they have only just dropped their
        // windows we need to make sure those last requests are sent to the
        // X Server.
        let wt = EventProcessor::window_target(&self.event_processor.target);
        wt.x_connection().sync_with_server().map_err(|x_err| {
            EventLoopError::Os(os_error!(OsError::XError(Arc::new(X11Error::Xlib(
                x_err
            )))))
        })?;

        exit
    }

    pub fn pump_events<F>(
        &mut self,
        timeout: Option<Duration>,
        mut callback: F,
    ) -> PumpStatus
    where
        F: FnMut(Event<T>, &RootAEL),
    {
        if !self.loop_running {
            self.loop_running = true;

            // run the initial loop iteration
            self.single_iteration(&mut callback, StartCause::Init);
        }

        // Consider the possibility that the `StartCause::Init` iteration could
        // request to Exit.
        if !self.exiting() {
            self.poll_events_with_timeout(timeout, &mut callback);
        }
        if let Some(code) = self.exit_code() {
            self.loop_running = false;

            callback(Event::LoopExiting, self.window_target());

            PumpStatus::Exit(code)
        } else {
            PumpStatus::Continue
        }
    }

    fn has_pending(&mut self) -> bool {
        self.event_processor.poll()
            || self.user_receiver.has_incoming()
            || self.redraw_receiver.has_incoming()
    }

    pub fn poll_events_with_timeout<F>(
        &mut self,
        mut timeout: Option<Duration>,
        mut callback: F,
    ) where
        F: FnMut(Event<T>, &RootAEL),
    {
        let start = Instant::now();

        let has_pending = self.has_pending();

        timeout = if has_pending {
            // If we already have work to do then we don't want to block on the next poll.
            Some(Duration::ZERO)
        } else {
            let control_flow_timeout = match self.control_flow() {
                ControlFlow::Wait => None,
                ControlFlow::Poll => Some(Duration::ZERO),
                ControlFlow::WaitUntil(wait_deadline) => {
                    Some(wait_deadline.saturating_duration_since(start))
                }
            };

            min_timeout(control_flow_timeout, timeout)
        };

        self.state.x11_readiness = Readiness::EMPTY;
        if let Err(error) = self
            .event_loop
            .dispatch(timeout, &mut self.state)
            .map_err(std::io::Error::from)
        {
            tracing::error!("Failed to poll for events: {error:?}");
            let exit_code = error.raw_os_error().unwrap_or(1);
            self.set_exit_code(exit_code);
            return;
        }

        // NB: `StartCause::Init` is handled as a special case and doesn't need
        // to be considered here
        let cause = match self.control_flow() {
            ControlFlow::Poll => StartCause::Poll,
            ControlFlow::Wait => StartCause::WaitCancelled {
                start,
                requested_resume: None,
            },
            ControlFlow::WaitUntil(deadline) => {
                if Instant::now() < deadline {
                    StartCause::WaitCancelled {
                        start,
                        requested_resume: Some(deadline),
                    }
                } else {
                    StartCause::ResumeTimeReached {
                        start,
                        requested_resume: deadline,
                    }
                }
            }
        };

        // False positive / spurious wake ups could lead to us spamming
        // redundant iterations of the event loop with no new events to
        // dispatch.
        //
        // If there's no readable event source then we just double check if we
        // have any pending `_receiver` events and if not we return without
        // running a loop iteration.
        // If we don't have any pending `_receiver`
        if !self.has_pending()
            && !matches!(
                &cause,
                StartCause::ResumeTimeReached { .. } | StartCause::Poll
            )
        {
            return;
        }

        self.single_iteration(&mut callback, cause);
    }

    fn single_iteration<F>(&mut self, callback: &mut F, cause: StartCause)
    where
        F: FnMut(Event<T>, &RootAEL),
    {
        callback(Event::NewEvents(cause), &self.event_processor.target);

        // NB: For consistency all platforms must emit a 'resumed' event even though X11
        // applications don't themselves have a formal suspend/resume lifecycle.
        if cause == StartCause::Init {
            callback(Event::Resumed, &self.event_processor.target);
        }

        // Process all pending events
        self.drain_events(callback);

        // Empty activation tokens.
        while let Ok((window_id, serial)) = self.activation_receiver.try_recv() {
            let token = self
                .event_processor
                .with_window(window_id.0 as xproto::Window, |window| {
                    window.generate_activation_token()
                });

            match token {
                Some(Ok(token)) => {
                    let event = Event::WindowEvent {
                        window_id: crate::window::WindowId(window_id),
                        event: WindowEvent::ActivationTokenDone {
                            serial,
                            token: crate::window::ActivationToken::_new(token),
                        },
                    };
                    callback(event, &self.event_processor.target)
                }
                Some(Err(e)) => {
                    tracing::error!("Failed to get activation token: {}", e);
                }
                None => {}
            }
        }

        // Empty the user event buffer
        {
            while let Ok(event) = self.user_receiver.try_recv() {
                callback(Event::UserEvent(event), &self.event_processor.target);
            }
        }

        // Empty the redraw requests
        {
            let mut windows = HashSet::new();

            while let Ok(window_id) = self.redraw_receiver.try_recv() {
                windows.insert(window_id);
            }

            for window_id in windows {
                let window_id = crate::window::WindowId(window_id);
                callback(
                    Event::WindowEvent {
                        window_id,
                        event: WindowEvent::RedrawRequested,
                    },
                    &self.event_processor.target,
                );
            }
        }

        // This is always the last event we dispatch before poll again
        {
            callback(Event::AboutToWait, &self.event_processor.target);
        }
    }

    fn drain_events<F>(&mut self, callback: &mut F)
    where
        F: FnMut(Event<T>, &RootAEL),
    {
        let mut xev = MaybeUninit::uninit();

        while unsafe { self.event_processor.poll_one_event(xev.as_mut_ptr()) } {
            let mut xev = unsafe { xev.assume_init() };
            self.event_processor
                .process_event(&mut xev, |window_target, event| {
                    if let Event::WindowEvent {
                        window_id: crate::window::WindowId(wid),
                        event: WindowEvent::RedrawRequested,
                    } = event
                    {
                        let window_target = EventProcessor::window_target(window_target);
                        window_target.redraw_sender.send(wid).unwrap();
                    } else {
                        callback(event, window_target);
                    }
                });
        }
    }

    fn control_flow(&self) -> ControlFlow {
        let window_target = EventProcessor::window_target(&self.event_processor.target);
        window_target.control_flow()
    }

    fn exiting(&self) -> bool {
        let window_target = EventProcessor::window_target(&self.event_processor.target);
        window_target.exiting()
    }

    fn set_exit_code(&self, code: i32) {
        let window_target = EventProcessor::window_target(&self.event_processor.target);
        window_target.set_exit_code(code);
    }

    fn exit_code(&self) -> Option<i32> {
        let window_target = EventProcessor::window_target(&self.event_processor.target);
        window_target.exit_code()
    }
}

impl<T> AsFd for EventLoop<T> {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.event_loop.as_fd()
    }
}

impl<T> AsRawFd for EventLoop<T> {
    fn as_raw_fd(&self) -> RawFd {
        self.event_loop.as_raw_fd()
    }
}

impl ActiveEventLoop {
    /// Returns the `XConnection` of this events loop.
    #[inline]
    pub(crate) fn x_connection(&self) -> &Arc<XConnection> {
        &self.xconn
    }

    pub fn available_monitors(&self) -> impl Iterator<Item = MonitorHandle> {
        self.xconn.available_monitors().into_iter().flatten()
    }

    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        self.xconn.primary_monitor().ok()
    }

    pub(crate) fn create_custom_cursor(
        &self,
        cursor: CustomCursorSource,
    ) -> RootCustomCursor {
        RootCustomCursor {
            inner: PlatformCustomCursor::X(
                CustomCursor::new(self, cursor.inner).unwrap(),
            ),
        }
    }

    pub fn listen_device_events(&self, allowed: DeviceEvents) {
        self.device_events.set(allowed);
    }

    /// Update the device event based on window focus.
    pub fn update_listen_device_events(&self, focus: bool) {
        let device_events = self.device_events.get() == DeviceEvents::Always
            || (focus && self.device_events.get() == DeviceEvents::WhenFocused);

        let mut mask = xinput::XIEventMask::from(0u32);
        if device_events {
            mask = xinput::XIEventMask::RAW_MOTION
                | xinput::XIEventMask::RAW_BUTTON_PRESS
                | xinput::XIEventMask::RAW_BUTTON_RELEASE
                | xinput::XIEventMask::RAW_KEY_PRESS
                | xinput::XIEventMask::RAW_KEY_RELEASE;
        }

        self.xconn
            .select_xinput_events(self.root, ALL_MASTER_DEVICES, mask)
            .expect_then_ignore_error("Failed to update device event filter");
    }

    pub fn raw_display_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawDisplayHandle, raw_window_handle::HandleError> {
        let display_handle = raw_window_handle::XlibDisplayHandle::new(
            // SAFETY: display will never be null
            Some(
                std::ptr::NonNull::new(self.xconn.display as *mut _)
                    .expect("X11 display should never be null"),
            ),
            self.xconn.default_screen_index() as c_int,
        );
        Ok(display_handle.into())
    }

    pub(crate) fn set_control_flow(&self, control_flow: ControlFlow) {
        self.control_flow.set(control_flow)
    }

    pub(crate) fn control_flow(&self) -> ControlFlow {
        self.control_flow.get()
    }

    pub(crate) fn exit(&self) {
        self.exit.set(Some(0))
    }

    pub(crate) fn clear_exit(&self) {
        self.exit.set(None)
    }

    pub(crate) fn exiting(&self) -> bool {
        self.exit.get().is_some()
    }

    pub(crate) fn set_exit_code(&self, code: i32) {
        self.exit.set(Some(code))
    }

    pub(crate) fn system_theme(&self) -> Option<Theme> {
        None
    }

    pub(crate) fn exit_code(&self) -> Option<i32> {
        self.exit.get()
    }
}

impl<T: 'static> EventLoopProxy<T> {
    pub fn send_event(&self, event: T) -> Result<(), EventLoopClosed<T>> {
        self.user_sender
            .send(event)
            .map_err(|e| EventLoopClosed(e.0))
    }
}

struct DeviceInfo<'a> {
    xconn: &'a XConnection,
    info: *const ffi::XIDeviceInfo,
    count: usize,
}

impl<'a> DeviceInfo<'a> {
    fn get(xconn: &'a XConnection, device: c_int) -> Option<Self> {
        unsafe {
            let mut count = 0;
            let info = (xconn.xinput2.XIQueryDevice)(xconn.display, device, &mut count);
            xconn.check_errors().ok()?;

            if info.is_null() || count == 0 {
                None
            } else {
                Some(DeviceInfo {
                    xconn,
                    info,
                    count: count as usize,
                })
            }
        }
    }
}

impl<'a> Drop for DeviceInfo<'a> {
    fn drop(&mut self) {
        assert!(!self.info.is_null());
        unsafe { (self.xconn.xinput2.XIFreeDeviceInfo)(self.info as *mut _) };
    }
}

impl<'a> Deref for DeviceInfo<'a> {
    type Target = [ffi::XIDeviceInfo];

    fn deref(&self) -> &Self::Target {
        unsafe { slice::from_raw_parts(self.info, self.count) }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(xinput::DeviceId);

impl DeviceId {
    #[allow(unused)]
    pub const unsafe fn dummy() -> Self {
        DeviceId(0)
    }
}

pub(crate) struct Window(Arc<UnownedWindow>);

impl Deref for Window {
    type Target = UnownedWindow;

    #[inline]
    fn deref(&self) -> &UnownedWindow {
        &self.0
    }
}

impl Window {
    pub(crate) fn new(
        event_loop: &ActiveEventLoop,
        attribs: WindowAttributes,
    ) -> Result<Self, RootOsError> {
        let window = Arc::new(UnownedWindow::new(event_loop, attribs)?);
        event_loop
            .windows
            .borrow_mut()
            .insert(window.id(), Arc::downgrade(&window));
        Ok(Window(window))
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        let window = self.deref();
        let xconn = &window.xconn;

        if let Ok(c) = xconn
            .xcb_connection()
            .destroy_window(window.id().0 as xproto::Window)
        {
            c.ignore_error();
        }
    }
}

/// Generic sum error type for X11 errors.
#[derive(Debug)]
pub enum X11Error {
    /// An error from the Xlib library.
    Xlib(XError),

    /// An error that occurred while trying to connect to the X server.
    Connect(ConnectError),

    /// An error that occurred over the connection medium.
    Connection(ConnectionError),

    /// An error that occurred logically on the X11 end.
    X11(LogicalError),

    /// The XID range has been exhausted.
    XidsExhausted(IdsExhausted),

    /// Got `null` from an Xlib function without a reason.
    UnexpectedNull(&'static str),

    /// Got an invalid activation token.
    InvalidActivationToken(Vec<u8>),

    /// An extension that we rely on is not available.
    MissingExtension(&'static str),

    /// Could not find a matching X11 visual for this visualid
    NoSuchVisual(xproto::Visualid),

    /// Unable to parse xsettings.
    XsettingsParse(xsettings::ParserError),

    /// Failed to get property.
    GetProperty(util::GetPropertyError),

    /// Could not find an ARGB32 pict format.
    NoArgb32Format,
}

impl fmt::Display for X11Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            X11Error::Xlib(e) => write!(f, "Xlib error: {e}"),
            X11Error::Connect(e) => write!(f, "X11 connection error: {e}"),
            X11Error::Connection(e) => write!(f, "X11 connection error: {e}"),
            X11Error::XidsExhausted(e) => write!(f, "XID range exhausted: {e}"),
            X11Error::GetProperty(e) => write!(f, "Failed to get X property {e}"),
            X11Error::X11(e) => write!(f, "X11 error: {e:?}"),
            X11Error::UnexpectedNull(s) => {
                write!(f, "Xlib function returned null: {s}")
            }
            X11Error::InvalidActivationToken(s) => write!(
                f,
                "Invalid activation token: {}",
                std::str::from_utf8(s).unwrap_or("<invalid utf8>")
            ),
            X11Error::MissingExtension(s) => write!(f, "Missing X11 extension: {s}"),
            X11Error::NoSuchVisual(visualid) => {
                write!(
                    f,
                    "Could not find a matching X11 visual for ID `{visualid:x}`",
                )
            }
            X11Error::XsettingsParse(err) => {
                write!(f, "Failed to parse xsettings: {err:?}")
            }
            X11Error::NoArgb32Format => f.write_str(
                "winit only supports X11 displays with ARGB32 picture formats",
            ),
        }
    }
}

impl std::error::Error for X11Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            X11Error::Xlib(e) => Some(e),
            X11Error::Connect(e) => Some(e),
            X11Error::Connection(e) => Some(e),
            X11Error::XidsExhausted(e) => Some(e),
            _ => None,
        }
    }
}

impl From<XError> for X11Error {
    fn from(e: XError) -> Self {
        X11Error::Xlib(e)
    }
}

impl From<ConnectError> for X11Error {
    fn from(e: ConnectError) -> Self {
        X11Error::Connect(e)
    }
}

impl From<ConnectionError> for X11Error {
    fn from(e: ConnectionError) -> Self {
        X11Error::Connection(e)
    }
}

impl From<LogicalError> for X11Error {
    fn from(e: LogicalError) -> Self {
        X11Error::X11(e)
    }
}

impl From<ReplyError> for X11Error {
    fn from(value: ReplyError) -> Self {
        match value {
            ReplyError::ConnectionError(e) => e.into(),
            ReplyError::X11Error(e) => e.into(),
        }
    }
}

impl From<ime::ImeContextCreationError> for X11Error {
    fn from(value: ime::ImeContextCreationError) -> Self {
        match value {
            ime::ImeContextCreationError::XError(e) => e.into(),
            ime::ImeContextCreationError::Null => Self::UnexpectedNull("XOpenIM"),
        }
    }
}

impl From<ReplyOrIdError> for X11Error {
    fn from(value: ReplyOrIdError) -> Self {
        match value {
            ReplyOrIdError::ConnectionError(e) => e.into(),
            ReplyOrIdError::X11Error(e) => e.into(),
            ReplyOrIdError::IdsExhausted => Self::XidsExhausted(IdsExhausted),
        }
    }
}

impl From<xsettings::ParserError> for X11Error {
    fn from(value: xsettings::ParserError) -> Self {
        Self::XsettingsParse(value)
    }
}

impl From<util::GetPropertyError> for X11Error {
    fn from(value: util::GetPropertyError) -> Self {
        Self::GetProperty(value)
    }
}

/// Type alias for a void cookie.
type VoidCookie<'a> = x11rb::cookie::VoidCookie<'a, X11rbConnection>;

/// Extension trait for `Result<VoidCookie, E>`.
trait CookieResultExt {
    /// Unwrap the send error and ignore the result.
    fn expect_then_ignore_error(self, msg: &str);
}

impl<'a, E: fmt::Debug> CookieResultExt for Result<VoidCookie<'a>, E> {
    fn expect_then_ignore_error(self, msg: &str) {
        self.expect(msg).ignore_error()
    }
}

fn mkwid(w: xproto::Window) -> crate::window::WindowId {
    crate::window::WindowId(crate::platform_impl::platform::WindowId(w as _))
}
fn mkdid(w: xinput::DeviceId) -> crate::event::DeviceId {
    crate::event::DeviceId(crate::platform_impl::DeviceId::X(DeviceId(w)))
}

#[derive(Debug)]
pub struct Device {
    _name: String,
    scroll_axes: Vec<(i32, ScrollAxis)>,
    // For master devices, this is the paired device (pointer <-> keyboard).
    // For slave devices, this is the master.
    attachment: c_int,
}

#[derive(Debug, Copy, Clone)]
struct ScrollAxis {
    increment: f64,
    orientation: ScrollOrientation,
    position: f64,
}

#[derive(Debug, Copy, Clone)]
enum ScrollOrientation {
    Vertical,
    Horizontal,
}

impl Device {
    fn new(info: &ffi::XIDeviceInfo) -> Self {
        let name = unsafe { CStr::from_ptr(info.name).to_string_lossy() };
        let mut scroll_axes = Vec::new();

        if Device::physical_device(info) {
            // Identify scroll axes
            for &class_ptr in Device::classes(info) {
                let ty = unsafe { (*class_ptr)._type };
                if ty == ffi::XIScrollClass {
                    let info = unsafe { &*(class_ptr as *const ffi::XIScrollClassInfo) };
                    scroll_axes.push((
                        info.number,
                        ScrollAxis {
                            increment: info.increment,
                            orientation: match info.scroll_type {
                                ffi::XIScrollTypeHorizontal => {
                                    ScrollOrientation::Horizontal
                                }
                                ffi::XIScrollTypeVertical => ScrollOrientation::Vertical,
                                _ => unreachable!(),
                            },
                            position: 0.0,
                        },
                    ));
                }
            }
        }

        let mut device = Device {
            _name: name.into_owned(),
            scroll_axes,
            attachment: info.attachment,
        };
        device.reset_scroll_position(info);
        device
    }

    fn reset_scroll_position(&mut self, info: &ffi::XIDeviceInfo) {
        if Device::physical_device(info) {
            for &class_ptr in Device::classes(info) {
                let ty = unsafe { (*class_ptr)._type };
                if ty == ffi::XIValuatorClass {
                    let info =
                        unsafe { &*(class_ptr as *const ffi::XIValuatorClassInfo) };
                    if let Some(&mut (_, ref mut axis)) = self
                        .scroll_axes
                        .iter_mut()
                        .find(|&&mut (axis, _)| axis == info.number)
                    {
                        axis.position = info.value;
                    }
                }
            }
        }
    }

    #[inline]
    fn physical_device(info: &ffi::XIDeviceInfo) -> bool {
        info._use == ffi::XISlaveKeyboard
            || info._use == ffi::XISlavePointer
            || info._use == ffi::XIFloatingSlave
    }

    #[inline]
    fn classes(info: &ffi::XIDeviceInfo) -> &[*const ffi::XIAnyClassInfo] {
        unsafe {
            slice::from_raw_parts(
                info.classes as *const *const ffi::XIAnyClassInfo,
                info.num_classes as usize,
            )
        }
    }
}

/// Convert the raw X11 representation for a 32-bit floating point to a double.
#[inline]
fn xinput_fp1616_to_float(fp: xinput::Fp1616) -> f64 {
    (fp as f64) / ((1 << 16) as f64)
}
