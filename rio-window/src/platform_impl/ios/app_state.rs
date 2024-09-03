#![deny(unused_results)]

use std::cell::{RefCell, RefMut};
use std::collections::HashSet;
use std::os::raw::c_void;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use std::{fmt, mem, ptr};

use core_foundation::base::CFRelease;
use core_foundation::date::CFAbsoluteTimeGetCurrent;
use core_foundation::runloop::{
    kCFRunLoopCommonModes, CFRunLoopAddTimer, CFRunLoopGetMain, CFRunLoopRef,
    CFRunLoopTimerCreate, CFRunLoopTimerInvalidate, CFRunLoopTimerRef,
    CFRunLoopTimerSetNextFireDate,
};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, sel};
use objc2_foundation::{
    CGRect, CGSize, MainThreadMarker, NSInteger, NSObjectProtocol,
    NSOperatingSystemVersion, NSProcessInfo,
};
use objc2_ui_kit::{UICoordinateSpace, UIView};

use super::window::WinitUIWindow;
use crate::dpi::PhysicalSize;
use crate::event::{Event, InnerSizeWriter, StartCause, WindowEvent};
use crate::event_loop::{ActiveEventLoop as RootActiveEventLoop, ControlFlow};
use crate::window::WindowId as RootWindowId;

macro_rules! bug {
    ($($msg:tt)*) => {
        panic!("winit iOS bug, file an issue: {}", format!($($msg)*))
    };
}

macro_rules! bug_assert {
    ($test:expr, $($msg:tt)*) => {
        assert!($test, "winit iOS bug, file an issue: {}", format!($($msg)*))
    };
}

#[derive(Debug)]
pub(crate) struct HandlePendingUserEvents;

pub(crate) struct EventLoopHandler {
    #[allow(clippy::type_complexity)]
    pub(crate) handler:
        Box<dyn FnMut(Event<HandlePendingUserEvents>, &RootActiveEventLoop)>,
    pub(crate) event_loop: RootActiveEventLoop,
}

impl fmt::Debug for EventLoopHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventLoopHandler")
            .field("handler", &"...")
            .field("event_loop", &self.event_loop)
            .finish()
    }
}

impl EventLoopHandler {
    fn handle_event(&mut self, event: Event<HandlePendingUserEvents>) {
        (self.handler)(event, &self.event_loop)
    }
}

#[derive(Debug)]
pub(crate) enum EventWrapper {
    StaticEvent(Event<HandlePendingUserEvents>),
    ScaleFactorChanged(ScaleFactorChanged),
}

#[derive(Debug)]
pub struct ScaleFactorChanged {
    pub(super) window: Retained<WinitUIWindow>,
    pub(super) suggested_size: PhysicalSize<u32>,
    pub(super) scale_factor: f64,
}

enum UserCallbackTransitionResult<'a> {
    Success {
        handler: EventLoopHandler,
        active_control_flow: ControlFlow,
        processing_redraws: bool,
    },
    ReentrancyPrevented {
        queued_events: &'a mut Vec<EventWrapper>,
    },
}

impl Event<HandlePendingUserEvents> {
    fn is_redraw(&self) -> bool {
        matches!(
            self,
            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            }
        )
    }
}

// this is the state machine for the app lifecycle
#[derive(Debug)]
#[must_use = "dropping `AppStateImpl` without inspecting it is probably a bug"]
enum AppStateImpl {
    NotLaunched {
        queued_windows: Vec<Retained<WinitUIWindow>>,
        queued_events: Vec<EventWrapper>,
        queued_gpu_redraws: HashSet<Retained<WinitUIWindow>>,
    },
    Launching {
        queued_windows: Vec<Retained<WinitUIWindow>>,
        queued_events: Vec<EventWrapper>,
        queued_handler: EventLoopHandler,
        queued_gpu_redraws: HashSet<Retained<WinitUIWindow>>,
    },
    ProcessingEvents {
        handler: EventLoopHandler,
        queued_gpu_redraws: HashSet<Retained<WinitUIWindow>>,
        active_control_flow: ControlFlow,
    },
    // special state to deal with reentrancy and prevent mutable aliasing.
    InUserCallback {
        queued_events: Vec<EventWrapper>,
        queued_gpu_redraws: HashSet<Retained<WinitUIWindow>>,
    },
    ProcessingRedraws {
        handler: EventLoopHandler,
        active_control_flow: ControlFlow,
    },
    Waiting {
        waiting_handler: EventLoopHandler,
        start: Instant,
    },
    PollFinished {
        waiting_handler: EventLoopHandler,
    },
    Terminated,
}

