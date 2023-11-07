use log::trace;
use std::{
    cell::{RefCell, RefMut},
    collections::VecDeque,
    fmt::{self, Debug},
    mem,
    rc::{Rc, Weak},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex, MutexGuard,
    },
    time::Instant,
};

use core_foundation::runloop::{CFRunLoopGetMain, CFRunLoopWakeUp};
use icrate::Foundation::{is_main_thread, NSSize};
use objc2::rc::{autoreleasepool, Id};
use once_cell::sync::Lazy;

use super::appkit::{NSApp, NSApplication, NSApplicationActivationPolicy, NSEvent};
use super::{
    event_loop::PanicInfo, menu, observer::EventLoopWaker, util::Never,
    window::WinitWindow,
};
use crate::{
    dpi::PhysicalSize,
    event::{Event, InnerSizeWriter, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoopWindowTarget as RootWindowTarget},
    window::WindowId,
};

static HANDLER: Lazy<Handler> = Lazy::new(Default::default);

impl<Never> Event<Never> {
    fn userify<T: 'static>(self) -> Event<T> {
        self.map_nonuser_event()
            // `Never` can't be constructed, so the `UserEvent` variant can't
            // be present here.
            .unwrap_or_else(|_| unreachable!())
    }
}

pub trait EventHandler: Debug {
    // Not sure probably it should accept Event<'static, Never>
    fn handle_nonuser_event(&mut self, event: Event<Never>);
    fn handle_user_events(&mut self);
}

pub(crate) type Callback<T> = RefCell<dyn FnMut(Event<T>, &RootWindowTarget<T>)>;

struct EventLoopHandler<T: 'static> {
    callback: Weak<Callback<T>>,
    window_target: Rc<RootWindowTarget<T>>,
    receiver: Rc<mpsc::Receiver<T>>,
}

impl<T> EventLoopHandler<T> {
    fn with_callback<F>(&mut self, f: F)
    where
        F: FnOnce(
            &mut EventLoopHandler<T>,
            RefMut<'_, dyn FnMut(Event<T>, &RootWindowTarget<T>)>,
        ),
    {
        // The `NSApp` and our `HANDLER` are global state and so it's possible that
        // we could get a delegate callback after the application has exit an
        // `EventLoop`. If the loop has been exit then our weak `self.callback`
        // will fail to upgrade.
        //
        // We don't want to panic or output any verbose logging if we fail to
        // upgrade the weak reference since it might be valid that the application
        // re-starts the `NSApp` after exiting a Winit `EventLoop`
        if let Some(callback) = self.callback.upgrade() {
            let callback = callback.borrow_mut();
            (f)(self, callback);
        }
    }
}

impl<T> Debug for EventLoopHandler<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventLoopHandler")
            .field("window_target", &self.window_target)
            .finish()
    }
}

impl<T> EventHandler for EventLoopHandler<T> {
    fn handle_nonuser_event(&mut self, event: Event<Never>) {
        self.with_callback(|this, mut callback| {
            (callback)(event.userify(), &this.window_target);
        });
    }

    fn handle_user_events(&mut self) {
        self.with_callback(|this, mut callback| {
            for event in this.receiver.try_iter() {
                (callback)(Event::UserEvent(event), &this.window_target);
            }
        });
    }
}

#[derive(Debug)]
enum EventWrapper {
    StaticEvent(Event<Never>),
    ScaleFactorChanged {
        window: Id<WinitWindow>,
        suggested_size: PhysicalSize<u32>,
        scale_factor: f64,
    },
}

#[derive(Default)]
struct Handler {
    stop_app_on_launch: AtomicBool,
    stop_app_before_wait: AtomicBool,
    stop_app_after_wait: AtomicBool,
    stop_app_on_redraw: AtomicBool,
    launched: AtomicBool,
    running: AtomicBool,
    in_callback: AtomicBool,
    control_flow: Mutex<ControlFlow>,
    exit: AtomicBool,
    start_time: Mutex<Option<Instant>>,
    callback: Mutex<Option<Box<dyn EventHandler>>>,
    pending_events: Mutex<VecDeque<EventWrapper>>,
    pending_redraw: Mutex<Vec<WindowId>>,
    wait_timeout: Mutex<Option<Instant>>,
    waker: Mutex<EventLoopWaker>,
}

