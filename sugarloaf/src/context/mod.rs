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
        #[cfg(target_arch = "wasm32")]
        let default_backend = wgpu::Backends::GL;
        #[cfg(not(target_arch = "wasm32"))]
        let default_backend = wgpu::Backends::all();

        let backend = wgpu::util::backend_bits_from_env().unwrap_or(default_backend);
        // let dx12_shader_compiler = wgpu::util::dx12_shader_compiler_from_env().unwrap_or_default();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: backend,
            ..Default::default()
        });

        log::info!("selected instance: {instance:?}");

        #[cfg(not(target_arch = "wasm32"))]
        {
            log::info!("Available adapters:");
            for a in instance.enumerate_adapters(wgpu::Backends::all()) {
                log::info!("    {:?}", a.get_info())
            }
        }

        log::info!("initializing the surface");

        let size = winit_window.inner_size();
        let scale = winit_window.scale_factor();

        let surface = unsafe { instance.create_surface(&winit_window).unwrap() };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Request adapter");

        log::info!("Selected adapter: {:?}", adapter.get_info());

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
