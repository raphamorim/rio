use super::super::main_thread::MainThreadMarker;
use super::super::DeviceId;
use super::backend;
use super::state::State;
use crate::dpi::PhysicalSize;
use crate::event::{
    DeviceEvent, DeviceId as RootDeviceId, ElementState, Event, RawKeyEvent, StartCause,
    WindowEvent,
};
use crate::event_loop::{ControlFlow, DeviceEvents};
use crate::platform::web::PollStrategy;
use crate::platform_impl::platform::backend::EventListenerHandle;
use crate::platform_impl::platform::r#async::{DispatchRunner, Waker, WakerSpawner};
use crate::platform_impl::platform::window::Inner;
use crate::window::WindowId;

use std::cell::{Cell, RefCell};
use std::collections::{HashSet, VecDeque};
use std::iter;
use std::ops::Deref;
use std::rc::{Rc, Weak};
use wasm_bindgen::prelude::Closure;
use web_sys::{Document, KeyboardEvent, PageTransitionEvent, PointerEvent, WheelEvent};
use web_time::{Duration, Instant};

pub struct Shared(Rc<Execution>);

pub(super) type EventHandler = dyn FnMut(Event<()>);

impl Clone for Shared {
    fn clone(&self) -> Self {
        Shared(self.0.clone())
    }
}

type OnEventHandle<T> = RefCell<Option<EventListenerHandle<dyn FnMut(T)>>>;

pub struct Execution {
    main_thread: MainThreadMarker,
    proxy_spawner: WakerSpawner<Weak<Self>>,
    control_flow: Cell<ControlFlow>,
    poll_strategy: Cell<PollStrategy>,
    exit: Cell<bool>,
    runner: RefCell<RunnerEnum>,
    suspended: Cell<bool>,
    event_loop_recreation: Cell<bool>,
    events: RefCell<VecDeque<EventWrapper>>,
    id: RefCell<u32>,
    window: web_sys::Window,
    document: Document,
    #[allow(clippy::type_complexity)]
    all_canvases: RefCell<
        Vec<(
            WindowId,
            Weak<RefCell<backend::Canvas>>,
            DispatchRunner<Inner>,
        )>,
    >,
    redraw_pending: RefCell<HashSet<WindowId>>,
    destroy_pending: RefCell<VecDeque<WindowId>>,
    page_transition_event_handle: RefCell<Option<backend::PageTransitionEventHandle>>,
    device_events: Cell<DeviceEvents>,
    on_mouse_move: OnEventHandle<PointerEvent>,
    on_wheel: OnEventHandle<WheelEvent>,
    on_mouse_press: OnEventHandle<PointerEvent>,
    on_mouse_release: OnEventHandle<PointerEvent>,
    on_key_press: OnEventHandle<KeyboardEvent>,
    on_key_release: OnEventHandle<KeyboardEvent>,
    on_visibility_change: OnEventHandle<web_sys::Event>,
}

enum RunnerEnum {
    /// The `EventLoop` is created but not being run.
    Pending,
    /// The `EventLoop` is being run.
    Running(Runner),
    /// The `EventLoop` is exited after being started with `EventLoop::run_app`. Since
    /// `EventLoop::run_app` takes ownership of the `EventLoop`, we can be certain
    /// that this event loop will never be run again.
    Destroyed,
}

impl RunnerEnum {
    fn maybe_runner(&self) -> Option<&Runner> {
        match self {
            RunnerEnum::Running(runner) => Some(runner),
            _ => None,
        }
    }
}

struct Runner {
    state: State,
    event_handler: Box<EventHandler>,
}

impl Runner {
    pub fn new(event_handler: Box<EventHandler>) -> Self {
        Runner {
            state: State::Init,
            event_handler,
        }
    }

