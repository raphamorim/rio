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
    layout::RootStyle, FragmentStyle, FragmentStyleDecoration, Object, RichText,
    Sugarloaf, SugarloafWindow, SugarloafWindowSize, UnderlineInfo, UnderlineShape,
};

fn main() {
    let width = 1200.0;
    let height = 500.0;
    let window_event_loop = rio_window::event_loop::EventLoop::new().unwrap();
    let mut application = Application::new(&window_event_loop, width, height);
    let _ = application.run(window_event_loop);
}

struct Application {
    sugarloaf: Option<Sugarloaf<'static>>,
    window: Option<Window>,
    height: f32,
    width: f32,
    rich_text: usize,
}

impl Application {
    fn new(event_loop: &EventLoop<()>, width: f32, height: f32) -> Self {
        event_loop.listen_device_events(DeviceEvents::Never);

        Application {
            rich_text: 0,
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
            .with_title("Text example")
            .with_inner_size(LogicalSize::new(self.width, self.height))
            .with_resizable(true);
        let window = active_event_loop.create_window(window_attribute).unwrap();

        let scale_factor = window.scale_factor();
        let font_size = 24.;

        let sugarloaf_layout = RootStyle::new(scale_factor as f32, font_size, 1.0);

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

        sugarloaf.set_background_color(Some(wgpu::Color::RED));
        self.rich_text = sugarloaf.create_rich_text();
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
                content.sel(self.rich_text).clear();
                content
                    .add_text(
                        "Sugarloaf",
                        FragmentStyle {
                            color: [1.0, 1.0, 1.0, 1.0],
                            background_color: Some([0.0, 0.0, 0.0, 1.0]),
                            ..FragmentStyle::default()
                        },
                    )
                    .new_line()
                    .add_text(
                        "│㏑¼",
                        FragmentStyle {
                            color: [0.0, 0.0, 0.0, 1.0],
                            background_color: Some([1.0, 1.0, 1.0, 1.0]),
                            width: 2.0,
                            ..FragmentStyle::default()
                        },
                    )
                    .add_text(
                        "🥶",
                        FragmentStyle {
                            color: [1.0, 0.0, 1.0, 1.0],
                            background_color: Some([0.3, 0.5, 1.0, 1.0]),
                            width: 2.0,
                            ..FragmentStyle::default()
                        },
                    )
                    .new_line()
                    .add_text(
                        "│regular -> ",
                        FragmentStyle {
                            decoration: Some(FragmentStyleDecoration::Underline(
                                UnderlineInfo {
                                    is_doubled: false,
                                    shape: UnderlineShape::Regular,
                                },
                            )),
                            color: [1.0, 1.0, 1.0, 1.0],
                            background_color: Some([0.0, 0.0, 0.0, 1.0]),
                            ..FragmentStyle::default()
                        },
                    )
                    .add_text(
                        " ",
                        FragmentStyle {
                            decoration: None,
                            color: [1.0, 1.0, 1.0, 1.0],
                            background_color: Some([0.0, 0.0, 0.0, 1.0]),
                            ..FragmentStyle::default()
                        },
                    )
                    .add_text(
                        "|still|",
                        FragmentStyle {
                            decoration: Some(FragmentStyleDecoration::Underline(
                                UnderlineInfo {
                                    is_doubled: false,
                                    shape: UnderlineShape::Regular,
                                },
                            )),
                            color: [1.0, 1.0, 1.0, 1.0],
                            background_color: Some([0.0, 0.0, 0.0, 1.0]),
                            ..FragmentStyle::default()
                        },
                    )
                    .add_text(
                        " ",
                        FragmentStyle {
                            decoration: None,
                            color: [1.0, 1.0, 1.0, 1.0],
                            background_color: Some([0.0, 0.0, 0.0, 1.0]),
                            ..FragmentStyle::default()
                        },
                    )
                    .add_text(
                        "│curly",
                        FragmentStyle {
                            decoration: Some(FragmentStyleDecoration::Underline(
                                UnderlineInfo {
                                    is_doubled: false,
                                    shape: UnderlineShape::Curly,
                                },
                            )),
                            color: [1.0, 1.0, 1.0, 1.0],
                            background_color: Some([0.0, 0.0, 0.0, 1.0]),
                            ..FragmentStyle::default()
                        },
                    )
                    .new_line()
                    .add_text(
                        "│dashed",
                        FragmentStyle {
                            decoration: Some(FragmentStyleDecoration::Underline(
                                UnderlineInfo {
                                    is_doubled: false,
                                    shape: UnderlineShape::Dashed,
                                },
                            )),
                            color: [1.0, 1.0, 1.0, 1.0],
                            background_color: Some([0.0, 0.0, 0.0, 1.0]),
                            ..FragmentStyle::default()
                        },
                    )
                    .add_text(
                        " ",
                        FragmentStyle {
                            decoration: None,
                            color: [1.0, 1.0, 1.0, 1.0],
                            background_color: Some([0.0, 0.0, 0.0, 1.0]),
                            ..FragmentStyle::default()
                        },
                    )
                    .add_text(
                        "dotted",
                        FragmentStyle {
                            decoration: Some(FragmentStyleDecoration::Underline(
                                UnderlineInfo {
                                    is_doubled: false,
                                    shape: UnderlineShape::Dotted,
                                },
                            )),
                            color: [1.0, 1.0, 1.0, 1.0],
                            background_color: Some([0.0, 0.0, 0.0, 1.0]),
                            ..FragmentStyle::default()
                        },
                    )
                    .build();

                sugarloaf.set_objects(vec![Object::RichText(RichText {
                    id: self.rich_text,
                    position: [10., 0.],
                    lines: None,
                })]);
                sugarloaf.render();
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            _ => (),
        }
    }
}
