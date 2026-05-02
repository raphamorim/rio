mod batch;
mod compositor;
pub mod cpu;
pub(crate) mod image_cache;
#[cfg(target_os = "linux")]
pub mod vulkan;

use crate::components::core::orthographic_projection;
#[cfg(feature = "wgpu")]
use crate::context::webgpu::WgpuContext;
use crate::context::{Context, ContextType};
use crate::font::FontLibrary;
use crate::layout::TextDimensions;
use crate::renderer::image_cache::ImageCache;
use crate::Graphics;
use compositor::{Compositor, Rect, Vertex};
use rustc_hash::FxHashMap;
use std::mem;
#[cfg(target_os = "macos")]
use std::sync::Arc;

#[cfg(target_os = "macos")]
use parking_lot::Mutex;
#[cfg(feature = "wgpu")]
use wgpu::util::DeviceExt;

#[cfg(target_os = "macos")]
use crate::context::metal::MetalContext;
#[cfg(target_os = "macos")]
use metal::*;

#[cfg(feature = "wgpu")]
use std::borrow::Cow;

#[cfg(feature = "wgpu")]
pub const BLEND: Option<wgpu::BlendState> = Some(wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::SrcAlpha,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
});

// `WgpuRenderer` is much larger than `MetalRenderer` (which shrunk to a
// pool handle + a few pipeline states after the triple-buffer refactor),
// so the enum-variant size disparity is intentional. We don't `Box` the
// hot variants — they're each constructed exactly once per Sugarloaf
// instance, and the inline storage avoids an extra allocation + indirection
// on every render call.
#[allow(clippy::large_enum_variant)]
pub enum RendererType {
    #[cfg(feature = "wgpu")]
    Wgpu(WgpuRenderer),
    #[cfg(target_os = "macos")]
    Metal(MetalRenderer),
    /// Native Vulkan backend (Linux). Mirrors the `Metal` variant in
    /// scope: no librashader filters. Phase 1 = clear-and-present;
    /// real pipelines land in later phases.
    #[cfg(target_os = "linux")]
    Vulkan(vulkan::VulkanRenderer),
    /// CPU backend: no GPU brush; rasterization happens in `cpu::CpuPipeline` at present time.
    Cpu,
}

#[cfg(feature = "wgpu")]
pub struct WgpuRenderer {
    vertex_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    constant_bind_group: wgpu::BindGroup,
    layout_bind_group: wgpu::BindGroup,
    layout_bind_group_layout: wgpu::BindGroupLayout,
    transform: wgpu::Buffer,
    pipeline: wgpu::RenderPipeline,
    instanced_pipeline: wgpu::RenderPipeline,
    current_transform: [f32; 16],
    supported_vertex_buffer: usize,
    supported_instance_buffer: usize,
    // Image pipeline (separate from text)
    image_pipeline: wgpu::RenderPipeline,
    image_bind_group_layout: wgpu::BindGroupLayout,
    image_vertex_buffer: wgpu::Buffer,
    /// Dedicated one-instance vertex buffer for the background image,
    /// kept separate from the kitty `image_vertex_buffer` so it cannot
    /// collide with kitty placement slots.
    background_image_vertex_buffer: wgpu::Buffer,
}

/// GPU-side uniforms shared by every Metal pipeline.
///
/// `input_colorspace` encodes how the shader should interpret the sRGB-encoded
/// RGB values it receives from the CPU (theme / ANSI colors) before writing
/// them to the DisplayP3-tagged framebuffer:
/// - `0` = sRGB. Apply the sRGB → DisplayP3 primaries matrix after
///   linearization so `#ff0000` displays as the sRGB-standard red rather than
///   P3-pure red.
/// - `1` = DisplayP3. Treat inputs as already-P3, skip the matrix.
/// - `2` = Rec.2020. Skipped (matrix pending), matches `1` in practice.
///
/// The field is stored as a `u8` plus padding up to 16 bytes so the whole
/// struct stays 16-byte aligned for Metal's `constant` buffer binding;
/// `#[repr(C)]` guarantees the field order.
#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct Globals {
    transform: [f32; 16],
    input_colorspace: u8,
    _pad: [u8; 15],
}

/// Metal `MTLBuffer.contents` is a thread-safe pointer per Apple's docs; we
/// only ever write into it before `commit()` and only ever read from it on
/// the GPU after `commit()`, so the buffer can cross threads safely (the
/// completion handler that returns it to the pool runs on a Metal-internal
/// thread). Pool ownership transitions are mutex-protected.
#[cfg(target_os = "macos")]
#[derive(Debug)]
pub(crate) struct PooledMetalBuffer(pub metal::Buffer);
#[cfg(target_os = "macos")]
unsafe impl Send for PooledMetalBuffer {}
#[cfg(target_os = "macos")]
unsafe impl Sync for PooledMetalBuffer {}

/// One bump-allocated buffer per in-flight frame. Every per-frame pipeline
/// (text quads, non-quad geometry, kitty/sixel images, bg image, bg fill)
/// sub-allocates from the same buffer using a monotonically advancing
/// offset; each `set_vertex_buffer` call binds at the matching offset.
/// Mirrors zed's `gpui_macos::InstanceBuffer`.
#[cfg(target_os = "macos")]
#[derive(Debug)]
pub(crate) struct InstanceBuffer {
    pub buffer: PooledMetalBuffer,
    pub size: usize,
}

/// Free list of equally-sized `metal::Buffer`s, plus the current target
/// size. `acquire` pops a free buffer or allocates a new one; `release`
/// pushes the buffer back if its size still matches the current target
/// (otherwise it's dropped — see `grow`). Backpressure comes from the
/// drawable-count limit on the layer; the pool naturally stays small.
#[cfg(target_os = "macos")]
#[derive(Debug)]
pub(crate) struct InstanceBufferPool {
    buffer_size: usize,
    buffers: Vec<PooledMetalBuffer>,
}

#[cfg(target_os = "macos")]
impl InstanceBufferPool {
    /// Initial buffer size matches what rio used to allocate up-front
    /// (~20k QuadInstances × 96 B ≈ 1.9 MiB). zed's default is the same
    /// 2 MiB.
    const INITIAL_SIZE: usize = 2 * 1024 * 1024;
    /// Hard ceiling — beyond this, abort the frame rather than grow
    /// unbounded. Same cap zed uses.
    const MAX_SIZE: usize = 256 * 1024 * 1024;

    pub fn new() -> Self {
        Self {
            buffer_size: Self::INITIAL_SIZE,
            buffers: Vec::new(),
        }
    }

    pub fn acquire(&mut self, device: &Device) -> InstanceBuffer {
        let buffer = self.buffers.pop().unwrap_or_else(|| {
            // On Apple Silicon (unified memory) `Shared` is a true zero-copy
            // CPU/GPU mapping; `WriteCombined` skips the CPU read cache since
            // we only ever write into this buffer. On Intel/AMD discrete GPUs
            // there's no unified memory, so `Shared` would PCIe-shuttle every
            // access — `Managed` lets us upload via a manual `did_modify_range`
            // and keeps the GPU read fast. Same split zed uses.
            let options = if device.has_unified_memory() {
                MTLResourceOptions::StorageModeShared
                    | MTLResourceOptions::CPUCacheModeWriteCombined
            } else {
                MTLResourceOptions::StorageModeManaged
            };
            let buf = device.new_buffer(self.buffer_size as u64, options);
            buf.set_label("sugarloaf::pooled instance buffer");
            PooledMetalBuffer(buf)
        });
        InstanceBuffer {
            buffer,
            size: self.buffer_size,
        }
    }

    pub fn release(&mut self, buffer: InstanceBuffer) {
        // Stale (post-grow) buffers are silently dropped; they fall out
        // of scope and metal-rs releases the underlying MTLBuffer.
        if buffer.size == self.buffer_size {
            self.buffers.push(buffer.buffer);
        }
    }

    /// Doubles the target buffer size and discards the free list — old
    /// in-flight buffers will be rejected by `release` on completion and
    /// dropped naturally. Returns `false` if we're already at the cap.
    pub fn grow(&mut self) -> bool {
        let next = self.buffer_size.saturating_mul(2);
        if next > Self::MAX_SIZE {
            return false;
        }
        self.buffer_size = next;
        self.buffers.clear();
        true
    }

    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }
}

/// Align a bump-allocator offset up to 256 bytes. Metal vertex-buffer
/// offsets must be a multiple of `minimumLinearTextureAlignmentForPixelFormat`,
/// which on every current Apple GPU is ≤ 256. 256 is the safe choice and
/// matches zed's `align_offset`.
#[cfg(target_os = "macos")]
#[inline]
fn align_offset(offset: &mut usize) {
    *offset = (*offset).div_ceil(256) * 256;
}