unsafe impl Send for Handler {}
unsafe impl Sync for Handler {}

impl Handler {
    fn events(&self) -> MutexGuard<'_, VecDeque<EventWrapper>> {
        self.pending_events.lock().unwrap()
    }

    fn redraw(&self) -> MutexGuard<'_, Vec<WindowId>> {
        self.pending_redraw.lock().unwrap()
    }

    fn waker(&self) -> MutexGuard<'_, EventLoopWaker> {
        self.waker.lock().unwrap()
    }

    /// `true` after `ApplicationDelegate::applicationDidFinishLaunching` called
    ///
    /// NB: This is global / `NSApp` state and since the app will only be launched
    /// once but an `EventLoop` may be run more than once then only the first
    /// `EventLoop` will observe the `NSApp` before it is launched.
    fn is_launched(&self) -> bool {
        self.launched.load(Ordering::Acquire)
    }

    /// Set via `ApplicationDelegate::applicationDidFinishLaunching`
    fn set_launched(&self) {
        self.launched.store(true, Ordering::Release);
    }

    /// `true` if an `EventLoop` is currently running
    ///
    /// NB: This is global / `NSApp` state and may persist beyond the lifetime of
    /// a running `EventLoop`.
    ///
    /// # Caveat
    /// This is only intended to be called from the main thread
    fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Set when an `EventLoop` starts running, after the `NSApp` is launched
    ///
    /// # Caveat
    /// This is only intended to be called from the main thread
    fn set_running(&self) {
        self.running.store(true, Ordering::Relaxed);
    }

    /// Clears the `running` state and resets the `control_flow` state when an `EventLoop` exits
    ///
    /// Since an `EventLoop` may be run more than once we need make sure to reset the
    /// `control_flow` state back to `Poll` each time the loop exits.
    ///
    /// Note: that if the `NSApp` has been launched then that state is preserved, and we won't
    /// need to re-launch the app if subsequent EventLoops are run.
    ///
    /// # Caveat
    /// This is only intended to be called from the main thread
    fn internal_exit(&self) {
        // Relaxed ordering because we don't actually have multiple threads involved, we just want
        // interiour mutability
        //
        // XXX: As an aside; having each individual bit of state for `Handler` be atomic or wrapped in a
        // `Mutex` for the sake of interior mutability seems a bit odd, and also a potential foot
        // gun in case the state is unwittingly accessed across threads because the fine-grained locking
        // wouldn't ensure that there's interior consistency.
        //
        // Maybe the whole thing should just be put in a static `Mutex<>` to make it clear
        // the we can mutate more than one peice of state while maintaining consistency. (though it
        // looks like there have been recuring re-entrancy issues with callback handling that might
        // make that awkward)
        self.running.store(false, Ordering::Relaxed);
        self.set_stop_app_on_redraw_requested(false);
        self.set_stop_app_before_wait(false);
        self.set_stop_app_after_wait(false);
        self.set_wait_timeout(None);
    }

    pub fn exit(&self) {
        self.exit.store(true, Ordering::Relaxed)
    }

    pub fn exiting(&self) -> bool {
        self.exit.load(Ordering::Relaxed)
    }

    pub fn request_stop_app_on_launch(&self) {
        // Relaxed ordering because we don't actually have multiple threads involved, we just want
        // interior mutability
        self.stop_app_on_launch.store(true, Ordering::Relaxed);
    }

    pub fn should_stop_app_on_launch(&self) -> bool {
        // Relaxed ordering because we don't actually have multiple threads involved, we just want
        // interior mutability
        self.stop_app_on_launch.load(Ordering::Relaxed)
    }

    pub fn set_stop_app_before_wait(&self, stop_before_wait: bool) {
        // Relaxed ordering because we don't actually have multiple threads involved, we just want
        // interior mutability
        self.stop_app_before_wait
            .store(stop_before_wait, Ordering::Relaxed);
    }

    pub fn should_stop_app_before_wait(&self) -> bool {
        // Relaxed ordering because we don't actually have multiple threads involved, we just want
        // interior mutability
        self.stop_app_before_wait.load(Ordering::Relaxed)
    }

    pub fn set_stop_app_after_wait(&self, stop_after_wait: bool) {
        // Relaxed ordering because we don't actually have multiple threads involved, we just want
        // interior mutability
        self.stop_app_after_wait
            .store(stop_after_wait, Ordering::Relaxed);
    }

    pub fn set_wait_timeout(&self, new_timeout: Option<Instant>) {
        let mut timeout = self.wait_timeout.lock().unwrap();
        *timeout = new_timeout;
    }

    pub fn wait_timeout(&self) -> Option<Instant> {
        *self.wait_timeout.lock().unwrap()
    }

    pub fn should_stop_app_after_wait(&self) -> bool {
        // Relaxed ordering because we don't actually have multiple threads involved, we just want
        // interior mutability
        self.stop_app_after_wait.load(Ordering::Relaxed)
    }

    pub fn set_stop_app_on_redraw_requested(&self, stop_on_redraw: bool) {
        // Relaxed ordering because we don't actually have multiple threads involved, we just want
        // interior mutability
        self.stop_app_on_redraw
            .store(stop_on_redraw, Ordering::Relaxed);
    }

    pub fn should_stop_app_on_redraw_requested(&self) -> bool {
        // Relaxed ordering because we don't actually have multiple threads involved, we just want
        // interior mutability
        self.stop_app_on_redraw.load(Ordering::Relaxed)
    }

    fn set_control_flow(&self, new_control_flow: ControlFlow) {
        *self.control_flow.lock().unwrap() = new_control_flow
    }

    fn control_flow(&self) -> ControlFlow {
        *self.control_flow.lock().unwrap()
    }

    fn get_start_time(&self) -> Option<Instant> {
        *self.start_time.lock().unwrap()
    }

    fn update_start_time(&self) {
        *self.start_time.lock().unwrap() = Some(Instant::now());
    }

    fn take_events(&self) -> VecDeque<EventWrapper> {
        mem::take(&mut *self.events())
    }

    fn should_redraw(&self) -> Vec<WindowId> {
        mem::take(&mut *self.redraw())
    }

    fn get_in_callback(&self) -> bool {
        self.in_callback.load(Ordering::Acquire)
    }

    fn set_in_callback(&self, in_callback: bool) {
        self.in_callback.store(in_callback, Ordering::Release);
    }

    fn have_callback(&self) -> bool {
        self.callback.lock().unwrap().is_some()
    }

    fn handle_nonuser_event(&self, event: Event<Never>) {
        if let Some(ref mut callback) = *self.callback.lock().unwrap() {
            callback.handle_nonuser_event(event)
        }
    }

    fn handle_user_events(&self) {
        if let Some(ref mut callback) = *self.callback.lock().unwrap() {
            callback.handle_user_events();
        }
    }

    fn handle_scale_factor_changed_event(
        &self,
        window: &WinitWindow,
        suggested_size: PhysicalSize<u32>,
        scale_factor: f64,
    ) {
        if let Some(ref mut callback) = *self.callback.lock().unwrap() {
            let new_inner_size = Arc::new(Mutex::new(suggested_size));
            let scale_factor_changed_event = Event::WindowEvent {
                window_id: WindowId(window.id()),
                event: WindowEvent::ScaleFactorChanged {
                    scale_factor,
                    inner_size_writer: InnerSizeWriter::new(Arc::downgrade(
                        &new_inner_size,
                    )),
                },
            };

            callback.handle_nonuser_event(scale_factor_changed_event);

            let physical_size = *new_inner_size.lock().unwrap();
            drop(new_inner_size);
            let logical_size = physical_size.to_logical(scale_factor);
            let size = NSSize::new(logical_size.width, logical_size.height);
            window.setContentSize(size);

            let resized_event = Event::WindowEvent {
                window_id: WindowId(window.id()),
                event: WindowEvent::Resized(physical_size),
            };
            callback.handle_nonuser_event(resized_event);
        }
    }
}