pub(crate) struct AppState {
    // This should never be `None`, except for briefly during a state transition.
    app_state: Option<AppStateImpl>,
    control_flow: ControlFlow,
    waker: EventLoopWaker,
}

impl AppState {
    pub(crate) fn get_mut(_mtm: MainThreadMarker) -> RefMut<'static, AppState> {
        // basically everything in UIKit requires the main thread, so it's pointless to use the
        // std::sync APIs.
        // must be mut because plain `static` requires `Sync`
        static mut APP_STATE: RefCell<Option<AppState>> = RefCell::new(None);

        let mut guard = unsafe { APP_STATE.borrow_mut() };
        if guard.is_none() {
            #[inline(never)]
            #[cold]
            fn init_guard(guard: &mut RefMut<'static, Option<AppState>>) {
                let waker = EventLoopWaker::new(unsafe { CFRunLoopGetMain() });
                **guard = Some(AppState {
                    app_state: Some(AppStateImpl::NotLaunched {
                        queued_windows: Vec::new(),
                        queued_events: Vec::new(),
                        queued_gpu_redraws: HashSet::new(),
                    }),
                    control_flow: ControlFlow::default(),
                    waker,
                });
            }
            init_guard(&mut guard);
        }
        RefMut::map(guard, |state| state.as_mut().unwrap())
    }

    fn state(&self) -> &AppStateImpl {
        match &self.app_state {
            Some(ref state) => state,
            None => bug!("`AppState` previously failed a state transition"),
        }
    }

    fn state_mut(&mut self) -> &mut AppStateImpl {
        match &mut self.app_state {
            Some(ref mut state) => state,
            None => bug!("`AppState` previously failed a state transition"),
        }
    }

    fn take_state(&mut self) -> AppStateImpl {
        match self.app_state.take() {
            Some(state) => state,
            None => bug!("`AppState` previously failed a state transition"),
        }
    }

    fn set_state(&mut self, new_state: AppStateImpl) {
        bug_assert!(
            self.app_state.is_none(),
            "attempted to set an `AppState` without calling `take_state` first {:?}",
            self.app_state
        );
        self.app_state = Some(new_state)
    }

    fn replace_state(&mut self, new_state: AppStateImpl) -> AppStateImpl {
        match &mut self.app_state {
            Some(ref mut state) => mem::replace(state, new_state),
            None => bug!("`AppState` previously failed a state transition"),
        }
    }

    fn has_launched(&self) -> bool {
        !matches!(
            self.state(),
            AppStateImpl::NotLaunched { .. } | AppStateImpl::Launching { .. }
        )
    }

    fn has_terminated(&self) -> bool {
        matches!(self.state(), AppStateImpl::Terminated)
    }

    fn will_launch_transition(&mut self, queued_handler: EventLoopHandler) {
        let (queued_windows, queued_events, queued_gpu_redraws) = match self.take_state()
        {
            AppStateImpl::NotLaunched {
                queued_windows,
                queued_events,
                queued_gpu_redraws,
            } => (queued_windows, queued_events, queued_gpu_redraws),
            s => bug!("unexpected state {:?}", s),
        };
        self.set_state(AppStateImpl::Launching {
            queued_windows,
            queued_events,
            queued_handler,
            queued_gpu_redraws,
        });
    }

    fn did_finish_launching_transition(
        &mut self,
    ) -> (Vec<Retained<WinitUIWindow>>, Vec<EventWrapper>) {
        let (windows, events, handler, queued_gpu_redraws) = match self.take_state() {
            AppStateImpl::Launching {
                queued_windows,
                queued_events,
                queued_handler,
                queued_gpu_redraws,
            } => (
                queued_windows,
                queued_events,
                queued_handler,
                queued_gpu_redraws,
            ),
            s => bug!("unexpected state {:?}", s),
        };
        self.set_state(AppStateImpl::ProcessingEvents {
            handler,
            active_control_flow: self.control_flow,
            queued_gpu_redraws,
        });
        (windows, events)
    }

    fn wakeup_transition(&mut self) -> Option<EventWrapper> {
        // before `AppState::did_finish_launching` is called, pretend there is no running
        // event loop.
        if !self.has_launched() || self.has_terminated() {
            return None;
        }

        let (handler, event) = match (self.control_flow, self.take_state()) {
            (ControlFlow::Poll, AppStateImpl::PollFinished { waiting_handler }) => (
                waiting_handler,
                EventWrapper::StaticEvent(Event::NewEvents(StartCause::Poll)),
            ),
            (
                ControlFlow::Wait,
                AppStateImpl::Waiting {
                    waiting_handler,
                    start,
                },
            ) => (
                waiting_handler,
                EventWrapper::StaticEvent(Event::NewEvents(StartCause::WaitCancelled {
                    start,
                    requested_resume: None,
                })),
            ),
            (
                ControlFlow::WaitUntil(requested_resume),
                AppStateImpl::Waiting {
                    waiting_handler,
                    start,
                },
            ) => {
                let event = if Instant::now() >= requested_resume {
                    EventWrapper::StaticEvent(Event::NewEvents(
                        StartCause::ResumeTimeReached {
                            start,
                            requested_resume,
                        },
                    ))
                } else {
                    EventWrapper::StaticEvent(Event::NewEvents(
                        StartCause::WaitCancelled {
                            start,
                            requested_resume: Some(requested_resume),
                        },
                    ))
                };
                (waiting_handler, event)
            }
            s => bug!("`EventHandler` unexpectedly woke up {:?}", s),
        };

        self.set_state(AppStateImpl::ProcessingEvents {
            handler,
            queued_gpu_redraws: Default::default(),
            active_control_flow: self.control_flow,
        });
        Some(event)
    }

    fn try_user_callback_transition(&mut self) -> UserCallbackTransitionResult<'_> {
        // If we're not able to process an event due to recursion or `Init` not having been sent out
        // yet, then queue the events up.
        match self.state_mut() {
            &mut AppStateImpl::Launching {
                ref mut queued_events,
                ..
            }
            | &mut AppStateImpl::NotLaunched {
                ref mut queued_events,
                ..
            }
            | &mut AppStateImpl::InUserCallback {
                ref mut queued_events,
                ..
            } => {
                // A lifetime cast: early returns are not currently handled well with NLL, but
                // polonius handles them well. This transmute is a safe workaround.
                return unsafe {
                    mem::transmute::<
                        UserCallbackTransitionResult<'_>,
                        UserCallbackTransitionResult<'_>,
                    >(
                        UserCallbackTransitionResult::ReentrancyPrevented {
                            queued_events,
                        },
                    )
                };
            }

            &mut AppStateImpl::ProcessingEvents { .. }
            | &mut AppStateImpl::ProcessingRedraws { .. } => {}

            s @ &mut AppStateImpl::PollFinished { .. }
            | s @ &mut AppStateImpl::Waiting { .. }
            | s @ &mut AppStateImpl::Terminated => {
                bug!("unexpected attempted to process an event {:?}", s)
            }
        }

        let (handler, queued_gpu_redraws, active_control_flow, processing_redraws) =
            match self.take_state() {
                AppStateImpl::Launching { .. }
                | AppStateImpl::NotLaunched { .. }
                | AppStateImpl::InUserCallback { .. } => unreachable!(),
                AppStateImpl::ProcessingEvents {
                    handler,
                    queued_gpu_redraws,
                    active_control_flow,
                } => (handler, queued_gpu_redraws, active_control_flow, false),
                AppStateImpl::ProcessingRedraws {
                    handler,
                    active_control_flow,
                } => (handler, Default::default(), active_control_flow, true),
                AppStateImpl::PollFinished { .. }
                | AppStateImpl::Waiting { .. }
                | AppStateImpl::Terminated => unreachable!(),
            };
        self.set_state(AppStateImpl::InUserCallback {
            queued_events: Vec::new(),
            queued_gpu_redraws,
        });
        UserCallbackTransitionResult::Success {
            handler,
            active_control_flow,
            processing_redraws,
        }
    }

    fn main_events_cleared_transition(&mut self) -> HashSet<Retained<WinitUIWindow>> {
        let (handler, queued_gpu_redraws, active_control_flow) = match self.take_state() {
            AppStateImpl::ProcessingEvents {
                handler,
                queued_gpu_redraws,
                active_control_flow,
            } => (handler, queued_gpu_redraws, active_control_flow),
            s => bug!("unexpected state {:?}", s),
        };
        self.set_state(AppStateImpl::ProcessingRedraws {
            handler,
            active_control_flow,
        });
        queued_gpu_redraws
    }

    fn events_cleared_transition(&mut self) {
        if !self.has_launched() || self.has_terminated() {
            return;
        }
        let (waiting_handler, old) = match self.take_state() {
            AppStateImpl::ProcessingRedraws {
                handler,
                active_control_flow,
            } => (handler, active_control_flow),
            s => bug!("unexpected state {:?}", s),
        };

        let new = self.control_flow;
        match (old, new) {
            (ControlFlow::Wait, ControlFlow::Wait) => {
                let start = Instant::now();
                self.set_state(AppStateImpl::Waiting {
                    waiting_handler,
                    start,
                });
            }
            (
                ControlFlow::WaitUntil(old_instant),
                ControlFlow::WaitUntil(new_instant),
            ) if old_instant == new_instant => {
                let start = Instant::now();
                self.set_state(AppStateImpl::Waiting {
                    waiting_handler,
                    start,
                });
            }
            (_, ControlFlow::Wait) => {
                let start = Instant::now();
                self.set_state(AppStateImpl::Waiting {
                    waiting_handler,
                    start,
                });
                self.waker.stop()
            }
            (_, ControlFlow::WaitUntil(new_instant)) => {
                let start = Instant::now();
                self.set_state(AppStateImpl::Waiting {
                    waiting_handler,
                    start,
                });
                self.waker.start_at(new_instant)
            }
            // Unlike on macOS, handle Poll to Poll transition here to call the waker
            (_, ControlFlow::Poll) => {
                self.set_state(AppStateImpl::PollFinished { waiting_handler });
                self.waker.start()
            }
        }
    }

    fn terminated_transition(&mut self) -> EventLoopHandler {
        match self.replace_state(AppStateImpl::Terminated) {
            AppStateImpl::ProcessingEvents { handler, .. } => handler,
            s => bug!("`LoopExiting` happened while not processing events {:?}", s),
        }
    }

    pub(crate) fn set_control_flow(&mut self, control_flow: ControlFlow) {
        self.control_flow = control_flow;
    }

    pub(crate) fn control_flow(&self) -> ControlFlow {
        self.control_flow
    }
}

