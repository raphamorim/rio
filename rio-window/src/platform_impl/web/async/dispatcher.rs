use super::super::main_thread::MainThreadMarker;
use super::{channel, Receiver, Sender, Wrapper};
use std::cell::Ref;
use std::sync::{Arc, Condvar, Mutex};

pub struct Dispatcher<T: 'static>(Wrapper<T, Sender<Closure<T>>, Closure<T>>);

struct Closure<T>(Box<dyn FnOnce(&T) + Send>);

impl<T> Dispatcher<T> {
    #[track_caller]
    pub fn new(
        main_thread: MainThreadMarker,
        value: T,
    ) -> Option<(Self, DispatchRunner<T>)> {
        let (sender, receiver) = channel::<Closure<T>>();

        Wrapper::new(
            main_thread,
            value,
            |value, Closure(closure)| {
                // SAFETY: The given `Closure` here isn't really `'static`, so we shouldn't do
                // anything funny with it here. See `Self::queue()`.
                closure(value.borrow().as_ref().unwrap())
            },
            {
                let receiver = receiver.clone();
                move |value| async move {
                    while let Ok(Closure(closure)) = receiver.next().await {
                        // SAFETY: The given `Closure` here isn't really `'static`, so we shouldn't
                        // do anything funny with it here. See
                        // `Self::queue()`.
                        closure(value.borrow().as_ref().unwrap())
                    }
                }
            },
            sender,
            |sender, closure| {
                // SAFETY: The given `Closure` here isn't really `'static`, so we shouldn't do
                // anything funny with it here. See `Self::queue()`.
                sender.send(closure).unwrap()
            },
        )
        .map(|wrapper| (Self(wrapper.clone()), DispatchRunner { wrapper, receiver }))
    }

    pub fn value(&self) -> Option<Ref<'_, T>> {
        self.0.value()
    }

    pub fn dispatch(&self, f: impl 'static + FnOnce(&T) + Send) {
        if let Some(value) = self.0.value() {
            f(&value)
        } else {
            self.0.send(Closure(Box::new(f)))
        }
    }

    pub fn queue<R: Send>(&self, f: impl FnOnce(&T) -> R + Send) -> R {
        if let Some(value) = self.0.value() {
            f(&value)
        } else {
            let pair = Arc::new((Mutex::new(None), Condvar::new()));
            let closure = Box::new({
                let pair = pair.clone();
                move |value: &T| {
                    *pair.0.lock().unwrap() = Some(f(value));
                    pair.1.notify_one();
                }
            }) as Box<dyn FnOnce(&T) + Send>;
            // SAFETY: The `transmute` is necessary because `Closure` requires `'static`. This is
            // safe because this function won't return until `f` has finished executing. See
            // `Self::new()`.
            let closure = Closure(unsafe { std::mem::transmute(closure) });

            self.0.send(closure);

            let mut started = pair.0.lock().unwrap();

            while started.is_none() {
                started = pair.1.wait(started).unwrap();
            }

            started.take().unwrap()
        }
    }
}

pub struct DispatchRunner<T: 'static> {
    wrapper: Wrapper<T, Sender<Closure<T>>, Closure<T>>,
    receiver: Receiver<Closure<T>>,
}

impl<T> DispatchRunner<T> {
    pub fn run(&self) {
        while let Some(Closure(closure)) = self
            .receiver
            .try_recv()
            .expect("should only be closed when `Dispatcher` is dropped")
        {
            // SAFETY: The given `Closure` here isn't really `'static`, so we shouldn't do anything
            // funny with it here. See `Self::queue()`.
            closure(
                &self
                    .wrapper
                    .value()
                    .expect("don't call this outside the main thread"),
            )
        }
    }
}
