use js_sys::Array;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen::JsCast;
use web_sys::{Element, IntersectionObserver, IntersectionObserverEntry};

pub(super) struct IntersectionObserverHandle {
    observer: IntersectionObserver,
    _closure: Closure<dyn FnMut(Array)>,
}

impl IntersectionObserverHandle {
    pub fn new<F>(element: &Element, mut callback: F) -> Self
    where
        F: 'static + FnMut(bool),
    {
        let closure = Closure::new(move |entries: Array| {
            let entry: IntersectionObserverEntry = entries.get(0).unchecked_into();
            callback(entry.is_intersecting());
        });
        let observer = IntersectionObserver::new(closure.as_ref().unchecked_ref())
            // we don't provide any `options`
            .expect("Invalid `options`");
        observer.observe(element);

        Self {
            observer,
            _closure: closure,
        }
    }
}

impl Drop for IntersectionObserverHandle {
    fn drop(&mut self) {
        self.observer.disconnect()
    }
}