pub(crate) fn set_key_window(mtm: MainThreadMarker, window: &Retained<WinitUIWindow>) {
    let mut this = AppState::get_mut(mtm);
    match this.state_mut() {
        &mut AppStateImpl::NotLaunched {
            ref mut queued_windows,
            ..
        } => return queued_windows.push(window.clone()),
        &mut AppStateImpl::ProcessingEvents { .. }
        | &mut AppStateImpl::InUserCallback { .. }
        | &mut AppStateImpl::ProcessingRedraws { .. } => {}
        s @ &mut AppStateImpl::Launching { .. }
        | s @ &mut AppStateImpl::Waiting { .. }
        | s @ &mut AppStateImpl::PollFinished { .. } => bug!("unexpected state {:?}", s),
        &mut AppStateImpl::Terminated => {
            panic!("Attempt to create a `Window` after the app has terminated")
        }
    }
    drop(this);
    window.makeKeyAndVisible();
}

pub(crate) fn queue_gl_or_metal_redraw(
    mtm: MainThreadMarker,
    window: Retained<WinitUIWindow>,
) {
    let mut this = AppState::get_mut(mtm);
    match this.state_mut() {
        &mut AppStateImpl::NotLaunched {
            ref mut queued_gpu_redraws,
            ..
        }
        | &mut AppStateImpl::Launching {
            ref mut queued_gpu_redraws,
            ..
        }
        | &mut AppStateImpl::ProcessingEvents {
            ref mut queued_gpu_redraws,
            ..
        }
        | &mut AppStateImpl::InUserCallback {
            ref mut queued_gpu_redraws,
            ..
        } => {
            let _ = queued_gpu_redraws.insert(window);
        }
        s @ &mut AppStateImpl::ProcessingRedraws { .. }
        | s @ &mut AppStateImpl::Waiting { .. }
        | s @ &mut AppStateImpl::PollFinished { .. } => bug!("unexpected state {:?}", s),
        &mut AppStateImpl::Terminated => {
            panic!("Attempt to create a `Window` after the app has terminated")
        }
    }
}

