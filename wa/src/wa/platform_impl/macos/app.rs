#![allow(clippy::unnecessary_cast)]

use icrate::Foundation::NSObject;
use objc2::{declare_class, msg_send, mutability, ClassType};

use super::appkit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSResponder};
use super::{app_state::AppState, DEVICE_ID};
use crate::event::{DeviceEvent, ElementState, Event};

declare_class!(
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(super) struct WinitApplication;

    unsafe impl ClassType for WinitApplication {
        #[inherits(NSResponder, NSObject)]
        type Super = NSApplication;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "WinitApplication";
    }

    unsafe impl WinitApplication {
        // Normally, holding Cmd + any key never sends us a `keyUp` event for that key.
        // Overriding `sendEvent:` like this fixes that. (https://stackoverflow.com/a/15294196)
        // Fun fact: Firefox still has this bug! (https://bugzilla.mozilla.org/show_bug.cgi?id=1299553)
        #[method(sendEvent:)]
        fn send_event(&self, event: &NSEvent) {
            // For posterity, there are some undocumented event types
            // (https://github.com/servo/cocoa-rs/issues/155)
            // but that doesn't really matter here.
            let event_type = event.type_();
            let modifier_flags = event.modifierFlags();
            if event_type == NSEventType::NSKeyUp
                && modifier_flags.contains(NSEventModifierFlags::NSCommandKeyMask)
            {
                if let Some(key_window) = self.keyWindow() {
                    unsafe { key_window.sendEvent(event) };
                }
            } else {
                maybe_dispatch_device_event(event);
                unsafe { msg_send![super(self), sendEvent: event] }
            }
        }
    }
);

fn maybe_dispatch_device_event(event: &NSEvent) {
    let event_type = event.type_();
    match event_type {
        NSEventType::NSMouseMoved
        | NSEventType::NSLeftMouseDragged
        | NSEventType::NSOtherMouseDragged
        | NSEventType::NSRightMouseDragged => {
            let delta_x = event.deltaX() as f64;
            let delta_y = event.deltaY() as f64;

            if delta_x != 0.0 {
                queue_device_event(DeviceEvent::Motion {
                    axis: 0,
                    value: delta_x,
                });
            }

            if delta_y != 0.0 {
                queue_device_event(DeviceEvent::Motion {
                    axis: 1,
                    value: delta_y,
                })
            }

            if delta_x != 0.0 || delta_y != 0.0 {
                queue_device_event(DeviceEvent::MouseMotion {
                    delta: (delta_x, delta_y),
                });
            }
        }
        NSEventType::NSLeftMouseDown
        | NSEventType::NSRightMouseDown
        | NSEventType::NSOtherMouseDown => {
            queue_device_event(DeviceEvent::Button {
                button: event.buttonNumber() as u32,
                state: ElementState::Pressed,
            });
        }
        NSEventType::NSLeftMouseUp | NSEventType::NSRightMouseUp | NSEventType::NSOtherMouseUp => {
            queue_device_event(DeviceEvent::Button {
                button: event.buttonNumber() as u32,
                state: ElementState::Released,
            });
        }
        _ => (),
    }
}

fn queue_device_event(event: DeviceEvent) {
    let event = Event::DeviceEvent {
        device_id: DEVICE_ID,
        event,
    };
    AppState::queue_event(event);
}
