#![allow(clippy::single_match)]

use std::thread;
#[cfg(not(web_platform))]
use std::time;

use ::tracing::{info, warn};
#[cfg(web_platform)]
use web_time as time;

use rio_window::application::ApplicationHandler;
use rio_window::event::{ElementState, KeyEvent, StartCause, WindowEvent};
use rio_window::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use rio_window::keyboard::{Key, NamedKey};
use rio_window::window::{Window, WindowId};

#[path = "util/fill.rs"]
mod fill;
#[path = "util/tracing.rs"]
mod tracing;

const WAIT_TIME: time::Duration = time::Duration::from_millis(100);
const POLL_SLEEP_TIME: time::Duration = time::Duration::from_millis(100);

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    #[default]
    Wait,
    WaitUntil,
    Poll,
}

fn main() -> Result<(), impl std::error::Error> {
    #[cfg(web_platform)]
    console_error_panic_hook::set_once();

    tracing::init();

    info!("Press '1' to switch to Wait mode.");
    info!("Press '2' to switch to WaitUntil mode.");
    info!("Press '3' to switch to Poll mode.");
    info!("Press 'R' to toggle request_redraw() calls.");
    info!("Press 'Esc' to close the window.");

    let event_loop = EventLoop::new().unwrap();

    let mut app = ControlFlowDemo::default();
    event_loop.run_app(&mut app)
}

#[derive(Default)]
struct ControlFlowDemo {
    mode: Mode,
    request_redraw: bool,
    wait_cancelled: bool,
    close_requested: bool,
    window: Option<Window>,
}

impl ApplicationHandler for ControlFlowDemo {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        info!("new_events: {cause:?}");

        self.wait_cancelled = match cause {
            StartCause::WaitCancelled { .. } => self.mode == Mode::WaitUntil,
            _ => false,
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attributes = Window::default_attributes().with_title(
            "Press 1, 2, 3 to change control flow mode. Press R to toggle redraw requests.",
        );
        self.window = Some(event_loop.create_window(window_attributes).unwrap());
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        info!("{event:?}");

        match event {
            WindowEvent::CloseRequested => {
                self.close_requested = true;
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: key,
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => match key.as_ref() {
                // WARNING: Consider using `key_without_modifiers()` if available on your platform.
                // See the `key_binding` example
                Key::Character("1") => {
                    self.mode = Mode::Wait;
                    warn!("mode: {:?}", self.mode);
                }
                Key::Character("2") => {
                    self.mode = Mode::WaitUntil;
                    warn!("mode: {:?}", self.mode);
                }
                Key::Character("3") => {
                    self.mode = Mode::Poll;
                    warn!("mode: {:?}", self.mode);
                }
                Key::Character("r") => {
                    self.request_redraw = !self.request_redraw;
                    warn!("request_redraw: {}", self.request_redraw);
                }
                Key::Named(NamedKey::Escape) => {
                    self.close_requested = true;
                }
                _ => (),
            },
            WindowEvent::RedrawRequested => {
                let window = self.window.as_ref().unwrap();
                window.pre_present_notify();
                fill::fill_window(window);
            }
            _ => (),
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.request_redraw && !self.wait_cancelled && !self.close_requested {
            self.window.as_ref().unwrap().request_redraw();
        }

        match self.mode {
            Mode::Wait => event_loop.set_control_flow(ControlFlow::Wait),
            Mode::WaitUntil => {
                if !self.wait_cancelled {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(
                        time::Instant::now() + WAIT_TIME,
                    ));
                }
            }
            Mode::Poll => {
                thread::sleep(POLL_SLEEP_TIME);
                event_loop.set_control_flow(ControlFlow::Poll);
            }
        };

        if self.close_requested {
            event_loop.exit();
        }
    }
}
