use objc2::{declare_class, mutability, ClassType, DeclaredClass};
use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol};
use objc2_ui_kit::{UIApplication, UIWindow};

use super::app_state::{self, EventWrapper};
use super::window::WinitUIWindow;
use crate::event::{Event, WindowEvent};
use crate::window::WindowId as RootWindowId;

declare_class!(
    pub struct AppDelegate;

    unsafe impl ClassType for AppDelegate {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "WinitApplicationDelegate";
    }

    impl DeclaredClass for AppDelegate {}

    // UIApplicationDelegate protocol
    unsafe impl AppDelegate {
        #[method(application:didFinishLaunchingWithOptions:)]
        fn did_finish_launching(&self, _application: &UIApplication, _: *mut NSObject) -> bool {
            app_state::did_finish_launching(MainThreadMarker::new().unwrap());
            true
        }

        #[method(applicationDidBecomeActive:)]
        fn did_become_active(&self, _application: &UIApplication) {
            let mtm = MainThreadMarker::new().unwrap();
            app_state::handle_nonuser_event(mtm, EventWrapper::StaticEvent(Event::Resumed))
        }

        #[method(applicationWillResignActive:)]
        fn will_resign_active(&self, _application: &UIApplication) {
            let mtm = MainThreadMarker::new().unwrap();
            app_state::handle_nonuser_event(mtm, EventWrapper::StaticEvent(Event::Suspended))
        }

        #[method(applicationWillEnterForeground:)]
        fn will_enter_foreground(&self, application: &UIApplication) {
            self.send_occluded_event_for_all_windows(application, false);
        }

        #[method(applicationDidEnterBackground:)]
        fn did_enter_background(&self, application: &UIApplication) {
            self.send_occluded_event_for_all_windows(application, true);
        }

        #[method(applicationWillTerminate:)]
        fn will_terminate(&self, application: &UIApplication) {
            let mut events = Vec::new();
            #[allow(deprecated)]
            for window in application.windows().iter() {
                if window.is_kind_of::<WinitUIWindow>() {
                    // SAFETY: We just checked that the window is a `winit` window
                    let window = unsafe {
                        let ptr: *const UIWindow = window;
                        let ptr: *const WinitUIWindow = ptr.cast();
                        &*ptr
                    };
                    events.push(EventWrapper::StaticEvent(Event::WindowEvent {
                        window_id: RootWindowId(window.id()),
                        event: WindowEvent::Destroyed,
                    }));
                }
            }
            let mtm = MainThreadMarker::new().unwrap();
            app_state::handle_nonuser_events(mtm, events);
            app_state::terminated(mtm);
        }

        #[method(applicationDidReceiveMemoryWarning:)]
        fn did_receive_memory_warning(&self, _application: &UIApplication) {
            let mtm = MainThreadMarker::new().unwrap();
            app_state::handle_nonuser_event(mtm, EventWrapper::StaticEvent(Event::MemoryWarning))
        }
    }
);

impl AppDelegate {
    fn send_occluded_event_for_all_windows(
        &self,
        application: &UIApplication,
        occluded: bool,
    ) {
        let mut events = Vec::new();
        #[allow(deprecated)]
        for window in application.windows().iter() {
            if window.is_kind_of::<WinitUIWindow>() {
                // SAFETY: We just checked that the window is a `winit` window
                let window = unsafe {
                    let ptr: *const UIWindow = window;
                    let ptr: *const WinitUIWindow = ptr.cast();
                    &*ptr
                };
                events.push(EventWrapper::StaticEvent(Event::WindowEvent {
                    window_id: RootWindowId(window.id()),
                    event: WindowEvent::Occluded(occluded),
                }));
            }
        }
        let mtm = MainThreadMarker::new().unwrap();
        app_state::handle_nonuser_events(mtm, events);
    }
}
