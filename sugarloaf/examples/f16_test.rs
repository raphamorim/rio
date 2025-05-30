use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use rio_window::application::ApplicationHandler;
use rio_window::event_loop::{ActiveEventLoop, DeviceEvents};
use rio_window::window::{Window, WindowId};
use rio_window::{
    dpi::LogicalSize, event::WindowEvent, event_loop::EventLoop, window::WindowAttributes,
};
use std::error::Error;
use sugarloaf::{
    layout::RootStyle, FragmentStyle, Object, RichText, Sugarloaf, SugarloafWindow,
    SugarloafWindowSize,
};

fn main() {
    let width = 800.0;
    let height = 600.0;
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
            sugarloaf: None,
            window: None,
            width,
            height,
            rich_text: 0,
        }
    }

    fn run(&mut self, event_loop: EventLoop<()>) -> Result<(), Box<dyn Error>> {
        let result = event_loop.run_app(self);
        result.map_err(Into::into)
    }
}

impl ApplicationHandler for Application {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attributes = WindowAttributes::default()
            .with_title("F16 Support Test")
            .with_inner_size(LogicalSize::new(self.width, self.height));

        let window = event_loop.create_window(window_attributes).unwrap();

        let scale_factor = window.scale_factor();
        let font_size = 24.0;

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
        .expect("Failed to create Sugarloaf instance");

        // Test f16 support
        println!("F16 support enabled: {}", sugarloaf.ctx.supports_f16());

        sugarloaf.set_background_color(Some(wgpu::Color::BLACK));
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
        let _window = self.window.as_mut().unwrap();

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                let f16_supported = sugarloaf.ctx.supports_f16();

                let content = sugarloaf.content();
                content.sel(self.rich_text).clear();

                let f16_status = if f16_supported {
                    "F16 Support: ENABLED ✓"
                } else {
                    "F16 Support: DISABLED ✗"
                };

                content
                    .add_text(
                        f16_status,
                        FragmentStyle {
                            color: if f16_supported {
                                [0.0, 1.0, 0.0, 1.0] // Green for enabled
                            } else {
                                [1.0, 0.5, 0.0, 1.0] // Orange for disabled
                            },
                            ..FragmentStyle::default()
                        },
                    )
                    .new_line()
                    .add_text(
                        "Half precision floating point support in WGSL shaders",
                        FragmentStyle {
                            color: [0.8, 0.8, 0.8, 1.0],
                            ..FragmentStyle::default()
                        },
                    )
                    .new_line()
                    .add_text(
                        "Reduces memory bandwidth and improves performance",
                        FragmentStyle {
                            color: [0.6, 0.6, 0.6, 1.0],
                            ..FragmentStyle::default()
                        },
                    )
                    .new_line()
                    .add_text(
                        "Texture Formats:",
                        FragmentStyle {
                            color: [1.0, 1.0, 1.0, 1.0],
                            ..FragmentStyle::default()
                        },
                    )
                    .new_line()
                    .add_text(
                        &format!(
                            "• RGBA: {}",
                            if f16_supported {
                                "Rgba16Float (8 bytes/pixel)"
                            } else {
                                "Rgba8Unorm (4 bytes/pixel)"
                            }
                        ),
                        FragmentStyle {
                            color: [0.7, 0.7, 0.7, 1.0],
                            ..FragmentStyle::default()
                        },
                    )
                    .new_line()
                    .add_text(
                        &format!(
                            "• Memory savings: {}",
                            if f16_supported {
                                "~50% bandwidth reduction for interpolated data"
                            } else {
                                "Using standard precision"
                            }
                        ),
                        FragmentStyle {
                            color: [0.7, 0.7, 0.7, 1.0],
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
            }
            _ => {}
        }
    }
}