pub(crate) enum AppState {}

impl AppState {
    /// Associate the application's event callback with the (global static) Handler state
    ///
    /// # Safety
    /// This is ignoring the lifetime of the application callback (which may not be 'static)
    /// and can lead to undefined behaviour if the callback is not cleared before the end of
    /// its real lifetime.
    ///
    /// All public APIs that take an event callback (`run`, `run_on_demand`,
    /// `pump_events`) _must_ pair a call to `set_callback` with
    /// a call to `clear_callback` before returning to avoid undefined behaviour.
    pub unsafe fn set_callback<T>(
        callback: Weak<Callback<T>>,
        window_target: Rc<RootWindowTarget<T>>,
        receiver: Rc<mpsc::Receiver<T>>,
    ) {
        *HANDLER.callback.lock().unwrap() = Some(Box::new(EventLoopHandler {
            callback,
            window_target,
            receiver,
        }));
    }

    pub fn clear_callback() {
        HANDLER.callback.lock().unwrap().take();
    }

    pub fn is_launched() -> bool {
        HANDLER.is_launched()
    }

    pub fn is_running() -> bool {
        HANDLER.is_running()
    }

    pub fn open_urls(urls: Vec<url::Url>) {
        HANDLER.handle_nonuser_event(Event::Opened { urls });
    }

