#![allow(clippy::single_match)]

// Limit this example to only compatible platforms.
#[cfg(any(windows_platform, macos_platform, x11_platform, wayland_platform,))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::Duration;

    use rio_window::application::ApplicationHandler;
    use rio_window::event::WindowEvent;
    use rio_window::event_loop::{ActiveEventLoop, EventLoop};
    use rio_window::platform::run_on_demand::EventLoopExtRunOnDemand;
    use rio_window::window::{Window, WindowId};

    #[path = "util/fill.rs"]
    mod fill;

    #[derive(Default)]
    struct App {
        idx: usize,
        window_id: Option<WindowId>,
        window: Option<Window>,
    }

    impl ApplicationHandler for App {
        fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }

        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            let window_attributes = Window::default_attributes()
                .with_title("Fantastic window number one!")
                .with_inner_size(rio_window::dpi::LogicalSize::new(128.0, 128.0));
            let window = event_loop.create_window(window_attributes).unwrap();
            self.window_id = Some(window.id());
            self.window = Some(window);
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            window_id: WindowId,
            event: WindowEvent,
        ) {
            if event == WindowEvent::Destroyed && self.window_id == Some(window_id) {
                println!(
                    "--------------------------------------------------------- Window {} Destroyed",
                    self.idx
                );
                self.window_id = None;
                event_loop.exit();
                return;
            }

            let window = match self.window.as_mut() {
                Some(window) => window,
                None => return,
            };

            match event {
                WindowEvent::CloseRequested => {
                    println!(
                        "--------------------------------------------------------- Window {} \
                         CloseRequested",
                        self.idx
                    );
                    fill::cleanup_window(window);
                    self.window = None;
                }
                WindowEvent::RedrawRequested => {
                    fill::fill_window(window);
                }
                _ => (),
            }
        }
    }

    tracing_subscriber::fmt::init();

    let mut event_loop = EventLoop::new().unwrap();

    let mut app = App {
        idx: 1,
        ..Default::default()
    };
    event_loop.run_app_on_demand(&mut app)?;

    println!(
        "--------------------------------------------------------- Finished first loop"
    );
    println!(
        "--------------------------------------------------------- Waiting 5 seconds"
    );
    std::thread::sleep(Duration::from_secs(5));

    app.idx += 1;
    event_loop.run_app_on_demand(&mut app)?;
    println!(
        "--------------------------------------------------------- Finished second loop"
    );
    Ok(())
}

#[cfg(not(any(windows_platform, macos_platform, x11_platform, wayland_platform,)))]
fn main() {
    println!("This example is not supported on this platform");
}
