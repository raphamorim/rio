use objc2::sel;
use std::any::Any;
use std::cell::Cell;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::os::raw::c_void;
use std::panic::{catch_unwind, resume_unwind, RefUnwindSafe, UnwindSafe};
use std::ptr;
use std::rc::{Rc, Weak};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use core_foundation::base::{CFIndex, CFRelease};
use core_foundation::runloop::{
    kCFRunLoopCommonModes, CFRunLoopAddSource, CFRunLoopGetMain, CFRunLoopSourceContext,
    CFRunLoopSourceCreate, CFRunLoopSourceRef, CFRunLoopSourceSignal, CFRunLoopWakeUp,
};
use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::ProtocolObject;
use objc2::{msg_send_id, ClassType};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSWindow};
use objc2_foundation::{MainThreadMarker, NSObjectProtocol};

use super::app::WinitApplication;
use super::app_delegate::{ApplicationDelegate, HandlePendingUserEvents};
use super::event::dummy_event;
use super::monitor::{self, MonitorHandle};
use super::observer::setup_control_flow_observers;
use crate::error::EventLoopError;
use crate::event::Event;
use crate::event_loop::{
    ActiveEventLoop as RootWindowTarget, ControlFlow, DeviceEvents, EventLoopClosed,
};
use crate::platform::macos::ActivationPolicy;
use crate::platform::pump_events::PumpStatus;
use crate::platform_impl::platform::cursor::CustomCursor;
use crate::window::Theme;
use crate::window::{CustomCursor as RootCustomCursor, CustomCursorSource};

#[derive(Default)]
pub struct PanicInfo {
    inner: Cell<Option<Box<dyn Any + Send + 'static>>>,
}

// WARNING:
// As long as this struct is used through its `impl`, it is UnwindSafe.
// (If `get_mut` is called on `inner`, unwind safety may get broken.)
impl UnwindSafe for PanicInfo {}
impl RefUnwindSafe for PanicInfo {}
impl PanicInfo {
    pub fn is_panicking(&self) -> bool {
        let inner = self.inner.take();
        let result = inner.is_some();
        self.inner.set(inner);
        result
    }

    /// Overwrites the current state if the current state is not panicking
    pub fn set_panic(&self, p: Box<dyn Any + Send + 'static>) {
        if !self.is_panicking() {
            self.inner.set(Some(p));
        }
    }

    pub fn take(&self) -> Option<Box<dyn Any + Send + 'static>> {
        self.inner.take()
    }
}

#[derive(Debug)]
pub struct ActiveEventLoop {
    delegate: Retained<ApplicationDelegate>,
    pub(super) mtm: MainThreadMarker,
}

impl ActiveEventLoop {
    pub(super) fn new_root(delegate: Retained<ApplicationDelegate>) -> RootWindowTarget {
        let mtm = MainThreadMarker::from(&*delegate);
        let p = Self { delegate, mtm };
        RootWindowTarget {
            p,
            _marker: PhantomData,
        }
    }

    pub(super) fn app_delegate(&self) -> &ApplicationDelegate {
        &self.delegate
    }

    pub fn create_custom_cursor(&self, source: CustomCursorSource) -> RootCustomCursor {
        RootCustomCursor {
            inner: CustomCursor::new(source.inner),
        }
    }

    #[inline]
    pub fn available_monitors(&self) -> VecDeque<MonitorHandle> {
        monitor::available_monitors()
    }

