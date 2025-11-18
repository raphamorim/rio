#![allow(clippy::uninlined_format_args)]

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use rio_window::application::ApplicationHandler;
use rio_window::event_loop::ControlFlow;
use rio_window::event_loop::{ActiveEventLoop, DeviceEvents};
use rio_window::window::{Window, WindowId};
use rio_window::{
    dpi::LogicalSize, event::WindowEvent, event_loop::EventLoop, window::WindowAttributes,
};
use std::error::Error;
use sugarloaf::{
    layout::RootStyle, SpanStyle, Sugarloaf, SugarloafWindow, SugarloafWindowSize,
};

fn main() {
    let width = 800.0;
    let height = 400.0;
    let window_event_loop = rio_window::event_loop::EventLoop::new().unwrap();
    let mut application = Application::new(&window_event_loop, width, height);
    let _ = application.run(window_event_loop);
}

// User-defined content IDs
const TEXT_ID_0: usize = 0;
const TEXT_ID_1: usize = 1;
const TEXT_ID_2: usize = 2;
const RECT_ID_0: usize = 10;
const RECT_ID_1: usize = 11;
const RECT_ID_2: usize = 12;

struct Application {
    sugarloaf: Option<Sugarloaf<'static>>,
    window: Option<Window>,
    height: f32,
    width: f32,
}

impl Application {
    fn new(event_loop: &EventLoop<()>, width: f32, height: f32) -> Self {
        event_loop.listen_device_events(DeviceEvents::Never);

        Application {
            sugarloaf: None,
            window: None,
            width,
            height,
        }
    }

    fn run(&mut self, event_loop: EventLoop<()>) -> Result<(), Box<dyn Error>> {
        let result = event_loop.run_app(self);
        result.map_err(Into::into)
    }
}