pub(crate) fn will_launch(mtm: MainThreadMarker, queued_handler: EventLoopHandler) {
    AppState::get_mut(mtm).will_launch_transition(queued_handler)
}

pub fn did_finish_launching(mtm: MainThreadMarker) {
    let mut this = AppState::get_mut(mtm);
    let windows = match this.state_mut() {
        AppStateImpl::Launching { queued_windows, .. } => mem::take(queued_windows),
        s => bug!("unexpected state {:?}", s),
    };

    this.waker.start();

    // have to drop RefMut because the window setup code below can trigger new events
    drop(this);

    for window in windows {
        // Do a little screen dance here to account for windows being created before
        // `UIApplicationMain` is called. This fixes visual issues such as being
        // offcenter and sized incorrectly. Additionally, to fix orientation issues, we
        // gotta reset the `rootViewController`.
        //
        // relevant iOS log:
        // ```
        // [ApplicationLifecycle] Windows were created before application initialization
        // completed. This may result in incorrect visual appearance.
        // ```
        let screen = window.screen();
        let _: () = unsafe { msg_send![&window, setScreen: ptr::null::<AnyObject>()] };
        window.setScreen(&screen);

        let controller = window.rootViewController();
        window.setRootViewController(None);
        window.setRootViewController(controller.as_deref());

        window.makeKeyAndVisible();
    }

    let (windows, events) = AppState::get_mut(mtm).did_finish_launching_transition();

    let events = std::iter::once(EventWrapper::StaticEvent(Event::NewEvents(
        StartCause::Init,
    )))
    .chain(events);
    handle_nonuser_events(mtm, events);

    // the above window dance hack, could possibly trigger new windows to be created.
    // we can just set those windows up normally, as they were created after didFinishLaunching
    for window in windows {
        window.makeKeyAndVisible();
    }
}