    #[inline]
    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        let monitor = monitor::primary_monitor();
        Some(monitor)
    }

    #[inline]
    pub fn listen_device_events(&self, _allowed: DeviceEvents) {}

    #[inline]
    pub fn raw_display_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawDisplayHandle, raw_window_handle::HandleError> {
        Ok(raw_window_handle::RawDisplayHandle::AppKit(
            raw_window_handle::AppKitDisplayHandle::new(),
        ))
    }

    pub(crate) fn set_control_flow(&self, control_flow: ControlFlow) {
        self.delegate.set_control_flow(control_flow)
    }

    pub(crate) fn control_flow(&self) -> ControlFlow {
        self.delegate.control_flow()
    }

    pub(crate) fn system_theme(&self) -> Option<Theme> {
        let app = NSApplication::sharedApplication(self.mtm);

        if app.respondsToSelector(sel!(effectiveAppearance)) {
            Some(super::window_delegate::appearance_to_theme(
                &app.effectiveAppearance(),
            ))
        } else {
            Some(Theme::Light)
        }
    }

    pub(crate) fn exit(&self) {
        self.delegate.exit()
    }

    pub(crate) fn clear_exit(&self) {
        self.delegate.clear_exit()
    }

    pub(crate) fn exiting(&self) -> bool {
        self.delegate.exiting()
    }

    pub(crate) fn owned_display_handle(&self) -> OwnedDisplayHandle {
        OwnedDisplayHandle
    }

    pub(crate) fn hide_application(&self) {
        NSApplication::sharedApplication(self.mtm).hide(None)
    }

    pub(crate) fn hide_other_applications(&self) {
        NSApplication::sharedApplication(self.mtm).hideOtherApplications(None)
    }

    pub(crate) fn set_allows_automatic_window_tabbing(&self, enabled: bool) {
        NSWindow::setAllowsAutomaticWindowTabbing(enabled, self.mtm)
    }

    pub(crate) fn allows_automatic_window_tabbing(&self) -> bool {
        NSWindow::allowsAutomaticWindowTabbing(self.mtm)
    }
}

fn map_user_event<T: 'static>(
    mut handler: impl FnMut(Event<T>, &RootWindowTarget),
    receiver: Rc<mpsc::Receiver<T>>,
) -> impl FnMut(Event<HandlePendingUserEvents>, &RootWindowTarget) {
    move |event, window_target| match event.map_nonuser_event() {
        Ok(event) => (handler)(event, window_target),
        Err(_) => {
            for event in receiver.try_iter() {
                (handler)(Event::UserEvent(event), window_target);
            }
        }
    }
}

pub struct EventLoop<T: 'static> {
    /// Store a reference to the application for convenience.
    ///
    /// We intentionally don't store `WinitApplication` since we want to have
    /// the possibility of swapping that out at some point.
    app: Retained<NSApplication>,
    /// The application delegate that we've registered.
    ///
    /// The delegate is only weakly referenced by NSApplication, so we must
    /// keep it around here as well.
    delegate: Retained<ApplicationDelegate>,

    // Event sender and receiver, used for EventLoopProxy.
    sender: mpsc::Sender<T>,
    receiver: Rc<mpsc::Receiver<T>>,

    window_target: RootWindowTarget,
    panic_info: Rc<PanicInfo>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PlatformSpecificEventLoopAttributes {
    pub(crate) activation_policy: ActivationPolicy,
    pub(crate) default_menu: bool,
    pub(crate) activate_ignoring_other_apps: bool,
}

impl Default for PlatformSpecificEventLoopAttributes {
    fn default() -> Self {
        Self {
            activation_policy: Default::default(), // Regular
            default_menu: true,
            activate_ignoring_other_apps: true,
        }
    }
}

impl<T> EventLoop<T> {
    pub(crate) fn new(
        attributes: &PlatformSpecificEventLoopAttributes,
    ) -> Result<Self, EventLoopError> {
        let mtm = MainThreadMarker::new()
            .expect("on macOS, `EventLoop` must be created on the main thread!");

        let app: Retained<NSApplication> =
            unsafe { msg_send_id![WinitApplication::class(), sharedApplication] };

        if !app.is_kind_of::<WinitApplication>() {
            panic!(
                "`winit` requires control over the principal class. You must create the event \
                 loop before other parts of your application initialize NSApplication"
            );
        }

        let activation_policy = match attributes.activation_policy {
            ActivationPolicy::Regular => NSApplicationActivationPolicy::Regular,
            ActivationPolicy::Accessory => NSApplicationActivationPolicy::Accessory,
            ActivationPolicy::Prohibited => NSApplicationActivationPolicy::Prohibited,
        };
        let delegate = ApplicationDelegate::new(
            mtm,
            activation_policy,
            attributes.default_menu,
            attributes.activate_ignoring_other_apps,
        );

        autoreleasepool(|_| {
            app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        });

        let panic_info: Rc<PanicInfo> = Default::default();
        setup_control_flow_observers(Rc::downgrade(&panic_info));

        let (sender, receiver) = mpsc::channel();
        Ok(EventLoop {
            app,
            delegate: delegate.clone(),
            sender,
            receiver: Rc::new(receiver),
            window_target: RootWindowTarget {
                p: ActiveEventLoop { delegate, mtm },
                _marker: PhantomData,
            },
            panic_info,
        })
    }