    /// Returns the corresponding `StartCause` for the current `state`, or `None`
    /// when in `Exit` state.
    fn maybe_start_cause(&self) -> Option<StartCause> {
        Some(match self.state {
            State::Init => StartCause::Init,
            State::Poll { .. } => StartCause::Poll,
            State::Wait { start } => StartCause::WaitCancelled {
                start,
                requested_resume: None,
            },
            State::WaitUntil { start, end, .. } => StartCause::WaitCancelled {
                start,
                requested_resume: Some(end),
            },
            State::Exit => return None,
        })
    }

    fn handle_single_event(&mut self, runner: &Shared, event: impl Into<EventWrapper>) {
        match event.into() {
            EventWrapper::Event(event) => (self.event_handler)(event),
            EventWrapper::ScaleChange {
                canvas,
                size,
                scale,
            } => {
                if let Some(canvas) = canvas.upgrade() {
                    canvas.borrow().handle_scale_change(
                        runner,
                        |event| (self.event_handler)(event),
                        size,
                        scale,
                    )
                }
            }
        }
    }
}

impl Shared {
    pub fn new() -> Self {
        let main_thread =
            MainThreadMarker::new().expect("only callable from inside the `Window`");
        #[allow(clippy::disallowed_methods)]
        let window = web_sys::window().expect("only callable from inside the `Window`");
        #[allow(clippy::disallowed_methods)]
        let document = window.document().expect("Failed to obtain document");

        Shared(Rc::<Execution>::new_cyclic(|weak| {
            let proxy_spawner =
                WakerSpawner::new(main_thread, weak.clone(), |runner, count| {
                    if let Some(runner) = runner.upgrade() {
                        Shared(runner)
                            .send_events(iter::repeat(Event::UserEvent(())).take(count))
                    }
                })
                .expect("`EventLoop` has to be created in the main thread");

            Execution {
                main_thread,
                proxy_spawner,
                control_flow: Cell::new(ControlFlow::default()),
                poll_strategy: Cell::new(PollStrategy::default()),
                exit: Cell::new(false),
                runner: RefCell::new(RunnerEnum::Pending),
                suspended: Cell::new(false),
                event_loop_recreation: Cell::new(false),
                events: RefCell::new(VecDeque::new()),
                window,
                document,
                id: RefCell::new(0),
                all_canvases: RefCell::new(Vec::new()),
                redraw_pending: RefCell::new(HashSet::new()),
                destroy_pending: RefCell::new(VecDeque::new()),
                page_transition_event_handle: RefCell::new(None),
                device_events: Cell::default(),
                on_mouse_move: RefCell::new(None),
                on_wheel: RefCell::new(None),
                on_mouse_press: RefCell::new(None),
                on_mouse_release: RefCell::new(None),
                on_key_press: RefCell::new(None),
                on_key_release: RefCell::new(None),
                on_visibility_change: RefCell::new(None),
            }
        }))
    }

    pub fn main_thread(&self) -> MainThreadMarker {
        self.0.main_thread
    }

    pub fn window(&self) -> &web_sys::Window {
        &self.0.window
    }

    pub fn document(&self) -> &Document {
        &self.0.document
    }

    pub fn add_canvas(
        &self,
        id: WindowId,
        canvas: Weak<RefCell<backend::Canvas>>,
        runner: DispatchRunner<Inner>,
    ) {
        self.0.all_canvases.borrow_mut().push((id, canvas, runner));
    }

    pub fn notify_destroy_window(&self, id: WindowId) {
        self.0.destroy_pending.borrow_mut().push_back(id);
    }

