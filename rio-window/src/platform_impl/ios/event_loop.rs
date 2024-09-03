use std::collections::VecDeque;
use std::ffi::{c_char, c_int, c_void};
use std::marker::PhantomData;
use std::ptr::{self, NonNull};
use std::sync::mpsc::{self, Receiver, Sender};

use core_foundation::base::{CFIndex, CFRelease};
use core_foundation::runloop::{
    kCFRunLoopAfterWaiting, kCFRunLoopBeforeWaiting, kCFRunLoopCommonModes,
    kCFRunLoopDefaultMode, kCFRunLoopExit, CFRunLoopActivity, CFRunLoopAddObserver,
    CFRunLoopAddSource, CFRunLoopGetMain, CFRunLoopObserverCreate, CFRunLoopObserverRef,
    CFRunLoopSourceContext, CFRunLoopSourceCreate, CFRunLoopSourceInvalidate,
    CFRunLoopSourceRef, CFRunLoopSourceSignal, CFRunLoopWakeUp,
};
use objc2::rc::Retained;
use objc2::{msg_send_id, ClassType};
use objc2_foundation::{MainThreadMarker, NSString};
use objc2_ui_kit::{
    UIApplication, UIApplicationMain, UIDevice, UIScreen, UIUserInterfaceIdiom,
};

use crate::error::EventLoopError;
use crate::event::Event;
use crate::event_loop::{
    ActiveEventLoop as RootActiveEventLoop, ControlFlow, DeviceEvents, EventLoopClosed,
};
use crate::platform::ios::Idiom;
use crate::platform_impl::platform::app_state::{
    EventLoopHandler, HandlePendingUserEvents,
};
use crate::window::{CustomCursor, CustomCursorSource};

use super::app_delegate::AppDelegate;
use super::app_state::AppState;
use super::{app_state, monitor, MonitorHandle};

#[derive(Debug)]
pub struct ActiveEventLoop {
    pub(super) mtm: MainThreadMarker,
}

impl ActiveEventLoop {
    pub fn create_custom_cursor(&self, source: CustomCursorSource) -> CustomCursor {
        let _ = source.inner;
        CustomCursor {
            inner: super::PlatformCustomCursor,
        }
    }

    pub fn available_monitors(&self) -> VecDeque<MonitorHandle> {
        monitor::uiscreens(self.mtm)
    }

    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        #[allow(deprecated)]
        Some(MonitorHandle::new(UIScreen::mainScreen(self.mtm)))
    }

    #[inline]
    pub fn listen_device_events(&self, _allowed: DeviceEvents) {}

    #[inline]
    pub fn raw_display_handle_rwh_06(
        &self,
    ) -> Result<raw_window_handle::RawDisplayHandle, raw_window_handle::HandleError> {
        Ok(raw_window_handle::RawDisplayHandle::UiKit(
            raw_window_handle::UiKitDisplayHandle::new(),
        ))
    }

    pub(crate) fn set_control_flow(&self, control_flow: ControlFlow) {
        AppState::get_mut(self.mtm).set_control_flow(control_flow)
    }

    pub(crate) fn control_flow(&self) -> ControlFlow {
        AppState::get_mut(self.mtm).control_flow()
    }

    pub(crate) fn exit(&self) {
        // https://developer.apple.com/library/archive/qa/qa1561/_index.html
        // it is not possible to quit an iOS app gracefully and programmatically
        tracing::warn!("`ControlFlow::Exit` ignored on iOS");
    }

    pub(crate) fn exiting(&self) -> bool {
        false
    }

    pub(crate) fn owned_display_handle(&self) -> OwnedDisplayHandle {
        OwnedDisplayHandle
    }
}

#[derive(Clone)]
pub(crate) struct OwnedDisplayHandle;

impl OwnedDisplayHandle {
    #[inline]
    pub fn raw_display_handle_rwh_06(
        &self,
    ) -> Result<raw_window_handle::RawDisplayHandle, raw_window_handle::HandleError> {
        Ok(raw_window_handle::UiKitDisplayHandle::new().into())
    }
}

