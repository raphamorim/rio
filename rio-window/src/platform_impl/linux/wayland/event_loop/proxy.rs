//! An event loop proxy.

use std::sync::mpsc::SendError;

use sctk::reexports::calloop::channel::Sender;

use crate::event_loop::EventLoopClosed;

/// A handle that can be sent across the threads and used to wake up the `EventLoop`.
pub struct EventLoopProxy<T: 'static> {
    user_events_sender: Sender<T>,
}

impl<T: 'static> Clone for EventLoopProxy<T> {
    fn clone(&self) -> Self {
        EventLoopProxy {
            user_events_sender: self.user_events_sender.clone(),
        }
    }
}

impl<T: 'static> EventLoopProxy<T> {
    pub fn new(user_events_sender: Sender<T>) -> Self {
        Self { user_events_sender }
    }

    pub fn send_event(&self, event: T) -> Result<(), EventLoopClosed<T>> {
        self.user_events_sender
            .send(event)
            .map_err(|SendError(error)| EventLoopClosed(error))
    }
}
