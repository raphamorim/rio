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
    layout::RootStyle, FragmentStyle, Object, Quad, RichText, Sugarloaf, SugarloafWindow,
    SugarloafWindowSize,
};

fn main() {
    let width = 800.0;
    let height = 400.0;
    let window_event_loop = rio_window::event_loop::EventLoop::new().unwrap();
    let mut application = Application::new(&window_event_loop, width, height);
    let _ = application.run(window_event_loop);
}

struct Application {
    sugarloaf: Option<Sugarloaf<'static>>,
    window: Option<Window>,
    rich_texts: Vec<usize>,
    height: f32,
    width: f32,
}

impl Application {
    fn new(event_loop: &EventLoop<()>, width: f32, height: f32) -> Self {
        event_loop.listen_device_events(DeviceEvents::Never);

        Application {
            sugarloaf: None,
            window: None,
            rich_texts: vec![],
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

        self.rich_texts.push(sugarloaf.create_rich_text());
        self.rich_texts.push(sugarloaf.create_rich_text());
        self.rich_texts.push(sugarloaf.create_rich_text());

        sugarloaf.set_rich_text_font_size(&1, 24.0);
        sugarloaf.set_rich_text_font_size(&2, 12.0);

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

        let objects = vec![
            Object::Quad(Quad {
                color: [1.0, 0.5, 0.5, 0.5],
                position: [5., 5.],
                shadow_blur_radius: 2.0,
                shadow_offset: [1.0, 1.0],
                shadow_color: [1.0, 1.0, 0.0, 1.0],
                border_color: [1.0, 0.0, 1.0, 1.0],
                border_width: 2.0,
                border_radius: [10.0, 10.0, 10.0, 10.0],
                size: [200.0, 200.0],
            }),
            Object::RichText(RichText {
                id: self.rich_texts[0],
                position: [5., 5.],
                lines: None,
            }),
            Object::Quad(Quad {
                color: [1.0, 0.5, 0.5, 0.5],
                position: [220., 5.],
                shadow_blur_radius: 0.0,
                shadow_offset: [0.0, 0.0],
                shadow_color: [1.0, 1.0, 0.0, 1.0],
                border_color: [1.0, 0.0, 1.0, 1.0],
                border_width: 2.0,
                border_radius: [0.0, 0.0, 0.0, 0.0],
                size: [200.0, 150.0],
            }),
            Object::RichText(RichText {
                id: self.rich_texts[1],
                position: [220., 5.],
                lines: None,
            }),
            Object::Quad(Quad {
                color: [1.0, 0.5, 0.5, 0.5],
                position: [440., 5.],
                shadow_blur_radius: 0.0,
                shadow_offset: [0.0, 0.0],
                shadow_color: [1.0, 1.0, 0.0, 1.0],
                border_color: [1.0, 0.0, 1.0, 1.0],
                border_width: 2.0,
                border_radius: [0.0, 0.0, 0.0, 0.0],
                size: [320.0, 150.0],
            }),
            Object::RichText(RichText {
                id: self.rich_texts[2],
                position: [440., 5.],
                lines: None,
            }),
        ];

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ScaleFactorChanged {
                // mut inner_size_writer,
                scale_factor,
                ..
            } => {
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
                let content = sugarloaf.content();
                let time = std::time::Instant::now();
                for rich_text in &self.rich_texts {
                    if rich_text < &2 {
                        content.sel(*rich_text).clear();

                        content.new_line().add_text(
                            &format!("Text area {:?}", rich_text),
                            FragmentStyle {
                                color: [1.0, 1.0, 1.0, 1.0],
                                background_color: Some([0.0, 0.0, 0.0, 1.0]),
                                ..FragmentStyle::default()
                            },
                        );
                        content
                            .new_line()
                            .add_text(
                                &format!("{:?}", time.elapsed()),
                                FragmentStyle {
                                    color: [1.0, 1.0, 1.0, 1.0],
                                    ..FragmentStyle::default()
                                },
                            )
                            .build();
                    } else if let Some(state) = content.get_state(rich_text) {
                        // Line has initialised
                        if !state.lines.is_empty() {
                            content
                                .sel(*rich_text)
                                .clear_line(1)
                                .add_text_on_line(
                                    1,
                                    &format!("Updated {:?}", time.elapsed()),
                                    FragmentStyle {
                                        color: [1.0, 1.0, 1.0, 1.0],
                                        ..FragmentStyle::default()
                                    },
                                )
                                .build_line(1);
                        } else {
                            // Line has not initialised
                            content
                                .sel(*rich_text)
                                .new_line()
                                .add_text(
                                    &format!("Should not update {:?}", time.elapsed()),
                                    FragmentStyle {
                                        color: [1.0, 1.0, 1.0, 1.0],
                                        ..FragmentStyle::default()
                                    },
                                )
                                .new_line()
                                .add_text(
                                    &format!("Should update {:?}", time.elapsed()),
                                    FragmentStyle {
                                        color: [1.0, 1.0, 1.0, 1.0],
                                        ..FragmentStyle::default()
                                    },
                                )
                                .build();
                        }
                    }
                }

                sugarloaf.set_objects(objects);
                sugarloaf.render();
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            _ => (),
        }
    }
}