impl ApplicationHandler for Application {
    fn resumed(&mut self, active_event_loop: &ActiveEventLoop) {
        let window_attribute = WindowAttributes::default()
            .with_title("Multi text example")
            .with_inner_size(LogicalSize::new(self.width, self.height))
            .with_resizable(true);
        let window = active_event_loop.create_window(window_attribute).unwrap();

        let scale_factor = window.scale_factor();
        let font_size = 20.;
        let line_height = 1.0;

        let sugarloaf_layout =
            RootStyle::new(scale_factor as f32, font_size, line_height);

        let size = window.inner_size();
        let sugarloaf_window = SugarloafWindow {
            handle: window.window_handle().unwrap().into(),
            display: window.display_handle().unwrap().into(),
            scale: scale_factor as f32,
            size: SugarloafWindowSize {
                width: size.width as f32,
                height: size.height as f32,
            },
        };

        let mut sugarloaf = Sugarloaf::new(
            sugarloaf_window,
            sugarloaf::SugarloafRenderer::default(),
            &sugarloaf::font::FontLibrary::default(),
            sugarloaf_layout,
        )
        .expect("Sugarloaf instance should be created");

        // Initialize text areas with different font sizes
        sugarloaf.text(Some(TEXT_ID_0)); // Default font size
        sugarloaf.text(Some(TEXT_ID_1)); // Will set font size below
        sugarloaf.text(Some(TEXT_ID_2)); // Will set font size below

        sugarloaf.set_text_font_size(&TEXT_ID_1, 24.0);
        sugarloaf.set_text_font_size(&TEXT_ID_2, 12.0);

        sugarloaf.set_background_color(None);
        window.request_redraw();

        self.sugarloaf = Some(sugarloaf);
        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.sugarloaf.is_none() || self.window.is_none() {
            return;
        }

        let sugarloaf = self.sugarloaf.as_mut().unwrap();
        let window = self.window.as_mut().unwrap();

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let scale_factor_f32 = scale_factor as f32;
                let new_inner_size = window.inner_size();
                sugarloaf.rescale(scale_factor_f32);
                sugarloaf.resize(new_inner_size.width, new_inner_size.height);
                window.request_redraw();
            }
            WindowEvent::Resized(new_size) => {
                sugarloaf.resize(new_size.width, new_size.height);
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let time = std::time::Instant::now();

                // First text area
                sugarloaf
                    .text(TEXT_ID_0)
                    .clear()
                    .new_line()
                    .add_span(
                        &format!("Text area {:?}", TEXT_ID_0),
                        SpanStyle {
                            color: [1.0, 1.0, 1.0, 1.0],
                            background_color: Some([0.0, 0.0, 0.0, 1.0]),
                            ..SpanStyle::default()
                        },
                    )
                    .new_line()
                    .add_span(
                        &format!("{:?}", time.elapsed()),
                        SpanStyle {
                            color: [1.0, 1.0, 1.0, 1.0],
                            ..SpanStyle::default()
                        },
                    );
                sugarloaf.build_text_by_id(TEXT_ID_0);

                // Second text area
                sugarloaf
                    .text(TEXT_ID_1)
                    .clear()
                    .new_line()
                    .add_span(
                        &format!("Text area {:?}", TEXT_ID_1),
                        SpanStyle {
                            color: [1.0, 1.0, 1.0, 1.0],
                            background_color: Some([0.0, 0.0, 0.0, 1.0]),
                            ..SpanStyle::default()
                        },
                    )
                    .new_line()
                    .add_span(
                        &format!("{:?}", time.elapsed()),
                        SpanStyle {
                            color: [1.0, 1.0, 1.0, 1.0],
                            ..SpanStyle::default()
                        },
                    );
                sugarloaf.build_text_by_id(TEXT_ID_1);

                // Third text area - demonstrates partial updates
                let needs_init = sugarloaf
                    .get_text_by_id(TEXT_ID_2)
                    .map_or(true, |state| state.lines.is_empty());

                if needs_init {
                    // Initial setup
                    sugarloaf
                        .text(TEXT_ID_2)
                        .new_line()
                        .add_span(
                            &format!("Should not update {:?}", time.elapsed()),
                            SpanStyle {
                                color: [1.0, 1.0, 1.0, 1.0],
                                ..SpanStyle::default()
                            },
                        )
                        .new_line()
                        .add_span(
                            &format!("Should update {:?}", time.elapsed()),
                            SpanStyle {
                                color: [1.0, 1.0, 1.0, 1.0],
                                ..SpanStyle::default()
                            },
                        );
                    sugarloaf.build_text_by_id(TEXT_ID_2);
                } else {
                    // Partial update - only update line 1
                    sugarloaf.text(TEXT_ID_2).clear_line(1).add_span_on_line(
                        1,
                        &format!("Updated {:?}", time.elapsed()),
                        SpanStyle {
                            color: [1.0, 1.0, 1.0, 1.0],
                            ..SpanStyle::default()
                        },
                    );
                    sugarloaf.build_text_by_id_line_number(TEXT_ID_2, 1);
                }

                // Add background rectangles (cached)
                sugarloaf.rect(
                    Some(RECT_ID_0),
                    5.,
                    5.,
                    200.0,
                    200.0,
                    [1.0, 0.5, 0.5, 0.5],
                    0.0,
                );
                sugarloaf.rect(
                    Some(RECT_ID_1),
                    220.,
                    5.,
                    200.0,
                    150.0,
                    [1.0, 0.5, 0.5, 0.5],
                    0.0,
                );
                sugarloaf.rect(
                    Some(RECT_ID_2),
                    440.,
                    5.,
                    320.0,
                    150.0,
                    [1.0, 0.5, 0.5, 0.5],
                    0.0,
                );

                // Position and show text
                sugarloaf.set_position(TEXT_ID_0, 5., 5.);
                sugarloaf.set_visibility(TEXT_ID_0, true);

                sugarloaf.set_position(TEXT_ID_1, 220., 5.);
                sugarloaf.set_visibility(TEXT_ID_1, true);

                sugarloaf.set_position(TEXT_ID_2, 440., 5.);
                sugarloaf.set_visibility(TEXT_ID_2, true);

                sugarloaf.render();
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            _ => (),
        }
    }
}