    // Set the event callback to use for the event loop runner
    // This the event callback is a fairly thin layer over the user-provided callback that closes
    // over a RootActiveEventLoop reference
    pub fn set_listener(&self, event_handler: Box<EventHandler>) {
        {
            let mut runner = self.0.runner.borrow_mut();
            assert!(matches!(*runner, RunnerEnum::Pending));
            *runner = RunnerEnum::Running(Runner::new(event_handler));
        }
        self.init();

        *self.0.page_transition_event_handle.borrow_mut() =
            Some(backend::on_page_transition(
                self.window().clone(),
                {
                    let runner = self.clone();
                    move |event: PageTransitionEvent| {
                        if event.persisted() {
                            runner.0.suspended.set(false);
                            runner.send_event(Event::Resumed);
                        }
                    }
                },
                {
                    let runner = self.clone();
                    move |event: PageTransitionEvent| {
                        runner.0.suspended.set(true);
                        if event.persisted() {
                            runner.send_event(Event::Suspended);
                        } else {
                            runner.handle_unload();
                        }
                    }
                },
            ));

        let runner = self.clone();
        let window = self.window().clone();
        *self.0.on_mouse_move.borrow_mut() = Some(EventListenerHandle::new(
            self.window().clone(),
            "pointermove",
            Closure::new(move |event: PointerEvent| {
                if !runner.device_events() {
                    return;
                }

                let pointer_type = event.pointer_type();

                if pointer_type != "mouse" {
                    return;
                }

                // chorded button event
                let device_id = RootDeviceId(DeviceId(event.pointer_id()));

                if let Some(button) = backend::event::mouse_button(&event) {
                    debug_assert_eq!(
                        pointer_type, "mouse",
                        "expect pointer type of a chorded button event to be a mouse"
                    );

                    let state = if backend::event::mouse_buttons(&event)
                        .contains(button.into())
                    {
                        ElementState::Pressed
                    } else {
                        ElementState::Released
                    };

                    runner.send_event(Event::DeviceEvent {
                        device_id,
                        event: DeviceEvent::Button {
                            button: button.to_id(),
                            state,
                        },
                    });

                    return;
                }

                // pointer move event
                let mut delta = backend::event::MouseDelta::init(&window, &event);
                runner.send_events(backend::event::pointer_move_event(event).flat_map(
                    |event| {
                        let delta = delta
                            .delta(&event)
                            .to_physical(backend::scale_factor(&window));

                        let x_motion = (delta.x != 0.0).then_some(Event::DeviceEvent {
                            device_id,
                            event: DeviceEvent::Motion {
                                axis: 0,
                                value: delta.x,
                            },
                        });

                        let y_motion = (delta.y != 0.0).then_some(Event::DeviceEvent {
                            device_id,
                            event: DeviceEvent::Motion {
                                axis: 1,
                                value: delta.y,
                            },
                        });

                        x_motion.into_iter().chain(y_motion).chain(iter::once(
                            Event::DeviceEvent {
                                device_id,
                                event: DeviceEvent::MouseMotion {
                                    delta: (delta.x, delta.y),
                                },
                            },
                        ))
                    },
                ));
            }),
        ));
        let runner = self.clone();
        let window = self.window().clone();
        *self.0.on_wheel.borrow_mut() = Some(EventListenerHandle::new(
            self.window().clone(),
            "wheel",
            Closure::new(move |event: WheelEvent| {
                if !runner.device_events() {
                    return;
                }

                if let Some(delta) = backend::event::mouse_scroll_delta(&window, &event) {
                    runner.send_event(Event::DeviceEvent {
                        device_id: RootDeviceId(DeviceId(0)),
                        event: DeviceEvent::MouseWheel { delta },
                    });
                }
            }),
        ));
        let runner = self.clone();
        *self.0.on_mouse_press.borrow_mut() = Some(EventListenerHandle::new(
            self.window().clone(),
            "pointerdown",
            Closure::new(move |event: PointerEvent| {
                if !runner.device_events() {
                    return;
                }

                if event.pointer_type() != "mouse" {
                    return;
                }

                let button = backend::event::mouse_button(&event)
                    .expect("no mouse button pressed");
                runner.send_event(Event::DeviceEvent {
                    device_id: RootDeviceId(DeviceId(event.pointer_id())),
                    event: DeviceEvent::Button {
                        button: button.to_id(),
                        state: ElementState::Pressed,
                    },
                });
            }),
        ));
        let runner = self.clone();
        *self.0.on_mouse_release.borrow_mut() = Some(EventListenerHandle::new(
            self.window().clone(),
            "pointerup",
            Closure::new(move |event: PointerEvent| {
                if !runner.device_events() {
                    return;
                }

                if event.pointer_type() != "mouse" {
                    return;
                }

                let button = backend::event::mouse_button(&event)
                    .expect("no mouse button pressed");
                runner.send_event(Event::DeviceEvent {
                    device_id: RootDeviceId(DeviceId(event.pointer_id())),
                    event: DeviceEvent::Button {
                        button: button.to_id(),
                        state: ElementState::Released,
                    },
                });
            }),
        ));
        let runner = self.clone();
        *self.0.on_key_press.borrow_mut() = Some(EventListenerHandle::new(
            self.window().clone(),
            "keydown",
            Closure::new(move |event: KeyboardEvent| {
                if !runner.device_events() {
                    return;
                }

                runner.send_event(Event::DeviceEvent {
                    device_id: RootDeviceId(unsafe { DeviceId::dummy() }),
                    event: DeviceEvent::Key(RawKeyEvent {
                        physical_key: backend::event::key_code(&event),
                        state: ElementState::Pressed,
                    }),
                });
            }),
        ));
        let runner = self.clone();
        *self.0.on_key_release.borrow_mut() = Some(EventListenerHandle::new(
            self.window().clone(),
            "keyup",
            Closure::new(move |event: KeyboardEvent| {
                if !runner.device_events() {
                    return;
                }

                runner.send_event(Event::DeviceEvent {
                    device_id: RootDeviceId(unsafe { DeviceId::dummy() }),
                    event: DeviceEvent::Key(RawKeyEvent {
                        physical_key: backend::event::key_code(&event),
                        state: ElementState::Released,
                    }),
                });
            }),
        ));
        let runner = self.clone();
        *self.0.on_visibility_change.borrow_mut() = Some(EventListenerHandle::new(
            // Safari <14 doesn't support the `visibilitychange` event on `Window`.
            self.document().clone(),
            "visibilitychange",
            Closure::new(move |_| {
                if !runner.0.suspended.get() {
                    for (id, canvas, _) in &*runner.0.all_canvases.borrow() {
                        if let Some(canvas) = canvas.upgrade() {
                            let is_visible = backend::is_visible(runner.document());
                            // only fire if:
                            // - not visible and intersects
                            // - not visible and we don't know if it intersects yet
                            // - visible and intersects
                            if let (false, Some(true) | None) | (true, Some(true)) =
                                (is_visible, canvas.borrow().is_intersecting)
                            {
                                runner.send_event(Event::WindowEvent {
                                    window_id: *id,
                                    event: WindowEvent::Occluded(!is_visible),
                                });
                            }
                        }
                    }
                }
            }),
        ));
    }

