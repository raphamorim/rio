#[cfg(target_arch = "wasm32")]
use std::str::FromStr;
#[cfg(target_arch = "wasm32")]
use web_sys::{ImageBitmapRenderingContext, OffscreenCanvas};

#[cfg(target_arch = "wasm32")]
struct OffscreenCanvasSetup {
    offscreen_canvas: OffscreenCanvas,
    bitmap_renderer: ImageBitmapRenderingContext,
}

#[cfg(target_arch = "wasm32")]
/// Parse the query string as returned by `web_sys::window()?.location().search()?` and get a
/// specific key out of it.
pub fn parse_url_query_string<'a>(query: &'a str, search_key: &str) -> Option<&'a str> {
    let query_string = query.strip_prefix('?')?;

    for pair in query_string.split('&') {
        let mut pair = pair.split('=');
        let key = pair.next()?;
        let value = pair.next()?;

        if key == search_key {
            return Some(value);
        }
    }

    None
}

#[derive(Debug)]
pub struct Context {
    pub device: wgpu::Device,
    pub surface: wgpu::Surface,
    pub queue: wgpu::Queue,
    pub staging_belt: wgpu::util::StagingBelt,
    pub format: wgpu::TextureFormat,
    pub size: winit::dpi::PhysicalSize<u32>,
    pub scale: f32,
}

impl Context {
    pub async fn new(
        winit_window: &winit::window::Window,
        power_preference: wgpu::PowerPreference,
    ) -> Context {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            // dx12_shader_compiler: wgpu::Dx12Compiler::Fxc,
            ..Default::default()
        });

        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowExtWebSys;
            let query_string = web_sys::window().unwrap().location().search().unwrap();
            let level: log::Level = parse_url_query_string(&query_string, "RUST_LOG")
                .and_then(|x| x.parse().ok())
                .unwrap_or(log::Level::Error);
            console_log::init_with_level(level).expect("could not initialize logger");
            std::panic::set_hook(Box::new(console_error_panic_hook::hook));
            // On wasm, append the canvas to the document body
            web_sys::window()
                .and_then(|win| win.document())
                .and_then(|doc| doc.body())
                .and_then(|body| {
                    body.append_child(&web_sys::Element::from(winit_window.canvas()))
                        .ok()
                })
                .expect("couldn't append canvas to document body");
        }

        #[cfg(target_arch = "wasm32")]
        let mut offscreen_canvas_setup: Option<OffscreenCanvasSetup> = None;
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            use winit::platform::web::WindowExtWebSys;

            let query_string = web_sys::window().unwrap().location().search().unwrap();
            if let Some(offscreen_canvas_param) =
                parse_url_query_string(&query_string, "offscreen_canvas")
            {
                if FromStr::from_str(offscreen_canvas_param) == Ok(true) {
                    log::info!("Creating OffscreenCanvasSetup");

                    let offscreen_canvas = OffscreenCanvas::new(1024, 768)
                        .expect("couldn't create OffscreenCanvas");

                    let bitmap_renderer = winit_window
                        .canvas()
                        .get_context("bitmaprenderer")
                        .expect("couldn't create ImageBitmapRenderingContext (Result)")
                        .expect("couldn't create ImageBitmapRenderingContext (Option)")
                        .dyn_into::<ImageBitmapRenderingContext>()
                        .expect("couldn't convert into ImageBitmapRenderingContext");

                    offscreen_canvas_setup = Some(OffscreenCanvasSetup {
                        offscreen_canvas,
                        bitmap_renderer,
                    })
                }
            }
        };

        log::info!("initializing the surface");

        let size = winit_window.inner_size();
        let scale = winit_window.scale_factor();

        #[cfg(any(not(target_arch = "wasm32"), target_os = "emscripten"))]
        let surface: wgpu::Surface =
            unsafe { instance.create_surface(&winit_window).unwrap() };

        #[cfg(all(target_arch = "wasm32", not(target_os = "emscripten")))]
        let surface = unsafe {
            if let Some(offscreen_canvas_setup) = &offscreen_canvas_setup {
                log::info!("creating surface from OffscreenCanvas");
                instance.create_surface_from_offscreen_canvas(
                    offscreen_canvas_setup.offscreen_canvas.clone(),
                )
            } else {
                instance.create_surface(&winit_window)
            }
        }
        .unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Request adapter");

        let caps = surface.get_capabilities(&adapter);
        let formats = caps.formats;
        let format = *formats.last().expect("No supported formats for surface");

        let (device, queue) = (async {
            {
                if let Ok(result) = adapter
                    .request_device(&wgpu::DeviceDescriptor::default(), None)
                    .await
                {
                    result
                } else {
                    // These downlevel limits will allow the code to run on all possible hardware
                    adapter
                        .request_device(
                            &wgpu::DeviceDescriptor {
                                label: None,
                                features: wgpu::Features::default(),
                                limits: wgpu::Limits::downlevel_webgl2_defaults(),
                            },
                            None,
                        )
                        .await
                        .expect("Request device")
                }
            }
        })
        .await;

        let staging_belt = wgpu::util::StagingBelt::new(2 * 1024);

        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: size.width,
                height: size.height,
                view_formats: vec![],
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                present_mode: wgpu::PresentMode::AutoVsync,
            },
        );

        Context {
            device,
            queue,
            surface,
            staging_belt,
            format,
            size,
            scale: scale as f32,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.size.width = width;
        self.size.height = height;
        self.surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.format,
                width,
                height,
                view_formats: vec![],
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                present_mode: wgpu::PresentMode::AutoVsync,
            },
        );
    }
}
