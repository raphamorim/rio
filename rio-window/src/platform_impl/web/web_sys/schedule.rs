use js_sys::{Function, Object, Promise, Reflect};
use std::cell::OnceCell;
use std::time::Duration;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{AbortController, AbortSignal, MessageChannel, MessagePort};

use crate::platform::web::PollStrategy;

#[derive(Debug)]
pub struct Schedule {
    _closure: Closure<dyn FnMut()>,
    inner: Inner,
}

#[derive(Debug)]
enum Inner {
    Scheduler {
        controller: AbortController,
    },
    IdleCallback {
        window: web_sys::Window,
        handle: u32,
    },
    Timeout {
        window: web_sys::Window,
        handle: i32,
        port: MessagePort,
        _timeout_closure: Closure<dyn FnMut()>,
    },
}

impl Schedule {
    pub fn new<F>(strategy: PollStrategy, window: &web_sys::Window, f: F) -> Schedule
    where
        F: 'static + FnMut(),
    {
        if strategy == PollStrategy::Scheduler && has_scheduler_support(window) {
            Self::new_scheduler(window, f, None)
        } else if strategy == PollStrategy::IdleCallback
            && has_idle_callback_support(window)
        {
            Self::new_idle_callback(window.clone(), f)
        } else {
            Self::new_timeout(window.clone(), f, None)
        }
    }

    pub fn new_with_duration<F>(
        window: &web_sys::Window,
        f: F,
        duration: Duration,
    ) -> Schedule
    where
        F: 'static + FnMut(),
    {
        if has_scheduler_support(window) {
            Self::new_scheduler(window, f, Some(duration))
        } else {
            Self::new_timeout(window.clone(), f, Some(duration))
        }
    }

    fn new_scheduler<F>(
        window: &web_sys::Window,
        f: F,
        duration: Option<Duration>,
    ) -> Schedule
    where
        F: 'static + FnMut(),
    {
        let window: &WindowSupportExt = window.unchecked_ref();
        let scheduler = window.scheduler();

        let closure = Closure::new(f);
        let mut options = SchedulerPostTaskOptions::new();
        let controller =
            AbortController::new().expect("Failed to create `AbortController`");
        options.signal(&controller.signal());

        if let Some(duration) = duration {
            // `Duration::as_millis()` always rounds down (because of truncation), we want to round
            // up instead. This makes sure that the we never wake up **before** the given time.
            let duration = duration
                .as_secs()
                .checked_mul(1000)
                .and_then(|secs| secs.checked_add(duration_millis_ceil(duration).into()))
                .unwrap_or(u64::MAX);

            options.delay(duration as f64);
        }

        thread_local! {
            static REJECT_HANDLER: Closure<dyn FnMut(JsValue)> = Closure::new(|_| ());
        }
        REJECT_HANDLER.with(|handler| {
            let _ = scheduler
                .post_task_with_options(closure.as_ref().unchecked_ref(), &options)
                .catch(handler);
        });

        Schedule {
            _closure: closure,
            inner: Inner::Scheduler { controller },
        }
    }

    fn new_idle_callback<F>(window: web_sys::Window, f: F) -> Schedule
    where
        F: 'static + FnMut(),
    {
        let closure = Closure::new(f);
        let handle = window
            .request_idle_callback(closure.as_ref().unchecked_ref())
            .expect("Failed to request idle callback");

        Schedule {
            _closure: closure,
            inner: Inner::IdleCallback { window, handle },
        }
    }