    // Generate a strictly increasing ID
    // This is used to differentiate windows when handling events
    pub fn generate_id(&self) -> u32 {
        let mut id = self.0.id.borrow_mut();
        *id += 1;

        *id
    }

    pub fn request_redraw(&self, id: WindowId) {
        self.0.redraw_pending.borrow_mut().insert(id);
        self.send_events::<EventWrapper>(iter::empty());
    }

    pub fn init(&self) {
        // NB: For consistency all platforms must emit a 'resumed' event even though web
        // applications don't themselves have a formal suspend/resume lifecycle.
        self.run_until_cleared(
            [Event::NewEvents(StartCause::Init), Event::Resumed].into_iter(),
        );
    }

    // Run the polling logic for the Poll ControlFlow, which involves clearing the queue
    pub fn poll(&self) {
        let start_cause = Event::NewEvents(StartCause::Poll);
        self.run_until_cleared(iter::once(start_cause));
    }

    // Run the logic for waking from a WaitUntil, which involves clearing the queue
    // Generally there shouldn't be events built up when this is called
    pub fn resume_time_reached(&self, start: Instant, requested_resume: Instant) {
        let start_cause = Event::NewEvents(StartCause::ResumeTimeReached {
            start,
            requested_resume,
        });
        self.run_until_cleared(iter::once(start_cause));
    }