fn map_user_event<T: 'static>(
    mut handler: impl FnMut(Event<T>, &RootActiveEventLoop),
    receiver: mpsc::Receiver<T>,
) -> impl FnMut(Event<HandlePendingUserEvents>, &RootActiveEventLoop) {
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
    mtm: MainThreadMarker,
    sender: Sender<T>,
    receiver: Receiver<T>,
    window_target: RootActiveEventLoop,
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PlatformSpecificEventLoopAttributes {}

impl<T: 'static> EventLoop<T> {
    pub(crate) fn new(
        _: &PlatformSpecificEventLoopAttributes,
    ) -> Result<EventLoop<T>, EventLoopError> {
        let mtm = MainThreadMarker::new()
            .expect("On iOS, `EventLoop` must be created on the main thread");

        static mut SINGLETON_INIT: bool = false;
        unsafe {
            assert!(
                !SINGLETON_INIT,
                "Only one `EventLoop` is supported on iOS. `EventLoopProxy` might be helpful"
            );
            SINGLETON_INIT = true;
        }

        let (sender, receiver) = mpsc::channel();

        // this line sets up the main run loop before `UIApplicationMain`
        setup_control_flow_observers();

        Ok(EventLoop {
            mtm,
            sender,
            receiver,
            window_target: RootActiveEventLoop {
                p: ActiveEventLoop { mtm },
                _marker: PhantomData,
            },
        })
    }

    pub fn run<F>(self, handler: F) -> !
    where
        F: FnMut(Event<T>, &RootActiveEventLoop),
    {
        let application: Option<Retained<UIApplication>> =
            unsafe { msg_send_id![UIApplication::class(), sharedApplication] };
        assert!(
            application.is_none(),
            "\
                `EventLoop` cannot be `run` after a call to `UIApplicationMain` on iOS\nNote: \
             `EventLoop::run_app` calls `UIApplicationMain` on iOS",
        );

        let handler = map_user_event(handler, self.receiver);

        let handler = unsafe {
            std::mem::transmute::<
                Box<dyn FnMut(Event<HandlePendingUserEvents>, &RootActiveEventLoop)>,
                Box<dyn FnMut(Event<HandlePendingUserEvents>, &RootActiveEventLoop)>,
            >(Box::new(handler))
        };

        let handler = EventLoopHandler {
            handler,
            event_loop: self.window_target,
        };

        app_state::will_launch(self.mtm, handler);

        // Ensure application delegate is initialized
        let _ = AppDelegate::class();

        extern "C" {
            // These functions are in crt_externs.h.
            fn _NSGetArgc() -> *mut c_int;
            fn _NSGetArgv() -> *mut *mut *mut c_char;
        }

        unsafe {
            UIApplicationMain(
                *_NSGetArgc(),
                NonNull::new(*_NSGetArgv()).unwrap(),
                None,
                Some(&NSString::from_str(AppDelegate::NAME)),
            )
        };
        unreachable!()
    }

    pub fn create_proxy(&self) -> EventLoopProxy<T> {
        EventLoopProxy::new(self.sender.clone())
    }

    pub fn window_target(&self) -> &RootActiveEventLoop {
        &self.window_target
    }
}

// EventLoopExtIOS
impl<T: 'static> EventLoop<T> {
    pub fn idiom(&self) -> Idiom {
        match UIDevice::currentDevice(self.mtm).userInterfaceIdiom() {
            UIUserInterfaceIdiom::Unspecified => Idiom::Unspecified,
            UIUserInterfaceIdiom::Phone => Idiom::Phone,
            UIUserInterfaceIdiom::Pad => Idiom::Pad,
            UIUserInterfaceIdiom::TV => Idiom::TV,
            UIUserInterfaceIdiom::CarPlay => Idiom::CarPlay,
            _ => Idiom::Unspecified,
        }
    }
}

pub struct EventLoopProxy<T> {
    sender: Sender<T>,
    source: CFRunLoopSourceRef,
}

unsafe impl<T: Send> Send for EventLoopProxy<T> {}
unsafe impl<T: Send> Sync for EventLoopProxy<T> {}

impl<T> Clone for EventLoopProxy<T> {
    fn clone(&self) -> EventLoopProxy<T> {
        EventLoopProxy::new(self.sender.clone())
    }
}

impl<T> Drop for EventLoopProxy<T> {
    fn drop(&mut self) {
        unsafe {
            CFRunLoopSourceInvalidate(self.source);
            CFRelease(self.source as _);
        }
    }
}