    pub fn window_target(&self) -> &RootWindowTarget {
        &self.window_target
    }

    pub fn set_confirm_before_quit(&self, confirmation: bool) {
        self.delegate.set_confirm_before_quit(confirmation);
    }

    pub fn run<F>(mut self, handler: F) -> Result<(), EventLoopError>
    where
        F: FnMut(Event<T>, &RootWindowTarget),
    {
        self.run_on_demand(handler)
    }

    // NB: we don't base this on `pump_events` because for `MacOs` we can't support
    // `pump_events` elegantly (we just ask to run the loop for a "short" amount of
    // time and so a layered implementation would end up using a lot of CPU due to
    // redundant wake ups.
    pub fn run_on_demand<F>(&mut self, handler: F) -> Result<(), EventLoopError>
    where
        F: FnMut(Event<T>, &RootWindowTarget),
    {
        let handler = map_user_event(handler, self.receiver.clone());

        self.delegate.set_event_handler(handler, || {
            autoreleasepool(|_| {
                // clear / normalize pump_events state
                self.delegate.set_wait_timeout(None);
                self.delegate.set_stop_before_wait(false);
                self.delegate.set_stop_after_wait(false);
                self.delegate.set_stop_on_redraw(false);

                if self.delegate.is_launched() {
                    debug_assert!(!self.delegate.is_running());
                    self.delegate.set_is_running(true);
                    self.delegate.dispatch_init_events();
                }

                // SAFETY: We do not run the application re-entrantly
                unsafe { self.app.run() };

                // While the app is running it's possible that we catch a panic
                // to avoid unwinding across an objective-c ffi boundary, which
                // will lead to us stopping the `NSApplication` and saving the
                // `PanicInfo` so that we can resume the unwind at a controlled,
                // safe point in time.
                if let Some(panic) = self.panic_info.take() {
                    resume_unwind(panic);
                }

                self.delegate.internal_exit()
            })
        });

        Ok(())
    }

    pub fn pump_events<F>(&mut self, timeout: Option<Duration>, handler: F) -> PumpStatus
    where
        F: FnMut(Event<T>, &RootWindowTarget),
    {
        let handler = map_user_event(handler, self.receiver.clone());

        self.delegate.set_event_handler(handler, || {
            autoreleasepool(|_| {
                // As a special case, if the application hasn't been launched yet then we at least
                // run the loop until it has fully launched.
                if !self.delegate.is_launched() {
                    debug_assert!(!self.delegate.is_running());

                    self.delegate.set_stop_on_launch();
                    // SAFETY: We do not run the application re-entrantly
                    unsafe { self.app.run() };

                    // Note: we dispatch `NewEvents(Init)` + `Resumed` events after the application
                    // has launched
                } else if !self.delegate.is_running() {
                    // Even though the application may have been launched, it's possible we aren't
                    // running if the `EventLoop` was run before and has since
                    // exited. This indicates that we just starting to re-run
                    // the same `EventLoop` again.
                    self.delegate.set_is_running(true);
                    self.delegate.dispatch_init_events();
                } else {
                    // Only run for as long as the given `Duration` allows so we don't block the
                    // external loop.
                    match timeout {
                        Some(Duration::ZERO) => {
                            self.delegate.set_wait_timeout(None);
                            self.delegate.set_stop_before_wait(true);
                        }
                        Some(duration) => {
                            self.delegate.set_stop_before_wait(false);
                            let timeout = Instant::now() + duration;
                            self.delegate.set_wait_timeout(Some(timeout));
                            self.delegate.set_stop_after_wait(true);
                        }
                        None => {
                            self.delegate.set_wait_timeout(None);
                            self.delegate.set_stop_before_wait(false);
                            self.delegate.set_stop_after_wait(true);
                        }
                    }
                    self.delegate.set_stop_on_redraw(true);
                    // SAFETY: We do not run the application re-entrantly
                    unsafe { self.app.run() };
                }

                // While the app is running it's possible that we catch a panic
                // to avoid unwinding across an objective-c ffi boundary, which
                // will lead to us stopping the application and saving the
                // `PanicInfo` so that we can resume the unwind at a controlled,
                // safe point in time.
                if let Some(panic) = self.panic_info.take() {
                    resume_unwind(panic);
                }

                if self.delegate.exiting() {
                    self.delegate.internal_exit();
                    PumpStatus::Exit(0)
                } else {
                    PumpStatus::Continue
                }
            })
        })
    }