    // Add an event to the event loop runner, from the user or an event handler
    //
    // It will determine if the event should be immediately sent to the user or buffered for later
    pub(crate) fn send_event<E: Into<EventWrapper>>(&self, event: E) {
        self.send_events(iter::once(event));
    }

    // Add a series of events to the event loop runner
    //
    // It will determine if the event should be immediately sent to the user or buffered for later
    pub(crate) fn send_events<E: Into<EventWrapper>>(
        &self,
        events: impl IntoIterator<Item = E>,
    ) {
        // If the event loop is closed, it should discard any new events
        if self.is_closed() {
            return;
        }
        // If we can run the event processing right now, or need to queue this and wait for later
        let mut process_immediately = true;
        match self.0.runner.try_borrow().as_ref().map(Deref::deref) {
            Ok(RunnerEnum::Running(ref runner)) => {
                // If we're currently polling, queue this and wait for the poll() method to be
                // called
                if let State::Poll { .. } = runner.state {
                    process_immediately = false;
                }
            }
            Ok(RunnerEnum::Pending) => {
                // The runner still hasn't been attached: queue this event and wait for it to be
                process_immediately = false;
            }
            // Some other code is mutating the runner, which most likely means
            // the event loop is running and busy. So we queue this event for
            // it to be processed later.
            Err(_) => {
                process_immediately = false;
            }
            // This is unreachable since `self.is_closed() == true`.
            Ok(RunnerEnum::Destroyed) => unreachable!(),
        }
        if !process_immediately {
            // Queue these events to look at later
            self.0
                .events
                .borrow_mut()
                .extend(events.into_iter().map(Into::into));
            return;
        }
        // At this point, we know this is a fresh set of events
        // Now we determine why new events are incoming, and handle the events
        let start_cause = match (self.0.runner.borrow().maybe_runner())
            .unwrap_or_else(|| {
                unreachable!("The runner cannot process events when it is not attached")
            })
            .maybe_start_cause()
        {
            Some(c) => c,
            // If we're in the exit state, don't do event processing
            None => return,
        };
        // Take the start event, then the events provided to this function, and run an iteration of
        // the event loop
        let start_event = Event::NewEvents(start_cause);
        let events = iter::once(EventWrapper::from(start_event))
            .chain(events.into_iter().map(Into::into));
        self.run_until_cleared(events);
    }

    // Process the destroy-pending windows. This should only be called from
    // `run_until_cleared`, somewhere between emitting `NewEvents` and `AboutToWait`.
    fn process_destroy_pending_windows(&self) {
        while let Some(id) = self.0.destroy_pending.borrow_mut().pop_front() {
            self.0
                .all_canvases
                .borrow_mut()
                .retain(|&(item_id, ..)| item_id != id);
            self.handle_event(Event::WindowEvent {
                window_id: id,
                event: crate::event::WindowEvent::Destroyed,
            });
            self.0.redraw_pending.borrow_mut().remove(&id);
        }
    }

    // Given the set of new events, run the event loop until the main events and redraw events are
    // cleared
    //
    // This will also process any events that have been queued or that are queued during processing
    fn run_until_cleared<E: Into<EventWrapper>>(&self, events: impl Iterator<Item = E>) {
        for event in events {
            self.handle_event(event.into());
        }
        self.process_destroy_pending_windows();

        // Collect all of the redraw events to avoid double-locking the RefCell
        let redraw_events: Vec<WindowId> =
            self.0.redraw_pending.borrow_mut().drain().collect();
        for window_id in redraw_events {
            self.handle_event(Event::WindowEvent {
                window_id,
                event: WindowEvent::RedrawRequested,
            });
        }

        self.handle_event(Event::AboutToWait);

        self.apply_control_flow();
        // If the event loop is closed, it has been closed this iteration and now the closing
        // event should be emitted
        if self.is_closed() {
            self.handle_loop_destroyed();
        }
    }

