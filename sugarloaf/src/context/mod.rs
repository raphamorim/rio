use crate::sugarloaf::{SugarloafWindow, SugarloafWindowSize};

pub struct Context {
    pub device: wgpu::Device,
    pub surface: wgpu::Surface<'static>,
    pub queue: wgpu::Queue,
    pub format: wgpu::TextureFormat,
    pub size: SugarloafWindowSize,
    pub scale: f32,
    alpha_mode: wgpu::CompositeAlphaMode,
    pub adapter_info: wgpu::AdapterInfo,
}

#[inline]
#[cfg(not(target_os = "macos"))]
fn find_best_texture_format(formats: Vec<wgpu::TextureFormat>) -> wgpu::TextureFormat {
    let mut format: wgpu::TextureFormat = formats.first().unwrap().to_owned();

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

    let filtered_formats: Vec<wgpu::TextureFormat> = formats
        .iter()
        .copied()
        .filter(|&x| {
            !wgpu::TextureFormat::is_srgb(&x) && !unsupported_formats.contains(&x)
        })
        .collect();

    if !filtered_formats.is_empty() {
        filtered_formats.first().unwrap().clone_into(&mut format);
    }

    log::info!("Sugarloaf selected format: {format:?} from {:?}", formats);

    format
}

impl Context {
    pub async fn new(
        sugarloaf_window: SugarloafWindow,
        renderer_config: &crate::sugarloaf::SugarloafRenderer,
    ) -> Context {
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
        let backend =
            wgpu::util::backend_bits_from_env().unwrap_or(renderer_config.backend);
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

        let size = sugarloaf_window.size;
        let scale = sugarloaf_window.scale;

        let surface: wgpu::Surface<'static> =
            instance.create_surface(sugarloaf_window).unwrap();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: renderer_config.power_preference,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Request adapter");

        log::info!("Selected adapter: {:?}", adapter.get_info());

        let caps = surface.get_capabilities(&adapter);

        #[cfg(target_os = "macos")]
        let format = wgpu::TextureFormat::Bgra8Unorm;
        #[cfg(not(target_os = "macos"))]
        let format = find_best_texture_format(caps.formats);

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
                                required_features: wgpu::Features::empty(),
                                required_limits: wgpu::Limits::downlevel_webgl2_defaults(
                                ),
                            },
                            None,
                        )
                        .await
                        .expect("Request device")
                }
            }
        })
        .await;

        let alpha_mode = if caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
        {
            wgpu::CompositeAlphaMode::PostMultiplied
        } else if caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else {
            wgpu::CompositeAlphaMode::Auto
        };

        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: size.width as u32,
                height: size.height as u32,
                view_formats: vec![],
                alpha_mode,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
            },
        );

        Context {
            device,
            queue,
            surface,
            format,
            alpha_mode,
            size: SugarloafWindowSize {
                width: size.width,
                height: size.height,
            },
            scale,
            adapter_info: adapter.get_info(),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.size.width = width as f32;
        self.size.height = height as f32;
        self.surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.format,
                width,
                height,
                view_formats: vec![],
                alpha_mode: self.alpha_mode,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
            },
        );
    }
}
