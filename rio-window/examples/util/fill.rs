//! Fill the window buffer with a solid color.
//!
//! Launching a window without drawing to it has unpredictable results varying from platform to
//! platform. In order to have well-defined examples, this module provides an easy way to
//! fill the window buffer with a solid color.
//!
//! The `softbuffer` crate is used, largely because of its ease of use. `glutin` or `wgpu` could
//! also be used to fill the window buffer, but they are more complicated to use.

#[allow(unused_imports)]
pub use platform::cleanup_window;
pub use platform::fill_window;

#[cfg(not(target_os = "ios"))]
mod platform {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::mem;
    use std::mem::ManuallyDrop;
    use std::num::NonZeroU32;

    use rio_window::window::{Window, WindowId};
    use softbuffer::{Context, Surface};

    thread_local! {
        // NOTE: You should never do things like that, create context and drop it before
        // you drop the event loop. We do this for brevity to not blow up examples. We use
        // ManuallyDrop to prevent destructors from running.
        //
        // A static, thread-local map of graphics contexts to open windows.
        static GC: ManuallyDrop<RefCell<Option<GraphicsContext>>> = const { ManuallyDrop::new(RefCell::new(None)) };
    }

    /// The graphics context used to draw to a window.
    struct GraphicsContext {
        /// The global softbuffer context.
        context: RefCell<Context<&'static Window>>,

        /// The hash map of window IDs to surfaces.
        surfaces: HashMap<WindowId, Surface<&'static Window, &'static Window>>,
    }

    impl GraphicsContext {
        fn new(w: &Window) -> Self {
            Self {
                context: RefCell::new(
                    Context::new(unsafe {
                        mem::transmute::<&'_ Window, &'static Window>(w)
                    })
                    .expect("Failed to create a softbuffer context"),
                ),
                surfaces: HashMap::new(),
            }
        }

        fn create_surface(
            &mut self,
            window: &Window,
        ) -> &mut Surface<&'static Window, &'static Window> {
            self.surfaces.entry(window.id()).or_insert_with(|| {
                Surface::new(&self.context.borrow(), unsafe {
                    mem::transmute::<&'_ Window, &'static Window>(window)
                })
                .expect("Failed to create a softbuffer surface")
            })
        }

        fn destroy_surface(&mut self, window: &Window) {
            self.surfaces.remove(&window.id());
        }
    }

    pub fn fill_window(window: &Window) {
        GC.with(|gc| {
            let size = window.inner_size();
            let (Some(width), Some(height)) =
                (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
            else {
                return;
            };

            // Either get the last context used or create a new one.
            let mut gc = gc.borrow_mut();
            let surface = gc
                .get_or_insert_with(|| GraphicsContext::new(window))
                .create_surface(window);

            // Fill a buffer with a solid color.
            const DARK_GRAY: u32 = 0xff181818;

            surface
                .resize(width, height)
                .expect("Failed to resize the softbuffer surface");

            let mut buffer = surface
                .buffer_mut()
                .expect("Failed to get the softbuffer buffer");
            buffer.fill(DARK_GRAY);
            buffer
                .present()
                .expect("Failed to present the softbuffer buffer");
        })
    }

    #[allow(dead_code)]
    pub fn cleanup_window(window: &Window) {
        GC.with(|gc| {
            let mut gc = gc.borrow_mut();
            if let Some(context) = gc.as_mut() {
                context.destroy_surface(window);
            }
        });
    }
}
