use crate::sugarloaf::{SugarloafWindow, SugarloafWindowSize};

pub struct Context {
    pub device: wgpu::Device,
    pub surface: wgpu::Surface,
    pub queue: wgpu::Queue,
    pub format: wgpu::TextureFormat,
    pub size: SugarloafWindowSize,
    pub scale: f32,
    pub adapter_info: wgpu::AdapterInfo,
}

impl Context {
    pub async fn new(
        sugarloaf_window: &SugarloafWindow,
        power_preference: wgpu::PowerPreference,
    ) -> Context {
        #[cfg(target_arch = "wasm32")]
        let default_backend = wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL;
        #[cfg(not(target_arch = "wasm32"))]
        let default_backend = wgpu::Backends::all();

        // The backend can be configured using the `WGPU_BACKEND`
        // environment variable. If the variable is not set, the primary backend
        // will be used. The following values are allowed:
        // - `vulkan`
        // - `metal`
        // - `dx12`
        // - `dx11`
        // - `gl`
        // - `webgpu`
        // - `primary`
        let backend = wgpu::util::backend_bits_from_env().unwrap_or(default_backend);
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

        let size = &sugarloaf_window.size;
        let scale = sugarloaf_window.scale;

        let surface = unsafe {
            instance.create_surface(sugarloaf_window).unwrap()
        };

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

        // TODO: Fix formats with signs
        // FIXME: On Nvidia GPUs usage Rgba16Float texture format causes driver to enable HDR.
        // Reason for this is currently output color space is poorly defined in wgpu and
        // anything other than Srgb texture formats can cause undeterministic output color
        // space selection which also causes colors to mismatch. Optionally we can whitelist
        // only the Srgb texture formats for now until output color space selection lands in wgpu. See #205
        // TODO: use output color format for the CanvasConfiguration when it lands on the wgpu
        #[cfg(windows)]
        let unsupported_formats = [
            wgpu::TextureFormat::Rgba8Snorm,
            wgpu::TextureFormat::Rgba16Float,
        ];

        // not reproduce-able on mac
        #[cfg(not(windows))]
        let unsupported_formats = [wgpu::TextureFormat::Rgba8Snorm];

        let filtered_formats: Vec<wgpu::TextureFormat> = caps
            .formats
            .iter()
            .copied()
            .filter(|&x| {
                !wgpu::TextureFormat::is_srgb(&x) && !unsupported_formats.contains(&x)
            })
            .collect();

        let mut format: wgpu::TextureFormat = caps.formats.first().unwrap().to_owned();
        if !filtered_formats.is_empty() {
            format = filtered_formats.first().unwrap().to_owned();
        }

        log::info!(
            "Sugarloaf selected format: {format:?} from {:?}",
            caps.formats
        );
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
                                features: wgpu::Features::empty(),
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
            format,
            size: SugarloafWindowSize {
                width: size.width,
                height: size.height,
            },
            scale,
            adapter_info: adapter.get_info(),
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
