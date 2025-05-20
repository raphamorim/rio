use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use rio_window::{
    application::ApplicationHandler, event_loop::EventLoop, window::Window,
};
use sugarloaf::{
    font::FontLibrary, layout::RootStyle, Sugarloaf, SugarloafRenderer, SugarloafWindow,
    SugarloafWindowSize,
};

struct State;

impl ApplicationHandler for State {
    fn window_event(
        &mut self,
        _event_loop: &rio_window::event_loop::ActiveEventLoop,
        _window_id: rio_window::window::WindowId,
        _event: rio_window::event::WindowEvent,
    ) {
    }
    fn resumed(&mut self, event_loop: &rio_window::event_loop::ActiveEventLoop) {
        let window = event_loop
            .create_window(Window::default_attributes())
            .unwrap();
        let mut sugarloaf = Sugarloaf::new(
            SugarloafWindow {
                handle: window.window_handle().unwrap().as_raw(),
                display: window.display_handle().unwrap().as_raw(),
                size: SugarloafWindowSize {
                    width: 1.,
                    height: 1.,
                },
                scale: window.scale_factor() as _,
            },
            SugarloafRenderer::default(),
            &FontLibrary::default(),
            RootStyle::default(),
        )
        .unwrap();
        sugarloaf.update_filters(&["newpixiecrt".to_string()]);
        println!("{:?}", sugarloaf.get_filter_parameters());
        event_loop.exit();
    }
}

fn main() {
    let event_loop = EventLoop::with_user_event().build().unwrap();
    event_loop.run_app(&mut State).unwrap();
}
