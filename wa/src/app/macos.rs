use crate::native::apple::frameworks::*;
use std::cell::{RefMut, RefCell};
use crate::EventHandler;

pub struct Handler {
    pub state: Option<HandlerState>,
    waker: EventLoopWaker,
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
    Terminated
}

impl Handler {
    pub fn get_mut() -> RefMut<'static, Handler> {
        // basically everything in UIKit requires the main thread, so it's pointless to use the
        // std::sync APIs.
        // must be mut because plain `static` requires `Sync`
        static mut APP_STATE: RefCell<Option<Handler>> = RefCell::new(None);

        let mut guard = unsafe { APP_STATE.borrow_mut() };
        if guard.is_none() {
            #[inline(never)]
            #[cold]
            fn init_guard(guard: &mut RefMut<'static, Option<Handler>>) {
                let waker = EventLoopWaker::new(unsafe { CFRunLoopGetMain() });
                **guard = Some(Handler {
                    state: Some(HandlerState::NotLaunched),
                    waker,
                });
            }
            init_guard(&mut guard);
        }
        RefMut::map(guard, |state| state.as_mut().unwrap())
    }

    fn state(&self) -> &HandlerState {
        match &self.state {
            Some(ref st) => st,
            None => panic!("`HandlerState` previously failed a state transition"),
        }
    }

    pub fn state_mut(&mut self) -> &mut HandlerState {
        match &mut self.state {
            Some(ref mut st) => st,
            None => panic!("`HandlerState` previously failed a state transition"),
        }
    }

    pub fn set_state(&mut self, new_state: HandlerState) {
        self.state = Some(new_state)
    }
}

pub fn create_window() {
    let mut this = Handler::get_mut();
    match this.state_mut() {
        &mut HandlerState::Running { ref mut handler , .. } => {
            handler.create_window();
            return;
        },
        &mut HandlerState::Waiting { ref mut handler , .. } => {
            handler.create_window();
            return;
        },
        _ => {},
    }
    drop(this);
}

pub fn create_tab(tab_payload: Option<&str>) {
    let mut this = Handler::get_mut();
    match this.state_mut() {
        &mut HandlerState::Running { ref mut handler , .. } => {
            handler.create_tab(tab_payload);
            return;
        },
        &mut HandlerState::Waiting { ref mut handler , .. } => {
            handler.create_tab(tab_payload);
            return;
        },
        _ => {},
    }
    drop(this);
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
                std::ptr::null_mut(),
                std::f64::MAX,
                0.000_000_1,
                0,
                0,
                wakeup_main_loop,
                std::ptr::null_mut(),
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

    // fn start_at(&mut self, instant: std::time::Instant) {
    //     let now = std::time::Instant::now();
    //     if now >= instant {
    //         self.start();
    //     } else {
    //         unsafe {
    //             let current = CFAbsoluteTimeGetCurrent();
    //             let duration = instant - now;
    //             let fsecs =
    //                 duration.subsec_nanos() as f64 / 1_000_000_000.0 + duration.as_secs() as f64;
    //             CFRunLoopTimerSetNextFireDate(self.timer, current + fsecs)
    //         }
    //     }
    // }
}