// AppState::did_finish_launching handles the special transition `Init`
pub fn handle_wakeup_transition(mtm: MainThreadMarker) {
    let mut this = AppState::get_mut(mtm);
    let wakeup_event = match this.wakeup_transition() {
        None => return,
        Some(wakeup_event) => wakeup_event,
    };
    drop(this);

    handle_nonuser_event(mtm, wakeup_event)
}

pub(crate) fn handle_nonuser_event(mtm: MainThreadMarker, event: EventWrapper) {
    handle_nonuser_events(mtm, std::iter::once(event))
}

pub(crate) fn handle_nonuser_events<I: IntoIterator<Item = EventWrapper>>(
    mtm: MainThreadMarker,
    events: I,
) {
    let mut this = AppState::get_mut(mtm);
    if this.has_terminated() {
        return;
    }

    let (mut handler, active_control_flow, processing_redraws) =
        match this.try_user_callback_transition() {
            UserCallbackTransitionResult::ReentrancyPrevented { queued_events } => {
                queued_events.extend(events);
                return;
            }
            UserCallbackTransitionResult::Success {
                handler,
                active_control_flow,
                processing_redraws,
            } => (handler, active_control_flow, processing_redraws),
        };
    drop(this);

    for wrapper in events {
        match wrapper {
            EventWrapper::StaticEvent(event) => {
                if !processing_redraws && event.is_redraw() {
                    tracing::info!(
                        "processing `RedrawRequested` during the main event loop"
                    );
                } else if processing_redraws && !event.is_redraw() {
                    tracing::warn!(
                        "processing non `RedrawRequested` event after the main event loop: {:#?}",
                        event
                    );
                }
                handler.handle_event(event)
            }
            EventWrapper::ScaleFactorChanged(event) => {
                handle_hidpi_proxy(&mut handler, event)
            }
        }
    }

    loop {
        let mut this = AppState::get_mut(mtm);
        let queued_events = match this.state_mut() {
            &mut AppStateImpl::InUserCallback {
                ref mut queued_events,
                queued_gpu_redraws: _,
            } => mem::take(queued_events),
            s => bug!("unexpected state {:?}", s),
        };
        if queued_events.is_empty() {
            let queued_gpu_redraws = match this.take_state() {
                AppStateImpl::InUserCallback {
                    queued_events: _,
                    queued_gpu_redraws,
                } => queued_gpu_redraws,
                _ => unreachable!(),
            };
            this.app_state = Some(if processing_redraws {
                bug_assert!(
                    queued_gpu_redraws.is_empty(),
                    "redraw queued while processing redraws"
                );
                AppStateImpl::ProcessingRedraws {
                    handler,
                    active_control_flow,
                }
            } else {
                AppStateImpl::ProcessingEvents {
                    handler,
                    queued_gpu_redraws,
                    active_control_flow,
                }
            });
            break;
        }
        drop(this);

        for wrapper in queued_events {
            match wrapper {
                EventWrapper::StaticEvent(event) => {
                    if !processing_redraws && event.is_redraw() {
                        tracing::info!(
                            "processing `RedrawRequested` during the main event loop"
                        );
                    } else if processing_redraws && !event.is_redraw() {
                        tracing::warn!(
                            "processing non-`RedrawRequested` event after the main event loop: \
                             {:#?}",
                            event
                        );
                    }
                    handler.handle_event(event)
                }
                EventWrapper::ScaleFactorChanged(event) => {
                    handle_hidpi_proxy(&mut handler, event)
                }
            }
        }
    }
}

