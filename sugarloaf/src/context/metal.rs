use crate::sugarloaf::{SugarloafWindow, SugarloafWindowSize};
use ::objc::runtime::Object;
use ::objc::{msg_send, sel, sel_impl};
use core_graphics_types::geometry::CGSize;
use metal::{
    CommandBuffer, CommandQueue, Device, MTLGPUFamily, MTLPixelFormat, MetalDrawable,
    MetalLayer, RenderCommandEncoder, Texture,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub struct MetalContext {
    pub device: Device,
    pub command_queue: CommandQueue,
    pub layer: MetalLayer,
    pub size: SugarloafWindowSize,
    pub scale: f32,
    pub supports_f16: bool,
}

pub struct MetalTexture {
    pub texture: Texture,
    pub drawable: MetalDrawable,
}

pub struct MetalCommandEncoder {
    pub command_buffer: CommandBuffer,
}

pub struct MetalRenderPass<'a> {
    pub encoder: RenderCommandEncoder,
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl MetalContext {
    // type Device = Device;
    // type Queue = CommandQueue;
    // type Surface = MetalLayer;
    // type TextureFormat = MTLPixelFormat;
    // type CommandEncoder = MetalCommandEncoder;
    // type RenderPass<'a> = MetalRenderPass<'a>;
    // type Texture = MetalTexture;
    // type TextureView = Texture;
    // type Buffer = Buffer;
    // type BindGroup = (); // Metal doesn't have bind groups, we'll use argument buffers
    // type RenderPipeline = RenderPipelineState;
    // type ComputePipeline = ComputePipelineState;
    // type Sampler = SamplerState;

    pub fn new(sugarloaf_window: SugarloafWindow) -> Self {
        let device = Device::system_default().expect("Failed to create Metal device");
        let command_queue = device.new_command_queue();

        let size = sugarloaf_window.size;
        let scale = sugarloaf_window.scale;

        // Create Metal layer
        let layer = MetalLayer::new();
        layer.set_device(&device);
        layer.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        layer.set_presents_with_transaction(false);

        // Use CGSize from core_graphics_types
        let drawable_size = CGSize {
            width: size.width as f64,
            height: size.height as f64,
        };
        layer.set_drawable_size(drawable_size);
        layer.set_contents_scale(scale as f64);

        // Attach layer to window
        unsafe {
            match sugarloaf_window.window_handle().unwrap().as_raw() {
                RawWindowHandle::AppKit(handle) => {
                    let ns_view = handle.ns_view.as_ptr() as *mut Object;
                    let _: () = msg_send![ns_view, setLayer: layer.as_ref()];
                    let _: () = msg_send![ns_view, setWantsLayer: true];
                }
                _ => panic!("Metal backend only supports macOS AppKit windows"),
            }
        }

        // Check for f16 support (Metal generally supports it on modern hardware)
        let supports_f16 = device.supports_family(MTLGPUFamily::Apple1);

        tracing::info!("Metal device created: {:?}", device.name());
        tracing::info!("Metal F16 support: {}", supports_f16);
        tracing::info!("SIMD shaders enabled by default");

        MetalContext {
            device,
            command_queue,
            layer,
            size: SugarloafWindowSize {
                width: size.width,
                height: size.height,
            },
            scale,
            supports_f16,
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.size.width = width as f32;
        self.size.height = height as f32;
        let drawable_size = CGSize {
            width: width as f64,
            height: height as f64,
        };
        self.layer.set_drawable_size(drawable_size);
    }

    fn get_current_texture(&self) -> Result<MetalTexture, String> {
        if let Some(drawable) = self.layer.next_drawable() {
            let texture = drawable.texture().to_owned();
            Ok(MetalTexture {
                texture,
                drawable: drawable.to_owned(),
            })
        } else {
            Err("Failed to get next drawable".to_string())
        }
    }

    // fn create_command_encoder(&self) -> Self::CommandEncoder {
    //     let command_buffer = self.command_queue.new_command_buffer().to_owned();
    //     MetalCommandEncoder { command_buffer }
    // }

    fn submit_commands(&self, encoder: MetalCommandEncoder) {
        encoder.command_buffer.commit();
    }

    fn present_texture(&self, texture: MetalTexture) {
        texture.drawable.present();
    }

    fn size(&self) -> SugarloafWindowSize {
        self.size
    }

    fn scale(&self) -> f32 {
        self.scale
    }

    fn supports_f16(&self) -> bool {
        self.supports_f16
    }
}