/// Bump-allocate `byte_len` bytes inside `buf`, copying from `src`.
/// Returns the offset where the data was written, or `None` on overflow.
/// Caller must `align_offset` first if the binding requires alignment
/// stricter than 1 byte (every `set_vertex_buffer` does).
#[cfg(target_os = "macos")]
#[inline]
unsafe fn bump_copy<T>(
    buf: &InstanceBuffer,
    offset: &mut usize,
    src: *const T,
    count: usize,
) -> Option<usize> {
    let bytes = count * mem::size_of::<T>();
    let next = *offset + bytes;
    if next > buf.size {
        return None;
    }
    let dst = unsafe { (buf.buffer.0.contents() as *mut u8).add(*offset) };
    unsafe { std::ptr::copy_nonoverlapping(src as *const u8, dst, bytes) };
    let where_ = *offset;
    *offset = next;
    Some(where_)
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
pub struct MetalRenderer {
    pipeline_state: RenderPipelineState,
    instanced_pipeline_state: RenderPipelineState,
    sampler: SamplerState,
    /// Interpretation of sRGB-encoded input colors. Set once at construction
    /// from the `[window] colorspace` config; written into every `Globals`
    /// uniform via `set_vertex_bytes`. Values: `0 = sRGB`, `1 = DisplayP3`,
    /// `2 = Rec.2020`.
    input_colorspace: u8,
    // Image pipeline (separate from text)
    image_pipeline_state: RenderPipelineState,
    /// Pool of equally-sized buffers, one acquired per in-flight frame.
    /// All per-frame data (text vertices, quad instances, image instances,
    /// bg fill, bg image instance) bump-allocates from the acquired buffer.
    /// `add_completed_handler` releases the buffer back here when the GPU
    /// finishes. `Arc<Mutex<…>>` so the completion thread can release.
    #[allow(clippy::arc_with_non_send_sync)]
    instance_buffer_pool: Arc<Mutex<InstanceBufferPool>>,
}

#[cfg(target_os = "macos")]
impl MetalRenderer {
    pub fn new(context: &MetalContext, colorspace: crate::sugarloaf::Colorspace) -> Self {
        let input_colorspace = match colorspace {
            crate::sugarloaf::Colorspace::Srgb => 0u8,
            crate::sugarloaf::Colorspace::DisplayP3 => 1u8,
            crate::sugarloaf::Colorspace::Rec2020 => 2u8,
        };

        // Create Metal shader library
        let shader_source = include_str!("renderer.metal");
        let library = context
            .device
            .new_library_with_source(shader_source, &CompileOptions::new())
            .expect("Failed to create shader library");

        let vertex_function = library
            .get_function("vs_main", None)
            .expect("Failed to get vertex function");
        let fragment_function = library
            .get_function("fs_main", None)
            .expect("Failed to get fragment function");

        // Create vertex descriptor for rich text rendering
        // Vertex layout (88 bytes total):
        // - pos: [f32; 3] = 12 bytes (offset 0)
        // - color: [f32; 4] = 16 bytes (offset 12)
        // - uv: [f32; 2] = 8 bytes (offset 28)
        // - layers: [i32; 2] = 8 bytes (offset 36)
        // - corner_radii: [f32; 4] = 16 bytes (offset 44)
        // - rect_size: [f32; 2] = 8 bytes (offset 60)
        // - underline_style: i32 = 4 bytes (offset 68)
        // - clip_rect: [f32; 4] = 16 bytes (offset 72)
        // Total: 88 bytes
        let vertex_descriptor = VertexDescriptor::new();
        let attributes = vertex_descriptor.attributes();

        // Position (attribute 0) - vec3<f32>
        attributes
            .object_at(0)
            .unwrap()
            .set_format(MTLVertexFormat::Float3);
        attributes.object_at(0).unwrap().set_offset(0);
        attributes.object_at(0).unwrap().set_buffer_index(0);

        // Color (attribute 1) - vec4<f32>
        attributes
            .object_at(1)
            .unwrap()
            .set_format(MTLVertexFormat::Float4);
        attributes.object_at(1).unwrap().set_offset(12);
        attributes.object_at(1).unwrap().set_buffer_index(0);

        // UV (attribute 2) - vec2<f32>
        attributes
            .object_at(2)
            .unwrap()
            .set_format(MTLVertexFormat::Float2);
        attributes.object_at(2).unwrap().set_offset(28);
        attributes.object_at(2).unwrap().set_buffer_index(0);

        // Layers (attribute 3) - vec2<i32>
        attributes
            .object_at(3)
            .unwrap()
            .set_format(MTLVertexFormat::Int2);
        attributes.object_at(3).unwrap().set_offset(36);
        attributes.object_at(3).unwrap().set_buffer_index(0);

        // Corner radii (attribute 4) - vec4<f32>
        attributes
            .object_at(4)
            .unwrap()
            .set_format(MTLVertexFormat::Float4);
        attributes.object_at(4).unwrap().set_offset(44);
        attributes.object_at(4).unwrap().set_buffer_index(0);

        // Rect size (attribute 5) - vec2<f32>
        attributes
            .object_at(5)
            .unwrap()
            .set_format(MTLVertexFormat::Float2);
        attributes.object_at(5).unwrap().set_offset(60);
        attributes.object_at(5).unwrap().set_buffer_index(0);

        // Underline style (attribute 6) - i32
        attributes
            .object_at(6)
            .unwrap()
            .set_format(MTLVertexFormat::Int);
        attributes.object_at(6).unwrap().set_offset(68);
        attributes.object_at(6).unwrap().set_buffer_index(0);

        // Clip rect (attribute 7) - vec4<f32>
        attributes
            .object_at(7)
            .unwrap()
            .set_format(MTLVertexFormat::Float4);
        attributes.object_at(7).unwrap().set_offset(72);
        attributes.object_at(7).unwrap().set_buffer_index(0);

        // Set up buffer layout
        let layouts = vertex_descriptor.layouts();
        layouts
            .object_at(0)
            .unwrap()
            .set_stride(mem::size_of::<Vertex>() as u64);
        layouts
            .object_at(0)
            .unwrap()
            .set_step_function(MTLVertexStepFunction::PerVertex);
        layouts.object_at(0).unwrap().set_step_rate(1);

        // Create render pipeline
        let pipeline_descriptor = RenderPipelineDescriptor::new();
        pipeline_descriptor.set_vertex_function(Some(&vertex_function));
        pipeline_descriptor.set_fragment_function(Some(&fragment_function));
        pipeline_descriptor.set_vertex_descriptor(Some(vertex_descriptor));

        let color_attachment = pipeline_descriptor
            .color_attachments()
            .object_at(0)
            .unwrap();
        // Must match the drawable format in `context/metal.rs` — HW will
        // reject the pipeline otherwise. Plain `BGRA8Unorm` → gamma-space
        // alpha blending (ghostty `alpha-blending = native`).
        color_attachment.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        color_attachment.set_blending_enabled(true);
        // Match WGSL BLEND settings exactly:
        // color: src_factor: SrcAlpha, dst_factor: OneMinusSrcAlpha, operation: Add
        color_attachment.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
        color_attachment
            .set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
        color_attachment.set_rgb_blend_operation(MTLBlendOperation::Add);
        // alpha: src_factor: One, dst_factor: OneMinusSrcAlpha, operation: Add
        color_attachment.set_source_alpha_blend_factor(MTLBlendFactor::One);
        color_attachment
            .set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
        color_attachment.set_alpha_blend_operation(MTLBlendOperation::Add);

        let pipeline_state = context
            .device
            .new_render_pipeline_state(&pipeline_descriptor)
            .expect("Failed to create render pipeline state");

        // Instanced pipeline (vs_instanced + fs_main, no vertex descriptor)
        let instanced_vertex_fn = library
            .get_function("vs_instanced", None)
            .expect("Failed to get instanced vertex function");
        let instanced_pipeline_descriptor = RenderPipelineDescriptor::new();
        instanced_pipeline_descriptor.set_vertex_function(Some(&instanced_vertex_fn));
        instanced_pipeline_descriptor.set_fragment_function(Some(&fragment_function));
        // No vertex descriptor — instance data read from buffer(0) directly

        let inst_color = instanced_pipeline_descriptor
            .color_attachments()
            .object_at(0)
            .unwrap();
        inst_color.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        inst_color.set_blending_enabled(true);
        inst_color.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
        inst_color.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
        inst_color.set_rgb_blend_operation(MTLBlendOperation::Add);
        inst_color.set_source_alpha_blend_factor(MTLBlendFactor::One);
        inst_color
            .set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
        inst_color.set_alpha_blend_operation(MTLBlendOperation::Add);

        let instanced_pipeline_state = context
            .device
            .new_render_pipeline_state(&instanced_pipeline_descriptor)
            .expect("Failed to create instanced pipeline state");

        // Create sampler for texture sampling - IMPROVED SAMPLER SETTINGS
        let sampler_descriptor = SamplerDescriptor::new();
        // Match WGPU settings: Linear filtering for crisp text
        sampler_descriptor.set_min_filter(MTLSamplerMinMagFilter::Linear);
        sampler_descriptor.set_mag_filter(MTLSamplerMinMagFilter::Linear);
        sampler_descriptor.set_mip_filter(MTLSamplerMipFilter::Linear);
        // ClampToEdge addressing to prevent texture bleeding
        sampler_descriptor.set_address_mode_s(MTLSamplerAddressMode::ClampToEdge);
        sampler_descriptor.set_address_mode_t(MTLSamplerAddressMode::ClampToEdge);
        sampler_descriptor.set_address_mode_r(MTLSamplerAddressMode::ClampToEdge);
        let sampler = context.device.new_sampler(&sampler_descriptor);

        let image_shader_source = include_str!("image.metal");
        let image_library = context
            .device
            .new_library_with_source(image_shader_source, &CompileOptions::new())
            .expect("Failed to create image shader library");

        let image_vertex_fn = image_library
            .get_function("image_vs_main", None)
            .expect("Failed to get image vertex function");
        let image_fragment_fn = image_library
            .get_function("image_fs_main", None)
            .expect("Failed to get image fragment function");

        let image_vertex_descriptor = VertexDescriptor::new();
        let image_attrs = image_vertex_descriptor.attributes();

        // dest_pos: vec2<f32> at offset 0
        image_attrs
            .object_at(0)
            .unwrap()
            .set_format(MTLVertexFormat::Float2);
        image_attrs.object_at(0).unwrap().set_offset(0);
        image_attrs.object_at(0).unwrap().set_buffer_index(0);

        // dest_size: vec2<f32> at offset 8
        image_attrs
            .object_at(1)
            .unwrap()
            .set_format(MTLVertexFormat::Float2);
        image_attrs.object_at(1).unwrap().set_offset(8);
        image_attrs.object_at(1).unwrap().set_buffer_index(0);

        // source_rect: vec4<f32> at offset 16
        image_attrs
            .object_at(2)
            .unwrap()
            .set_format(MTLVertexFormat::Float4);
        image_attrs.object_at(2).unwrap().set_offset(16);
        image_attrs.object_at(2).unwrap().set_buffer_index(0);

        let image_layouts = image_vertex_descriptor.layouts();
        image_layouts
            .object_at(0)
            .unwrap()
            .set_stride(mem::size_of::<ImageInstance>() as u64);
        image_layouts
            .object_at(0)
            .unwrap()
            .set_step_function(MTLVertexStepFunction::PerInstance);
        image_layouts.object_at(0).unwrap().set_step_rate(1);

        let image_pipeline_descriptor = RenderPipelineDescriptor::new();
        image_pipeline_descriptor.set_vertex_function(Some(&image_vertex_fn));
        image_pipeline_descriptor.set_fragment_function(Some(&image_fragment_fn));
        image_pipeline_descriptor.set_vertex_descriptor(Some(image_vertex_descriptor));

        let image_color_attachment = image_pipeline_descriptor
            .color_attachments()
            .object_at(0)
            .unwrap();
        image_color_attachment.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        image_color_attachment.set_blending_enabled(true);
        // Premultiplied alpha: One, OneMinusSrcAlpha
        image_color_attachment.set_source_rgb_blend_factor(MTLBlendFactor::One);
        image_color_attachment
            .set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
        image_color_attachment.set_rgb_blend_operation(MTLBlendOperation::Add);
        image_color_attachment.set_source_alpha_blend_factor(MTLBlendFactor::One);
        image_color_attachment
            .set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
        image_color_attachment.set_alpha_blend_operation(MTLBlendOperation::Add);

        let image_pipeline_state = context
            .device
            .new_render_pipeline_state(&image_pipeline_descriptor)
            .expect("Failed to create image pipeline state");

        Self {
            pipeline_state,
            instanced_pipeline_state,
            sampler,
            input_colorspace,
            image_pipeline_state,
            instance_buffer_pool: Arc::new(Mutex::new(InstanceBufferPool::new())),
        }
    }

    /// Encode the text/quad pipeline draws into `render_encoder`,
    /// bump-allocating vertex/instance data from `instance_buffer` at
    /// `instance_offset`. Returns `false` if the pool buffer overflows
    /// — caller is expected to `end_encoding`, grow the pool, and retry
    /// the whole frame.
    ///
    /// Globals (transform + input_colorspace) are uploaded inline via
    /// `set_vertex_bytes` / `set_fragment_bytes` (no buffer needed for an
    /// 80-byte struct).
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render(
        &mut self,
        instances: &[batch::QuadInstance],
        vertices: &[Vertex],
        draw_cmds: &[batch::DrawCmd],
        images: &ImageCache,
        render_encoder: &RenderCommandEncoderRef,
        context: &MetalContext,
        instance_buffer: &InstanceBuffer,
        instance_offset: &mut usize,
    ) -> bool {
        if instances.is_empty() && vertices.is_empty() {
            return true;
        }

        let globals = Globals {
            transform: orthographic_projection(context.size.width, context.size.height),
            input_colorspace: self.input_colorspace,
            _pad: [0; 15],
        };

        // Bump-allocate instance + vertex data.
        let instances_bytes_offset = if !instances.is_empty() {
            align_offset(instance_offset);
            match unsafe {
                bump_copy(
                    instance_buffer,
                    instance_offset,
                    instances.as_ptr(),
                    instances.len(),
                )
            } {
                Some(o) => o,
                None => return false,
            }
        } else {
            0
        };

        let vertices_bytes_offset = if !vertices.is_empty() {
            align_offset(instance_offset);
            match unsafe {
                bump_copy(
                    instance_buffer,
                    instance_offset,
                    vertices.as_ptr(),
                    vertices.len(),
                )
            } {
                Some(o) => o,
                None => return false,
            }
        } else {
            0
        };

        render_encoder.set_fragment_sampler_state(0, Some(&self.sampler));

        let color_textures = images.get_metal_textures();
        let mask_texture = images.get_mask_texture();

        let mut current_pipeline_instanced = false;
        let mut pipeline_set = false;

        let globals_ptr = &globals as *const Globals as *const std::ffi::c_void;
        let globals_size = mem::size_of::<Globals>() as u64;

        for cmd in draw_cmds {
            let (color_layer, mask_layer) = match cmd {
                batch::DrawCmd::Instanced {
                    color_layer,
                    mask_layer,
                    ..
                } => (*color_layer, *mask_layer),
                batch::DrawCmd::Vertices {
                    color_layer,
                    mask_layer,
                    ..
                } => (*color_layer, *mask_layer),
            };

            // Bind textures
            if color_layer > 0 {
                let idx = (color_layer - 1) as usize;
                if idx < color_textures.len() {
                    render_encoder.set_fragment_texture(0, Some(color_textures[idx]));
                } else {
                    render_encoder.set_fragment_texture(0, None);
                }
            } else {
                render_encoder.set_fragment_texture(0, None);
            }

            if mask_layer > 0 {
                if let Some(mask_tex) = mask_texture {
                    render_encoder.set_fragment_texture(1, Some(mask_tex));
                } else {
                    render_encoder.set_fragment_texture(1, None);
                }
            } else {
                render_encoder.set_fragment_texture(1, None);
            }

            match cmd {
                batch::DrawCmd::Instanced { offset, count, .. } => {
                    if !pipeline_set || !current_pipeline_instanced {
                        render_encoder
                            .set_render_pipeline_state(&self.instanced_pipeline_state);
                        // Inline Globals via setBytes (no buffer needed).
                        render_encoder.set_vertex_bytes(1, globals_size, globals_ptr);
                        render_encoder.set_fragment_bytes(1, globals_size, globals_ptr);
                        current_pipeline_instanced = true;
                        pipeline_set = true;
                    }
                    let byte_offset = (instances_bytes_offset
                        + (*offset as usize) * mem::size_of::<batch::QuadInstance>())
                        as u64;
                    render_encoder.set_vertex_buffer(
                        0,
                        Some(&instance_buffer.buffer.0),
                        byte_offset,
                    );
                    render_encoder.draw_primitives_instanced(
                        MTLPrimitiveType::TriangleStrip,
                        0,
                        4,
                        *count as u64,
                    );
                }
                batch::DrawCmd::Vertices { offset, count, .. } => {
                    if !pipeline_set || current_pipeline_instanced {
                        render_encoder.set_render_pipeline_state(&self.pipeline_state);
                        render_encoder.set_vertex_buffer(
                            0,
                            Some(&instance_buffer.buffer.0),
                            vertices_bytes_offset as u64,
                        );
                        render_encoder.set_vertex_bytes(1, globals_size, globals_ptr);
                        render_encoder.set_fragment_bytes(1, globals_size, globals_ptr);
                        current_pipeline_instanced = false;
                        pipeline_set = true;
                    }
                    render_encoder.draw_primitives(
                        MTLPrimitiveType::Triangle,
                        *offset as u64,
                        *count as u64,
                    );
                }
            }
        }
        true
    }
}

/// Backend-agnostic per-image GPU texture.
/// Dropped when removed from the map → GPU memory freed immediately.
///
/// The Vulkan variant carries an inline `VulkanImageTexture`
/// (image + view + memory + per-image descriptor pool/set) which is
/// larger than the other variants. Boxing it would buy nothing —
/// each entry is constructed once, lives until eviction, and is
/// only stored in the per-image map.
#[allow(clippy::large_enum_variant)]
enum ImageTexture {
    #[cfg(feature = "wgpu")]
    Wgpu {
        _texture: wgpu::Texture, // kept alive so `view` stays valid
        view: wgpu::TextureView,
    },
    #[cfg(target_os = "macos")]
    Metal(metal::Texture),
    /// Native Vulkan upload — owns image + view + descriptor set
    /// (the descriptor set's binding 0 is wired to the image view +
    /// shared sampler at upload time, so draw paths just bind it).
    #[cfg(target_os = "linux")]
    Vulkan(vulkan::VulkanImageTexture),
}

/// Per-image texture entry stored in the renderer.
struct ImageTextureEntry {
    gpu: ImageTexture,
    transmit_time: std::time::Instant,
}

/// Per-instance data for image rendering (one instance = one image placement).
/// The vertex shader generates 4 quad corners from vertex_id.
///
/// `pub` because it appears in the signature of
/// `vulkan::VulkanRenderer::render_image_overlays` (also `pub` so
/// the `Renderer` dispatcher can call it). Not part of the crate's
/// public API in spirit — just in visibility.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
pub struct ImageInstance {
    /// Screen position of the image top-left (physical pixels).
    pub dest_pos: [f32; 2],
    /// Size of the image on screen (physical pixels).
    pub dest_size: [f32; 2],
    /// Source rectangle in the texture: xy = origin, zw = size (normalized 0..1).
    pub source_rect: [f32; 4],
}

/// Which layer to render the image in. Mirrors ghostty's
/// three-bucket split (`renderer/image.zig:94-97`,
/// `renderer/generic.zig:1647-1695`):
///
/// - `BelowBg`   — `z < BG_LIMIT`. Drawn before the cell-bg pass; sits
///   underneath everything terminal-related.
/// - `BelowText` — `BG_LIMIT ≤ z < 0`. Drawn between cell-bg and
///   cell-text passes — the kitty default for "image with text on top".
/// - `AboveText` — `z >= 0`. Drawn after the cell-text pass; sits on
///   top of all glyphs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ImageLayer {
    BelowBg,
    BelowText,
    AboveText,
}

