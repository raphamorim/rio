use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::VecDeque,
    marker::PhantomData,
    mem,
    os::raw::c_void,
    panic::{catch_unwind, resume_unwind, AssertUnwindSafe, RefUnwindSafe, UnwindSafe},
    ptr,
    rc::{Rc, Weak},
    sync::mpsc,
    time::{Duration, Instant},
};

use core_foundation::base::{CFIndex, CFRelease};
use core_foundation::runloop::{
    kCFRunLoopCommonModes, CFRunLoopAddSource, CFRunLoopGetMain, CFRunLoopSourceContext,
    CFRunLoopSourceCreate, CFRunLoopSourceRef, CFRunLoopSourceSignal, CFRunLoopWakeUp,
};
use icrate::Foundation::MainThreadMarker;
use objc2::rc::{autoreleasepool, Id};
use objc2::runtime::NSObjectProtocol;
use objc2::{msg_send_id, ClassType};

use super::appkit::{NSApp, NSApplication, NSApplicationActivationPolicy, NSEvent, NSWindow};
use crate::{
    error::EventLoopError,
    event::Event,
    event_loop::{
        ControlFlow, DeviceEvents, EventLoopClosed, EventLoopWindowTarget as RootWindowTarget,
    },
    platform::{macos::ActivationPolicy, pump_events::PumpStatus},
    wa::platform_impl::platform::{
        app::WinitApplication,
        app_delegate::ApplicationDelegate,
        app_state::{AppState, Callback},
        monitor::{self, MonitorHandle},
        observer::setup_control_flow_observers,
    },
};

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
    /// Overwrites the curret state if the current state is not panicking
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
pub struct EventLoopWindowTarget<T: 'static> {
    mtm: MainThreadMarker,
    p: PhantomData<T>,
}

impl<T: 'static> EventLoopWindowTarget<T> {
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

    #[cfg(feature = "rwh_05")]
    #[inline]
    pub fn raw_display_handle_rwh_05(&self) -> rwh_05::RawDisplayHandle {
        rwh_05::RawDisplayHandle::AppKit(rwh_05::AppKitDisplayHandle::empty())
    }

    #[cfg(feature = "rwh_06")]
    #[inline]
    pub fn raw_display_handle_rwh_06(
        &self,
    ) -> Result<rwh_06::RawDisplayHandle, rwh_06::HandleError> {
        Ok(rwh_06::RawDisplayHandle::AppKit(
            rwh_06::AppKitDisplayHandle::new(),
        ))
    }

    pub(crate) fn set_control_flow(&self, control_flow: ControlFlow) {
        AppState::set_control_flow(control_flow)
    }

    pub(crate) fn control_flow(&self) -> ControlFlow {
        AppState::control_flow()
    }

    pub(crate) fn exit(&self) {
        AppState::exit()
    }

    pub(crate) fn exiting(&self) -> bool {
        AppState::exiting()
    }
}

impl<T> EventLoopWindowTarget<T> {
    pub(crate) fn hide_application(&self) {
        NSApplication::shared(self.mtm).hide(None)
    }

    pub(crate) fn hide_other_applications(&self) {
        NSApplication::shared(self.mtm).hideOtherApplications(None)
    }

    pub(crate) fn set_allows_automatic_window_tabbing(&self, enabled: bool) {
        NSWindow::setAllowsAutomaticWindowTabbing(enabled)
    }

    pub(crate) fn allows_automatic_window_tabbing(&self) -> bool {
        NSWindow::allowsAutomaticWindowTabbing()
    }
}

