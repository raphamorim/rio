// Copyright (c) 2024-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// A lot of this file has been originally retired from https://github.com/rust-windowing/winit/blob/ab33fb8eda45f9a23587465d787a70a309c67ec4/src/event_loop.rs licensed under MIT
// https://github.com/rust-windowing/winit/blob/master/LICENSE

use crate::native::apple::frameworks::{
    kCFRunLoopCommonModes, CFIndex, CFRelease, CFRunLoopAddSource, CFRunLoopGetMain,
    CFRunLoopSourceContext, CFRunLoopSourceCreate, CFRunLoopSourceRef,
    CFRunLoopSourceSignal, CFRunLoopWakeUp,
};
use std::fmt;
use std::os::raw::c_void;
use std::ptr;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc;

static EVENT_LOOP_CREATED: AtomicBool = AtomicBool::new(false);

pub struct EventLoop<T: 'static> {
    // Event sender and receiver, used for EventLoopProxy.
    pub sender: mpsc::Sender<T>,
    pub receiver: Rc<mpsc::Receiver<T>>,
}

#[derive(Debug)]
pub enum EventLoopError {
    /// The event loop can't be re-created.
    RecreationAttempt,
    /// Application has exit with an error status.
    ExitFailure(i32),
}

impl<T> EventLoop<T> {
    /// Creates an [`EventLoopProxy`] that can be used to dispatch user events
    /// to the main event loop, possibly from another thread.
    pub fn create_proxy(&self) -> EventLoopProxy<T> {
        EventLoopProxy::new(self.sender.clone())
    }

    pub fn build() -> Result<EventLoop<T>, EventLoopError> {
        if EVENT_LOOP_CREATED.swap(true, Ordering::Relaxed) {
            return Err(EventLoopError::RecreationAttempt);
        }

        use std::sync::mpsc::channel;
        let (tx, rx) = channel();

        Ok(EventLoop {
            sender: tx,
            receiver: rx.into(),
        })
    }
}

impl<T: 'static> fmt::Debug for EventLoopProxy<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("EventLoopProxy { .. }")
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

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct EventLoopClosed<T>(pub T);

impl<T> fmt::Display for EventLoopClosed<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Tried to wake up a closed `EventLoop`")
    }
}

impl<T: fmt::Debug> std::error::Error for EventLoopClosed<T> {}