/// Threshold separating `BelowBg` from `BelowText`. Matches ghostty's
/// `bg_limit = std.math.minInt(i32) / 2` at
/// `renderer/image.zig:377`.
const IMAGE_BG_LIMIT: i32 = i32::MIN / 2;

/// A single image draw command for the image pipeline.
struct ImageDraw {
    image_id: u32,
    instance: ImageInstance,
    layer: ImageLayer,
}

/// Decoded background image pixels (RGBA8) waiting to be uploaded to the GPU.
pub struct BackgroundImagePixels {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

pub struct Renderer {
    brush_type: RendererType,
    comp: Compositor,
    instances: Vec<batch::QuadInstance>,
    vertices: Vec<Vertex>,
    draw_cmds: Vec<batch::DrawCmd>,
    images: ImageCache,
    /// Per-image GPU textures (one map, any backend).
    image_textures: FxHashMap<u32, ImageTextureEntry>,
    /// Image draw commands for the current frame.
    image_draws: Vec<ImageDraw>,
    /// Pending background image upload (consumed by `prepare`).
    background_image_dirty: Option<BackgroundImagePixels>,
    /// Dedicated GPU texture for the background image, sized to the
    /// image dimensions instead of going through the glyph atlas.
    background_image_texture: Option<ImageTextureEntry>,
    /// Metal swap-chain state. One semaphore + one frame index for
    /// the whole renderer regardless of how many split-pane grids
    /// exist — mirrors ghostty's `SwapChain` at
    /// `renderer/generic.zig:247`. Each render acquires one permit,
    /// advances the index, hands the index to every grid's
    /// `render_bg_metal` / `render_text_metal`, and releases the
    /// permit from the command-buffer completion handler.
    #[cfg(target_os = "macos")]
    metal_frame_permits: crate::grid::metal::FramePermits,
    #[cfg(target_os = "macos")]
    metal_frame_index: usize,
}

/// Upload `pixels` to a fresh GPU texture using whatever backend `context`
/// is bound to. Mirrors the per-image upload in `render_graphic_overlays`,
/// but produces a standalone `ImageTextureEntry` sized exactly to the image
/// instead of consuming a slot in the glyph atlas.
fn upload_background_image_texture(
    context: &mut crate::context::Context,
    pixels: &BackgroundImagePixels,
) -> Option<ImageTextureEntry> {
    if pixels.width == 0 || pixels.height == 0 {
        return None;
    }
    let gpu = match &context.inner {
        crate::context::ContextType::Cpu(_) => return None,
        // Vulkan path: the renderer owns the descriptor-set layout
        // and shared sampler; we read them off the live brush_type
        // here. Only the renderer is on the Sugarloaf struct, not
        // the context, so the call site below threads them in.
        // Actually: this function is a free fn taking only the
        // context — we need to defer the upload until we have the
        // renderer too. We do that by panicking here and pushing
        // the real upload into a renderer method (see
        // `Renderer::upload_background_image_vulkan`). When the
        // dispatcher (`prepare`) sees a Vulkan ctx + dirty pixels,
        // it calls the renderer method directly instead of this
        // free function.
        #[cfg(feature = "wgpu")]
        crate::context::ContextType::Wgpu(ctx) => {
            let texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("sugarloaf::background image"),
                size: wgpu::Extent3d {
                    width: pixels.width,
                    height: pixels.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            ctx.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &pixels.pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(pixels.width * 4),
                    rows_per_image: Some(pixels.height),
                },
                wgpu::Extent3d {
                    width: pixels.width,
                    height: pixels.height,
                    depth_or_array_layers: 1,
                },
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            ImageTexture::Wgpu {
                _texture: texture,
                view,
            }
        }
        #[cfg(target_os = "macos")]
        crate::context::ContextType::Metal(ctx) => {
            let desc = metal::TextureDescriptor::new();
            // `_sRGB` is mandatory: with bilinear sampling enabled on the
            // image sampler, the HW interpolates between texels in the
            // texture's native space. With a non-sRGB format the texels
            // are gamma-encoded, so interpolation happens in gamma space
            // and midtones at scaled edges come out visibly darker than
            // ghostty's. With `_sRGB` the HW decodes each texel to linear
            // before mixing, producing the correct linear-light blend
            // (matches ghostty's `bgra8unorm_srgb` in `Metal.zig:374`).
            // The fragment shader then `unlinearize`s the sampled value
            // back to gamma-encoded sRGB before writing to the gamma
            // framebuffer.
            desc.set_pixel_format(metal::MTLPixelFormat::RGBA8Unorm_sRGB);
            desc.set_width(pixels.width as u64);
            desc.set_height(pixels.height as u64);
            desc.set_usage(
                metal::MTLTextureUsage::ShaderRead | metal::MTLTextureUsage::ShaderWrite,
            );
            let mtl_tex = ctx.device.new_texture(&desc);
            mtl_tex.set_label("sugarloaf::background image");
            mtl_tex.replace_region(
                metal::MTLRegion {
                    origin: metal::MTLOrigin { x: 0, y: 0, z: 0 },
                    size: metal::MTLSize {
                        width: pixels.width as u64,
                        height: pixels.height as u64,
                        depth: 1,
                    },
                },
                0,
                pixels.pixels.as_ptr() as *const std::ffi::c_void,
                (pixels.width * 4) as u64,
            );
            ImageTexture::Metal(mtl_tex)
        }
        // Vulkan goes through `Renderer::upload_background_image_vulkan`
        // (the renderer holds the descriptor set + sampler this free fn
        // can't see), so this match arm just declines and lets the
        // dispatcher try the renderer-bound path. Linux-only.
        #[cfg(target_os = "linux")]
        crate::context::ContextType::Vulkan(_) => return None,
        #[cfg(not(feature = "wgpu"))]
        crate::context::ContextType::_Phantom(_) => unreachable!(),
    };
    Some(ImageTextureEntry {
        gpu,
        transmit_time: std::time::Instant::now(),
    })
}

impl Renderer {
    pub fn new(context: &Context, colorspace: crate::sugarloaf::Colorspace) -> Self {
        // `colorspace` only matters to the Metal path (macOS); on other
        // platforms the shader doesn't know the sRGB→P3 matrix yet so we
        // silently drop it here.
        #[cfg(not(target_os = "macos"))]
        let _ = colorspace;
        let brush_type = match &context.inner {
            #[cfg(feature = "wgpu")]
            ContextType::Wgpu(wgpu_context) => {
                RendererType::Wgpu(WgpuRenderer::new(wgpu_context))
            }
            #[cfg(target_os = "macos")]
            ContextType::Metal(metal_context) => {
                RendererType::Metal(MetalRenderer::new(metal_context, colorspace))
            }
            #[cfg(target_os = "linux")]
            ContextType::Vulkan(vulkan_context) => RendererType::Vulkan(
                vulkan::VulkanRenderer::new(vulkan_context, colorspace),
            ),
            ContextType::Cpu(_) => RendererType::Cpu,
            #[cfg(not(feature = "wgpu"))]
            ContextType::_Phantom(_) => unreachable!(),
        };

        Self {
            brush_type,
            comp: Compositor::new(),
            instances: vec![],
            vertices: vec![],
            draw_cmds: vec![],
            images: ImageCache::new(context),
            image_textures: FxHashMap::default(),
            image_draws: Vec::new(),
            background_image_dirty: None,
            background_image_texture: None,
            #[cfg(target_os = "macos")]
            metal_frame_permits: crate::grid::metal::new_frame_permits(),
            #[cfg(target_os = "macos")]
            metal_frame_index: 0,
        }
    }

    /// Drain per-frame batch state that was populated via
    /// `rect` / `quad` / etc. Normally `comp.batches` is drained
    /// inside `compute_updates` → `comp.finish()` during a render;
    /// when the caller skips the GPU submit entirely (see
    /// `Sugarloaf::discard_frame`), those recorded primitives would
    /// otherwise pile into the next presented frame.
    #[inline]
    pub(crate) fn discard_frame_batches(&mut self) {
        self.comp.batches.reset();
    }