pub struct EventLoop<T: 'static> {
    /// Store a reference to the application for convenience.
    ///
    /// We intentially don't store `WinitApplication` since we want to have
    /// the possiblity of swapping that out at some point.
    app: Id<NSApplication>,
    /// The delegate is only weakly referenced by NSApplication, so we keep
    /// it around here as well.
    _delegate: Id<ApplicationDelegate>,

    // Event sender and receiver, used for EventLoopProxy.
    sender: mpsc::Sender<T>,
    receiver: Rc<mpsc::Receiver<T>>,

    window_target: Rc<RootWindowTarget<T>>,
    panic_info: Rc<PanicInfo>,

    /// We make sure that the callback closure is dropped during a panic
    /// by making the event loop own it.
    ///
    /// Every other reference should be a Weak reference which is only upgraded
    /// into a strong reference in order to call the callback but then the
    /// strong reference should be dropped as soon as possible.
    _callback: Option<Rc<Callback<T>>>,
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

        let app: Id<NSApplication> =
            unsafe { msg_send_id![WinitApplication::class(), sharedApplication] };

        if !app.is_kind_of::<WinitApplication>() {
            panic!("`winit` requires control over the principal class. You must create the event loop before other parts of your application initialize NSApplication");
        }

        use NSApplicationActivationPolicy::*;
        let activation_policy = match attributes.activation_policy {
            ActivationPolicy::Regular => NSApplicationActivationPolicyRegular,
            ActivationPolicy::Accessory => NSApplicationActivationPolicyAccessory,
            ActivationPolicy::Prohibited => NSApplicationActivationPolicyProhibited,
        };
        let delegate = ApplicationDelegate::new(
            activation_policy,
            attributes.default_menu,
            attributes.activate_ignoring_other_apps,
        );

        autoreleasepool(|_| {
            app.setDelegate(&delegate);
        });

        let panic_info: Rc<PanicInfo> = Default::default();
        setup_control_flow_observers(Rc::downgrade(&panic_info));

        let (sender, receiver) = mpsc::channel();
        Ok(EventLoop {
            app,
            _delegate: delegate,
            sender,
            receiver: Rc::new(receiver),
            window_target: Rc::new(RootWindowTarget {
                p: EventLoopWindowTarget {
                    mtm,
                    p: PhantomData,
                },
                _marker: PhantomData,
            }),
            panic_info,
            _callback: None,
        })
    }

    pub fn window_target(&self) -> &RootWindowTarget<T> {
        &self.window_target
    }

    pub fn run<F>(mut self, callback: F) -> Result<(), EventLoopError>
    where
        F: FnMut(Event<T>, &RootWindowTarget<T>),
    {
        self.run_on_demand(callback)
    }

    // NB: we don't base this on `pump_events` because for `MacOs` we can't support
    // `pump_events` elegantly (we just ask to run the loop for a "short" amount of
    // time and so a layered implementation would end up using a lot of CPU due to
    // redundant wake ups.
    pub fn run_on_demand<F>(&mut self, callback: F) -> Result<(), EventLoopError>
    where
        F: FnMut(Event<T>, &RootWindowTarget<T>),
    {
        if AppState::is_running() {
            return Err(EventLoopError::AlreadyRunning);
        }

        // # Safety
        // We are erasing the lifetime of the application callback here so that we
        // can (temporarily) store it within 'static global `AppState` that's
        // accessible to objc delegate callbacks.
        //
        // The safety of this depends on on making sure to also clear the callback
        // from the global `AppState` before we return from here, ensuring that
        // we don't retain a reference beyond the real lifetime of the callback.

        let callback = unsafe {
            mem::transmute::<
                Rc<RefCell<dyn FnMut(Event<T>, &RootWindowTarget<T>)>>,
                Rc<RefCell<dyn FnMut(Event<T>, &RootWindowTarget<T>)>>,
            >(Rc::new(RefCell::new(callback)))
        };

        self._callback = Some(Rc::clone(&callback));

        autoreleasepool(|_| {
            // A bit of juggling with the callback references to make sure
            // that `self.callback` is the only owner of the callback.
            let weak_cb: Weak<_> = Rc::downgrade(&callback);
            drop(callback);

            // # Safety
            // We make sure to call `AppState::clear_callback` before returning
            unsafe {
                AppState::set_callback(
                    weak_cb,
                    Rc::clone(&self.window_target),
                    Rc::clone(&self.receiver),
                );
            }

            // catch panics to make sure we can't unwind without clearing the set callback
            // (which would leave the global `AppState` in an undefined, unsafe state)
            let catch_result = catch_unwind(AssertUnwindSafe(|| {
                // clear / normalize pump_events state
                AppState::set_wait_timeout(None);
                AppState::set_stop_app_before_wait(false);
                AppState::set_stop_app_after_wait(false);
                AppState::set_stop_app_on_redraw_requested(false);

                if AppState::is_launched() {
                    debug_assert!(!AppState::is_running());
                    AppState::start_running(); // Set is_running = true + dispatch `NewEvents(Init)` + `Resumed`
                }
                unsafe { self.app.run() };

                // While the app is running it's possible that we catch a panic
                // to avoid unwinding across an objective-c ffi boundary, which
                // will lead to us stopping the `NSApp` and saving the
                // `PanicInfo` so that we can resume the unwind at a controlled,
                // safe point in time.
                if let Some(panic) = self.panic_info.take() {
                    resume_unwind(panic);
                }

                AppState::internal_exit()
            }));

            // # Safety
            // This pairs up with the `unsafe` call to `set_callback` above and ensures that
            // we always clear the application callback from the global `AppState` before
            // returning
            drop(self._callback.take());
            AppState::clear_callback();

            if let Err(payload) = catch_result {
                resume_unwind(payload)
            }
        });

        Ok(())
    }

    pub fn pump_events<F>(&mut self, timeout: Option<Duration>, callback: F) -> PumpStatus
    where
        F: FnMut(Event<T>, &RootWindowTarget<T>),
    {
        // # Safety
        // We are erasing the lifetime of the application callback here so that we
        // can (temporarily) store it within 'static global `AppState` that's
        // accessible to objc delegate callbacks.
        //
        // The safety of this depends on on making sure to also clear the callback
        // from the global `AppState` before we return from here, ensuring that
        // we don't retain a reference beyond the real lifetime of the callback.

        let callback = unsafe {
            mem::transmute::<
                Rc<RefCell<dyn FnMut(Event<T>, &RootWindowTarget<T>)>>,
                Rc<RefCell<dyn FnMut(Event<T>, &RootWindowTarget<T>)>>,
            >(Rc::new(RefCell::new(callback)))
        };

        self._callback = Some(Rc::clone(&callback));

        autoreleasepool(|_| {
            let app = NSApp();

            // A bit of juggling with the callback references to make sure
            // that `self.callback` is the only owner of the callback.
            let weak_cb: Weak<_> = Rc::downgrade(&callback);
            drop(callback);

            // # Safety
            // We will make sure to call `AppState::clear_callback` before returning
            // to ensure that we don't hold on to the callback beyond its (erased)
            // lifetime
            unsafe {
                AppState::set_callback(
                    weak_cb,
                    Rc::clone(&self.window_target),
                    Rc::clone(&self.receiver),
                );
            }

            // catch panics to make sure we can't unwind without clearing the set callback
            // (which would leave the global `AppState` in an undefined, unsafe state)
            let catch_result = catch_unwind(AssertUnwindSafe(|| {
                // As a special case, if the `NSApp` hasn't been launched yet then we at least run
                // the loop until it has fully launched.
                if !AppState::is_launched() {
                    debug_assert!(!AppState::is_running());

                    AppState::request_stop_on_launch();
                    unsafe {
                        app.run();
                    }

                    // Note: we dispatch `NewEvents(Init)` + `Resumed` events after the `NSApp` has launched
                } else if !AppState::is_running() {
                    // Even though the NSApp may have been launched, it's possible we aren't running
                    // if the `EventLoop` was run before and has since exited. This indicates that
                    // we just starting to re-run the same `EventLoop` again.
                    AppState::start_running(); // Set is_running = true + dispatch `NewEvents(Init)` + `Resumed`
                } else {
                    // Only run the NSApp for as long as the given `Duration` allows so we
                    // don't block the external loop.
                    match timeout {
                        Some(Duration::ZERO) => {
                            AppState::set_wait_timeout(None);
                            AppState::set_stop_app_before_wait(true);
                        }
                        Some(duration) => {
                            AppState::set_stop_app_before_wait(false);
                            let timeout = Instant::now() + duration;
                            AppState::set_wait_timeout(Some(timeout));
                            AppState::set_stop_app_after_wait(true);
                        }
                        None => {
                            AppState::set_wait_timeout(None);
                            AppState::set_stop_app_before_wait(false);
                            AppState::set_stop_app_after_wait(true);
                        }
                    }
                    AppState::set_stop_app_on_redraw_requested(true);
                    unsafe {
                        app.run();
                    }
                }

                // While the app is running it's possible that we catch a panic
                // to avoid unwinding across an objective-c ffi boundary, which
                // will lead to us stopping the `NSApp` and saving the
                // `PanicInfo` so that we can resume the unwind at a controlled,
                // safe point in time.
                if let Some(panic) = self.panic_info.take() {
                    resume_unwind(panic);
                }

                if AppState::exiting() {
                    AppState::internal_exit();
                    PumpStatus::Exit(0)
                } else {
                    PumpStatus::Continue
                }
            }));

            // # Safety
            // This pairs up with the `unsafe` call to `set_callback` above and ensures that
            // we always clear the application callback from the global `AppState` before
            // returning
            AppState::clear_callback();
            drop(self._callback.take());

            match catch_result {
                Ok(pump_status) => pump_status,
                Err(payload) => resume_unwind(payload),
            }
        })
    }

    pub fn create_proxy(&self) -> EventLoopProxy<T> {
        EventLoopProxy::new(self.sender.clone())
    }
}

/// Catches panics that happen inside `f` and when a panic
/// happens, stops the `sharedApplication`
#[inline]
pub fn stop_app_on_panic<F: FnOnce() -> R + UnwindSafe, R>(
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
            let app = NSApp();
            app.stop(None);
            // Posting a dummy event to get `stop` to take effect immediately.
            // See: https://stackoverflow.com/questions/48041279/stopping-the-nsapplication-main-event-loop/48064752#48064752
            app.postEvent_atStart(&NSEvent::dummy(), true);
            None
        }
    }
}

pub struct EventLoopProxy<T> {
    sender: mpsc::Sender<T>,
    source: CFRunLoopSourceRef,
}

unsafe impl<T: Send> Send for EventLoopProxy<T> {}

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
                CFRunLoopSourceCreate(ptr::null_mut(), CFIndex::max_value() - 1, &mut context);
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