    // If `pump_events` is called to progress the event loop then we bootstrap the event
    // loop via `[NSApp run]` but will use `CFRunLoopRunInMode` for subsequent calls to
    // `pump_events`
    pub fn request_stop_on_launch() {
        HANDLER.request_stop_app_on_launch();
    }

    pub fn set_stop_app_before_wait(stop_before_wait: bool) {
        HANDLER.set_stop_app_before_wait(stop_before_wait);
    }

    pub fn set_stop_app_after_wait(stop_after_wait: bool) {
        HANDLER.set_stop_app_after_wait(stop_after_wait);
    }

    pub fn set_wait_timeout(timeout: Option<Instant>) {
        HANDLER.set_wait_timeout(timeout);
    }

    pub fn set_stop_app_on_redraw_requested(stop_on_redraw: bool) {
        HANDLER.set_stop_app_on_redraw_requested(stop_on_redraw);
    }

    pub fn set_control_flow(control_flow: ControlFlow) {
        HANDLER.set_control_flow(control_flow)
    }

    pub fn control_flow() -> ControlFlow {
        HANDLER.control_flow()
    }

    pub fn internal_exit() {
        HANDLER.set_in_callback(true);
        HANDLER.handle_nonuser_event(Event::LoopExiting);
        HANDLER.set_in_callback(false);
        HANDLER.internal_exit();
        Self::clear_callback();
    }

    pub fn exit() {
        HANDLER.exit()
    }

    pub fn exiting() -> bool {
        HANDLER.exiting()
    }

    pub fn dispatch_init_events() {
        HANDLER.set_in_callback(true);
        HANDLER.handle_nonuser_event(Event::NewEvents(StartCause::Init));
        // NB: For consistency all platforms must emit a 'resumed' event even though macOS
        // applications don't themselves have a formal suspend/resume lifecycle.
        HANDLER.handle_nonuser_event(Event::Resumed);
        HANDLER.set_in_callback(false);
    }

    pub fn start_running() {
        debug_assert!(HANDLER.is_launched());

        HANDLER.set_running();
        Self::dispatch_init_events()
    }