    /// Replace the background image. Pass `None` to clear it. The pixels
    /// are uploaded into a dedicated GPU texture on the next `prepare`
    /// call (so we don't go through the glyph atlas).
    pub fn set_background_image_pixels(&mut self, pixels: Option<BackgroundImagePixels>) {
        if pixels.is_some() {
            self.background_image_dirty = pixels;
        } else {
            self.background_image_dirty = None;
            self.background_image_texture = None;
        }
    }

    #[inline]
    pub fn prepare(
        &mut self,
        context: &mut crate::context::Context,
        _state: &crate::sugarloaf::state::SugarState,
        _graphics: &mut Graphics,
        image_data: &mut rustc_hash::FxHashMap<
            u32,
            crate::sugarloaf::graphics::GraphicDataEntry,
        >,
        image_overlays: &rustc_hash::FxHashMap<
            usize,
            Vec<crate::sugarloaf::graphics::GraphicOverlay>,
        >,
    ) {
        self.instances.clear();
        self.vertices.clear();
        self.draw_cmds.clear();

        // The per-id `Content.states` walk is gone — non-Text content
        // arms (Rect/RoundedRect/Line/Triangle/Polygon/Arc/Image) had
        // no rio caller passing `Some(id)`, so the Content registry
        // never accumulated them. Immediate-mode primitives flow
        // through `Renderer::rect/quad/...` straight into
        // `comp.batches`; rich-text emission is handled by the grid
        // pass and `sugarloaf::text`.

        // Image overlays: rio is responsible for not leaving stale
        // overlays for hidden panels (callers `clear_image_overlays_for`
        // on hide / panel removal). The renderer just drains whatever
        // `image_overlays` currently holds.
        let overlays: Vec<_> =
            image_overlays.iter().flat_map(|(_, v)| v.iter()).collect();
        if !overlays.is_empty() {
            self.render_graphic_overlays(context, image_data, &overlays);
        } else {
            // No overlays visible — clear draw commands so stale images
            // don't keep rendering. Keep image_textures and image_data
            // so images can be re-rendered when scrolling back.
            self.image_draws.clear();
        }

        // Upload pending background image (if any) before the render pass
        // begins. The texture stays cached until a new image arrives or
        // `set_background_image_pixels(None)` is called.
        if let Some(pixels) = self.background_image_dirty.take() {
            // Vulkan needs the renderer's descriptor-set layout +
            // sampler to wire the per-image descriptor set, so it
            // takes a different path that knows about both.
            #[cfg(target_os = "linux")]
            let used_vulkan =
                if matches!(&context.inner, crate::context::ContextType::Vulkan(_)) {
                    self.upload_background_image_vulkan(context, &pixels);
                    true
                } else {
                    false
                };
            #[cfg(not(target_os = "linux"))]
            let used_vulkan = false;

            if !used_vulkan {
                self.background_image_texture =
                    upload_background_image_texture(context, &pixels);
            }
        }

        self.instances.clear();
        self.vertices.clear();
        self.draw_cmds.clear();
        self.images.process_atlases(context);
        self.comp
            .finish(&mut self.instances, &mut self.vertices, &mut self.draw_cmds);

        // Useful for debug occasionally
        // let inst_bytes =
        // self.instances.len() * std::mem::size_of::<batch::QuadInstance>();
        // let vert_bytes = self.vertices.len() * std::mem::size_of::<Vertex>();
        // println!(
        // "gpu upload: {} instances ({:.2} MB) + {} verts ({:.2} MB) = {:.2} MB",
        // self.instances.len(),
        // inst_bytes as f64 / (1024.0 * 1024.0),
        // self.vertices.len(),
        // vert_bytes as f64 / (1024.0 * 1024.0),
        // (inst_bytes + vert_bytes) as f64 / (1024.0 * 1024.0),
        // );
    }

    #[inline]
    /// Get character cell dimensions using font metrics (fast, no rendering)
    pub fn get_character_cell_dimensions(
        &self,
        font_library: &FontLibrary,
        font_size: f32,
        line_height: f32,
    ) -> Option<TextDimensions> {
        // Use read lock instead of write lock since we're not modifying
        if let Some(font_library_data) = font_library.inner.try_read() {
            let font_id = 0; // FONT_ID_REGULAR

            // Use existing method to get cached metrics
            drop(font_library_data); // Drop read lock
            let mut font_library_data = font_library.inner.write();
            if let Some((ascent, descent, leading)) =
                font_library_data.get_font_metrics(&font_id, font_size)
            {
                // Calculate character width using font metrics
                // For monospace fonts, we can estimate character width
                let char_width = font_size * 0.6; // Common monospace width ratio
                let total_line_height = (ascent + descent + leading) * line_height;

                return Some(TextDimensions {
                    width: char_width.max(1.0),
                    height: total_line_height.max(1.0),
                    scale: 1.0,
                });
            }
        }
        None
    }