    fn new_timeout<F>(
        window: web_sys::Window,
        f: F,
        duration: Option<Duration>,
    ) -> Schedule
    where
        F: 'static + FnMut(),
    {
        let channel = MessageChannel::new().unwrap();
        let closure = Closure::new(f);
        let port_1 = channel.port1();
        port_1.set_onmessage(Some(closure.as_ref().unchecked_ref()));
        port_1.start();

        let port_2 = channel.port2();
        let timeout_closure = Closure::new(move || {
            port_2
                .post_message(&JsValue::UNDEFINED)
                .expect("Failed to send message")
        });
        let handle = if let Some(duration) = duration {
            // `Duration::as_millis()` always rounds down (because of truncation), we want to round
            // up instead. This makes sure that the we never wake up **before** the given time.
            let duration = duration
                .as_secs()
                .try_into()
                .ok()
                .and_then(|secs: i32| secs.checked_mul(1000))
                .and_then(|secs: i32| {
                    let millis: i32 = duration_millis_ceil(duration)
                        .try_into()
                        .expect("millis are somehow bigger then 1K");
                    secs.checked_add(millis)
                })
                .unwrap_or(i32::MAX);

            window.set_timeout_with_callback_and_timeout_and_arguments_0(
                timeout_closure.as_ref().unchecked_ref(),
                duration,
            )
        } else {
            window.set_timeout_with_callback(timeout_closure.as_ref().unchecked_ref())
        }
        .expect("Failed to set timeout");

        Schedule {
            _closure: closure,
            inner: Inner::Timeout {
                window,
                handle,
                port: port_1,
                _timeout_closure: timeout_closure,
            },
        }
    }
}

impl Drop for Schedule {
    fn drop(&mut self) {
        match &self.inner {
            Inner::Scheduler { controller, .. } => controller.abort(),
            Inner::IdleCallback { window, handle, .. } => {
                window.cancel_idle_callback(*handle)
            }
            Inner::Timeout {
                window,
                handle,
                port,
                ..
            } => {
                window.clear_timeout_with_handle(*handle);
                port.close();
                port.set_onmessage(None);
            }
        }
    }
}

// TODO: Replace with `u32::div_ceil()` when we hit Rust v1.73.
fn duration_millis_ceil(duration: Duration) -> u32 {
    let micros = duration.subsec_micros();

    // From <https://doc.rust-lang.org/1.73.0/src/core/num/uint_macros.rs.html#2086-2094>.
    let d = micros / 1000;
    let r = micros % 1000;
    if r > 0 && 1000 > 0 {
        d + 1
    } else {
        d
    }
}

fn has_scheduler_support(window: &web_sys::Window) -> bool {
    thread_local! {
        static SCHEDULER_SUPPORT: OnceCell<bool> = const { OnceCell::new() };
    }

    SCHEDULER_SUPPORT.with(|support| {
        *support.get_or_init(|| {
            #[wasm_bindgen]
            extern "C" {
                type SchedulerSupport;

                #[wasm_bindgen(method, getter, js_name = scheduler)]
                fn has_scheduler(this: &SchedulerSupport) -> JsValue;
            }

            let support: &SchedulerSupport = window.unchecked_ref();

            !support.has_scheduler().is_undefined()
        })
    })
}

fn has_idle_callback_support(window: &web_sys::Window) -> bool {
    thread_local! {
        static IDLE_CALLBACK_SUPPORT: OnceCell<bool> = const { OnceCell::new() };
    }

    IDLE_CALLBACK_SUPPORT.with(|support| {
        *support.get_or_init(|| {
            #[wasm_bindgen]
            extern "C" {
                type IdleCallbackSupport;

                #[wasm_bindgen(method, getter, js_name = requestIdleCallback)]
                fn has_request_idle_callback(this: &IdleCallbackSupport) -> JsValue;
            }

            let support: &IdleCallbackSupport = window.unchecked_ref();
            !support.has_request_idle_callback().is_undefined()
        })
    })
}

#[wasm_bindgen]
extern "C" {
    type WindowSupportExt;

    #[wasm_bindgen(method, getter)]
    fn scheduler(this: &WindowSupportExt) -> Scheduler;

    type Scheduler;

    #[wasm_bindgen(method, js_name = postTask)]
    fn post_task_with_options(
        this: &Scheduler,
        callback: &Function,
        options: &SchedulerPostTaskOptions,
    ) -> Promise;

    type SchedulerPostTaskOptions;
}

impl SchedulerPostTaskOptions {
    fn new() -> Self {
        Object::new().unchecked_into()
    }

    fn delay(&mut self, val: f64) -> &mut Self {
        let r = Reflect::set(self, &JsValue::from("delay"), &val.into());
        debug_assert!(r.is_ok(), "Failed to set `delay` property");
        self
    }

    fn signal(&mut self, val: &AbortSignal) -> &mut Self {
        let r = Reflect::set(self, &JsValue::from("signal"), &val.into());
        debug_assert!(r.is_ok(), "Failed to set `signal` property");
        self
    }
}
