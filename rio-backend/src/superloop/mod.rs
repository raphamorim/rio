use crate::event::sync::FairMutex;
use crate::event::RioEvent;
use std::collections::VecDeque;
use std::sync::Arc;

pub struct InnerData {
    list: VecDeque<RioEvent>,
    redraw: Vec<u8>,
    priority_list: Vec<RioEvent>,
}

pub struct Inner(InnerData);

impl Inner {
    /// Create a new, empty event listener list.
    pub fn new() -> Self {
        Self(InnerData {
            list: VecDeque::new(),
            redraw: Vec::new(),
            priority_list: Vec::new(),
        })
    }
}

pub struct Instance {
    pub inner: Inner,
}

impl Instance {
    pub fn new() -> Instance {
        Instance {
            inner: Inner::new(),
        }
    }
}

#[derive(Clone)]
pub struct Superloop(Arc<FairMutex<Instance>>);

impl Superloop {
    pub fn new() -> Superloop {
        Superloop(Arc::new(FairMutex::new(Instance {
            inner: Inner::new(),
        })))
    }

    #[inline]
    pub fn event(&mut self) -> (RioEvent, bool) {
        let inner = &mut self.0.lock().inner.0;

        let redraw = if !inner.redraw.is_empty() {
            inner.redraw.pop();
            true
        } else {
            false
        };

        if !inner.priority_list.is_empty() {
            return (inner.priority_list.pop().unwrap_or(RioEvent::Noop), redraw);
        }

        let current = inner.list.pop_front().unwrap_or(RioEvent::Noop);

        // println!("{:?}", current);
        (current, redraw)
    }

    #[inline]
    pub fn send_event(&mut self, event: RioEvent, _id: u16) {
        self.0.lock().inner.0.list.push_back(event);
    }

    #[inline]
    pub fn send_event_with_high_priority(&mut self, event: RioEvent, _id: u16) {
        self.0.lock().inner.0.priority_list.push(event);
    }

    #[inline]
    pub fn send_redraw(&mut self, _id: u16) {
        self.0.lock().inner.0.redraw.push(0);
    }
}

impl std::fmt::Debug for Superloop {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Instance")
    }
}

impl core::panic::UnwindSafe for Superloop {}
impl core::panic::RefUnwindSafe for Superloop {}