fn handle_user_events(mtm: MainThreadMarker) {
    let mut this = AppState::get_mut(mtm);
    let (mut handler, active_control_flow, processing_redraws) =
        match this.try_user_callback_transition() {
            UserCallbackTransitionResult::ReentrancyPrevented { .. } => {
                bug!("unexpected attempted to process an event")
            }
            UserCallbackTransitionResult::Success {
                handler,
                active_control_flow,
                processing_redraws,
            } => (handler, active_control_flow, processing_redraws),
        };
    if processing_redraws {
        bug!("user events attempted to be sent out while `ProcessingRedraws`");
    }
    drop(this);

    handler.handle_event(Event::UserEvent(HandlePendingUserEvents));

    loop {
        let mut this = AppState::get_mut(mtm);
        let queued_events = match this.state_mut() {
            &mut AppStateImpl::InUserCallback {
                ref mut queued_events,
                queued_gpu_redraws: _,
            } => mem::take(queued_events),
            s => bug!("unexpected state {:?}", s),
        };
        if queued_events.is_empty() {
            let queued_gpu_redraws = match this.take_state() {
                AppStateImpl::InUserCallback {
                    queued_events: _,
                    queued_gpu_redraws,
                } => queued_gpu_redraws,
                _ => unreachable!(),
            };
            this.app_state = Some(AppStateImpl::ProcessingEvents {
                handler,
                queued_gpu_redraws,
                active_control_flow,
            });
            break;
        }
        drop(this);

        for wrapper in queued_events {
            match wrapper {
                EventWrapper::StaticEvent(event) => handler.handle_event(event),
                EventWrapper::ScaleFactorChanged(event) => {
                    handle_hidpi_proxy(&mut handler, event)
                }
            }
        }

        handler.handle_event(Event::UserEvent(HandlePendingUserEvents));
    }
}

