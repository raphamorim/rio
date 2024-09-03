#[allow(dead_code)]
fn needs_send<T: Send>() {}

#[test]
fn event_loop_proxy_send() {
    #[allow(dead_code)]
    fn is_send<T: 'static + Send>() {
        // ensures that `rio_window::EventLoopProxy<T: Send>` implements `Send`
        needs_send::<rio_window::event_loop::EventLoopProxy<T>>();
    }
}

#[test]
fn window_send() {
    // ensures that `rio_window::Window` implements `Send`
    needs_send::<rio_window::window::Window>();
}

#[test]
fn window_builder_send() {
    needs_send::<rio_window::window::WindowAttributes>();
}

#[test]
fn ids_send() {
    // ensures that the various `..Id` types implement `Send`
    needs_send::<rio_window::window::WindowId>();
    needs_send::<rio_window::event::DeviceId>();
    needs_send::<rio_window::monitor::MonitorHandle>();
}

#[test]
fn custom_cursor_send() {
    needs_send::<rio_window::window::CustomCursorSource>();
    needs_send::<rio_window::window::CustomCursor>();
}
