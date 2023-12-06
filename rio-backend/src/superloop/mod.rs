use crate::event::sync::FairMutex;
use crate::event::RioEvent;
use std::collections::LinkedList;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

pub struct InnerData {
    list: LinkedList<RioEvent>,
    _len: AtomicUsize,
}

pub struct Inner(InnerData);

impl Inner {
    /// Create a new, empty event listener list.
    pub fn new() -> Self {
        Self(InnerData {
            list: LinkedList::new(),
            _len: 0.into(),
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
    pub fn event(&mut self) -> RioEvent {
        self.0
            .lock()
            .inner
            .0
            .list
            .pop_front()
            .or(Some(RioEvent::Noop))
            .unwrap()
    }

    pub fn send_event(&mut self, event: RioEvent, _id: u8) {
        self.0.lock().inner.0.list.push_back(event);
    }
}

impl std::fmt::Debug for Superloop {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Instance")
    }
}

impl core::panic::UnwindSafe for Superloop {}
impl core::panic::RefUnwindSafe for Superloop {}