    /// Render image overlays using per-image GPU textures.
    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn render_graphic_overlays(
        &mut self,
        context: &mut crate::context::Context,
        image_data: &mut rustc_hash::FxHashMap<
            u32,
            crate::sugarloaf::graphics::GraphicDataEntry,
        >,
        overlays: &[&crate::sugarloaf::graphics::GraphicOverlay],
    ) {
        // Note: don't evict textures not in the current overlay set —
        // images may be temporarily off-screen and need their texture
        // when scrolling back into view.

        // Upload/update per-image textures
        for overlay in overlays {
            let entry = match image_data.get(&overlay.image_id) {
                Some(e) => e,
                None => continue,
            };

            // Skip if texture is current
            if let Some(existing) = self.image_textures.get(&overlay.image_id) {
                if existing.transmit_time == entry.transmit_time {
                    continue;
                }
            }

            let (width, height, pixels) = match &entry.handle.data {
                crate::components::core::image::Data::Rgba {
                    width,
                    height,
                    pixels,
                } => (*width, *height, pixels.as_ref()),
                _ => continue,
            };

            if width == 0 || height == 0 {
                continue;
            }

            // CPU backend: image overlays not supported in v1, skip.
            if matches!(&context.inner, crate::context::ContextType::Cpu(_)) {
                continue;
            }
            // Vulkan: synchronous one-shot upload via the renderer's
            // descriptor-set layout + sampler. The submit-and-wait
            // is fine here — kitty placements come in bursts (a
            // single image transmit, then many placements), and the
            // upload is the cost we'd pay regardless. Move to a
            // deferred per-frame pattern later if profiling shows
            // image-heavy workloads stall.
            #[cfg(target_os = "linux")]
            if let crate::context::ContextType::Vulkan(vk_ctx) = &context.inner {
                let RendererType::Vulkan(brush) = &self.brush_type else {
                    continue;
                };
                let texture = vulkan::VulkanImageTexture::upload_rgba(
                    vk_ctx,
                    pixels,
                    width,
                    height,
                    brush.image_texture_descriptor_set_layout,
                    brush.image_sampler,
                );
                self.image_textures.insert(
                    overlay.image_id,
                    ImageTextureEntry {
                        gpu: ImageTexture::Vulkan(texture),
                        transmit_time: entry.transmit_time,
                    },
                );
                continue;
            }
            let gpu = match &context.inner {
                crate::context::ContextType::Cpu(_) => unreachable!(),
                #[cfg(target_os = "linux")]
                crate::context::ContextType::Vulkan(_) => unreachable!(),
                #[cfg(feature = "wgpu")]
                crate::context::ContextType::Wgpu(ctx) => {
                    let texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("kitty image"),
                        size: wgpu::Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsages::COPY_DST
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    });
                    ctx.queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        pixels,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(width * 4),
                            rows_per_image: Some(height),
                        },
                        wgpu::Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                    );
                    let view =
                        texture.create_view(&wgpu::TextureViewDescriptor::default());
                    ImageTexture::Wgpu {
                        _texture: texture,
                        view,
                    }
                }
                #[cfg(target_os = "macos")]
                crate::context::ContextType::Metal(ctx) => {
                    let desc = metal::TextureDescriptor::new();
                    // `_sRGB`: bilinear sampling must interpolate in
                    // linear light, otherwise scaled midtones come out
                    // dark — see the matching note on the background-image
                    // texture above.
                    desc.set_pixel_format(metal::MTLPixelFormat::RGBA8Unorm_sRGB);
                    desc.set_width(width as u64);
                    desc.set_height(height as u64);
                    desc.set_usage(
                        metal::MTLTextureUsage::ShaderRead
                            | metal::MTLTextureUsage::ShaderWrite,
                    );
                    let mtl_tex = ctx.device.new_texture(&desc);
                    mtl_tex.set_label("kitty image");
                    mtl_tex.replace_region(
                        metal::MTLRegion {
                            origin: metal::MTLOrigin { x: 0, y: 0, z: 0 },
                            size: metal::MTLSize {
                                width: width as u64,
                                height: height as u64,
                                depth: 1,
                            },
                        },
                        0,
                        pixels.as_ptr() as *const std::ffi::c_void,
                        (width * 4) as u64,
                    );
                    ImageTexture::Metal(mtl_tex)
                }
                // `_Phantom` is the lifetime-placeholder variant that
                // only exists when wgpu is feature-gated out. Naming
                // it explicitly (instead of `_ => continue`) forces
                // the compiler to flag any future variant added to
                // `ContextType` — a previous wildcard arm shadowed
                // the platform arms above and silently dropped every
                // kitty image upload on macOS+no-wgpu builds.
                #[cfg(not(feature = "wgpu"))]
                crate::context::ContextType::_Phantom(_) => continue,
            };

            self.image_textures.insert(
                overlay.image_id,
                ImageTextureEntry {
                    gpu,
                    transmit_time: entry.transmit_time,
                },
            );
        }

        // Build image draw commands (one instance per image placement)
        self.image_draws.clear();
        for overlay in overlays {
            if !self.image_textures.contains_key(&overlay.image_id) {
                continue;
            }
            self.image_draws.push(ImageDraw {
                image_id: overlay.image_id,
                instance: ImageInstance {
                    dest_pos: [overlay.x, overlay.y],
                    dest_size: [overlay.width, overlay.height],
                    source_rect: overlay.source_rect,
                },
                layer: if overlay.z_index < IMAGE_BG_LIMIT {
                    ImageLayer::BelowBg
                } else if overlay.z_index < 0 {
                    ImageLayer::BelowText
                } else {
                    ImageLayer::AboveText
                },
            });
        }
    }

    /// Draw image overlays for a specific layer using the image pipeline (Metal).
    ///
    /// The vertex buffer is shared across all draws (and across the
    /// BelowText/AboveText layer passes), so each draw writes to its
    /// own slot indexed by its position in `image_draws` and binds the
    /// vertex buffer with the matching offset. Writing every instance
    /// to slot 0 (the previous behaviour) made every draw read the same
    /// last-written instance, so a screen with N kitty placements would
    /// only ever render the most recent one.
    #[cfg(target_os = "macos")]
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    fn draw_images_metal(
        image_draws: &[ImageDraw],
        image_textures: &FxHashMap<u32, ImageTextureEntry>,
        brush: &MetalRenderer,
        render_encoder: &metal::RenderCommandEncoderRef,
        layer: ImageLayer,
        instance_buffer: &InstanceBuffer,
        instance_offset: &mut usize,
        globals: &Globals,
    ) -> bool {
        let has_any = image_draws.iter().any(|d| d.layer == layer);
        if !has_any {
            return true;
        }

        render_encoder.set_render_pipeline_state(&brush.image_pipeline_state);
        let globals_ptr = globals as *const Globals as *const std::ffi::c_void;
        let globals_size = mem::size_of::<Globals>() as u64;
        render_encoder.set_vertex_bytes(1, globals_size, globals_ptr);
        // image_fs_main reads `input_colorspace` from Globals.
        render_encoder.set_fragment_bytes(1, globals_size, globals_ptr);
        render_encoder.set_fragment_sampler_state(0, Some(&brush.sampler));

        let stride = mem::size_of::<ImageInstance>();

        for draw in image_draws.iter().filter(|d| d.layer == layer) {
            let img = match image_textures.get(&draw.image_id) {
                Some(e) => e,
                None => continue,
            };
            // Without the `wgpu` feature `ImageTexture` only carries
            // the `Metal` variant on macOS, so the match is
            // infallible. Allow the clippy lint locally rather than
            // splitting into two cfg branches; the Wgpu arm below is
            // a real code path when the feature is on.
            #[allow(clippy::infallible_destructuring_match)]
            let tex = match &img.gpu {
                ImageTexture::Metal(tex) => tex,
                #[cfg(feature = "wgpu")]
                _ => continue,
            };

            align_offset(instance_offset);
            let offset = match unsafe {
                bump_copy(
                    instance_buffer,
                    instance_offset,
                    &draw.instance as *const ImageInstance,
                    1,
                )
            } {
                Some(o) => o,
                None => return false,
            };
            let _ = stride;

            render_encoder.set_vertex_buffer(
                0,
                Some(&instance_buffer.buffer.0),
                offset as u64,
            );
            render_encoder.set_fragment_texture(0, Some(tex));
            render_encoder.draw_primitives_instanced(
                metal::MTLPrimitiveType::TriangleStrip,
                0,
                4,
                1,
            );
        }
        true
    }

    /// Draw a single fullscreen background image quad through the image
    /// pipeline. Mirrors `draw_images_metal` but uses the dedicated
    /// `background_image_vertex_buffer` so it never collides with kitty
    /// placements, and reads the bg texture from `background_image_texture`.
    #[cfg(target_os = "macos")]
    #[must_use]
    fn draw_background_image_metal(
        background_image_texture: &Option<ImageTextureEntry>,
        brush: &MetalRenderer,
        render_encoder: &metal::RenderCommandEncoderRef,
        physical_size: (f32, f32),
        instance_buffer: &InstanceBuffer,
        instance_offset: &mut usize,
        globals: &Globals,
    ) -> bool {
        let entry = match background_image_texture {
            Some(e) => e,
            None => return true,
        };
        // See `draw_images_metal` for why this is `#[allow]`-ed +
        // the Wgpu arm is feature-gated.
        #[allow(clippy::infallible_destructuring_match)]
        let tex = match &entry.gpu {
            ImageTexture::Metal(tex) => tex,
            #[cfg(feature = "wgpu")]
            _ => return true,
        };

        let instance = ImageInstance {
            dest_pos: [0.0, 0.0],
            dest_size: [physical_size.0, physical_size.1],
            source_rect: [0.0, 0.0, 1.0, 1.0],
        };
        align_offset(instance_offset);
        let offset = match unsafe {
            bump_copy(
                instance_buffer,
                instance_offset,
                &instance as *const ImageInstance,
                1,
            )
        } {
            Some(o) => o,
            None => return false,
        };

        render_encoder.set_render_pipeline_state(&brush.image_pipeline_state);
        let globals_ptr = globals as *const Globals as *const std::ffi::c_void;
        let globals_size = mem::size_of::<Globals>() as u64;
        render_encoder.set_vertex_bytes(1, globals_size, globals_ptr);
        render_encoder.set_fragment_bytes(1, globals_size, globals_ptr);
        render_encoder.set_fragment_sampler_state(0, Some(&brush.sampler));
        render_encoder.set_vertex_buffer(
            0,
            Some(&instance_buffer.buffer.0),
            offset as u64,
        );
        render_encoder.set_fragment_texture(0, Some(tex));
        render_encoder.draw_primitives_instanced(
            metal::MTLPrimitiveType::TriangleStrip,
            0,
            4,
            1,
        );
        true
    }

    /// Full-screen GPU bg-color fill — runs first every frame on a
    /// transparent-cleared surface. Drives the bg through the shader's
    /// `prepare_output_rgb` so the colorspace + transfer-curve work
    /// happens once, on the GPU, exactly the same as every other quad.
    /// Replaces the previous `MTLClearColor` + Rust-side
    /// `prepare_output_rgb_f64` path (one-shot CPU encode, with a
    /// matrix that had to stay in sync with `renderer.metal`).
    ///
    /// The instanced pipeline blend factors are
    /// `SrcAlpha / OneMinusSrcAlpha` for RGB and `One / OneMinusSrcAlpha`
    /// for alpha; on the cleared `(0,0,0,0)` surface this writes
    /// `(bg_gamma * bg.a, bg.a)`, which is correctly premultiplied —
    /// translucent windows now pass the right bytes to the compositor
    /// (the old `MTLClearColor` path stored non-premultiplied components
    /// and made translucent bgs read too bright).
    #[cfg(target_os = "macos")]
    #[must_use]
    fn draw_bg_fill_metal(
        brush: &MetalRenderer,
        render_encoder: &metal::RenderCommandEncoderRef,
        physical_size: (f32, f32),
        bg_color: [f32; 4],
        instance_buffer: &InstanceBuffer,
        instance_offset: &mut usize,
        globals: &Globals,
    ) -> bool {
        use crate::renderer::batch::QuadInstance;

        let instance = QuadInstance {
            pos: [0.0, 0.0, 0.0],
            color: bg_color,
            uv_rect: [0.0; 4],
            layers: [0, 0],
            size: [physical_size.0, physical_size.1],
            corner_radii: [0.0; 4],
            underline_style: 0,
            clip_rect: [0.0; 4],
        };
        align_offset(instance_offset);
        let offset = match unsafe {
            bump_copy(
                instance_buffer,
                instance_offset,
                &instance as *const QuadInstance,
                1,
            )
        } {
            Some(o) => o,
            None => return false,
        };

        render_encoder.set_render_pipeline_state(&brush.instanced_pipeline_state);
        render_encoder.set_vertex_buffer(
            0,
            Some(&instance_buffer.buffer.0),
            offset as u64,
        );
        let globals_ptr = globals as *const Globals as *const std::ffi::c_void;
        let globals_size = mem::size_of::<Globals>() as u64;
        render_encoder.set_vertex_bytes(1, globals_size, globals_ptr);
        render_encoder.set_fragment_bytes(1, globals_size, globals_ptr);
        render_encoder.draw_primitives_instanced(
            metal::MTLPrimitiveType::TriangleStrip,
            0,
            4,
            1,
        );
        true
    }

    #[inline]
    pub fn reset(&mut self) {
        self.image_textures.clear();
        self.image_draws.clear();
    }

    #[inline]
    pub fn clear_atlas(&mut self) {
        self.images.clear_atlas();
        self.image_textures.clear();
        self.image_draws.clear();
        tracing::info!("Renderer atlas cleared");
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
        depth: f32,
        order: u8,
    ) {
        self.comp.batches.rect(
            &Rect {
                x,
                y,
                width,
                height,
            },
            depth,
            &color,
            order,
        );
    }

    /// Add a rounded rectangle with the specified border radius
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn rounded_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
        depth: f32,
        border_radius: f32,
        order: u8,
    ) {
        self.comp.batches.rounded_rect(
            &Rect {
                x,
                y,
                width,
                height,
            },
            depth,
            &color,
            border_radius,
            order,
        );
    }

    /// Add a quad with per-corner radii
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn quad(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        background_color: [f32; 4],
        corner_radii: [f32; 4],
        depth: f32,
        order: u8,
    ) {
        self.comp.batches.quad(
            &Rect {
                x,
                y,
                width,
                height,
            },
            depth,
            &background_color,
            corner_radii,
            order,
        );
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn add_image_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
        coords: [f32; 4],
        depth: f32,
        atlas_layer: i32,
    ) {
        self.comp.batches.add_image_rect(
            &Rect {
                x,
                y,
                width,
                height,
            },
            depth,
            &color,
            &coords,
            atlas_layer,
        );
    }

    #[inline]
    pub fn polygon(&mut self, points: &[(f32, f32)], depth: f32, color: [f32; 4]) {
        self.comp
            .batches
            .add_antialiased_polygon(points, depth, color);
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn triangle(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
        depth: f32,
        color: [f32; 4],
    ) {
        self.comp
            .batches
            .add_triangle(x1, y1, x2, y2, x3, y3, depth, color);
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn line(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        width: f32,
        depth: f32,
        color: [f32; 4],
    ) {
        self.comp
            .batches
            .add_line(x1, y1, x2, y2, width, depth, color);
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn arc(
        &mut self,
        center_x: f32,
        center_y: f32,
        radius: f32,
        start_angle_deg: f32,
        end_angle_deg: f32,
        stroke_width: f32,
        depth: f32,
        color: [f32; 4],
    ) {
        self.comp.batches.add_arc(
            center_x,
            center_y,
            radius,
            start_angle_deg,
            end_angle_deg,
            stroke_width,
            depth,
            &color,
        );
    }

    #[inline]
    #[cfg(feature = "wgpu")]
    pub fn render<'pass>(
        &'pass mut self,
        ctx: &mut WgpuContext,
        rpass: &mut wgpu::RenderPass<'pass>,
    ) {
        // Destructure to get independent borrows of different fields
        let Self {
            brush_type,
            images,
            instances,
            vertices,
            draw_cmds,
            image_draws,
            image_textures,
            background_image_texture,
            ..
        } = self;

        if let RendererType::Wgpu(brush) = brush_type {
            let color_views = images.get_texture_views();
            let mask_texture_view = images.get_mask_texture_view();

            let has_images = !image_draws.is_empty();
            let has_background = background_image_texture.is_some();
            if (color_views.is_empty() || (instances.is_empty() && vertices.is_empty()))
                && !has_images
                && !has_background
            {
                return;
            }

            // Background image: drawn first so all subsequent text/rects
            // composite on top. Single fullscreen instance, dedicated
            // vertex buffer, reuses the kitty image pipeline + sampler.
            if let Some(bg_tex) = background_image_texture.as_ref() {
                if let ImageTexture::Wgpu { view, .. } = &bg_tex.gpu {
                    let instance = ImageInstance {
                        dest_pos: [0.0, 0.0],
                        dest_size: [ctx.size.width, ctx.size.height],
                        source_rect: [0.0, 0.0, 1.0, 1.0],
                    };
                    ctx.queue.write_buffer(
                        &brush.background_image_vertex_buffer,
                        0,
                        bytemuck::bytes_of(&instance),
                    );
                    let bg_bind =
                        ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("background image bind group"),
                            layout: &brush.image_bind_group_layout,
                            entries: &[wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(view),
                            }],
                        });
                    rpass.set_pipeline(&brush.image_pipeline);
                    rpass.set_bind_group(0, &brush.constant_bind_group, &[]);
                    rpass.set_bind_group(1, &bg_bind, &[]);
                    rpass.set_vertex_buffer(
                        0,
                        brush.background_image_vertex_buffer.slice(..),
                    );
                    rpass.draw(0..4, 0..1);
                    // Restore text pipeline state for downstream batches.
                    rpass.set_pipeline(&brush.pipeline);
                    rpass.set_bind_group(0, &brush.constant_bind_group, &[]);
                }
            }

            if has_images && image_draws.iter().any(|d| d.layer == ImageLayer::BelowText)
            {
                // Each draw must use a unique slot in the shared vertex
                // buffer. Writing every instance to offset 0 (the old
                // behaviour) made the GPU read only the last-written
                // instance, so a screen with N kitty placements only
                // ever rendered the most recent one. The buffer is
                // sized for `MAX_IMAGE_INSTANCES` instances; the same
                // index space is used by the AboveText pass below so
                // both layers see consistent instance data.
                // Bumped from 64 to accommodate kitty Unicode placeholders
                // which can produce up to cols*rows draws per visible image
                // (one per placeholder cell with its own source rect).
                const MAX_IMAGE_INSTANCES: usize = 1024;
                if image_draws.len() > MAX_IMAGE_INSTANCES {
                    tracing::warn!(
                        "image_draws ({}) exceeds vertex buffer capacity ({}); \
                         extra placements will not render this frame",
                        image_draws.len(),
                        MAX_IMAGE_INSTANCES
                    );
                }
                let limit = image_draws.len().min(MAX_IMAGE_INSTANCES);
                let stride = std::mem::size_of::<ImageInstance>() as u64;

                rpass.set_pipeline(&brush.image_pipeline);
                rpass.set_bind_group(0, &brush.constant_bind_group, &[]);
                for (i, draw) in image_draws.iter().take(limit).enumerate() {
                    if draw.layer != ImageLayer::BelowText {
                        continue;
                    }
                    if let Some(img) = image_textures.get(&draw.image_id) {
                        if let ImageTexture::Wgpu { view, .. } = &img.gpu {
                            let bg = ctx.device.create_bind_group(
                                &wgpu::BindGroupDescriptor {
                                    label: None,
                                    layout: &brush.image_bind_group_layout,
                                    entries: &[wgpu::BindGroupEntry {
                                        binding: 0,
                                        resource: wgpu::BindingResource::TextureView(
                                            view,
                                        ),
                                    }],
                                },
                            );
                            let offset = i as u64 * stride;
                            ctx.queue.write_buffer(
                                &brush.image_vertex_buffer,
                                offset,
                                bytemuck::bytes_of(&draw.instance),
                            );
                            rpass.set_bind_group(1, &bg, &[]);
                            rpass.set_vertex_buffer(
                                0,
                                brush.image_vertex_buffer.slice(offset..offset + stride),
                            );
                            rpass.draw(0..4, 0..1);
                        }
                    }
                }
                rpass.set_pipeline(&brush.pipeline);
                rpass.set_bind_group(0, &brush.constant_bind_group, &[]);
            }

            // Upload buffers once
            if !instances.is_empty() {
                if instances.len() > brush.supported_instance_buffer {
                    brush.instance_buffer.destroy();
                    brush.supported_instance_buffer =
                        (instances.len() as f32 * 1.25) as usize;
                    brush.instance_buffer =
                        ctx.device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some("rich_text::Instance Buffer (resized)"),
                            size: mem::size_of::<batch::QuadInstance>() as u64
                                * brush.supported_instance_buffer as u64,
                            usage: wgpu::BufferUsages::VERTEX
                                | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        });
                }
                ctx.queue.write_buffer(
                    &brush.instance_buffer,
                    0,
                    bytemuck::cast_slice(instances),
                );
            }
            if !vertices.is_empty() {
                if vertices.len() > brush.supported_vertex_buffer {
                    brush.vertex_buffer.destroy();
                    brush.supported_vertex_buffer =
                        (vertices.len() as f32 * 1.25) as usize;
                    brush.vertex_buffer =
                        ctx.device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some("rich_text::Vertices Buffer (resized)"),
                            size: mem::size_of::<Vertex>() as u64
                                * brush.supported_vertex_buffer as u64,
                            usage: wgpu::BufferUsages::VERTEX
                                | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        });
                }
                ctx.queue.write_buffer(
                    &brush.vertex_buffer,
                    0,
                    bytemuck::cast_slice(vertices),
                );
            }

            // Text pipeline: dispatch draw commands
            let mut current_pipeline_instanced = false;
            let mut pipeline_set = false;

            for cmd in draw_cmds {
                let (color_layer, mask_layer) = match cmd {
                    batch::DrawCmd::Instanced {
                        color_layer,
                        mask_layer,
                        ..
                    } => (*color_layer, *mask_layer),
                    batch::DrawCmd::Vertices {
                        color_layer,
                        mask_layer,
                        ..
                    } => (*color_layer, *mask_layer),
                };

                // Bind textures for this batch
                let color_view = if color_layer > 0 {
                    let idx = (color_layer - 1) as usize;
                    color_views.get(idx).unwrap_or(&color_views[0])
                } else {
                    &color_views[0]
                };
                let final_mask_view = if mask_layer > 0 {
                    mask_texture_view.unwrap_or(color_views[0])
                } else {
                    color_views[0]
                };
                brush.update_bind_group(ctx, color_view, final_mask_view);

                match cmd {
                    batch::DrawCmd::Instanced { offset, count, .. } => {
                        if !pipeline_set || !current_pipeline_instanced {
                            rpass.set_pipeline(&brush.instanced_pipeline);
                            rpass.set_bind_group(0, &brush.constant_bind_group, &[]);
                            current_pipeline_instanced = true;
                            pipeline_set = true;
                        }
                        rpass.set_bind_group(1, &brush.layout_bind_group, &[]);
                        let byte_offset =
                            *offset as u64 * mem::size_of::<batch::QuadInstance>() as u64;
                        rpass.set_vertex_buffer(
                            0,
                            brush.instance_buffer.slice(byte_offset..),
                        );
                        rpass.draw(0..4, 0..*count);
                    }
                    batch::DrawCmd::Vertices { offset, count, .. } => {
                        if !pipeline_set || current_pipeline_instanced {
                            rpass.set_pipeline(&brush.pipeline);
                            rpass.set_bind_group(0, &brush.constant_bind_group, &[]);
                            rpass.set_vertex_buffer(0, brush.vertex_buffer.slice(..));
                            current_pipeline_instanced = false;
                            pipeline_set = true;
                        }
                        rpass.set_bind_group(1, &brush.layout_bind_group, &[]);
                        rpass.draw(*offset..*offset + *count, 0..1);
                    }
                }
            }

            if has_images && image_draws.iter().any(|d| d.layer == ImageLayer::AboveText)
            {
                // See BelowText pass above for the rationale; both
                // passes share the same indexing into image_draws so
                // each placement always reads its own slot.
                // Bumped from 64 to accommodate kitty Unicode placeholders
                // which can produce up to cols*rows draws per visible image
                // (one per placeholder cell with its own source rect).
                const MAX_IMAGE_INSTANCES: usize = 1024;
                let limit = image_draws.len().min(MAX_IMAGE_INSTANCES);
                let stride = std::mem::size_of::<ImageInstance>() as u64;

                rpass.set_pipeline(&brush.image_pipeline);
                rpass.set_bind_group(0, &brush.constant_bind_group, &[]);
                for (i, draw) in image_draws.iter().take(limit).enumerate() {
                    if draw.layer != ImageLayer::AboveText {
                        continue;
                    }
                    if let Some(img) = image_textures.get(&draw.image_id) {
                        if let ImageTexture::Wgpu { view, .. } = &img.gpu {
                            let bg = ctx.device.create_bind_group(
                                &wgpu::BindGroupDescriptor {
                                    label: None,
                                    layout: &brush.image_bind_group_layout,
                                    entries: &[wgpu::BindGroupEntry {
                                        binding: 0,
                                        resource: wgpu::BindingResource::TextureView(
                                            view,
                                        ),
                                    }],
                                },
                            );
                            let offset = i as u64 * stride;
                            ctx.queue.write_buffer(
                                &brush.image_vertex_buffer,
                                offset,
                                bytemuck::bytes_of(&draw.instance),
                            );
                            rpass.set_bind_group(1, &bg, &[]);
                            rpass.set_vertex_buffer(
                                0,
                                brush.image_vertex_buffer.slice(offset..offset + stride),
                            );
                            rpass.draw(0..4, 0..1);
                        }
                    }
                }
            }
        }
    }

    /// Drive an entire Metal frame: acquire a pooled buffer, encode all
    /// passes (bg fill, bg image, BelowText images, text/quads, AboveText
    /// images) into a single render command encoder, present and commit.
    ///
    /// On buffer overflow we end encoding, drop the partial command
    /// buffer (never committed), grow the pool, and retry the frame.
    /// Mirrors zed's `MetalRenderer::draw` (`gpui_macos/src/metal_renderer.rs`).
    ///
    /// The completion handler returns the buffer to the pool when the GPU
    /// finishes — this is what makes 3 frames safely in-flight: each
    /// frame owns its own buffer for the lifetime of GPU execution, so
    /// the CPU can write the next frame's data without racing.
    #[cfg(target_os = "macos")]
    pub fn render_metal(
        &mut self,
        context: &MetalContext,
        bg_color: Option<[f32; 4]>,
        grids: &mut [(&mut crate::grid::GridRenderer, crate::grid::GridUniforms)],
        text: &mut crate::text::Text,
    ) {
        use block::ConcreteBlock;
        use std::cell::Cell as StdCell;

        let brush = match &mut self.brush_type {
            RendererType::Metal(b) => b,
            _ => return,
        };

        let surface_texture = match context.get_current_texture() {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("Metal surface error: {}", e);
                return;
            }
        };

        // Acquire one swap-chain permit for the whole renderer.
        // Blocks if 3 frames are already in flight — backpressure
        // that keeps the CPU from outrunning the GPU. Mirrors
        // ghostty's `SwapChain.nextFrame` at
        // `renderer/generic.zig:295`. Single Arc, single permit, no
        // matter how many split-pane grids the renderer is driving.
        crate::grid::metal::acquire_frame_permit(&self.metal_frame_permits);
        self.metal_frame_index =
            (self.metal_frame_index + 1) % crate::grid::metal::FRAMES_IN_FLIGHT_PUB;
        let frame = self.metal_frame_index;

        loop {
            let instance_buffer =
                brush.instance_buffer_pool.lock().acquire(&context.device);
            let mut instance_offset: usize = 0;

            let command_buffer = context.command_queue.new_command_buffer();
            command_buffer.set_label("Sugarloaf Metal Render");

            let render_pass_descriptor = metal::RenderPassDescriptor::new();
            let color_attachment = render_pass_descriptor
                .color_attachments()
                .object_at(0)
                .unwrap();
            color_attachment.set_texture(Some(&surface_texture.texture));
            color_attachment.set_store_action(metal::MTLStoreAction::Store);
            color_attachment.set_load_action(metal::MTLLoadAction::Clear);
            color_attachment
                .set_clear_color(metal::MTLClearColor::new(0.0, 0.0, 0.0, 0.0));

            let render_encoder =
                command_buffer.new_render_command_encoder(render_pass_descriptor);
            render_encoder.set_label("Sugarloaf Metal Render Pass");

            let globals = Globals {
                transform: orthographic_projection(
                    context.size.width,
                    context.size.height,
                ),
                input_colorspace: brush.input_colorspace,
                _pad: [0; 15],
            };

            let physical_size = (context.size.width, context.size.height);
            let has_images = !self.image_draws.is_empty();

            let ok = (|| {
                // Always draw the window bg fill. With the grid
                // owning per-cell bg, `padding_extend` in the grid
                // shader was initially used to extend edge cell
                // colors into the window margin — but that only
                // works for a single full-window grid. Once splits
                // exist, each panel's grid covers only its own rect,
                // so the window margin + gutters between panels
                // rely on this fullscreen fill again.
                if let Some(rgba) = bg_color {
                    if !Self::draw_bg_fill_metal(
                        brush,
                        render_encoder,
                        physical_size,
                        rgba,
                        &instance_buffer,
                        &mut instance_offset,
                        &globals,
                    ) {
                        return false;
                    }
                }
                if !Self::draw_background_image_metal(
                    &self.background_image_texture,
                    brush,
                    render_encoder,
                    physical_size,
                    &instance_buffer,
                    &mut instance_offset,
                    &globals,
                ) {
                    return false;
                }
                true
            })();
            // Three-bucket image z-ordering — mirrors ghostty's
            // `renderer/generic.zig:1640-1695`:
            //
            //   bg fill / image (already drawn above)
            //   ↓
            //   kitty z < BG_LIMIT  (BelowBg)
            //   ↓
            //   grid bg pass (per panel)
            //   ↓
            //   kitty BG_LIMIT ≤ z < 0  (BelowText)
            //   ↓
            //   grid text pass (per panel)
            //   ↓
            //   kitty z >= 0  (AboveText)
            //   ↓
            //   rich-text UI overlays (brush.render)
            //   ↓
            //   UI text pass (labels)
            //
            // The two grid passes run inside a single iteration loop
            // per panel — for multi-panel layouts the bg/text
            // ordering is per-grid (one panel's text doesn't paint
            // over another panel's bg image, and vice versa).
            let ok = ok
                && (|| {
                    if has_images
                        && !Self::draw_images_metal(
                            &self.image_draws,
                            &self.image_textures,
                            brush,
                            render_encoder,
                            ImageLayer::BelowBg,
                            &instance_buffer,
                            &mut instance_offset,
                            &globals,
                        )
                    {
                        return false;
                    }
                    for (grid, uniforms) in grids.iter_mut() {
                        grid.render_bg_metal(render_encoder, frame, uniforms);
                    }
                    if has_images
                        && !Self::draw_images_metal(
                            &self.image_draws,
                            &self.image_textures,
                            brush,
                            render_encoder,
                            ImageLayer::BelowText,
                            &instance_buffer,
                            &mut instance_offset,
                            &globals,
                        )
                    {
                        return false;
                    }
                    for (grid, uniforms) in grids.iter_mut() {
                        grid.render_text_metal(render_encoder, frame, uniforms);
                    }
                    if has_images
                        && !Self::draw_images_metal(
                            &self.image_draws,
                            &self.image_textures,
                            brush,
                            render_encoder,
                            ImageLayer::AboveText,
                            &instance_buffer,
                            &mut instance_offset,
                            &globals,
                        )
                    {
                        return false;
                    }
                    if !brush.render(
                        &self.instances,
                        &self.vertices,
                        &self.draw_cmds,
                        &self.images,
                        render_encoder,
                        context,
                        &instance_buffer,
                        &mut instance_offset,
                    ) {
                        return false;
                    }
                    // UI text pass. Lazy-init on the first frame with
                    // a Metal ctx; subsequent calls are no-ops. Runs
                    // after brush.render / above-text images so UI
                    // labels sit on top of everything else.
                    text.init_metal(&context.device, &context.command_queue);
                    text.render_metal(
                        render_encoder,
                        [context.size.width, context.size.height],
                    );
                    true
                })();

            if !ok {
                // Discard the partial encoder + command buffer (never
                // committed → no GPU work). Drop the buffer (it will not
                // be returned to the pool because `release` rejects it
                // after `grow` bumps the target size).
                render_encoder.end_encoding();
                drop(instance_buffer);
                let mut pool = brush.instance_buffer_pool.lock();
                let prev = pool.buffer_size();
                if !pool.grow() {
                    tracing::error!(
                        "instance buffer would exceed cap (current {} bytes); \
                         dropping frame",
                        prev
                    );
                    // No completion handler will fire to release the
                    // swap-chain permit we acquired above — release
                    // it here so the next frame can run.
                    crate::grid::metal::release_frame_permit(&self.metal_frame_permits);
                    return;
                }
                tracing::info!(
                    "instance buffer grew {} → {} bytes",
                    prev,
                    pool.buffer_size()
                );
                continue;
            }

            render_encoder.end_encoding();

            // Completion handler returns the buffer to the pool +
            // releases the swap-chain permit on GPU finish. The block
            // fires on a Metal-internal thread; the `FramePermits`
            // Arc inside the closure hops into its condvar to wake
            // any frame waiting on `acquire_frame_permit`. One Arc
            // clone per render — no per-grid duplication.
            let pool = brush.instance_buffer_pool.clone();
            let buffer_cell = StdCell::new(Some(instance_buffer));
            let permits = self.metal_frame_permits.clone();
            let block = ConcreteBlock::new(move |_cb: &metal::CommandBufferRef| {
                if let Some(b) = buffer_cell.take() {
                    pool.lock().release(b);
                }
                crate::grid::metal::release_frame_permit(&permits);
            })
            .copy();
            command_buffer.add_completed_handler(&block);

            command_buffer.present_drawable(&surface_texture.drawable);
            command_buffer.commit();
            return;
        }
    }

    /// Synchronously upload a background image into a Vulkan texture
    /// and descriptor set. Called from the prepare path when the
    /// user calls `Sugarloaf::set_background_image`. The
    /// submit-and-wait is acceptable for one-shot uploads
    /// (config-load time); kitty per-frame images take a different
    /// deferred path.
    #[cfg(target_os = "linux")]
    fn upload_background_image_vulkan(
        &mut self,
        context: &crate::context::Context,
        pixels: &BackgroundImagePixels,
    ) {
        let crate::context::ContextType::Vulkan(ctx) = &context.inner else {
            return;
        };
        let RendererType::Vulkan(brush) = &self.brush_type else {
            return;
        };
        let texture = vulkan::VulkanImageTexture::upload_rgba(
            ctx,
            &pixels.pixels,
            pixels.width,
            pixels.height,
            brush.image_texture_descriptor_set_layout,
            brush.image_sampler,
        );
        self.background_image_texture = Some(ImageTextureEntry {
            gpu: ImageTexture::Vulkan(texture),
            transmit_time: std::time::Instant::now(),
        });
    }

    /// Record sugarloaf's own draws inside the active dynamic-rendering
    /// pass that `Sugarloaf::render_vulkan` opens. Order:
    /// 1. Background image (full-screen quad).
    /// 2. BelowText image overlays (kitty / sixel placements with
    /// `dest_pos.z < 0`).
    /// 3. Rich-text quad pass — `quad()` / `rect()` calls + cell
    /// underline decorations (dashed/dotted/curly handled in
    /// `quad.frag.glsl`).
    /// 4. Non-quad geometry — `polygon()` / `line()` / `triangle()`
    /// / `arc()` calls (cursor underline shape, hint highlights).
    /// 5. AboveText image overlays.
    /// 6. Optional bootstrap rect (`RIO_VULKAN_BOOTSTRAP=1`).
    ///
    /// Glyph atlas sampling through this pipeline isn't ported —
    /// grid text + UI text overlay each own dedicated atlas
    /// pipelines, so the rich-text path doesn't need it.
    #[cfg(target_os = "linux")]
    pub fn render_vulkan(
        &mut self,
        cmd: ash::vk::CommandBuffer,
        frame: &crate::context::vulkan::VulkanFrame,
    ) {
        let viewport = [frame.extent.width as f32, frame.extent.height as f32];
        let slot = frame.slot;

        // Resolve image draws into (descriptor_set, instance) pairs
        // before the &mut brush borrow takes hold; the per-image
        // texture lookup needs an immutable borrow on
        // `self.image_textures` which would conflict with the
        // brush's `&mut self`.
        let below: Vec<(ash::vk::DescriptorSet, ImageInstance)> = self
            .image_draws
            .iter()
            .filter(|d| d.layer == ImageLayer::BelowText)
            .filter_map(|d| {
                let entry = self.image_textures.get(&d.image_id)?;
                if let ImageTexture::Vulkan(tex) = &entry.gpu {
                    Some((tex.descriptor_set, d.instance))
                } else {
                    None
                }
            })
            .collect();
        let above: Vec<(ash::vk::DescriptorSet, ImageInstance)> = self
            .image_draws
            .iter()
            .filter(|d| d.layer == ImageLayer::AboveText)
            .filter_map(|d| {
                let entry = self.image_textures.get(&d.image_id)?;
                if let ImageTexture::Vulkan(tex) = &entry.gpu {
                    Some((tex.descriptor_set, d.instance))
                } else {
                    None
                }
            })
            .collect();

        if let RendererType::Vulkan(brush) = &mut self.brush_type {
            if let Some(bg) = &self.background_image_texture {
                if let ImageTexture::Vulkan(tex) = &bg.gpu {
                    brush.render_background_image(
                        cmd,
                        slot,
                        viewport,
                        tex.descriptor_set,
                    );
                }
            }

            brush.render_image_overlays(cmd, slot, viewport, &below);
            brush.render_quads(cmd, slot, viewport, &self.instances);
            brush.render_geometry(cmd, slot, viewport, &self.vertices);
            brush.render_image_overlays(cmd, slot, viewport, &above);
            brush.draw_bootstrap(cmd);
        }
    }

    /// Vertices accumulated for the current frame (CPU rasterizer reads these).
    pub(crate) fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }

    /// Per-quad instances accumulated for the current frame. The CPU
    /// rasterizer walks these for `rect` / `quad` / `rounded_rect` /
    /// `underline` calls, which the immediate-mode UI uses for splits,
    /// panel borders, scrollbars, and dim overlays. The GPU paths
    /// upload them to a per-instance vertex buffer; we just iterate.
    pub(crate) fn instances(&self) -> &[crate::renderer::batch::QuadInstance] {
        &self.instances
    }

    /// Image cache for CPU rasterizer atlas sampling.
    pub(crate) fn image_cache(&self) -> &ImageCache {
        &self.images
    }

    pub fn resize(&mut self, context: &mut Context) {
        let transform = match &context.inner {
            #[cfg(feature = "wgpu")]
            ContextType::Wgpu(wgpu_ctx) => {
                orthographic_projection(wgpu_ctx.size.width, wgpu_ctx.size.height)
            }
            #[cfg(target_os = "macos")]
            ContextType::Metal(metal_ctx) => {
                orthographic_projection(metal_ctx.size.width, metal_ctx.size.height)
            }
            #[cfg(target_os = "linux")]
            ContextType::Vulkan(vulkan_ctx) => {
                orthographic_projection(vulkan_ctx.size.width, vulkan_ctx.size.height)
            }
            ContextType::Cpu(cpu_ctx) => {
                orthographic_projection(cpu_ctx.size.width, cpu_ctx.size.height)
            }
            #[cfg(not(feature = "wgpu"))]
            ContextType::_Phantom(_) => unreachable!(),
        };

        match &mut self.brush_type {
            #[cfg(feature = "wgpu")]
            RendererType::Wgpu(wgpu_brush) => {
                if transform != wgpu_brush.current_transform {
                    let queue = match &context.inner {
                        ContextType::Wgpu(wgpu_ctx) => &wgpu_ctx.queue,
                        _ => unreachable!(),
                    };

                    queue.write_buffer(
                        &wgpu_brush.transform,
                        0,
                        bytemuck::bytes_of(&transform),
                    );
                    wgpu_brush.current_transform = transform;
                }
            }
            #[cfg(target_os = "macos")]
            RendererType::Metal(_metal_brush) => {
                // No-op: Metal Globals (transform + colorspace) are
                // pushed inline per frame via `set_vertex_bytes` in
                // `MetalRenderer::render`, so there's nothing to upload
                // here on resize. The shader picks up the new viewport
                // on the next frame's `orthographic_projection` call.
                let _ = transform;
            }
            #[cfg(target_os = "linux")]
            RendererType::Vulkan(_vulkan_brush) => {
                // No-op: viewport + scissor are dynamic state set per
                // frame in `VulkanRenderer::render`. The swapchain
                // itself is rebuilt by `VulkanContext::resize`.
                let _ = transform;
            }
            RendererType::Cpu => {}
        }
    }
}

