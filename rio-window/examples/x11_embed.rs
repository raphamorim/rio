//! A demonstration of embedding a winit window in an existing X11 application.
use std::error::Error;

#[cfg(x11_platform)]
fn main() -> Result<(), Box<dyn Error>> {
    use rio_window::application::ApplicationHandler;
    use rio_window::event::WindowEvent;
    use rio_window::event_loop::{ActiveEventLoop, EventLoop};
    use rio_window::platform::x11::WindowAttributesExtX11;
    use rio_window::window::{Window, WindowId};

    #[path = "util/fill.rs"]
    mod fill;

    pub struct XEmbedDemo {
        parent_window_id: u32,
        window: Option<Window>,
    }

    impl ApplicationHandler for XEmbedDemo {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            let window_attributes = Window::default_attributes()
                .with_title("An embedded window!")
                .with_inner_size(rio_window::dpi::LogicalSize::new(128.0, 128.0))
                .with_embed_parent_window(self.parent_window_id);

            self.window = Some(event_loop.create_window(window_attributes).unwrap());
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            _window_id: WindowId,
            event: WindowEvent,
        ) {
            let window = self.window.as_ref().unwrap();
            match event {
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::RedrawRequested => {
                    window.pre_present_notify();
                    fill::fill_window(window);
                }
                _ => (),
            }
        }

        fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
            self.window.as_ref().unwrap().request_redraw();
        }
    }

    // First argument should be a 32-bit X11 window ID.
    let parent_window_id = std::env::args()
        .nth(1)
        .ok_or("Expected a 32-bit X11 window ID as the first argument.")?
        .parse::<u32>()?;

    tracing_subscriber::fmt::init();
    let event_loop = EventLoop::new()?;

    let mut app = XEmbedDemo {
        parent_window_id,
        window: None,
    };
    event_loop.run_app(&mut app).map_err(Into::into)
}

#[cfg(not(x11_platform))]
fn main() -> Result<(), Box<dyn Error>> {
    println!("This example is only supported on X11 platforms.");
    Ok(())
}
