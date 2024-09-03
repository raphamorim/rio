#[cfg(any(x11_platform, macos_platform, windows_platform))]
#[allow(deprecated)]
fn main() -> Result<(), impl std::error::Error> {
    use std::collections::HashMap;

    use rio_window::dpi::{LogicalPosition, LogicalSize, Position};
    use rio_window::event::{ElementState, Event, KeyEvent, WindowEvent};
    use rio_window::event_loop::{ActiveEventLoop, EventLoop};
    use rio_window::raw_window_handle::HasRawWindowHandle;
    use rio_window::window::Window;

    #[path = "util/fill.rs"]
    mod fill;

    fn spawn_child_window(parent: &Window, event_loop: &ActiveEventLoop) -> Window {
        let parent = parent.raw_window_handle().unwrap();
        let mut window_attributes = Window::default_attributes()
            .with_title("child window")
            .with_inner_size(LogicalSize::new(200.0f32, 200.0f32))
            .with_position(Position::Logical(LogicalPosition::new(0.0, 0.0)))
            .with_visible(true);
        // `with_parent_window` is unsafe. Parent window must be a valid window.
        window_attributes = unsafe { window_attributes.with_parent_window(Some(parent)) };

        event_loop.create_window(window_attributes).unwrap()
    }

    let mut windows = HashMap::new();

    let event_loop: EventLoop<()> = EventLoop::new().unwrap();
    let mut parent_window_id = None;

    event_loop.run(move |event: Event<()>, event_loop| {
        match event {
            Event::Resumed => {
                let attributes = Window::default_attributes()
                    .with_title("parent window")
                    .with_position(Position::Logical(LogicalPosition::new(0.0, 0.0)))
                    .with_inner_size(LogicalSize::new(640.0f32, 480.0f32));
                let window = event_loop.create_window(attributes).unwrap();

                parent_window_id = Some(window.id());

                println!("Parent window id: {parent_window_id:?})");
                windows.insert(window.id(), window);
            }
            Event::WindowEvent { window_id, event } => match event {
                WindowEvent::CloseRequested => {
                    windows.clear();
                    event_loop.exit();
                }
                WindowEvent::CursorEntered { device_id: _ } => {
                    // On x11, println when the cursor entered in a window even if the child window
                    // is created by some key inputs.
                    // the child windows are always placed at (0, 0) with size (200, 200) in the
                    // parent window, so we also can see this log when we move
                    // the cursor around (200, 200) in parent window.
                    println!("cursor entered in the window {window_id:?}");
                }
                WindowEvent::KeyboardInput {
                    event:
                        KeyEvent {
                            state: ElementState::Pressed,
                            ..
                        },
                    ..
                } => {
                    let parent_window = windows.get(&parent_window_id.unwrap()).unwrap();
                    let child_window = spawn_child_window(parent_window, event_loop);
                    let child_id = child_window.id();
                    println!("Child window created with id: {child_id:?}");
                    windows.insert(child_id, child_window);
                }
                WindowEvent::RedrawRequested => {
                    if let Some(window) = windows.get(&window_id) {
                        fill::fill_window(window);
                    }
                }
                _ => (),
            },
            _ => (),
        }
    })
}

#[cfg(not(any(x11_platform, macos_platform, windows_platform)))]
fn main() {
    panic!(
        "This example is supported only on x11, macOS, and Windows, with the `rwh_06` feature \
         enabled."
    );
}