#[cfg(feature = "wgpu")]
impl WgpuRenderer {
    pub fn new(context: &WgpuContext) -> Self {
        let supported_vertex_buffer = 500;

        let current_transform =
            orthographic_projection(context.size.width, context.size.height);
        let transform =
            context
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(&current_transform),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });

        // Create pipeline layout
        let constant_bind_group_layout =
            context
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: None,
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: wgpu::BufferSize::new(mem::size_of::<
                                    [f32; 16],
                                >(
                                )
                                    as wgpu::BufferAddress),
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::VERTEX
                                | wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(
                                wgpu::SamplerBindingType::Filtering,
                            ),
                            count: None,
                        },
                    ],
                });

        let layout_bind_group_layout =
            context
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: None,
                    entries: &[
                        // Color texture (binding 0)
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: context.get_optimal_texture_sample_type(),
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // Mask texture (binding 1)
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float {
                                    filterable: true,
                                },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                    ],
                });

        let pipeline_layout =
            context
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[
                        &constant_bind_group_layout,
                        &layout_bind_group_layout,
                    ],
                    ..Default::default()
                });

        let sampler = context.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            lod_min_clamp: 0f32,
            lod_max_clamp: 0f32,
            ..Default::default()
        });

        let constant_bind_group =
            context
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: &constant_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(
                                wgpu::BufferBinding {
                                    buffer: &transform,
                                    offset: 0,
                                    size: None,
                                },
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                    label: Some("rich_text::constant_bind_group"),
                });

        // Create initial layout bind group (will be updated when textures change)
        let layout_bind_group =
            context
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: &layout_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(
                                &context
                                    .device
                                    .create_texture(&wgpu::TextureDescriptor {
                                        label: Some("placeholder_color"),
                                        size: wgpu::Extent3d {
                                            width: 1,
                                            height: 1,
                                            depth_or_array_layers: 1,
                                        },
                                        mip_level_count: 1,
                                        sample_count: 1,
                                        dimension: wgpu::TextureDimension::D2,
                                        format: wgpu::TextureFormat::Rgba8Unorm,
                                        usage: wgpu::TextureUsages::TEXTURE_BINDING,
                                        view_formats: &[],
                                    })
                                    .create_view(&wgpu::TextureViewDescriptor::default()),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(
                                &context
                                    .device
                                    .create_texture(&wgpu::TextureDescriptor {
                                        label: Some("placeholder_mask"),
                                        size: wgpu::Extent3d {
                                            width: 1,
                                            height: 1,
                                            depth_or_array_layers: 1,
                                        },
                                        mip_level_count: 1,
                                        sample_count: 1,
                                        dimension: wgpu::TextureDimension::D2,
                                        format: wgpu::TextureFormat::R8Unorm,
                                        usage: wgpu::TextureUsages::TEXTURE_BINDING,
                                        view_formats: &[],
                                    })
                                    .create_view(&wgpu::TextureViewDescriptor::default()),
                            ),
                        },
                    ],
                    label: Some("rich_text::layout_bind_group"),
                });

        let shader_source = include_str!("renderer.wgsl");

        let shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
            });

        let pipeline =
            context
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    cache: None,
                    label: None,
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: mem::size_of::<Vertex>() as u64,
                            // https://docs.rs/wgpu/latest/wgpu/enum.VertexStepMode.html
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array!(
                                0 => Float32x3,  // pos
                                1 => Float32x4,  // color (background)
                                2 => Float32x2,  // uv
                                3 => Sint32x2,   // layers
                                4 => Float32x4,  // corner_radii
                                5 => Float32x2,  // rect_size
                                6 => Sint32,     // underline_style
                                7 => Float32x4,  // clip_rect
                            ),
                        }],
                    },
                    fragment: Some(wgpu::FragmentState {
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        module: &shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: context.format,
                            blend: BLEND,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        unclipped_depth: false,
                        conservative: false,
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview_mask: None,
                });

        // Instanced pipeline (vs_instanced + fs_main, instance step mode)
        let instanced_pipeline =
            context
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    cache: None,
                    label: Some("rich_text::instanced pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        module: &shader,
                        entry_point: Some("vs_instanced"),
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: mem::size_of::<batch::QuadInstance>() as u64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &wgpu::vertex_attr_array!(
                                0 => Float32x3,  // pos
                                1 => Float32x4,  // color
                                2 => Float32x4,  // uv_rect
                                3 => Sint32x2,   // layers
                                4 => Float32x2,  // size
                                5 => Float32x4,  // corner_radii
                                6 => Sint32,     // underline_style
                                7 => Float32x4,  // clip_rect
                            ),
                        }],
                    },
                    fragment: Some(wgpu::FragmentState {
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        module: &shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: context.format,
                            blend: BLEND,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleStrip,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        unclipped_depth: false,
                        conservative: false,
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview_mask: None,
                });

        let vertex_buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rich_text::Vertices Buffer"),
            size: mem::size_of::<Vertex>() as u64 * supported_vertex_buffer as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let supported_instance_buffer = 20_000usize;
        let instance_buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rich_text::Instance Buffer"),
            size: mem::size_of::<batch::QuadInstance>() as u64
                * supported_instance_buffer as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let image_shader_source = include_str!("image.wgsl");
        let image_shader =
            context
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("image shader"),
                    source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(image_shader_source)),
                });

        let image_bind_group_layout =
            context
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("image texture layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float {
                                filterable: true,
                            },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    }],
                });

        let image_pipeline_layout =
            context
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("image pipeline layout"),
                    bind_group_layouts: &[
                        &constant_bind_group_layout, // group 0: transform + sampler
                        &image_bind_group_layout,    // group 1: image texture
                    ],
                    immediate_size: 0,
                });

        // Premultiplied alpha blend for images
        let image_blend = Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        });

        let image_pipeline =
            context
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    cache: None,
                    label: Some("image pipeline"),
                    layout: Some(&image_pipeline_layout),
                    vertex: wgpu::VertexState {
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        module: &image_shader,
                        entry_point: Some("vs_main"),
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: mem::size_of::<ImageInstance>() as u64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &wgpu::vertex_attr_array!(
                                0 => Float32x2, // dest_pos
                                1 => Float32x2, // dest_size
                                2 => Float32x4, // source_rect
                            ),
                        }],
                    },
                    fragment: Some(wgpu::FragmentState {
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        module: &image_shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: context.format,
                            blend: image_blend,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleStrip,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        unclipped_depth: false,
                        conservative: false,
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview_mask: None,
                });

        let image_vertex_buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image instance buffer"),
            // 1024 max — see `MAX_IMAGE_INSTANCES` comment in render path.
            size: mem::size_of::<ImageInstance>() as u64 * 1024,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let background_image_vertex_buffer =
            context.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("background image instance buffer"),
                size: mem::size_of::<ImageInstance>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

        WgpuRenderer {
            layout_bind_group,
            layout_bind_group_layout,
            constant_bind_group,
            transform,
            pipeline,
            instanced_pipeline,
            vertex_buffer,
            instance_buffer,
            supported_vertex_buffer,
            supported_instance_buffer,
            current_transform,
            image_pipeline,
            image_bind_group_layout,
            image_vertex_buffer,
            background_image_vertex_buffer,
        }
    }

    #[inline]
    pub fn render<'pass>(
        &'pass mut self,
        ctx: &mut WgpuContext,
        instances: &[batch::QuadInstance],
        vertices: &[Vertex],
        rpass: &mut wgpu::RenderPass<'pass>,
    ) {
        if instances.is_empty() && vertices.is_empty() {
            return;
        }

        let queue = &mut ctx.queue;

        // Upload instance buffer
        if !instances.is_empty() {
            if instances.len() > self.supported_instance_buffer {
                self.instance_buffer.destroy();
                self.supported_instance_buffer = (instances.len() as f32 * 1.25) as usize;
                self.instance_buffer =
                    ctx.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("rich_text::Instance Buffer (resized)"),
                        size: mem::size_of::<batch::QuadInstance>() as u64
                            * self.supported_instance_buffer as u64,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
            }
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
        }

        // Upload vertex buffer
        if !vertices.is_empty() {
            if vertices.len() > self.supported_vertex_buffer {
                self.vertex_buffer.destroy();
                self.supported_vertex_buffer = (vertices.len() as f32 * 1.25) as usize;
                self.vertex_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("rich_text::Vertices Buffer (resized)"),
                    size: mem::size_of::<Vertex>() as u64
                        * self.supported_vertex_buffer as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
            }
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(vertices));
        }

        rpass.set_bind_group(0, &self.constant_bind_group, &[]);
        rpass.set_bind_group(1, &self.layout_bind_group, &[]);
        rpass.set_pipeline(&self.pipeline);
        rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        let vertex_count = vertices.len() as u32;
        rpass.draw(0..vertex_count, 0..1);
    }

    #[inline]
    pub fn render_range(
        &mut self,
        ctx: &mut WgpuContext,
        vertices: &[Vertex],
        rpass: &mut wgpu::RenderPass,
        range: std::ops::Range<usize>,
    ) {
        if range.is_empty() {
            return;
        }

        let queue = &mut ctx.queue;

        // Ensure buffer is large enough
        if vertices.len() > self.supported_vertex_buffer {
            self.vertex_buffer.destroy();
            self.supported_vertex_buffer = (vertices.len() as f32 * 1.25) as usize;
            self.vertex_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sugarloaf::rich_text::Pipeline vertices"),
                size: mem::size_of::<Vertex>() as u64
                    * self.supported_vertex_buffer as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        // Write all vertices to buffer (we need the full buffer for correct indexing)
        let vertices_bytes: &[u8] = bytemuck::cast_slice(vertices);
        if !vertices_bytes.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, vertices_bytes);
        }

        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.constant_bind_group, &[]);
        rpass.set_bind_group(1, &self.layout_bind_group, &[]);
        rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        // Draw only the specified range
        rpass.draw(range.start as u32..range.end as u32, 0..1);
    }

    pub fn update_bind_group(
        &mut self,
        ctx: &WgpuContext,
        color_view: &wgpu::TextureView,
        mask_view: &wgpu::TextureView,
    ) {
        // Always update bind group since different batches need different textures
        self.layout_bind_group =
            ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &self.layout_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(color_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(mask_view),
                    },
                ],
                label: Some("rich_text::Pipeline uniforms"),
            });
    }
}