pub fn handle_main_events_cleared(mtm: MainThreadMarker) {
    let mut this = AppState::get_mut(mtm);
    if !this.has_launched() || this.has_terminated() {
        return;
    }
    match this.state_mut() {
        AppStateImpl::ProcessingEvents { .. } => {}
        _ => bug!("`ProcessingRedraws` happened unexpectedly"),
    };
    drop(this);

    handle_user_events(mtm);

    let mut this = AppState::get_mut(mtm);
    let redraw_events: Vec<EventWrapper> = this
        .main_events_cleared_transition()
        .into_iter()
        .map(|window| {
            EventWrapper::StaticEvent(Event::WindowEvent {
                window_id: RootWindowId(window.id()),
                event: WindowEvent::RedrawRequested,
            })
        })
        .collect();
    drop(this);

    handle_nonuser_events(mtm, redraw_events);
    handle_nonuser_event(mtm, EventWrapper::StaticEvent(Event::AboutToWait));
}

pub fn handle_events_cleared(mtm: MainThreadMarker) {
    AppState::get_mut(mtm).events_cleared_transition();
}

pub fn terminated(mtm: MainThreadMarker) {
    let mut this = AppState::get_mut(mtm);
    let mut handler = this.terminated_transition();
    drop(this);

    handler.handle_event(Event::LoopExiting)
}

fn handle_hidpi_proxy(handler: &mut EventLoopHandler, event: ScaleFactorChanged) {
    let ScaleFactorChanged {
        suggested_size,
        scale_factor,
        window,
    } = event;
    let new_inner_size = Arc::new(Mutex::new(suggested_size));
    let event = Event::WindowEvent {
        window_id: RootWindowId(window.id()),
        event: WindowEvent::ScaleFactorChanged {
            scale_factor,
            inner_size_writer: InnerSizeWriter::new(Arc::downgrade(&new_inner_size)),
        },
    };
    handler.handle_event(event);
    let (view, screen_frame) = get_view_and_screen_frame(&window);
    let physical_size = *new_inner_size.lock().unwrap();
    drop(new_inner_size);
    let logical_size = physical_size.to_logical(scale_factor);
    let size = CGSize::new(logical_size.width, logical_size.height);
    let new_frame: CGRect = CGRect::new(screen_frame.origin, size);
    view.setFrame(new_frame);
}

fn get_view_and_screen_frame(window: &WinitUIWindow) -> (Retained<UIView>, CGRect) {
    let view_controller = window.rootViewController().unwrap();
    let view = view_controller.view().unwrap();
    let bounds = window.bounds();
    let screen = window.screen();
    let screen_space = screen.coordinateSpace();
    let screen_frame = window.convertRect_toCoordinateSpace(bounds, &screen_space);
    (view, screen_frame)
}

struct EventLoopWaker {
    timer: CFRunLoopTimerRef,
}

impl Drop for EventLoopWaker {
    fn drop(&mut self) {
        unsafe {
            CFRunLoopTimerInvalidate(self.timer);
            CFRelease(self.timer as _);
        }
    }
}

impl EventLoopWaker {
    fn new(rl: CFRunLoopRef) -> EventLoopWaker {
        extern "C" fn wakeup_main_loop(_timer: CFRunLoopTimerRef, _info: *mut c_void) {}
        unsafe {
            // Create a timer with a 0.1µs interval (1ns does not work) to mimic polling.
            // It is initially setup with a first fire time really far into the
            // future, but that gets changed to fire immediately in did_finish_launching
            let timer = CFRunLoopTimerCreate(
                ptr::null_mut(),
                std::f64::MAX,
                0.000_000_1,
                0,
                0,
                wakeup_main_loop,
                ptr::null_mut(),
            );
            CFRunLoopAddTimer(rl, timer, kCFRunLoopCommonModes);

            EventLoopWaker { timer }
        }
    }

    fn stop(&mut self) {
        unsafe { CFRunLoopTimerSetNextFireDate(self.timer, std::f64::MAX) }
    }

    fn start(&mut self) {
        unsafe { CFRunLoopTimerSetNextFireDate(self.timer, std::f64::MIN) }
    }