    fn handle_unload(&self) {
        self.exit();
        self.apply_control_flow();
        // We don't call `handle_loop_destroyed` here because we don't need to
        // perform cleanup when the web browser is going to destroy the page.
        self.handle_event(Event::LoopExiting);
    }

    // handle_event takes in events and either queues them or applies a callback
    //
    // It should only ever be called from `run_until_cleared`.
    fn handle_event(&self, event: impl Into<EventWrapper>) {
        if self.is_closed() {
            self.exit();
        }
        match *self.0.runner.borrow_mut() {
            RunnerEnum::Running(ref mut runner) => {
                runner.handle_single_event(self, event);
            }
            // If an event is being handled without a runner somehow, add it to the event queue so
            // it will eventually be processed
            RunnerEnum::Pending => self.0.events.borrow_mut().push_back(event.into()),
            // If the Runner has been destroyed, there is nothing to do.
            RunnerEnum::Destroyed => return,
        }

        let is_closed = self.exiting();

        // Don't take events out of the queue if the loop is closed or the runner doesn't exist
        // If the runner doesn't exist and this method recurses, it will recurse infinitely
        if !is_closed && self.0.runner.borrow().maybe_runner().is_some() {
            // Pre-fetch window commands to avoid having to wait until the next event loop cycle
            // and potentially block other threads in the meantime.
            for (_, window, runner) in self.0.all_canvases.borrow().iter() {
                if let Some(window) = window.upgrade() {
                    runner.run();
                    drop(window)
                }
            }

            // Take an event out of the queue and handle it
            // Make sure not to let the borrow_mut live during the next handle_event
            let event = {
                let mut events = self.0.events.borrow_mut();

                // Pre-fetch `UserEvent`s to avoid having to wait until the next event loop cycle.
                events.extend(
                    iter::repeat(Event::UserEvent(()))
                        .take(self.0.proxy_spawner.fetch())
                        .map(EventWrapper::from),
                );

                events.pop_front()
            };
            if let Some(event) = event {
                self.handle_event(event);
            }
        }
    }

    // Apply the new ControlFlow that has been selected by the user
    // Start any necessary timeouts etc
    fn apply_control_flow(&self) {
        let new_state = if self.exiting() {
            State::Exit
        } else {
            match self.control_flow() {
                ControlFlow::Poll => {
                    let cloned = self.clone();
                    State::Poll {
                        _request: backend::Schedule::new(
                            self.poll_strategy(),
                            self.window(),
                            move || cloned.poll(),
                        ),
                    }
                }
                ControlFlow::Wait => State::Wait {
                    start: Instant::now(),
                },
                ControlFlow::WaitUntil(end) => {
                    let start = Instant::now();

                    let delay = if end <= start {
                        Duration::from_millis(0)
                    } else {
                        end - start
                    };

                    let cloned = self.clone();

                    State::WaitUntil {
                        start,
                        end,
                        _timeout: backend::Schedule::new_with_duration(
                            self.window(),
                            move || cloned.resume_time_reached(start, end),
                            delay,
                        ),
                    }
                }
            }
        };

        if let RunnerEnum::Running(ref mut runner) = *self.0.runner.borrow_mut() {
            runner.state = new_state;
        }
    }