#[cfg(test)]
mod rect_positioning_tests {
    // ... existing tests remain the same ...
    #[derive(Debug)]
    struct GlyphRect {
        #[allow(unused)]
        pub x: f32,
        #[allow(unused)]
        pub y: f32,
        #[allow(unused)]
        pub width: f32,
        #[allow(unused)]
        pub height: f32,
        #[allow(unused)]
        pub baseline_y: f32,
        pub glyph_center_x: f32,
        pub glyph_center_y: f32,
    }

    #[derive(Debug)]
    struct LineRect {
        #[allow(unused)]
        pub x: f32,
        pub y: f32,
        #[allow(unused)]
        pub width: f32,
        #[allow(unused)]
        pub height: f32,
        #[allow(unused)]
        pub baseline_y: f32,
    }

    #[test]
    fn test_glyph_rect_positioning_and_centering() {
        // Test parameters
        let line_height = 20.0;
        let char_width = 8.0;
        let ascent = 12.0;
        let descent = 4.0;
        let _leading = 0.0;

        // Expected calculations (matching our current implementation)
        let padding_top = (line_height - ascent - descent) / 2.0; // (20 - 12 - 4) / 2 = 2.0
        let expected_baseline_y = 0.0 + padding_top + ascent; // 0 + 2 + 12 = 14.0

        // Create line rect
        let line_rect = LineRect {
            x: 0.0,
            y: 0.0,
            width: char_width,
            height: line_height,
            baseline_y: expected_baseline_y,
        };

        // Expected glyph rect (should be centered within line rect)
        let expected_glyph_rect = GlyphRect {
            x: 0.0,
            y: 0.0,
            width: char_width,
            height: line_height,
            baseline_y: expected_baseline_y,
            glyph_center_x: char_width / 2.0,  // 4.0
            glyph_center_y: line_height / 2.0, // 10.0
        };

        // Verify baseline is positioned correctly within the line rect
        assert!(
            expected_baseline_y > line_rect.y,
            "Baseline should be below line top"
        );
        assert!(
            expected_baseline_y < line_rect.y + line_rect.height,
            "Baseline should be above line bottom"
        );

        // Verify glyph center is in the middle of the rect
        assert_eq!(
            expected_glyph_rect.glyph_center_x,
            char_width / 2.0,
            "Glyph should be horizontally centered"
        );
        assert_eq!(
            expected_glyph_rect.glyph_center_y,
            line_height / 2.0,
            "Glyph should be vertically centered"
        );

        // Verify baseline relationship to glyph center
        let baseline_offset_from_center =
            expected_baseline_y - expected_glyph_rect.glyph_center_y;

        // The baseline should be slightly above center for typical fonts
        // With ascent=12, descent=4, the baseline should be at 14.0, center at 10.0
        // So baseline is 4.0 units above center, which makes sense
        assert_eq!(
            baseline_offset_from_center, 4.0,
            "Baseline should be 4.0 units above glyph center"
        );
    }

