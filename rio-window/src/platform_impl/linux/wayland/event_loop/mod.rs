//! The event-loop routines.

use std::cell::{Cell, RefCell};
use std::io::Result as IOResult;
use std::marker::PhantomData;
use std::mem;
use std::os::unix::io::{AsFd, AsRawFd, BorrowedFd, RawFd};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sctk::reexports::calloop::Error as CalloopError;
use sctk::reexports::calloop_wayland_source::WaylandSource;
use sctk::reexports::client::{globals, Connection, QueueHandle};

use crate::cursor::OnlyCursorImage;
use crate::dpi::LogicalSize;
use crate::error::{EventLoopError, OsError as RootOsError};
use crate::event::{Event, InnerSizeWriter, StartCause, WindowEvent};
use crate::event_loop::{
    ActiveEventLoop as RootActiveEventLoop, ControlFlow, DeviceEvents,
};
use crate::platform::pump_events::PumpStatus;
use crate::platform_impl::platform::min_timeout;
use crate::platform_impl::{
    ActiveEventLoop as PlatformActiveEventLoop, OsError, PlatformCustomCursor,
};
use crate::window::Theme;
use crate::window::{CustomCursor as RootCustomCursor, CustomCursorSource};

mod proxy;
pub mod sink;

pub use proxy::EventLoopProxy;
use sink::EventSink;

use super::state::{WindowCompositorUpdate, WinitState};
use super::window::state::FrameCallbackState;
use super::{logical_to_physical_rounded, DeviceId, WaylandError, WindowId};

type WaylandDispatcher =
    calloop::Dispatcher<'static, WaylandSource<WinitState>, WinitState>;

/// The Wayland event loop.
pub struct EventLoop<T: 'static> {
    /// Has `run` or `run_on_demand` been called or a call to `pump_events` that starts the loop
    loop_running: bool,

    buffer_sink: EventSink,
    compositor_updates: Vec<WindowCompositorUpdate>,
    window_ids: Vec<WindowId>,

    /// Sender of user events.
    user_events_sender: calloop::channel::Sender<T>,

    // XXX can't remove RefCell out of here, unless we can plumb generics into the `Window`, which
    // we don't really want, since it'll break public API by a lot.
    /// Pending events from the user.
    pending_user_events: Rc<RefCell<Vec<T>>>,

    /// The Wayland dispatcher to has raw access to the queue when needed, such as
    /// when creating a new window.
    wayland_dispatcher: WaylandDispatcher,

    /// Connection to the wayland server.
    connection: Connection,

    /// Event loop window target.
    window_target: RootActiveEventLoop,

    // XXX drop after everything else, just to be safe.
    /// Calloop's event loop.
    event_loop: calloop::EventLoop<'static, WinitState>,
}

