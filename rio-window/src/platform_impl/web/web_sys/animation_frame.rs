use std::cell::Cell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

pub struct AnimationFrameHandler {
    window: web_sys::Window,
    closure: Closure<dyn FnMut()>,
    handle: Rc<Cell<Option<i32>>>,
}

impl AnimationFrameHandler {
    pub fn new(window: web_sys::Window) -> Self {
        let handle = Rc::new(Cell::new(None));
        let closure = Closure::new({
            let handle = handle.clone();
            move || handle.set(None)
        });

        Self {
            window,
            closure,
            handle,
        }
    }

    pub fn on_animation_frame<F>(&mut self, mut f: F)
    where
        F: 'static + FnMut(),
    {
        let handle = self.handle.clone();
        self.closure = Closure::new(move || {
            handle.set(None);
            f();
        })
    }

    pub fn request(&self) {
        if let Some(handle) = self.handle.take() {
            self.window
                .cancel_animation_frame(handle)
                .expect("Failed to cancel animation frame");
        }

        let handle = self
            .window
            .request_animation_frame(self.closure.as_ref().unchecked_ref())
            .expect("Failed to request animation frame");

        self.handle.set(Some(handle));
    }

    pub fn cancel(&mut self) {
        if let Some(handle) = self.handle.take() {
            self.window
                .cancel_animation_frame(handle)
                .expect("Failed to cancel animation frame");
        }
    }
}

impl Drop for AnimationFrameHandler {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            self.window
                .cancel_animation_frame(handle)
                .expect("Failed to cancel animation frame");
        }
    }
}