    #[test]
    fn test_graphic_positioning_with_offsets() {
        // Test that graphics are positioned correctly based on cell offsets
        // This simulates the logic: gx = run_x - offset_x, gy = py - offset_y

        // Cell at position (100, 200) contains a graphic with offset (20, 30)
        let run_x = 100.0;
        let py = 200.0;
        let offset_x = 20;
        let offset_y = 30;

        // Calculate graphic position
        let gx = run_x - offset_x as f32;
        let gy = py - offset_y as f32;

        // The graphic's top-left should be at (80, 170)
        // because we back-calculate from the cell's position
        assert_eq!(gx, 80.0, "Graphic x should account for offset_x");
        assert_eq!(gy, 170.0, "Graphic y should account for offset_y");

        // Verify origin cell (offset 0,0) at same position
        let origin_run_x = 80.0;
        let origin_py = 170.0;
        let origin_offset_x = 0;
        let origin_offset_y = 0;

        let origin_gx = origin_run_x - origin_offset_x as f32;
        let origin_gy = origin_py - origin_offset_y as f32;

        // Both cells should calculate the same graphic position
        assert_eq!(
            gx, origin_gx,
            "Graphic position should be same from any cell"
        );
        assert_eq!(
            gy, origin_gy,
            "Graphic position should be same from any cell"
        );
    }

    #[test]
    fn test_graphic_deduplication() {
        // Test that the same graphic ID is only rendered once per frame
        use crate::GraphicId;
        use std::collections::HashSet;

        let mut last_rendered_graphic: HashSet<GraphicId> = HashSet::new();

        let graphic_id = GraphicId::new(42);

        // First cell with this graphic - should render
        assert!(
            !last_rendered_graphic.contains(&graphic_id),
            "First occurrence should not be in set"
        );
        last_rendered_graphic.insert(graphic_id);

        // Second cell with same graphic - should NOT render
        assert!(
            last_rendered_graphic.contains(&graphic_id),
            "Second occurrence should be in set, preventing duplicate render"
        );

        // Clear for next frame
        last_rendered_graphic.clear();
        assert!(
            !last_rendered_graphic.contains(&graphic_id),
            "After clear, graphic should be renderable again"
        );
    }
}