    fn handle_loop_destroyed(&self) {
        self.handle_event(Event::LoopExiting);
        let all_canvases = std::mem::take(&mut *self.0.all_canvases.borrow_mut());
        *self.0.page_transition_event_handle.borrow_mut() = None;
        *self.0.on_mouse_move.borrow_mut() = None;
        *self.0.on_wheel.borrow_mut() = None;
        *self.0.on_mouse_press.borrow_mut() = None;
        *self.0.on_mouse_release.borrow_mut() = None;
        *self.0.on_key_press.borrow_mut() = None;
        *self.0.on_key_release.borrow_mut() = None;
        *self.0.on_visibility_change.borrow_mut() = None;
        // Dropping the `Runner` drops the event handler closure, which will in
        // turn drop all `Window`s moved into the closure.
        *self.0.runner.borrow_mut() = RunnerEnum::Destroyed;
        for (_, canvas, _) in all_canvases {
            // In case any remaining `Window`s are still not dropped, we will need
            // to explicitly remove the event handlers associated with their canvases.
            if let Some(canvas) = canvas.upgrade() {
                let mut canvas = canvas.borrow_mut();
                canvas.remove_listeners();
            }
        }
        // At this point, the `self.0` `Rc` should only be strongly referenced
        // by the following:
        // * `self`, i.e. the item which triggered this event loop wakeup, which is usually a
        //   `wasm-bindgen` `Closure`, which will be dropped after returning to the JS glue code.
        // * The `ActiveEventLoop` leaked inside `EventLoop::run_app` due to the JS exception thrown
        //   at the end.
        // * For each undropped `Window`:
        //     * The `register_redraw_request` closure.
        //     * The `destroy_fn` closure.
        if self.0.event_loop_recreation.get() {
            crate::event_loop::EventLoopBuilder::<()>::allow_event_loop_recreation();
        }
    }

    // Check if the event loop is currently closed
    fn is_closed(&self) -> bool {
        match self.0.runner.try_borrow().as_ref().map(Deref::deref) {
            Ok(RunnerEnum::Running(runner)) => runner.state.exiting(),
            // The event loop is not closed since it is not initialized.
            Ok(RunnerEnum::Pending) => false,
            // The event loop is closed since it has been destroyed.
            Ok(RunnerEnum::Destroyed) => true,
            // Some other code is mutating the runner, which most likely means
            // the event loop is running and busy.
            Err(_) => false,
        }
    }

    pub fn listen_device_events(&self, allowed: DeviceEvents) {
        self.0.device_events.set(allowed)
    }

    fn device_events(&self) -> bool {
        match self.0.device_events.get() {
            DeviceEvents::Always => true,
            DeviceEvents::WhenFocused => {
                self.0.all_canvases.borrow().iter().any(|(_, canvas, _)| {
                    if let Some(canvas) = canvas.upgrade() {
                        canvas.borrow().has_focus.get()
                    } else {
                        false
                    }
                })
            }
            DeviceEvents::Never => false,
        }
    }

    pub fn event_loop_recreation(&self, allow: bool) {
        self.0.event_loop_recreation.set(allow)
    }

    pub(crate) fn control_flow(&self) -> ControlFlow {
        self.0.control_flow.get()
    }

    pub(crate) fn set_control_flow(&self, control_flow: ControlFlow) {
        self.0.control_flow.set(control_flow)
    }

    pub(crate) fn exit(&self) {
        self.0.exit.set(true)
    }

    pub(crate) fn exiting(&self) -> bool {
        self.0.exit.get()
    }

    pub(crate) fn set_poll_strategy(&self, strategy: PollStrategy) {
        self.0.poll_strategy.set(strategy)
    }

    pub(crate) fn poll_strategy(&self) -> PollStrategy {
        self.0.poll_strategy.get()
    }

    pub(crate) fn waker(&self) -> Waker<Weak<Execution>> {
        self.0.proxy_spawner.waker()
    }
}

pub(crate) enum EventWrapper {
    Event(Event<()>),
    ScaleChange {
        canvas: Weak<RefCell<backend::Canvas>>,
        size: PhysicalSize<u32>,
        scale: f64,
    },
}

impl From<Event<()>> for EventWrapper {
    fn from(value: Event<()>) -> Self {
        Self::Event(value)
    }
}
