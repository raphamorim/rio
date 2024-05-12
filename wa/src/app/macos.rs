use crate::native::apple::frameworks::*;
use crate::EventHandler;
use std::time::Instant;

pub struct Handler {
    pub state: Option<HandlerState>,
}

pub enum HandlerState {
    NotLaunched,
    Running {
        handler: Box<dyn EventHandler>,
    },
    Waiting {
        handler: Box<dyn EventHandler>,
        start: std::time::Instant,
    },
    Terminated,
}

// use std::cell::{RefCell, RefMut};
// impl Handler {
//     pub fn get_mut() -> RefMut<'static, Handler> {
//         // basically everything in UIKit requires the main thread, so it's pointless to use the
//         // std::sync APIs.
//         // must be mut because plain `static` requires `Sync`
//         static mut APP_STATE: RefCell<Option<Handler>> = RefCell::new(None);

//         let mut guard = unsafe { APP_STATE.borrow_mut() };
//         if guard.is_none() {
//             #[inline(never)]
//             #[cold]
//             fn init_guard(guard: &mut RefMut<'static, Option<Handler>>) {
//                 **guard = Some(Handler {
//                     state: Some(HandlerState::NotLaunched),
//                 });
//             }
//             init_guard(&mut guard);
//         }
//         RefMut::map(guard, |state| state.as_mut().unwrap())
//     }

//     fn state(&self) -> &HandlerState {
//         match &self.state {
//             Some(ref st) => st,
//             None => panic!("`HandlerState` previously failed a state transition"),
//         }
//     }

//     pub fn state_mut(&mut self) -> &mut HandlerState {
//         match &mut self.state {
//             Some(ref mut st) => st,
//             None => panic!("`HandlerState` previously failed a state transition"),
//         }
//     }

//     pub fn set_state(&mut self, new_state: HandlerState) {
//         self.state = Some(new_state)
//     }
// }

pub struct EventLoopWaker {
    timer: CFRunLoopTimerRef,

    /// An arbitrary instant in the past, that will trigger an immediate wake
    /// We save this as the `next_fire_date` for consistency so we can
    /// easily check if the next_fire_date needs updating.
    start_instant: Instant,

    /// This is what the `NextFireDate` has been set to.
    /// `None` corresponds to `waker.stop()` and `start_instant` is used
    /// for `waker.start()`
    next_fire_date: Option<Instant>,
}

impl Drop for EventLoopWaker {
    fn drop(&mut self) {
        unsafe {
            CFRunLoopTimerInvalidate(self.timer);
            CFRelease(self.timer as _);
        }
    }
}

impl Default for EventLoopWaker {
    fn default() -> EventLoopWaker {
        extern "C" fn wakeup_main_loop(_timer: CFRunLoopTimerRef, _info: *mut c_void) {}
        unsafe {
            // Create a timer with a 0.1µs interval (1ns does not work) to mimic polling.
            // It is initially setup with a first fire time really far into the
            // future, but that gets changed to fire immediately in did_finish_launching
            let timer: CFRunLoopTimerRef = CFRunLoopTimerCreate(
                std::ptr::null_mut(),
                std::f64::MAX,
                0.000_000_1,
                0,
                0,
                wakeup_main_loop,
                std::ptr::null_mut(),
            );
            let is_valid: bool = CFRunLoopTimerIsValid(timer);
            assert!(is_valid);

            CFRunLoopAddTimer(CFRunLoopGetMain(), timer, kCFRunLoopCommonModes);
            EventLoopWaker {
                timer,
                start_instant: Instant::now(),
                next_fire_date: None,
            }
        }
    }
}

impl EventLoopWaker {
    pub fn stop(&mut self) {
        if self.next_fire_date.is_some() {
            log::info!("stop");
            self.next_fire_date = None;
            unsafe { CFRunLoopTimerSetNextFireDate(self.timer, std::f64::MAX) }
        }
    }

    pub fn start(&mut self) {
        if self.next_fire_date != Some(self.start_instant) {
            log::info!("start");
            self.next_fire_date = Some(self.start_instant);
            unsafe { CFRunLoopTimerSetNextFireDate(self.timer, std::f64::MIN) }
        }
    }

    pub fn start_at(&mut self, instant: Option<Instant>) {
        let now = Instant::now();
        match instant {
            Some(instant) if now >= instant => {
                self.start();
            }
            Some(instant) => {
                if self.next_fire_date != Some(instant) {
                    self.next_fire_date = Some(instant);
                    unsafe {
                        let current = CFAbsoluteTimeGetCurrent();
                        let duration = instant - now;
                        let fsecs = duration.subsec_nanos() as f64 / 1_000_000_000.0
                            + duration.as_secs() as f64;
                        CFRunLoopTimerSetNextFireDate(self.timer, current + fsecs)
                    }
                }
            }
            None => {
                self.stop();
            }
        }
    }
}
