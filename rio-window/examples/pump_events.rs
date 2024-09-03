#![allow(clippy::single_match)]

// Limit this example to only compatible platforms.
#[cfg(any(
    windows_platform,
    macos_platform,
    x11_platform,
    wayland_platform,
    android_platform,
))]
fn main() -> std::process::ExitCode {
    use std::process::ExitCode;
    use std::thread::sleep;
    use std::time::Duration;

    use rio_window::application::ApplicationHandler;
    use rio_window::event::WindowEvent;
    use rio_window::event_loop::{ActiveEventLoop, EventLoop};
    use rio_window::platform::pump_events::{EventLoopExtPumpEvents, PumpStatus};
    use rio_window::window::{Window, WindowId};

    #[path = "util/fill.rs"]
    mod fill;

    #[derive(Default)]
    struct PumpDemo {
        window: Option<Window>,
    }

    impl ApplicationHandler for PumpDemo {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            let window_attributes =
                Window::default_attributes().with_title("A fantastic window!");
            self.window = Some(event_loop.create_window(window_attributes).unwrap());
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            _window_id: WindowId,
            event: WindowEvent,
        ) {
            println!("{event:?}");

            let window = match self.window.as_ref() {
                Some(window) => window,
                None => return,
            };

            match event {
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::RedrawRequested => {
                    fill::fill_window(window);
                    window.request_redraw();
                }
                _ => (),
            }
        }
    }

    let mut event_loop = EventLoop::new().unwrap();

    tracing_subscriber::fmt::init();

    let mut app = PumpDemo::default();

    loop {
        let timeout = Some(Duration::ZERO);
        let status = event_loop.pump_app_events(timeout, &mut app);

        if let PumpStatus::Exit(exit_code) = status {
            break ExitCode::from(exit_code as u8);
        }

        // Sleep for 1/60 second to simulate application work
        //
        // Since `pump_events` doesn't block it will be important to
        // throttle the loop in the app somehow.
        println!("Update()");
        sleep(Duration::from_millis(16));
    }
}

#[cfg(any(ios_platform, web_platform, orbital_platform))]
fn main() {
    println!("This platform doesn't support pump_events.");
}