impl<T: 'static> EventLoop<T> {
    pub fn new() -> Result<EventLoop<T>, EventLoopError> {
        macro_rules! map_err {
            ($e:expr, $err:expr) => {
                $e.map_err(|error| os_error!($err(error).into()))
            };
        }

        let connection =
            map_err!(Connection::connect_to_env(), WaylandError::Connection)?;

        let (globals, mut event_queue) = map_err!(
            globals::registry_queue_init(&connection),
            WaylandError::Global
        )?;
        let queue_handle = event_queue.handle();

        let event_loop = map_err!(
            calloop::EventLoop::<WinitState>::try_new(),
            WaylandError::Calloop
        )?;

        let mut winit_state =
            WinitState::new(&globals, &queue_handle, event_loop.handle())
                .map_err(|error| os_error!(error))?;

        // NOTE: do a roundtrip after binding the globals to prevent potential
        // races with the server.
        map_err!(
            event_queue.roundtrip(&mut winit_state),
            WaylandError::Dispatch
        )?;

        // Register Wayland source.
        let wayland_source = WaylandSource::new(connection.clone(), event_queue);
        let wayland_dispatcher = calloop::Dispatcher::new(
            wayland_source,
            |_, queue, winit_state: &mut WinitState| {
                let result = queue.dispatch_pending(winit_state);
                if result.is_ok()
                    && (!winit_state.events_sink.is_empty()
                        || !winit_state.window_compositor_updates.is_empty())
                {
                    winit_state.dispatched_events = true;
                }
                result
            },
        );

        map_err!(
            event_loop
                .handle()
                .register_dispatcher(wayland_dispatcher.clone()),
            WaylandError::Calloop
        )?;

        // Setup the user proxy.
        let pending_user_events = Rc::new(RefCell::new(Vec::new()));
        let pending_user_events_clone = pending_user_events.clone();
        let (user_events_sender, user_events_channel) = calloop::channel::channel();
        let result = event_loop
            .handle()
            .insert_source(
                user_events_channel,
                move |event, _, winit_state: &mut WinitState| {
                    if let calloop::channel::Event::Msg(msg) = event {
                        winit_state.dispatched_events = true;
                        pending_user_events_clone.borrow_mut().push(msg);
                    }
                },
            )
            .map_err(|error| error.error);
        map_err!(result, WaylandError::Calloop)?;

        // An event's loop awakener to wake up for window events from winit's windows.
        let (event_loop_awakener, event_loop_awakener_source) = map_err!(
            calloop::ping::make_ping()
                .map_err(|error| CalloopError::OtherError(Box::new(error).into())),
            WaylandError::Calloop
        )?;

        let result = event_loop
            .handle()
            .insert_source(
                event_loop_awakener_source,
                move |_, _, winit_state: &mut WinitState| {
                    // Mark that we have something to dispatch.
                    winit_state.dispatched_events = true;
                },
            )
            .map_err(|error| error.error);
        map_err!(result, WaylandError::Calloop)?;

        let window_target = ActiveEventLoop {
            connection: connection.clone(),
            wayland_dispatcher: wayland_dispatcher.clone(),
            event_loop_awakener,
            queue_handle,
            control_flow: Cell::new(ControlFlow::default()),
            exit: Cell::new(None),
            state: RefCell::new(winit_state),
        };

        let event_loop = Self {
            loop_running: false,
            compositor_updates: Vec::new(),
            buffer_sink: EventSink::default(),
            window_ids: Vec::new(),
            connection,
            wayland_dispatcher,
            user_events_sender,
            pending_user_events,
            event_loop,
            window_target: RootActiveEventLoop {
                p: PlatformActiveEventLoop::Wayland(Box::new(window_target)),
                _marker: PhantomData,
            },
        };

        Ok(event_loop)
    }

    pub fn run_on_demand<F>(&mut self, mut event_handler: F) -> Result<(), EventLoopError>
    where
        F: FnMut(Event<T>, &RootActiveEventLoop),
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
        // compositor.
        let _ = self.roundtrip().map_err(EventLoopError::Os);

        exit
    }

    pub fn pump_events<F>(
        &mut self,
        timeout: Option<Duration>,
        mut callback: F,
    ) -> PumpStatus
    where
        F: FnMut(Event<T>, &RootActiveEventLoop),
    {
        if !self.loop_running {
            self.loop_running = true;

            // Run the initial loop iteration.
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

    pub fn poll_events_with_timeout<F>(
        &mut self,
        mut timeout: Option<Duration>,
        mut callback: F,
    ) where
        F: FnMut(Event<T>, &RootActiveEventLoop),
    {
        let cause = loop {
            let start = Instant::now();

            timeout = {
                let control_flow_timeout = match self.control_flow() {
                    ControlFlow::Wait => None,
                    ControlFlow::Poll => Some(Duration::ZERO),
                    ControlFlow::WaitUntil(wait_deadline) => {
                        Some(wait_deadline.saturating_duration_since(start))
                    }
                };
                min_timeout(control_flow_timeout, timeout)
            };

            // NOTE Ideally we should flush as the last thing we do before polling
            // to wait for events, and this should be done by the calloop
            // WaylandSource but we currently need to flush writes manually.
            //
            // Checking for flush error is essential to perform an exit with error, since
            // once we have a protocol error, we could get stuck retrying...
            if self.connection.flush().is_err() {
                self.set_exit_code(1);
                return;
            }

            if let Err(error) = self.loop_dispatch(timeout) {
                // NOTE We exit on errors from dispatches, since if we've got protocol error
                // libwayland-client/wayland-rs will inform us anyway, but crashing downstream is
                // not really an option. Instead we inform that the event loop got
                // destroyed. We may communicate an error that something was
                // terminated, but winit doesn't provide us with an API to do that
                // via some event. Still, we set the exit code to the error's OS
                // error code, or to 1 if not possible.
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

            // Reduce spurious wake-ups.
            let dispatched_events = self.with_state(|state| state.dispatched_events);
            if matches!(cause, StartCause::WaitCancelled { .. }) && !dispatched_events {
                continue;
            }

            break cause;
        };

        self.single_iteration(&mut callback, cause);
    }

    fn single_iteration<F>(&mut self, callback: &mut F, cause: StartCause)
    where
        F: FnMut(Event<T>, &RootActiveEventLoop),
    {
        // NOTE currently just indented to simplify the diff

        // We retain these grow-only scratch buffers as part of the EventLoop
        // for the sake of avoiding lots of reallocs. We take them here to avoid
        // trying to mutably borrow `self` more than once and we swap them back
        // when finished.
        let mut compositor_updates = std::mem::take(&mut self.compositor_updates);
        let mut buffer_sink = std::mem::take(&mut self.buffer_sink);
        let mut window_ids = std::mem::take(&mut self.window_ids);

        callback(Event::NewEvents(cause), &self.window_target);

        // NB: For consistency all platforms must emit a 'resumed' event even though Wayland
        // applications don't themselves have a formal suspend/resume lifecycle.
        if cause == StartCause::Init {
            callback(Event::Resumed, &self.window_target);
        }

        // Handle pending user events. We don't need back buffer, since we can't dispatch
        // user events indirectly via callback to the user.
        for user_event in self.pending_user_events.borrow_mut().drain(..) {
            callback(Event::UserEvent(user_event), &self.window_target);
        }

        // Drain the pending compositor updates.
        self.with_state(|state| {
            compositor_updates.append(&mut state.window_compositor_updates)
        });

        for mut compositor_update in compositor_updates.drain(..) {
            let window_id = compositor_update.window_id;
            if compositor_update.scale_changed {
                let (physical_size, scale_factor) = self.with_state(|state| {
                    let windows = state.windows.get_mut();
                    let window = windows.get(&window_id).unwrap().lock().unwrap();
                    let scale_factor = window.scale_factor();
                    let size =
                        logical_to_physical_rounded(window.inner_size(), scale_factor);
                    (size, scale_factor)
                });

                // Stash the old window size.
                let old_physical_size = physical_size;

                let new_inner_size = Arc::new(Mutex::new(physical_size));
                callback(
                    Event::WindowEvent {
                        window_id: crate::window::WindowId(window_id),
                        event: WindowEvent::ScaleFactorChanged {
                            scale_factor,
                            inner_size_writer: InnerSizeWriter::new(Arc::downgrade(
                                &new_inner_size,
                            )),
                        },
                    },
                    &self.window_target,
                );

                let physical_size = *new_inner_size.lock().unwrap();
                drop(new_inner_size);

                // Resize the window when user altered the size.
                if old_physical_size != physical_size {
                    self.with_state(|state| {
                        let windows = state.windows.get_mut();
                        let mut window = windows.get(&window_id).unwrap().lock().unwrap();

                        let new_logical_size: LogicalSize<f64> =
                            physical_size.to_logical(scale_factor);
                        window.request_inner_size(new_logical_size.into());
                    });

                    // Make it queue resize.
                    compositor_update.resized = true;
                }
            }

            // NOTE: Rescale changed the physical size which winit operates in, thus we should
            // resize.
            if compositor_update.resized || compositor_update.scale_changed {
                let physical_size = self.with_state(|state| {
                    let windows = state.windows.get_mut();
                    let window = windows.get(&window_id).unwrap().lock().unwrap();

                    let scale_factor = window.scale_factor();
                    let size =
                        logical_to_physical_rounded(window.inner_size(), scale_factor);

                    // Mark the window as needed a redraw.
                    state
                        .window_requests
                        .get_mut()
                        .get_mut(&window_id)
                        .unwrap()
                        .redraw_requested
                        .store(true, Ordering::Relaxed);

                    size
                });

                callback(
                    Event::WindowEvent {
                        window_id: crate::window::WindowId(window_id),
                        event: WindowEvent::Resized(physical_size),
                    },
                    &self.window_target,
                );
            }

            if compositor_update.close_window {
                callback(
                    Event::WindowEvent {
                        window_id: crate::window::WindowId(window_id),
                        event: WindowEvent::CloseRequested,
                    },
                    &self.window_target,
                );
            }
        }

        // Push the events directly from the window.
        self.with_state(|state| {
            buffer_sink.append(&mut state.window_events_sink.lock().unwrap());
        });
        for event in buffer_sink.drain() {
            let event = event.map_nonuser_event().unwrap();
            callback(event, &self.window_target);
        }

        // Handle non-synthetic events.
        self.with_state(|state| {
            buffer_sink.append(&mut state.events_sink);
        });
        for event in buffer_sink.drain() {
            let event = event.map_nonuser_event().unwrap();
            callback(event, &self.window_target);
        }

        // Collect the window ids
        self.with_state(|state| {
            window_ids.extend(state.window_requests.get_mut().keys());
        });

        for window_id in window_ids.iter() {
            let event = self.with_state(|state| {
                let window_requests = state.window_requests.get_mut();
                if window_requests.get(window_id).unwrap().take_closed() {
                    mem::drop(window_requests.remove(window_id));
                    mem::drop(state.windows.get_mut().remove(window_id));
                    return Some(WindowEvent::Destroyed);
                }

                let mut window = state
                    .windows
                    .get_mut()
                    .get_mut(window_id)
                    .unwrap()
                    .lock()
                    .unwrap();

                if window.frame_callback_state() == FrameCallbackState::Requested {
                    return None;
                }

                // Reset the frame callbacks state.
                window.frame_callback_reset();
                let mut redraw_requested = window_requests
                    .get(window_id)
                    .unwrap()
                    .take_redraw_requested();

                // Redraw the frame while at it.
                redraw_requested |= window.refresh_frame();

                redraw_requested.then_some(WindowEvent::RedrawRequested)
            });

            if let Some(event) = event {
                callback(
                    Event::WindowEvent {
                        window_id: crate::window::WindowId(*window_id),
                        event,
                    },
                    &self.window_target,
                );
            }
        }

        // Reset the hint that we've dispatched events.
        self.with_state(|state| {
            state.dispatched_events = false;
        });

        // This is always the last event we dispatch before poll again
        callback(Event::AboutToWait, &self.window_target);

        // Update the window frames and schedule redraws.
        let mut wake_up = false;
        for window_id in window_ids.drain(..) {
            wake_up |= self.with_state(|state| {
                match state.windows.get_mut().get_mut(&window_id) {
                    Some(window) => {
                        let refresh = window.lock().unwrap().refresh_frame();
                        if refresh {
                            state
                                .window_requests
                                .get_mut()
                                .get_mut(&window_id)
                                .unwrap()
                                .redraw_requested
                                .store(true, Ordering::Relaxed);
                        }

                        refresh
                    }
                    None => false,
                }
            });
        }

        // Wakeup event loop if needed.
        //
        // If the user draws from the `AboutToWait` this is likely not required, however
        // we can't do much about it.
        if wake_up {
            match &self.window_target.p {
                PlatformActiveEventLoop::Wayland(window_target) => {
                    window_target.event_loop_awakener.ping();
                }
                #[cfg(x11_platform)]
                PlatformActiveEventLoop::X(_) => unreachable!(),
            }
        }

        std::mem::swap(&mut self.compositor_updates, &mut compositor_updates);
        std::mem::swap(&mut self.buffer_sink, &mut buffer_sink);
        std::mem::swap(&mut self.window_ids, &mut window_ids);
    }

    #[inline]
    pub fn create_proxy(&self) -> EventLoopProxy<T> {
        EventLoopProxy::new(self.user_events_sender.clone())
    }

    #[inline]
    pub fn window_target(&self) -> &RootActiveEventLoop {
        &self.window_target
    }

    fn with_state<'a, U: 'a, F: FnOnce(&'a mut WinitState) -> U>(
        &'a mut self,
        callback: F,
    ) -> U {
        let state = match &mut self.window_target.p {
            PlatformActiveEventLoop::Wayland(window_target) => {
                window_target.state.get_mut()
            }
            #[cfg(x11_platform)]
            _ => unreachable!(),
        };

        callback(state)
    }

    fn loop_dispatch<D: Into<Option<std::time::Duration>>>(
        &mut self,
        timeout: D,
    ) -> IOResult<()> {
        let state = match &mut self.window_target.p {
            PlatformActiveEventLoop::Wayland(window_target) => {
                window_target.state.get_mut()
            }
            #[cfg(feature = "x11")]
            _ => unreachable!(),
        };

        self.event_loop.dispatch(timeout, state).map_err(|error| {
            tracing::error!("Error dispatching event loop: {}", error);
            error.into()
        })
    }

    fn roundtrip(&mut self) -> Result<usize, RootOsError> {
        let state = match &mut self.window_target.p {
            PlatformActiveEventLoop::Wayland(window_target) => {
                window_target.state.get_mut()
            }
            #[cfg(feature = "x11")]
            _ => unreachable!(),
        };

        let mut wayland_source = self.wayland_dispatcher.as_source_mut();
        let event_queue = wayland_source.queue();
        event_queue.roundtrip(state).map_err(|error| {
            os_error!(OsError::WaylandError(Arc::new(WaylandError::Dispatch(
                error
            ))))
        })
    }

    fn control_flow(&self) -> ControlFlow {
        self.window_target.p.control_flow()
    }

    fn exiting(&self) -> bool {
        self.window_target.p.exiting()
    }

    fn set_exit_code(&self, code: i32) {
        self.window_target.p.set_exit_code(code)
    }

    fn exit_code(&self) -> Option<i32> {
        self.window_target.p.exit_code()
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

pub struct ActiveEventLoop {
    /// The event loop wakeup source.
    pub event_loop_awakener: calloop::ping::Ping,

    /// The main queue used by the event loop.
    pub queue_handle: QueueHandle<WinitState>,

    /// The application's latest control_flow state
    pub(crate) control_flow: Cell<ControlFlow>,

    /// The application's exit state.
    pub(crate) exit: Cell<Option<i32>>,

    // TODO remove that RefCell once we can pass `&mut` in `Window::new`.
    /// Winit state.
    pub state: RefCell<WinitState>,

    /// Dispatcher of Wayland events.
    pub wayland_dispatcher: WaylandDispatcher,

    /// Connection to the wayland server.
    pub connection: Connection,
}

impl ActiveEventLoop {
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

    pub(crate) fn exit_code(&self) -> Option<i32> {
        self.exit.get()
    }

    pub(crate) fn system_theme(&self) -> Option<Theme> {
        None
    }

    #[inline]
    pub fn listen_device_events(&self, _allowed: DeviceEvents) {}

    pub(crate) fn create_custom_cursor(
        &self,
        cursor: CustomCursorSource,
    ) -> RootCustomCursor {
        RootCustomCursor {
            inner: PlatformCustomCursor::Wayland(OnlyCursorImage(Arc::from(
                cursor.inner.0,
            ))),
        }
    }

    #[inline]
    pub fn raw_display_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawDisplayHandle, raw_window_handle::HandleError> {
        use sctk::reexports::client::Proxy;

        Ok(raw_window_handle::WaylandDisplayHandle::new({
            let ptr = self.connection.display().id().as_ptr();
            std::ptr::NonNull::new(ptr as *mut _)
                .expect("wl_display should never be null")
        })
        .into())
    }
}