    pub fn create_proxy(&self) -> EventLoopProxy<T> {
        EventLoopProxy::new(self.sender.clone())
    }
}

#[derive(Clone)]
pub(crate) struct OwnedDisplayHandle;

impl OwnedDisplayHandle {
    #[inline]
    pub fn raw_display_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawDisplayHandle, raw_window_handle::HandleError> {
        Ok(raw_window_handle::AppKitDisplayHandle::new().into())
    }
}

pub(super) fn stop_app_immediately(app: &NSApplication) {
    autoreleasepool(|_| {
        app.stop(None);
        // To stop event loop immediately, we need to post some event here.
        // See: https://stackoverflow.com/questions/48041279/stopping-the-nsapplication-main-event-loop/48064752#48064752
        app.postEvent_atStart(&dummy_event().unwrap(), true);
    });
}

/// Catches panics that happen inside `f` and when a panic
/// happens, stops the `sharedApplication`
#[inline]
pub fn stop_app_on_panic<F: FnOnce() -> R + UnwindSafe, R>(
    mtm: MainThreadMarker,
    panic_info: Weak<PanicInfo>,
    f: F,
) -> Option<R> {
    match catch_unwind(f) {
        Ok(r) => Some(r),
        Err(e) => {
            // It's important that we set the panic before requesting a `stop`
            // because some callback are still called during the `stop` message
            // and we need to know in those callbacks if the application is currently
            // panicking
            {
                let panic_info = panic_info.upgrade().unwrap();
                panic_info.set_panic(e);
            }
            let app = NSApplication::sharedApplication(mtm);
            stop_app_immediately(&app);
            None
        }
    }
}

pub struct EventLoopProxy<T> {
    sender: mpsc::Sender<T>,
    source: CFRunLoopSourceRef,
}

unsafe impl<T: Send> Send for EventLoopProxy<T> {}
unsafe impl<T: Send> Sync for EventLoopProxy<T> {}

impl<T> Drop for EventLoopProxy<T> {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.source as _);
        }
    }
}

impl<T> Clone for EventLoopProxy<T> {
    fn clone(&self) -> Self {
        EventLoopProxy::new(self.sender.clone())
    }
}

impl<T> EventLoopProxy<T> {
    fn new(sender: mpsc::Sender<T>) -> Self {
        unsafe {
            // just wake up the eventloop
            extern "C" fn event_loop_proxy_handler(_: *const c_void) {}

            // adding a Source to the main CFRunLoop lets us wake it up and
            // process user events through the normal OS EventLoop mechanisms.
            let rl = CFRunLoopGetMain();
            let mut context = CFRunLoopSourceContext {
                version: 0,
                info: ptr::null_mut(),
                retain: None,
                release: None,
                copyDescription: None,
                equal: None,
                hash: None,
                schedule: None,
                cancel: None,
                perform: event_loop_proxy_handler,
            };
            let source =
                CFRunLoopSourceCreate(ptr::null_mut(), CFIndex::MAX - 1, &mut context);
            CFRunLoopAddSource(rl, source, kCFRunLoopCommonModes);
            CFRunLoopWakeUp(rl);

            EventLoopProxy { sender, source }
        }
    }

    pub fn send_event(&self, event: T) -> Result<(), EventLoopClosed<T>> {
        self.sender
            .send(event)
            .map_err(|mpsc::SendError(x)| EventLoopClosed(x))?;
        unsafe {
            // let the main thread know there's a new event
            CFRunLoopSourceSignal(self.source);
            let rl = CFRunLoopGetMain();
            CFRunLoopWakeUp(rl);
        }
        Ok(())
    }
}
