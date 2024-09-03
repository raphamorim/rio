#![allow(clippy::unnecessary_cast)]

use objc2::{declare_class, msg_send, mutability, ClassType, DeclaredClass};
use objc2_app_kit::{
    NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSResponder,
};
use objc2_foundation::{MainThreadMarker, NSObject};

use super::app_delegate::ApplicationDelegate;
use crate::event::{DeviceEvent, ElementState};

declare_class!(
    pub(super) struct WinitApplication;

    unsafe impl ClassType for WinitApplication {
        #[inherits(NSResponder, NSObject)]
        type Super = NSApplication;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "WinitApplication";
    }

    impl DeclaredClass for WinitApplication {}

    unsafe impl WinitApplication {
        // Normally, holding Cmd + any key never sends us a `keyUp` event for that key.
        // Overriding `sendEvent:` like this fixes that. (https://stackoverflow.com/a/15294196)
        // Fun fact: Firefox still has this bug! (https://bugzilla.mozilla.org/show_bug.cgi?id=1299553)
        #[method(sendEvent:)]
        fn send_event(&self, event: &NSEvent) {
            // For posterity, there are some undocumented event types
            // (https://github.com/servo/cocoa-rs/issues/155)
            // but that doesn't really matter here.
            let event_type = unsafe { event.r#type() };
            let modifier_flags = unsafe { event.modifierFlags() };
            if event_type == NSEventType::KeyUp
                && modifier_flags.contains(NSEventModifierFlags::NSEventModifierFlagCommand)
            {
                if let Some(key_window) = self.keyWindow() {
                    key_window.sendEvent(event);
                }
            } else {
                let delegate = ApplicationDelegate::get(MainThreadMarker::from(self));
                maybe_dispatch_device_event(&delegate, event);
                unsafe { msg_send![super(self), sendEvent: event] }
            }
        }
    }
);

fn maybe_dispatch_device_event(delegate: &ApplicationDelegate, event: &NSEvent) {
    let event_type = unsafe { event.r#type() };
    #[allow(non_upper_case_globals)]
    match event_type {
        NSEventType::MouseMoved
        | NSEventType::LeftMouseDragged
        | NSEventType::OtherMouseDragged
        | NSEventType::RightMouseDragged => {
            let delta_x = unsafe { event.deltaX() } as f64;
            let delta_y = unsafe { event.deltaY() } as f64;

            if delta_x != 0.0 {
                delegate.queue_device_event(DeviceEvent::Motion {
                    axis: 0,
                    value: delta_x,
                });
            }

            if delta_y != 0.0 {
                delegate.queue_device_event(DeviceEvent::Motion {
                    axis: 1,
                    value: delta_y,
                })
            }

            if delta_x != 0.0 || delta_y != 0.0 {
                delegate.queue_device_event(DeviceEvent::MouseMotion {
                    delta: (delta_x, delta_y),
                });
            }
        }
        NSEventType::LeftMouseDown
        | NSEventType::RightMouseDown
        | NSEventType::OtherMouseDown => {
            delegate.queue_device_event(DeviceEvent::Button {
                button: unsafe { event.buttonNumber() } as u32,
                state: ElementState::Pressed,
            });
        }
        NSEventType::LeftMouseUp
        | NSEventType::RightMouseUp
        | NSEventType::OtherMouseUp => {
            delegate.queue_device_event(DeviceEvent::Button {
                button: unsafe { event.buttonNumber() } as u32,
                state: ElementState::Released,
            });
        }
        _ => (),
    }
}