    pub fn launched(
        activation_policy: NSApplicationActivationPolicy,
        create_default_menu: bool,
        activate_ignoring_other_apps: bool,
    ) {
        let app = NSApp();
        // We need to delay setting the activation policy and activating the app
        // until `applicationDidFinishLaunching` has been called. Otherwise the
        // menu bar is initially unresponsive on macOS 10.15.
        app.setActivationPolicy(activation_policy);

        window_activation_hack(&app);
        app.activateIgnoringOtherApps(activate_ignoring_other_apps);

        HANDLER.set_launched();
        HANDLER.waker().start();
        if create_default_menu {
            // The menubar initialization should be before the `NewEvents` event, to allow
            // overriding of the default menu even if it's created
            menu::initialize();
        }

        Self::start_running();

        // If the `NSApp` is being launched via `EventLoop::pump_events()` then we'll
        // want to stop the app once it is launched (and return to the external loop)
        //
        // In this case we still want to consider Winit's `EventLoop` to be "running",
        // so we call `start_running()` above.
        if HANDLER.should_stop_app_on_launch() {
            // Note: the original idea had been to only stop the underlying `RunLoop`
            // for the app but that didn't work as expected (`[NSApp run]` effectively
            // ignored the attempt to stop the RunLoop and re-started it.). So we
            // return from `pump_events` by stopping the `NSApp`
            Self::stop();
        }
    }

    // Called by RunLoopObserver after finishing waiting for new events
    pub fn wakeup(panic_info: Weak<PanicInfo>) {
        let panic_info = panic_info.upgrade().expect(
            "The panic info must exist here. This failure indicates a developer error.",
        );

        // Return when in callback due to https://github.com/rust-windowing/winit/issues/1779
        if panic_info.is_panicking()
            || HANDLER.get_in_callback()
            || !HANDLER.have_callback()
            || !HANDLER.is_running()
        {
            return;
        }

        if HANDLER.should_stop_app_after_wait() {
            Self::stop();
        }

        let start = HANDLER.get_start_time().unwrap();
        let cause = match HANDLER.control_flow() {
            ControlFlow::Poll => StartCause::Poll,
            ControlFlow::Wait => StartCause::WaitCancelled {
                start,
                requested_resume: None,
            },
            ControlFlow::WaitUntil(requested_resume) => {
                if Instant::now() >= requested_resume {
                    StartCause::ResumeTimeReached {
                        start,
                        requested_resume,
                    }
                } else {
                    StartCause::WaitCancelled {
                        start,
                        requested_resume: Some(requested_resume),
                    }
                }
            }
        };
        HANDLER.set_in_callback(true);
        HANDLER.handle_nonuser_event(Event::NewEvents(cause));
        HANDLER.set_in_callback(false);
    }

    // This is called from multiple threads at present
    pub fn queue_redraw(window_id: WindowId) {
        let mut pending_redraw = HANDLER.redraw();
        if !pending_redraw.contains(&window_id) {
            pending_redraw.push(window_id);
        }
        unsafe {
            let rl = CFRunLoopGetMain();
            CFRunLoopWakeUp(rl);
        }
    }

    pub fn handle_redraw(window_id: WindowId) {
        // Redraw request might come out of order from the OS.
        // -> Don't go back into the callback when our callstack originates from there
        if !HANDLER.in_callback.swap(true, Ordering::AcqRel) {
            HANDLER.handle_nonuser_event(Event::WindowEvent {
                window_id,
                event: WindowEvent::RedrawRequested,
            });
            HANDLER.set_in_callback(false);

            // `pump_events` will request to stop immediately _after_ dispatching RedrawRequested events
            // as a way to ensure that `pump_events` can't block an external loop indefinitely
            if HANDLER.should_stop_app_on_redraw_requested() {
                AppState::stop();
            }
        }
    }

    pub fn queue_event(event: Event<Never>) {
        if !is_main_thread() {
            panic!("Event queued from different thread: {event:#?}");
        }
        HANDLER.events().push_back(EventWrapper::StaticEvent(event));
    }

