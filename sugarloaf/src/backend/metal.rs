#[cfg(target_os = "macos")]
use crate::backend::{RenderBackend, RenderContext};
use crate::sugarloaf::{SugarloafWindow, SugarloafWindowSize};
use ::objc::runtime::Object;
use ::objc::{msg_send, sel, sel_impl};
use core_graphics_types::geometry::CGSize;
use metal::*;
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

impl RenderContext for MetalContext {
    type Device = Device;
    type Queue = CommandQueue;
    type Surface = MetalLayer;
    type TextureFormat = MTLPixelFormat;
    type CommandEncoder = MetalCommandEncoder;
    type RenderPass<'a> = MetalRenderPass<'a>;
    type Texture = MetalTexture;
    type TextureView = Texture;
    type Buffer = Buffer;
    type BindGroup = (); // Metal doesn't have bind groups, we'll use argument buffers
    type RenderPipeline = RenderPipelineState;
    type ComputePipeline = ComputePipelineState;
    type Sampler = SamplerState;

    fn new(sugarloaf_window: SugarloafWindow, _backend: RenderBackend) -> Self {
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

    fn get_current_texture(&self) -> Result<Self::Texture, String> {
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

    fn create_command_encoder(&self) -> Self::CommandEncoder {
        let command_buffer = self.command_queue.new_command_buffer().to_owned();
        MetalCommandEncoder { command_buffer }
    }

    fn submit_commands(&self, encoder: Self::CommandEncoder) {
        encoder.command_buffer.commit();
    }

    fn present_texture(&self, texture: Self::Texture) {
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

impl MetalContext {
    pub fn create_render_pass_descriptor(
        &self,
        texture: &MetalTexture,
        clear_color: Option<(f64, f64, f64, f64)>,
    ) -> RenderPassDescriptor {
        let render_pass_descriptor = RenderPassDescriptor::new();

        let color_attachment = render_pass_descriptor
            .color_attachments()
            .object_at(0)
            .unwrap();
        color_attachment.set_texture(Some(&texture.texture));

        if let Some((r, g, b, a)) = clear_color {
            color_attachment.set_load_action(MTLLoadAction::Clear);
            color_attachment.set_clear_color(MTLClearColor::new(r, g, b, a));
        } else {
            color_attachment.set_load_action(MTLLoadAction::Load);
        }

        color_attachment.set_store_action(MTLStoreAction::Store);

        render_pass_descriptor.to_owned()
    }

    pub fn create_buffer(&self, data: &[u8], options: MTLResourceOptions) -> Buffer {
        self.device.new_buffer_with_data(
            data.as_ptr() as *const std::ffi::c_void,
            data.len() as u64,
            options,
        )
    }

    pub fn create_texture(&self, descriptor: &TextureDescriptor) -> Texture {
        self.device.new_texture(descriptor)
    }

    pub fn create_render_pipeline(
        &self,
        descriptor: &RenderPipelineDescriptor,
    ) -> Result<RenderPipelineState, String> {
        self.device
            .new_render_pipeline_state(descriptor)
            .map_err(|e| format!("Failed to create render pipeline: {:?}", e))
    }

    pub fn create_library_from_source(&self, source: &str) -> Result<Library, String> {
        let options = CompileOptions::new();
        self.device
            .new_library_with_source(source, &options)
            .map_err(|e| format!("Failed to compile Metal shader: {:?}", e))
    }

    pub fn get_optimal_texture_format(&self, channels: u32) -> MTLPixelFormat {
        if self.supports_f16 {
            match channels {
                1 => MTLPixelFormat::R16Float,
                2 => MTLPixelFormat::RG16Float,
                4 => MTLPixelFormat::RGBA16Float,
                _ => MTLPixelFormat::RGBA8Unorm, // fallback
            }
        } else {
            MTLPixelFormat::RGBA8Unorm
        }
    }

    pub fn convert_rgba8_to_optimal_format(&self, rgba8_data: &[u8]) -> Vec<u8> {
        if self.supports_f16 {
            // Convert u8 RGBA to f16 RGBA
            let mut f16_data = Vec::with_capacity(rgba8_data.len() * 2);
            for chunk in rgba8_data.chunks(4) {
                if chunk.len() == 4 {
                    // Convert u8 [0-255] to f16 [0.0-1.0]
                    let r = half::f16::from_f32(chunk[0] as f32 / 255.0);
                    let g = half::f16::from_f32(chunk[1] as f32 / 255.0);
                    let b = half::f16::from_f32(chunk[2] as f32 / 255.0);
                    let a = half::f16::from_f32(chunk[3] as f32 / 255.0);

                    f16_data.extend_from_slice(&r.to_le_bytes());
                    f16_data.extend_from_slice(&g.to_le_bytes());
                    f16_data.extend_from_slice(&b.to_le_bytes());
                    f16_data.extend_from_slice(&a.to_le_bytes());
                }
            }
            f16_data
        } else {
            rgba8_data.to_vec()
        }
    }
}

impl<'a> MetalRenderPass<'a> {
    pub fn new(
        command_buffer: &CommandBuffer,
        descriptor: &RenderPassDescriptor,
    ) -> Self {
        let encoder = command_buffer
            .new_render_command_encoder(descriptor)
            .to_owned();
        MetalRenderPass {
            encoder,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn set_render_pipeline_state(&mut self, pipeline: &RenderPipelineState) {
        self.encoder.set_render_pipeline_state(pipeline);
    }

    pub fn set_vertex_buffer(
        &mut self,
        index: u64,
        buffer: Option<&Buffer>,
        offset: u64,
    ) {
        self.encoder
            .set_vertex_buffer(index, buffer.map(|v| &**v), offset);
    }

    pub fn set_fragment_buffer(
        &mut self,
        index: u64,
        buffer: Option<&Buffer>,
        offset: u64,
    ) {
        self.encoder
            .set_fragment_buffer(index, buffer.map(|v| &**v), offset);
    }

    pub fn set_vertex_texture(&mut self, index: u64, texture: Option<&Texture>) {
        self.encoder
            .set_vertex_texture(index, texture.map(|v| &**v));
    }

    pub fn set_fragment_texture(&mut self, index: u64, texture: Option<&Texture>) {
        self.encoder
            .set_fragment_texture(index, texture.map(|v| &**v));
    }

    pub fn set_fragment_sampler(&mut self, index: u64, sampler: Option<&SamplerState>) {
        self.encoder
            .set_fragment_sampler_state(index, sampler.map(|v| &**v));
    }

    pub fn draw_primitives(
        &mut self,
        primitive_type: MTLPrimitiveType,
        vertex_start: u64,
        vertex_count: u64,
    ) {
        self.encoder
            .draw_primitives(primitive_type, vertex_start, vertex_count);
    }

    pub fn draw_indexed_primitives(
        &mut self,
        primitive_type: MTLPrimitiveType,
        index_count: u64,
        index_type: MTLIndexType,
        index_buffer: &Buffer,
        index_buffer_offset: u64,
    ) {
        self.encoder.draw_indexed_primitives(
            primitive_type,
            index_count,
            index_type,
            index_buffer,
            index_buffer_offset,
        );
    }

    pub fn end_encoding(self) {
        self.encoder.end_encoding();
    }
}