    fn start_at(&mut self, instant: Instant) {
        let now = Instant::now();
        if now >= instant {
            self.start();
        } else {
            unsafe {
                let current = CFAbsoluteTimeGetCurrent();
                let duration = instant - now;
                let fsecs = duration.subsec_nanos() as f64 / 1_000_000_000.0
                    + duration.as_secs() as f64;
                CFRunLoopTimerSetNextFireDate(self.timer, current + fsecs)
            }
        }
    }
}

macro_rules! os_capabilities {
    (
        $(
            $(#[$attr:meta])*
            $error_name:ident: $objc_call:literal,
            $name:ident: $major:literal-$minor:literal
        ),*
        $(,)*
    ) => {
        #[derive(Clone, Debug)]
        pub struct OSCapabilities {
            $(
                pub $name: bool,
            )*

            os_version: NSOperatingSystemVersion,
        }

        impl OSCapabilities {
            fn from_os_version(os_version: NSOperatingSystemVersion) -> Self {
                $(let $name = meets_requirements(os_version, $major, $minor);)*
                Self { $($name,)* os_version, }
            }
        }

        impl OSCapabilities {$(
            $(#[$attr])*
            pub fn $error_name(&self, extra_msg: &str) {
                tracing::warn!(
                    concat!("`", $objc_call, "` requires iOS {}.{}+. This device is running iOS {}.{}.{}. {}"),
                    $major, $minor, self.os_version.majorVersion, self.os_version.minorVersion, self.os_version.patchVersion,
                    extra_msg
                )
            }
        )*}
    };
}

os_capabilities! {
    /// <https://developer.apple.com/documentation/uikit/uiview/2891103-safeareainsets?language=objc>
    #[allow(unused)] // error message unused
    safe_area_err_msg: "-[UIView safeAreaInsets]",
    safe_area: 11-0,
    /// <https://developer.apple.com/documentation/uikit/uiviewcontroller/2887509-setneedsupdateofhomeindicatoraut?language=objc>
    home_indicator_hidden_err_msg: "-[UIViewController setNeedsUpdateOfHomeIndicatorAutoHidden]",
    home_indicator_hidden: 11-0,
    /// <https://developer.apple.com/documentation/uikit/uiviewcontroller/2887507-setneedsupdateofscreenedgesdefer?language=objc>
    defer_system_gestures_err_msg: "-[UIViewController setNeedsUpdateOfScreenEdgesDeferringSystem]",
    defer_system_gestures: 11-0,
    /// <https://developer.apple.com/documentation/uikit/uiscreen/2806814-maximumframespersecond?language=objc>
    maximum_frames_per_second_err_msg: "-[UIScreen maximumFramesPerSecond]",
    maximum_frames_per_second: 10-3,
    /// <https://developer.apple.com/documentation/uikit/uitouch/1618110-force?language=objc>
    #[allow(unused)] // error message unused
    force_touch_err_msg: "-[UITouch force]",
    force_touch: 9-0,
}

fn meets_requirements(
    version: NSOperatingSystemVersion,
    required_major: NSInteger,
    required_minor: NSInteger,
) -> bool {
    (version.majorVersion, version.minorVersion) >= (required_major, required_minor)
}

fn get_version() -> NSOperatingSystemVersion {
    let process_info = NSProcessInfo::processInfo();
    let atleast_ios_8 = process_info.respondsToSelector(sel!(operatingSystemVersion));
    // Winit requires atleast iOS 8 because no one has put the time into supporting earlier os
    // versions. Older iOS versions are increasingly difficult to test. For example, Xcode 11 does
    // not support debugging on devices with an iOS version of less than 8. Another example, in
    // order to use an iOS simulator older than iOS 8, you must download an older version of Xcode
    // (<9), and at least Xcode 7 has been tested to not even run on macOS 10.15 - Xcode 8 might?
    //
    // The minimum required iOS version is likely to grow in the future.
    assert!(atleast_ios_8, "`winit` requires iOS version 8 or greater");
    process_info.operatingSystemVersion()
}

pub fn os_capabilities() -> OSCapabilities {
    // Cache the version lookup for efficiency
    static OS_CAPABILITIES: OnceLock<OSCapabilities> = OnceLock::new();
    OS_CAPABILITIES
        .get_or_init(|| OSCapabilities::from_os_version(get_version()))
        .clone()
}