    pub fn queue_static_scale_factor_changed_event(
        window: Id<WinitWindow>,
        suggested_size: PhysicalSize<u32>,
        scale_factor: f64,
    ) {
        HANDLER
            .events()
            .push_back(EventWrapper::ScaleFactorChanged {
                window,
                suggested_size,
                scale_factor,
            });
    }

    pub fn stop() {
        let app = NSApp();
        autoreleasepool(|_| {
            app.stop(None);
            // To stop event loop immediately, we need to post some event here.
            app.postEvent_atStart(&NSEvent::dummy(), true);
        });
    }

    // Called by RunLoopObserver before waiting for new events
    pub fn cleared(panic_info: Weak<PanicInfo>) {
        let panic_info = panic_info.upgrade().expect(
            "The panic info must exist here. This failure indicates a developer error.",
        );

        // Return when in callback due to https://github.com/rust-windowing/winit/issues/1779
        // XXX: how does it make sense that `get_in_callback()` can ever return `true` here if we're
        // about to return to the `CFRunLoop` to poll for new events?
        if panic_info.is_panicking()
            || HANDLER.get_in_callback()
            || !HANDLER.have_callback()
            || !HANDLER.is_running()
        {
            return;
        }

        HANDLER.set_in_callback(true);
        HANDLER.handle_user_events();
        for event in HANDLER.take_events() {
            match event {
                EventWrapper::StaticEvent(event) => {
                    HANDLER.handle_nonuser_event(event);
                }
                EventWrapper::ScaleFactorChanged {
                    window,
                    suggested_size,
                    scale_factor,
                } => {
                    HANDLER.handle_scale_factor_changed_event(
                        &window,
                        suggested_size,
                        scale_factor,
                    );
                }
            }
        }

        for window_id in HANDLER.should_redraw() {
            HANDLER.handle_nonuser_event(Event::WindowEvent {
                window_id,
                event: WindowEvent::RedrawRequested,
            });
        }

        HANDLER.handle_nonuser_event(Event::AboutToWait);
        HANDLER.set_in_callback(false);

        if HANDLER.exiting() {
            Self::stop();
        }

        if HANDLER.should_stop_app_before_wait() {
            Self::stop();
        }
        HANDLER.update_start_time();
        let wait_timeout = HANDLER.wait_timeout(); // configured by pump_events
        let app_timeout = match HANDLER.control_flow() {
            ControlFlow::Wait => None,
            ControlFlow::Poll => Some(Instant::now()),
            ControlFlow::WaitUntil(instant) => Some(instant),
        };
        HANDLER
            .waker()
            .start_at(min_timeout(wait_timeout, app_timeout));
    }
}

/// Returns the minimum `Option<Instant>`, taking into account that `None`
/// equates to an infinite timeout, not a zero timeout (so can't just use
/// `Option::min`)
fn min_timeout(a: Option<Instant>, b: Option<Instant>) -> Option<Instant> {
    a.map_or(b, |a_timeout| {
        b.map_or(Some(a_timeout), |b_timeout| Some(a_timeout.min(b_timeout)))
    })
}

/// A hack to make activation of multiple windows work when creating them before
/// `applicationDidFinishLaunching:` / `Event::Event::NewEvents(StartCause::Init)`.
///
/// Alternative to this would be the user calling `window.set_visible(true)` in
/// `StartCause::Init`.
///
/// If this becomes too bothersome to maintain, it can probably be removed
/// without too much damage.
fn window_activation_hack(app: &NSApplication) {
    // TODO: Proper ordering of the windows
    app.windows().into_iter().for_each(|window| {
        // Call `makeKeyAndOrderFront` if it was called on the window in `WinitWindow::new`
        // This way we preserve the user's desired initial visiblity status
        // TODO: Also filter on the type/"level" of the window, and maybe other things?
        if window.isVisible() {
            trace!("Activating visible window");
            window.makeKeyAndOrderFront(None);
        } else {
            trace!("Skipping activating invisible window");
        }
    })
}
