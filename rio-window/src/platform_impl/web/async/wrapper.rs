use super::super::main_thread::MainThreadMarker;
use std::cell::{Ref, RefCell};
use std::future::Future;
use std::marker::PhantomData;
use std::sync::Arc;

// Unsafe wrapper type that allows us to use `T` when it's not `Send` from other threads.
// `value` **must** only be accessed on the main thread.
pub struct Wrapper<V: 'static, S: Clone + Send, E> {
    value: Value<V>,
    handler: fn(&RefCell<Option<V>>, E),
    sender_data: S,
    sender_handler: fn(&S, E),
}

struct Value<V> {
    // SAFETY:
    // This value must not be accessed if not on the main thread.
    //
    // - We wrap this in an `Arc` to allow it to be safely cloned without accessing the value.
    // - The `RefCell` lets us mutably access in the main thread but is safe to drop in any thread
    //   because it has no `Drop` behavior.
    // - The `Option` lets us safely drop `T` only in the main thread.
    value: Arc<RefCell<Option<V>>>,
    // Prevent's `Send` or `Sync` to be automatically implemented.
    local: PhantomData<*const ()>,
}

// SAFETY: See `Self::value`.
unsafe impl<V> Send for Value<V> {}
// SAFETY: See `Self::value`.
unsafe impl<V> Sync for Value<V> {}

impl<V, S: Clone + Send, E> Wrapper<V, S, E> {
    #[track_caller]
    pub fn new<R: Future<Output = ()>>(
        _: MainThreadMarker,
        value: V,
        handler: fn(&RefCell<Option<V>>, E),
        receiver: impl 'static + FnOnce(Arc<RefCell<Option<V>>>) -> R,
        sender_data: S,
        sender_handler: fn(&S, E),
    ) -> Option<Self> {
        let value = Arc::new(RefCell::new(Some(value)));

        wasm_bindgen_futures::spawn_local({
            let value = Arc::clone(&value);
            async move {
                receiver(Arc::clone(&value)).await;
                drop(value.borrow_mut().take().unwrap());
            }
        });

        Some(Self {
            value: Value {
                value,
                local: PhantomData,
            },
            handler,
            sender_data,
            sender_handler,
        })
    }

    pub fn send(&self, event: E) {
        if MainThreadMarker::new().is_some() {
            (self.handler)(&self.value.value, event)
        } else {
            (self.sender_handler)(&self.sender_data, event)
        }
    }

    pub fn value(&self) -> Option<Ref<'_, V>> {
        MainThreadMarker::new()
            .map(|_| Ref::map(self.value.value.borrow(), |value| value.as_ref().unwrap()))
    }

    pub fn with_sender_data<T>(&self, f: impl FnOnce(&S) -> T) -> T {
        f(&self.sender_data)
    }
}

impl<V, S: Clone + Send, E> Clone for Wrapper<V, S, E> {
    fn clone(&self) -> Self {
        Self {
            value: Value {
                value: self.value.value.clone(),
                local: PhantomData,
            },
            handler: self.handler,
            sender_data: self.sender_data.clone(),
            sender_handler: self.sender_handler,
        }
    }
}