impl<T> EventLoopProxy<T> {
    fn new(sender: Sender<T>) -> EventLoopProxy<T> {
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
            let source = CFRunLoopSourceCreate(
                ptr::null_mut(),
                CFIndex::max_value() - 1,
                &mut context,
            );
            CFRunLoopAddSource(rl, source, kCFRunLoopCommonModes);
            CFRunLoopWakeUp(rl);

            EventLoopProxy { sender, source }
        }
    }

    pub fn send_event(&self, event: T) -> Result<(), EventLoopClosed<T>> {
        self.sender
            .send(event)
            .map_err(|::std::sync::mpsc::SendError(x)| EventLoopClosed(x))?;
        unsafe {
            // let the main thread know there's a new event
            CFRunLoopSourceSignal(self.source);
            let rl = CFRunLoopGetMain();
            CFRunLoopWakeUp(rl);
        }
        Ok(())
    }
}

fn setup_control_flow_observers() {
    unsafe {
        // begin is queued with the highest priority to ensure it is processed before other
        // observers
        extern "C" fn control_flow_begin_handler(
            _: CFRunLoopObserverRef,
            activity: CFRunLoopActivity,
            _: *mut c_void,
        ) {
            let mtm = MainThreadMarker::new().unwrap();
            #[allow(non_upper_case_globals)]
            match activity {
                kCFRunLoopAfterWaiting => app_state::handle_wakeup_transition(mtm),
                _ => unreachable!(),
            }
        }

        // Core Animation registers its `CFRunLoopObserver` that performs drawing operations in
        // `CA::Transaction::ensure_implicit` with a priority of `0x1e8480`. We set the main_end
        // priority to be 0, in order to send AboutToWait before RedrawRequested. This value was
        // chosen conservatively to guard against apple using different priorities for their redraw
        // observers in different OS's or on different devices. If it so happens that it's too
        // conservative, the main symptom would be non-redraw events coming in after `AboutToWait`.
        //
        // The value of `0x1e8480` was determined by inspecting stack traces and the associated
        // registers for every `CFRunLoopAddObserver` call on an iPad Air 2 running iOS 11.4.
        //
        // Also tested to be `0x1e8480` on iPhone 8, iOS 13 beta 4.
        extern "C" fn control_flow_main_end_handler(
            _: CFRunLoopObserverRef,
            activity: CFRunLoopActivity,
            _: *mut c_void,
        ) {
            let mtm = MainThreadMarker::new().unwrap();
            #[allow(non_upper_case_globals)]
            match activity {
                kCFRunLoopBeforeWaiting => app_state::handle_main_events_cleared(mtm),
                kCFRunLoopExit => {} // may happen when running on macOS
                _ => unreachable!(),
            }
        }

        // end is queued with the lowest priority to ensure it is processed after other observers
        extern "C" fn control_flow_end_handler(
            _: CFRunLoopObserverRef,
            activity: CFRunLoopActivity,
            _: *mut c_void,
        ) {
            let mtm = MainThreadMarker::new().unwrap();
            #[allow(non_upper_case_globals)]
            match activity {
                kCFRunLoopBeforeWaiting => app_state::handle_events_cleared(mtm),
                kCFRunLoopExit => {} // may happen when running on macOS
                _ => unreachable!(),
            }
        }

        let main_loop = CFRunLoopGetMain();

        let begin_observer = CFRunLoopObserverCreate(
            ptr::null_mut(),
            kCFRunLoopAfterWaiting,
            1, // repeat = true
            CFIndex::min_value(),
            control_flow_begin_handler,
            ptr::null_mut(),
        );
        CFRunLoopAddObserver(main_loop, begin_observer, kCFRunLoopDefaultMode);

        let main_end_observer = CFRunLoopObserverCreate(
            ptr::null_mut(),
            kCFRunLoopExit | kCFRunLoopBeforeWaiting,
            1, // repeat = true
            0, // see comment on `control_flow_main_end_handler`
            control_flow_main_end_handler,
            ptr::null_mut(),
        );
        CFRunLoopAddObserver(main_loop, main_end_observer, kCFRunLoopDefaultMode);

        let end_observer = CFRunLoopObserverCreate(
            ptr::null_mut(),
            kCFRunLoopExit | kCFRunLoopBeforeWaiting,
            1, // repeat = true
            CFIndex::max_value(),
            control_flow_end_handler,
            ptr::null_mut(),
        );
        CFRunLoopAddObserver(main_loop, end_observer, kCFRunLoopDefaultMode);
    }
}
