// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Native Vulkan renderer (Phase 1: clear + bootstrap pipeline).
//!
//! Mirrors `MetalRenderer` in shape: holds compiled pipelines + per-frame
//! resources, exposes a `render` method that records draw commands into a
//! caller-supplied command buffer. Phase 1 only constructs the bootstrap
//! pipeline (centered debug rect) and clears the swapchain image. The
//! rich-text / image / grid pipelines come in later phases — see the
//! plan in `context/vulkan.rs`.
//!
//! The bootstrap rect is invisible by default; set
//! `RIO_VULKAN_BOOTSTRAP=1` in the environment to make it visible (a
//! centered magenta quad). The pipeline is always *constructed* either
//! way so any SPIR-V / pipeline-state validation errors surface
//! immediately at sugarloaf startup, not lazily on first frame with the
//! flag enabled.

use ash::vk;
use std::sync::Arc;

use crate::context::vulkan::{
    allocate_host_visible_buffer_raw, VkShared, VulkanBuffer, VulkanContext, VulkanFrame,
    VulkanImage, FRAMES_IN_FLIGHT,
};
use crate::renderer::batch::{QuadInstance, Vertex};
use crate::renderer::ImageInstance;

/// Compiled SPIR-V. Generated at build time from the matching
/// `.glsl` source by `sugarloaf/build.rs` (which shells out to
/// `glslc` or `glslangValidator`) and dropped into `OUT_DIR`.
/// Edit the `.glsl` file and rebuild — there's no manual recompile
/// step. Source files live in `sugarloaf/src/renderer/`.
const CLEAR_VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/clear.vert.spv"));
const CLEAR_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/clear.frag.spv"));
const QUAD_VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/quad.vert.spv"));
const QUAD_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/quad.frag.spv"));
const GEOMETRY_VERT_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/geometry.vert.spv"));
const IMAGE_VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/image.vert.spv"));
const IMAGE_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/image.frag.spv"));

/// `std140`-padded `Globals` uniform — `mat4 transform` (64 B) +
/// `uint input_colorspace` + 12 B padding to round up to 80 B (a
/// multiple of 16). Mirrors the `Globals` block in
/// `quad.{vert,frag}.glsl`.
#[repr(C)]
#[derive(Copy, Clone)]
struct QuadGlobals {
    transform: [f32; 16],
    input_colorspace: u32,
    _pad: [u32; 3],
}

pub struct VulkanRenderer {
    /// Bootstrap pipeline — proves the SPIR-V → `vk::ShaderModule` →
    /// `vk::Pipeline` chain works end-to-end. Bound only when
    /// `bootstrap_visible` is true; otherwise the frame is just a
    /// clear-and-present.
    bootstrap_pipeline: vk::Pipeline,
    bootstrap_layout: vk::PipelineLayout,
    bootstrap_visible: bool,
    /// Shared device handle. The Arc keeps `vkDestroyDevice` from
    /// running until every per-resource holder (buffers, images,
    /// other renderers, atlases, the per-panel grid renderers in
    /// `Screen.grids`) is dropped, so this renderer's `Drop` can
    /// safely tear down pipelines regardless of struct field-order
    /// elsewhere — fixes the cross-struct ordering trap behind
    /// raphamorim/rio#1568. Also lets `render_quads` allocate from
    /// `&mut self` without needing a live `&VulkanContext` borrow.
    /// See `VkShared`.
    shared: Arc<VkShared>,
    /// Set once at construction from the configured `Colorspace`.
    /// Mirrors `MetalRenderer::input_colorspace`. Value is `0 = sRGB`,
    /// `1 = DisplayP3`, `2 = Rec.2020`.
    input_colorspace: u32,

    // ---------- quad pipeline (rich-text rect/rounded-rect path) ----------
    quad_pipeline: vk::Pipeline,
    quad_pipeline_layout: vk::PipelineLayout,
    quad_descriptor_pool: vk::DescriptorPool,
    quad_descriptor_set_layout: vk::DescriptorSetLayout,
    quad_descriptor_sets: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    /// Per-slot `Globals` uniform buffer.
    quad_uniform_buffers: [VulkanBuffer; FRAMES_IN_FLIGHT],
    /// Per-slot per-instance `QuadInstance` vertex buffer ring.
    /// Allocated lazily, grown on demand.
    quad_instance_buffers: [Option<VulkanBuffer>; FRAMES_IN_FLIGHT],
    quad_instance_capacity: [usize; FRAMES_IN_FLIGHT],

    // ---------- non-quad geometry pipeline ----------
    /// Renders `Vertex`-supplied geometry (`polygon()` / `line()` /
    /// `triangle()` / `arc()` calls). Shares the quad pipeline's
    /// descriptor set layout + uniform buffers + fragment shader;
    /// only the vertex shader and topology differ.
    geometry_pipeline: vk::Pipeline,
    /// Per-slot per-vertex `Vertex` buffer ring.
    geometry_vertex_buffers: [Option<VulkanBuffer>; FRAMES_IN_FLIGHT],
    geometry_vertex_capacity: [usize; FRAMES_IN_FLIGHT],

    // ---------- image pipeline ----------
    /// Set 0 = `Globals` uniform (per slot, shared with quad pipeline
    /// shape but separate buffers — image's `Globals` doesn't change
    /// per draw and could be coalesced; for now just duplicate to
    /// keep the descriptor sets simple).
    /// Set 1 = single combined image+sampler. Owned per
    /// `VulkanImageTexture` so each image carries its own descriptor.
    image_pipeline: vk::Pipeline,
    image_pipeline_layout: vk::PipelineLayout,
    image_uniform_descriptor_set_layout: vk::DescriptorSetLayout,
    /// Public so per-image `VulkanImageTexture` instances can allocate
    /// their own descriptor sets at upload time.
    pub image_texture_descriptor_set_layout: vk::DescriptorSetLayout,
    image_uniform_descriptor_pool: vk::DescriptorPool,
    image_uniform_descriptor_sets: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    image_uniform_buffers: [VulkanBuffer; FRAMES_IN_FLIGHT],
    /// Per-slot per-instance `ImageInstance` vertex buffer ring (for
    /// kitty/sixel images). The bg image gets its own dedicated
    /// 1-instance vertex buffer (`image_bg_vertex_buffers`).
    image_instance_buffers: [Option<VulkanBuffer>; FRAMES_IN_FLIGHT],
    image_instance_capacity: [usize; FRAMES_IN_FLIGHT],
    /// Dedicated single-instance vertex buffer for the background
    /// image (one per slot). Kept separate so the bg draw never
    /// collides with kitty placement slots — same pattern as the
    /// wgpu `background_image_vertex_buffer`.
    image_bg_vertex_buffers: [VulkanBuffer; FRAMES_IN_FLIGHT],
    /// Sampler shared by every image draw. Linear filtering for
    /// background images (smooth scaling); kitty graphics also looks
    /// fine with linear.
    pub image_sampler: vk::Sampler,
}

impl VulkanRenderer {
    pub fn new(ctx: &VulkanContext, colorspace: crate::sugarloaf::Colorspace) -> Self {
        let shared = ctx.shared().clone();
        let device = &shared.raw;
        let color_format = ctx.swapchain_format();
        let input_colorspace = match colorspace {
            crate::sugarloaf::Colorspace::Srgb => 0u32,
            crate::sugarloaf::Colorspace::DisplayP3 => 1u32,
            crate::sugarloaf::Colorspace::Rec2020 => 2u32,
        };

        let vert_module = create_shader_module(&device, CLEAR_VERT_SPV);
        let frag_module = create_shader_module(&device, CLEAR_FRAG_SPV);

        // Push constant: vec4 color, fragment stage, offset 0 — matches
        // `layout(push_constant) uniform PC { vec4 color; }` in
        // `clear.frag.glsl`.
        let push_constant_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(std::mem::size_of::<[f32; 4]>() as u32);

        let push_constant_ranges = [push_constant_range];
        let layout_info = vk::PipelineLayoutCreateInfo::default()
            .push_constant_ranges(&push_constant_ranges);
        let bootstrap_layout = unsafe {
            device
                .create_pipeline_layout(&layout_info, None)
                .expect("create_pipeline_layout(bootstrap)")
        };

        let pipeline_cache = ctx.pipeline_cache();
        let bootstrap_pipeline = build_bootstrap_pipeline(
            &device,
            pipeline_cache,
            bootstrap_layout,
            vert_module,
            frag_module,
            color_format,
        );

        // Shader modules are no longer needed once the pipeline is built.
        // The compiled SPIR-V is baked into the pipeline state.
        unsafe {
            device.destroy_shader_module(vert_module, None);
            device.destroy_shader_module(frag_module, None);
        }

        let bootstrap_visible = std::env::var_os("RIO_VULKAN_BOOTSTRAP")
            .map(|v| v != "0" && !v.is_empty())
            .unwrap_or(false);

        if bootstrap_visible {
            tracing::info!("Vulkan bootstrap rect enabled (RIO_VULKAN_BOOTSTRAP set)");
        }

        // Quad pipeline construction.
        let quad_uniform_buffers = std::array::from_fn(|_| {
            ctx.allocate_host_visible_buffer(
                std::mem::size_of::<QuadGlobals>() as u64,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            )
        });
        let quad_descriptor_set_layout = create_quad_descriptor_set_layout(&device);
        let quad_descriptor_pool = create_quad_descriptor_pool(&device);
        let quad_descriptor_sets = allocate_quad_descriptor_sets(
            &device,
            quad_descriptor_pool,
            quad_descriptor_set_layout,
        );
        for slot in 0..FRAMES_IN_FLIGHT {
            update_quad_descriptor_set(
                &device,
                quad_descriptor_sets[slot],
                &quad_uniform_buffers[slot],
            );
        }
        let quad_pipeline_layout =
            create_quad_pipeline_layout(&device, quad_descriptor_set_layout);
        let quad_pipeline = build_quad_pipeline(
            &device,
            pipeline_cache,
            quad_pipeline_layout,
            color_format,
        );
        // Geometry pipeline shares quad's descriptor set layout +
        // uniform buffers + fragment shader; just a different
        // vertex shader + input layout + topology.
        let geometry_pipeline = build_geometry_pipeline(
            &device,
            pipeline_cache,
            quad_pipeline_layout,
            color_format,
        );

        // Image pipeline construction.
        let image_uniform_descriptor_set_layout =
            create_image_uniform_descriptor_set_layout(&device);
        let image_texture_descriptor_set_layout =
            create_image_texture_descriptor_set_layout(&device);
        let image_uniform_descriptor_pool = create_image_uniform_descriptor_pool(&device);
        let image_uniform_descriptor_sets = allocate_quad_descriptor_sets(
            &device,
            image_uniform_descriptor_pool,
            image_uniform_descriptor_set_layout,
        );
        let image_uniform_buffers = std::array::from_fn(|_| {
            ctx.allocate_host_visible_buffer(
                std::mem::size_of::<QuadGlobals>() as u64,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            )
        });
        for slot in 0..FRAMES_IN_FLIGHT {
            update_quad_descriptor_set(
                &device,
                image_uniform_descriptor_sets[slot],
                &image_uniform_buffers[slot],
            );
        }
        let image_pipeline_layout = create_image_pipeline_layout(
            &device,
            image_uniform_descriptor_set_layout,
            image_texture_descriptor_set_layout,
        );
        let image_pipeline = build_image_pipeline(
            &device,
            pipeline_cache,
            image_pipeline_layout,
            color_format,
        );

        let image_bg_vertex_buffers = std::array::from_fn(|_| {
            ctx.allocate_host_visible_buffer(
                std::mem::size_of::<ImageInstance>() as u64,
                vk::BufferUsageFlags::VERTEX_BUFFER,
            )
        });
        let image_sampler = create_image_sampler(&device);

        Self {
            bootstrap_pipeline,
            bootstrap_layout,
            bootstrap_visible,
            shared,
            input_colorspace,
            quad_pipeline,
            quad_pipeline_layout,
            quad_descriptor_pool,
            quad_descriptor_set_layout,
            quad_descriptor_sets,
            quad_uniform_buffers,
            quad_instance_buffers: std::array::from_fn(|_| None),
            quad_instance_capacity: [0; FRAMES_IN_FLIGHT],
            geometry_pipeline,
            geometry_vertex_buffers: std::array::from_fn(|_| None),
            geometry_vertex_capacity: [0; FRAMES_IN_FLIGHT],
            image_pipeline,
            image_pipeline_layout,
            image_uniform_descriptor_set_layout,
            image_texture_descriptor_set_layout,
            image_uniform_descriptor_pool,
            image_uniform_descriptor_sets,
            image_uniform_buffers,
            image_instance_buffers: std::array::from_fn(|_| None),
            image_instance_capacity: [0; FRAMES_IN_FLIGHT],
            image_bg_vertex_buffers,
            image_sampler,
        }
    }

    /// Record the non-quad geometry pass into `cmd`. Caller has
    /// already opened the dynamic-rendering pass and set
    /// viewport/scissor. No-op when `vertices` is empty.
    ///
    /// Reuses `quad_pipeline_layout` + per-slot quad uniform set —
    /// the Globals binding is the same shape; we already wrote it
    /// in `render_quads` if there were quads this frame. If quads
    /// were skipped, we have to upload here too. (Doing it
    /// unconditionally keeps the call sites independent.)
    pub fn render_geometry(
        &mut self,
        cmd: vk::CommandBuffer,
        slot: usize,
        viewport: [f32; 2],
        vertices: &[Vertex],
    ) {
        if vertices.is_empty() {
            return;
        }
        debug_assert!(slot < FRAMES_IN_FLIGHT);

        // Make sure the per-slot quad uniform buffer (shared with
        // the geometry pipeline) holds the current frame's transform
        // even when no quads were drawn.
        let transform =
            crate::components::core::orthographic_projection(viewport[0], viewport[1]);
        let globals = QuadGlobals {
            transform,
            input_colorspace: self.input_colorspace,
            _pad: [0; 3],
        };
        unsafe {
            let dst = self.quad_uniform_buffers[slot].as_mut_ptr() as *mut QuadGlobals;
            std::ptr::write(dst, globals);
        }

        // Grow per-slot vertex buffer if needed.
        let vertex_count = vertices.len();
        let needed_bytes = std::mem::size_of_val(vertices);
        if vertex_count > self.geometry_vertex_capacity[slot] {
            let new_cap = vertex_count.next_power_of_two().max(256);
            self.geometry_vertex_buffers[slot] = Some(allocate_host_visible_buffer_raw(
                &self.shared,
                (new_cap * std::mem::size_of::<Vertex>()) as u64,
                vk::BufferUsageFlags::VERTEX_BUFFER,
            ));
            self.geometry_vertex_capacity[slot] = new_cap;
        }
        let vertex_buf = self.geometry_vertex_buffers[slot].as_ref().unwrap();
        unsafe {
            std::ptr::copy_nonoverlapping(
                vertices.as_ptr() as *const u8,
                vertex_buf.as_mut_ptr(),
                needed_bytes,
            );
        }

        unsafe {
            self.shared.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.geometry_pipeline,
            );
            self.shared.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.quad_pipeline_layout,
                0,
                &[self.quad_descriptor_sets[slot]],
                &[],
            );
            self.shared
                .cmd_bind_vertex_buffers(cmd, 0, &[vertex_buf.handle()], &[0]);
            // Caller-provided vertices are TRIANGLE_LIST — the emit
            // path tessellates polygons / arcs / lines into
            // triangles before pushing.
            self.shared.cmd_draw(cmd, vertex_count as u32, 1, 0, 0);
        }
    }

    /// Draw a batch of image overlays (kitty / sixel placements) for
    /// one layer (BelowText or AboveText). Each `(descriptor_set,
    /// instance)` pair is one image placement — caller has resolved
    /// the per-image descriptor set ahead of time. Writes all
    /// instances into the per-slot ring buffer in order, then issues
    /// one `cmd_draw(4, 1, ...)` per placement, binding the buffer
    /// at the matching byte offset.
    ///
    /// Pre-binds the uniform set + image pipeline once, then loops
    /// per-image just to update the texture descriptor + vertex
    /// buffer offset.
    pub fn render_image_overlays(
        &mut self,
        cmd: vk::CommandBuffer,
        slot: usize,
        viewport: [f32; 2],
        draws: &[(vk::DescriptorSet, ImageInstance)],
    ) {
        if draws.is_empty() {
            return;
        }
        debug_assert!(slot < FRAMES_IN_FLIGHT);

        // Update the (shared) image uniform with the current
        // viewport's transform.
        let transform =
            crate::components::core::orthographic_projection(viewport[0], viewport[1]);
        let globals = QuadGlobals {
            transform,
            input_colorspace: self.input_colorspace,
            _pad: [0; 3],
        };
        unsafe {
            let dst = self.image_uniform_buffers[slot].as_mut_ptr() as *mut QuadGlobals;
            std::ptr::write(dst, globals);
        }

        // Grow the per-slot kitty/sixel instance buffer if needed.
        let count = draws.len();
        let stride = std::mem::size_of::<ImageInstance>();
        let needed_bytes = count * stride;
        if count > self.image_instance_capacity[slot] {
            let new_cap = count.next_power_of_two().max(16);
            self.image_instance_buffers[slot] = Some(allocate_host_visible_buffer_raw(
                &self.shared,
                (new_cap * stride) as u64,
                vk::BufferUsageFlags::VERTEX_BUFFER,
            ));
            self.image_instance_capacity[slot] = new_cap;
        }
        let buf = self.image_instance_buffers[slot].as_ref().unwrap();
        unsafe {
            // Write instances contiguously, ordered the same as `draws`.
            let dst = buf.as_mut_ptr() as *mut ImageInstance;
            for (i, (_set, inst)) in draws.iter().enumerate() {
                std::ptr::write(dst.add(i), *inst);
            }
        }

        unsafe {
            self.shared.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.image_pipeline,
            );
            // Set 0 (uniform) is constant across draws — bind once.
            self.shared.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.image_pipeline_layout,
                0,
                &[self.image_uniform_descriptor_sets[slot]],
                &[],
            );
            for (i, (texture_set, _inst)) in draws.iter().enumerate() {
                // Set 1 (texture) changes per-draw — rebind.
                self.shared.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.image_pipeline_layout,
                    1,
                    &[*texture_set],
                    &[],
                );
                let byte_offset = (i * stride) as u64;
                self.shared.cmd_bind_vertex_buffers(
                    cmd,
                    0,
                    &[buf.handle()],
                    &[byte_offset],
                );
                let _ = needed_bytes; // shut up unused warning
                self.shared.cmd_draw(cmd, 4, 1, 0, 0);
            }
        }
    }

    /// Draw the background image, if any. Caller passes the per-image
    /// descriptor set (allocated at upload time via
    /// `VulkanImageTexture::new`). Writes a single ImageInstance with
    /// `dest_pos = [0, 0]` and `dest_size = viewport` so the image
    /// covers the whole window.
    pub fn render_background_image(
        &mut self,
        cmd: vk::CommandBuffer,
        slot: usize,
        viewport: [f32; 2],
        image_texture_descriptor_set: vk::DescriptorSet,
    ) {
        debug_assert!(slot < FRAMES_IN_FLIGHT);

        // Update uniforms (transform).
        let transform =
            crate::components::core::orthographic_projection(viewport[0], viewport[1]);
        let globals = QuadGlobals {
            transform,
            input_colorspace: self.input_colorspace,
            _pad: [0; 3],
        };
        unsafe {
            let dst = self.image_uniform_buffers[slot].as_mut_ptr() as *mut QuadGlobals;
            std::ptr::write(dst, globals);
        }

        // Build a single full-screen ImageInstance.
        let instance = ImageInstance {
            dest_pos: [0.0, 0.0],
            dest_size: viewport,
            source_rect: [0.0, 0.0, 1.0, 1.0],
        };
        unsafe {
            let dst =
                self.image_bg_vertex_buffers[slot].as_mut_ptr() as *mut ImageInstance;
            std::ptr::write(dst, instance);
        }

        unsafe {
            self.shared.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.image_pipeline,
            );
            self.shared.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.image_pipeline_layout,
                0,
                &[
                    self.image_uniform_descriptor_sets[slot],
                    image_texture_descriptor_set,
                ],
                &[],
            );
            self.shared.cmd_bind_vertex_buffers(
                cmd,
                0,
                &[self.image_bg_vertex_buffers[slot].handle()],
                &[0],
            );
            self.shared.cmd_draw(cmd, 4, 1, 0, 0);
        }
    }

    /// Record the rich-text quad pass into `cmd`. Caller has already
    /// opened the dynamic-rendering pass and set viewport/scissor.
    /// No-op when `instances` is empty.
    pub fn render_quads(
        &mut self,
        cmd: vk::CommandBuffer,
        slot: usize,
        viewport: [f32; 2],
        instances: &[QuadInstance],
    ) {
        if instances.is_empty() {
            return;
        }
        debug_assert!(slot < FRAMES_IN_FLIGHT);

        // Update uniforms.
        let transform =
            crate::components::core::orthographic_projection(viewport[0], viewport[1]);
        let globals = QuadGlobals {
            transform,
            input_colorspace: self.input_colorspace,
            _pad: [0; 3],
        };
        unsafe {
            let dst = self.quad_uniform_buffers[slot].as_mut_ptr() as *mut QuadGlobals;
            std::ptr::write(dst, globals);
        }

        // Grow per-slot instance buffer if needed.
        let instance_count = instances.len();
        let needed_bytes = std::mem::size_of_val(instances);
        if instance_count > self.quad_instance_capacity[slot] {
            let new_cap = instance_count.next_power_of_two().max(256);
            self.quad_instance_buffers[slot] = Some(allocate_host_visible_buffer_raw(
                &self.shared,
                (new_cap * std::mem::size_of::<QuadInstance>()) as u64,
                vk::BufferUsageFlags::VERTEX_BUFFER,
            ));
            self.quad_instance_capacity[slot] = new_cap;
        }
        let instance_buf = self.quad_instance_buffers[slot].as_ref().unwrap();
        unsafe {
            std::ptr::copy_nonoverlapping(
                instances.as_ptr() as *const u8,
                instance_buf.as_mut_ptr(),
                needed_bytes,
            );
        }

        unsafe {
            self.shared.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.quad_pipeline,
            );
            self.shared.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.quad_pipeline_layout,
                0,
                &[self.quad_descriptor_sets[slot]],
                &[],
            );
            self.shared
                .cmd_bind_vertex_buffers(cmd, 0, &[instance_buf.handle()], &[0]);
            // 4 vertices per instance (TRIANGLE_STRIP quad).
            self.shared.cmd_draw(cmd, 4, instance_count as u32, 0, 0);
        }
    }

    /// Whether the user opted into the magenta debug rect via
    /// `RIO_VULKAN_BOOTSTRAP=1`. Read by `Sugarloaf::render_vulkan`
    /// so the rect can be drawn between grid passes and the present
    /// barrier — keeping all draws inside the single render pass that
    /// the Sugarloaf-level orchestrator opens.
    #[inline]
    pub fn bootstrap_visible(&self) -> bool {
        self.bootstrap_visible
    }

    /// Record the bootstrap rect draw into `cmd`. Caller must already
    /// have `cmd_begin_rendering` open + viewport/scissor set. No-op
    /// when `bootstrap_visible == false`.
    pub fn draw_bootstrap(&self, cmd: vk::CommandBuffer) {
        if !self.bootstrap_visible {
            return;
        }
        unsafe {
            self.shared.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.bootstrap_pipeline,
            );
            let color: [f32; 4] = [1.0, 0.0, 1.0, 1.0];
            self.shared.cmd_push_constants(
                cmd,
                self.bootstrap_layout,
                vk::ShaderStageFlags::FRAGMENT,
                0,
                bytemuck::bytes_of(&color),
            );
            // Triangle strip, 4 vertices — `clear.vert.glsl`
            // generates a centered rect in NDC.
            self.shared.cmd_draw(cmd, 4, 1, 0, 0);
        }
    }
}

/// Free helper: emit the layout-transition barrier from `UNDEFINED` to
/// `COLOR_ATTACHMENT_OPTIMAL`. Run once at the top of a frame, before
/// `cmd_begin_rendering`. The discard of previous contents is
/// intentional — sugarloaf clears every frame, so we don't need to
/// preserve what the swapchain image held last present.
pub fn cmd_acquire_image_for_rendering(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
) {
    unsafe {
        let barrier = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
            .src_access_mask(vk::AccessFlags2::empty())
            .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(color_subresource_range());
        let barriers = [barrier];
        let dep = vk::DependencyInfo::default().image_memory_barriers(&barriers);
        device.cmd_pipeline_barrier2(cmd, &dep);
    }
}

/// Free helper: emit the post-rendering barrier so
/// `vkQueuePresentKHR` finds the image in `PRESENT_SRC_KHR`. Run once
/// after `cmd_end_rendering`, before `present_frame`.
pub fn cmd_release_image_to_present(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
) {
    unsafe {
        let barrier = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::BOTTOM_OF_PIPE)
            .dst_access_mask(vk::AccessFlags2::empty())
            .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(color_subresource_range());
        let barriers = [barrier];
        let dep = vk::DependencyInfo::default().image_memory_barriers(&barriers);
        device.cmd_pipeline_barrier2(cmd, &dep);
    }
}

/// Free helper: build a `RenderingInfo` clearing the swapchain image to
/// `clear_color`. Caller wraps draws between `cmd_begin_rendering` and
/// `cmd_end_rendering` using this.
pub fn build_rendering_info<'a>(
    frame: &'a VulkanFrame,
    color_attachments: &'a [vk::RenderingAttachmentInfo<'a>],
) -> vk::RenderingInfo<'a> {
    let render_area = vk::Rect2D {
        offset: vk::Offset2D { x: 0, y: 0 },
        extent: frame.extent,
    };
    vk::RenderingInfo::default()
        .render_area(render_area)
        .layer_count(1)
        .color_attachments(color_attachments)
}

/// Build the single color attachment used by every sugarloaf Vulkan
/// frame: clear → store, no MSAA, no depth.
pub fn build_color_attachment(
    frame: &VulkanFrame,
    clear_color: [f32; 4],
) -> vk::RenderingAttachmentInfo<'_> {
    let clear = vk::ClearValue {
        color: vk::ClearColorValue {
            float32: clear_color,
        },
    };
    vk::RenderingAttachmentInfo::default()
        .image_view(frame.image_view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(clear)
}

impl Drop for VulkanRenderer {
    fn drop(&mut self) {
        unsafe {
            // Idle the queue before tearing down pipelines. The
            // shared `Arc<VkShared>` keeps the underlying device
            // alive across this Drop — `vkDestroyDevice` runs only
            // when the last clone goes (in `VkShared::drop`), so
            // ordering relative to `VulkanContext::drop` is no
            // longer load-bearing.
            let _ = self.shared.device_wait_idle();

            self.shared.destroy_pipeline(self.image_pipeline, None);
            self.shared
                .destroy_pipeline_layout(self.image_pipeline_layout, None);
            self.shared.destroy_sampler(self.image_sampler, None);
            self.shared
                .destroy_descriptor_pool(self.image_uniform_descriptor_pool, None);
            self.shared.destroy_descriptor_set_layout(
                self.image_uniform_descriptor_set_layout,
                None,
            );
            self.shared.destroy_descriptor_set_layout(
                self.image_texture_descriptor_set_layout,
                None,
            );

            self.shared.destroy_pipeline(self.geometry_pipeline, None);
            self.shared.destroy_pipeline(self.quad_pipeline, None);
            self.shared
                .destroy_pipeline_layout(self.quad_pipeline_layout, None);
            self.shared
                .destroy_descriptor_pool(self.quad_descriptor_pool, None);
            self.shared
                .destroy_descriptor_set_layout(self.quad_descriptor_set_layout, None);
            self.shared.destroy_pipeline(self.bootstrap_pipeline, None);
            self.shared
                .destroy_pipeline_layout(self.bootstrap_layout, None);
            // Buffers (uniform, instance) drop themselves via VulkanBuffer.
        }
    }
}

// -----------------------------------------------------------------------
// Internal helpers
// -----------------------------------------------------------------------

fn color_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
}

/// Wrap `bytes` (an `include_bytes!` slice) in a `vk::ShaderModule`. The
/// SPIR-V spec requires u32 alignment, but `include_bytes!` is
/// byte-aligned — `ash::util::read_spv` does the alignment-safe copy
/// for us.
fn create_shader_module(device: &ash::Device, bytes: &[u8]) -> vk::ShaderModule {
    let code = ash::util::read_spv(&mut std::io::Cursor::new(bytes))
        .expect("read_spv (embedded shader is valid)");
    let info = vk::ShaderModuleCreateInfo::default().code(&code);
    unsafe {
        device
            .create_shader_module(&info, None)
            .expect("create_shader_module")
    }
}

fn build_bootstrap_pipeline(
    device: &ash::Device,
    pipeline_cache: vk::PipelineCache,
    layout: vk::PipelineLayout,
    vert: vk::ShaderModule,
    frag: vk::ShaderModule,
    color_format: vk::Format,
) -> vk::Pipeline {
    let entry = c"main";
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert)
            .name(entry),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag)
            .name(entry),
    ];

    // No vertex input — `clear.vert.glsl` uses `gl_VertexIndex` to
    // derive positions, no buffers bound.
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_STRIP)
        .primitive_restart_enable(false);

    // Viewport + scissor are dynamic so resize doesn't need a pipeline
    // rebuild.
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);

    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);

    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    // Premultiplied-over blend, matching the rest of the sugarloaf
    // pipelines (Metal/wgpu compositor blend). Real per-pipeline blend
    // modes land alongside the real pipelines in later phases.
    let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::ONE)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(vk::ColorComponentFlags::RGBA);
    let color_blend_attachments = [color_blend_attachment];
    let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
        .attachments(&color_blend_attachments);

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    // Dynamic rendering — no `VkRenderPass` needed. Just declare the
    // color attachment format the pipeline will write to.
    let color_attachment_formats = [color_format];
    let mut rendering = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&color_attachment_formats);

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .dynamic_state(&dynamic_state)
        .layout(layout)
        .push_next(&mut rendering);

    unsafe {
        device
            .create_graphics_pipelines(pipeline_cache, &[pipeline_info], None)
            .map_err(|(_, e)| e)
            .expect("create_graphics_pipelines(bootstrap)")[0]
    }
}

// =======================================================================
// Quad pipeline helpers
// =======================================================================

fn create_quad_descriptor_set_layout(device: &ash::Device) -> vk::DescriptorSetLayout {
    let bindings = [vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)];
    let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    unsafe {
        device
            .create_descriptor_set_layout(&info, None)
            .expect("create_descriptor_set_layout(quad)")
    }
}

fn create_quad_descriptor_pool(device: &ash::Device) -> vk::DescriptorPool {
    let sizes = [vk::DescriptorPoolSize {
        ty: vk::DescriptorType::UNIFORM_BUFFER,
        descriptor_count: FRAMES_IN_FLIGHT as u32,
    }];
    let info = vk::DescriptorPoolCreateInfo::default()
        .max_sets(FRAMES_IN_FLIGHT as u32)
        .pool_sizes(&sizes);
    unsafe {
        device
            .create_descriptor_pool(&info, None)
            .expect("create_descriptor_pool(quad)")
    }
}

fn allocate_quad_descriptor_sets(
    device: &ash::Device,
    pool: vk::DescriptorPool,
    layout: vk::DescriptorSetLayout,
) -> [vk::DescriptorSet; FRAMES_IN_FLIGHT] {
    let layouts = [layout; FRAMES_IN_FLIGHT];
    let info = vk::DescriptorSetAllocateInfo::default()
        .descriptor_pool(pool)
        .set_layouts(&layouts);
    let sets = unsafe {
        device
            .allocate_descriptor_sets(&info)
            .expect("allocate_descriptor_sets(quad)")
    };
    let mut out = [vk::DescriptorSet::null(); FRAMES_IN_FLIGHT];
    out.copy_from_slice(&sets);
    out
}

fn update_quad_descriptor_set(
    device: &ash::Device,
    set: vk::DescriptorSet,
    uniform: &VulkanBuffer,
) {
    let uniform_info = vk::DescriptorBufferInfo::default()
        .buffer(uniform.handle())
        .offset(0)
        .range(uniform.size());
    let infos = [uniform_info];
    let write = vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(0)
        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
        .buffer_info(&infos);
    unsafe {
        device.update_descriptor_sets(&[write], &[]);
    }
}

fn create_quad_pipeline_layout(
    device: &ash::Device,
    set_layout: vk::DescriptorSetLayout,
) -> vk::PipelineLayout {
    let set_layouts = [set_layout];
    let info = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
    unsafe {
        device
            .create_pipeline_layout(&info, None)
            .expect("create_pipeline_layout(quad)")
    }
}

fn build_quad_pipeline(
    device: &ash::Device,
    pipeline_cache: vk::PipelineCache,
    layout: vk::PipelineLayout,
    color_format: vk::Format,
) -> vk::Pipeline {
    let vert = create_shader_module(device, QUAD_VERT_SPV);
    let frag = create_shader_module(device, QUAD_FRAG_SPV);

    let entry = c"main";
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert)
            .name(entry),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag)
            .name(entry),
    ];

    // Vertex input mirrors `QuadInstance` (96 bytes, 8 attributes).
    let bindings = [vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(std::mem::size_of::<QuadInstance>() as u32)
        .input_rate(vk::VertexInputRate::INSTANCE)];
    let attrs = [
        // 0: pos vec3 @ 0
        vk::VertexInputAttributeDescription::default()
            .location(0)
            .binding(0)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(0),
        // 1: color vec4 @ 12
        vk::VertexInputAttributeDescription::default()
            .location(1)
            .binding(0)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(12),
        // 2: uv_rect vec4 @ 28
        vk::VertexInputAttributeDescription::default()
            .location(2)
            .binding(0)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(28),
        // 3: layers ivec2 @ 44
        vk::VertexInputAttributeDescription::default()
            .location(3)
            .binding(0)
            .format(vk::Format::R32G32_SINT)
            .offset(44),
        // 4: size vec2 @ 52
        vk::VertexInputAttributeDescription::default()
            .location(4)
            .binding(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(52),
        // 5: corner_radii vec4 @ 60
        vk::VertexInputAttributeDescription::default()
            .location(5)
            .binding(0)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(60),
        // 6: underline_style i32 @ 76
        vk::VertexInputAttributeDescription::default()
            .location(6)
            .binding(0)
            .format(vk::Format::R32_SINT)
            .offset(76),
        // 7: clip_rect vec4 @ 80
        vk::VertexInputAttributeDescription::default()
            .location(7)
            .binding(0)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(80),
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&bindings)
        .vertex_attribute_descriptions(&attrs);

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_STRIP);
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    // Same blend as Metal/wgpu rich-text pipeline:
    // src_color: SrcAlpha, dst_color: OneMinusSrcAlpha (gamma-space
    // alpha blending — matches sugarloaf's other pipelines).
    let blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(vk::ColorComponentFlags::RGBA);
    let blend_attachments = [blend_attachment];
    let color_blend =
        vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let color_attachment_formats = [color_format];
    let mut rendering = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&color_attachment_formats);

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .dynamic_state(&dynamic_state)
        .layout(layout)
        .push_next(&mut rendering);

    let pipeline = unsafe {
        device
            .create_graphics_pipelines(pipeline_cache, &[pipeline_info], None)
            .map_err(|(_, e)| e)
            .expect("create_graphics_pipelines(quad)")[0]
    };
    unsafe {
        device.destroy_shader_module(vert, None);
        device.destroy_shader_module(frag, None);
    }
    pipeline
}

// =======================================================================
// Image pipeline helpers
// =======================================================================

fn create_image_uniform_descriptor_set_layout(
    device: &ash::Device,
) -> vk::DescriptorSetLayout {
    // Same shape as the quad pipeline's uniform set: one
    // UNIFORM_BUFFER at binding 0, visible to vertex + fragment.
    let bindings = [vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)];
    let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    unsafe {
        device
            .create_descriptor_set_layout(&info, None)
            .expect("create_descriptor_set_layout(image uniform)")
    }
}

fn create_image_texture_descriptor_set_layout(
    device: &ash::Device,
) -> vk::DescriptorSetLayout {
    let bindings = [vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::FRAGMENT)];
    let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    unsafe {
        device
            .create_descriptor_set_layout(&info, None)
            .expect("create_descriptor_set_layout(image texture)")
    }
}

fn create_image_uniform_descriptor_pool(device: &ash::Device) -> vk::DescriptorPool {
    let sizes = [vk::DescriptorPoolSize {
        ty: vk::DescriptorType::UNIFORM_BUFFER,
        descriptor_count: FRAMES_IN_FLIGHT as u32,
    }];
    let info = vk::DescriptorPoolCreateInfo::default()
        .max_sets(FRAMES_IN_FLIGHT as u32)
        .pool_sizes(&sizes);
    unsafe {
        device
            .create_descriptor_pool(&info, None)
            .expect("create_descriptor_pool(image uniform)")
    }
}

fn create_image_pipeline_layout(
    device: &ash::Device,
    uniform_layout: vk::DescriptorSetLayout,
    texture_layout: vk::DescriptorSetLayout,
) -> vk::PipelineLayout {
    let set_layouts = [uniform_layout, texture_layout];
    let info = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
    unsafe {
        device
            .create_pipeline_layout(&info, None)
            .expect("create_pipeline_layout(image)")
    }
}

fn create_image_sampler(device: &ash::Device) -> vk::Sampler {
    // Linear filtering for smooth scaling of background images and
    // kitty graphics. ClampToEdge addressing prevents bleeding at
    // the texture edges.
    let info = vk::SamplerCreateInfo::default()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE);
    unsafe {
        device
            .create_sampler(&info, None)
            .expect("create_sampler(image)")
    }
}

fn build_image_pipeline(
    device: &ash::Device,
    pipeline_cache: vk::PipelineCache,
    layout: vk::PipelineLayout,
    color_format: vk::Format,
) -> vk::Pipeline {
    let vert = create_shader_module(device, IMAGE_VERT_SPV);
    let frag = create_shader_module(device, IMAGE_FRAG_SPV);

    let entry = c"main";
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert)
            .name(entry),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag)
            .name(entry),
    ];

    let bindings = [vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(std::mem::size_of::<ImageInstance>() as u32)
        .input_rate(vk::VertexInputRate::INSTANCE)];
    let attrs = [
        // 0: dest_pos vec2 @ 0
        vk::VertexInputAttributeDescription::default()
            .location(0)
            .binding(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(0),
        // 1: dest_size vec2 @ 8
        vk::VertexInputAttributeDescription::default()
            .location(1)
            .binding(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(8),
        // 2: source_rect vec4 @ 16
        vk::VertexInputAttributeDescription::default()
            .location(2)
            .binding(0)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(16),
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&bindings)
        .vertex_attribute_descriptions(&attrs);

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_STRIP);
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    // Image fragment returns premultiplied RGBA — source factor ONE.
    let blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::ONE)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(vk::ColorComponentFlags::RGBA);
    let blend_attachments = [blend_attachment];
    let color_blend =
        vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let color_attachment_formats = [color_format];
    let mut rendering = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&color_attachment_formats);

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .dynamic_state(&dynamic_state)
        .layout(layout)
        .push_next(&mut rendering);

    let pipeline = unsafe {
        device
            .create_graphics_pipelines(pipeline_cache, &[pipeline_info], None)
            .map_err(|(_, e)| e)
            .expect("create_graphics_pipelines(image)")[0]
    };
    unsafe {
        device.destroy_shader_module(vert, None);
        device.destroy_shader_module(frag, None);
    }
    pipeline
}

// =======================================================================
// Per-image texture: device-local image + per-image descriptor set
// =======================================================================

/// One uploaded image (background, kitty graphic, sixel) — owns its
/// `vk::Image`, view, memory, descriptor pool, and descriptor set.
/// The descriptor set's binding 0 is wired to the image's view +
/// the `VulkanRenderer`'s shared sampler at construction time, so
/// the renderer's `render_*` paths can bind it directly without
/// touching descriptor pools per draw.
pub struct VulkanImageTexture {
    pub image: VulkanImage,
    descriptor_pool: vk::DescriptorPool,
    pub descriptor_set: vk::DescriptorSet,
    /// Shared device handle. See `VkShared`.
    shared: Arc<VkShared>,
}

impl VulkanImageTexture {
    /// Synchronously upload `pixels` (RGBA8) into a fresh device-local
    /// image, transition to `SHADER_READ_ONLY_OPTIMAL`, and create a
    /// descriptor set bound to (image_view, sampler).
    ///
    /// The submit-and-wait is the right choice for one-shot uploads
    /// (background image, set once at config-load). Per-frame upload
    /// paths (kitty graphics) want a different code path that
    /// piggy-backs on the per-frame command buffer.
    pub fn upload_rgba(
        ctx: &VulkanContext,
        pixels: &[u8],
        width: u32,
        height: u32,
        descriptor_set_layout: vk::DescriptorSetLayout,
        sampler: vk::Sampler,
    ) -> Self {
        let shared = ctx.shared().clone();
        let device = &shared.raw;

        // `R8G8B8A8_SRGB` (vs `R8G8B8A8_UNORM`) tells the GPU to
        // sRGB-decode bytes at sample time. With bilinear filtering
        // enabled on the sampler this means interpolation happens in
        // *linear* light — without it, scaled image edges come out
        // visibly dark (gamma-space midtones), matching the same
        // choice on `RGBA8Unorm_sRGB` in `context::metal`.
        let image = ctx.allocate_sampled_image(
            width,
            height,
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
        );

        // Staging buffer.
        let staging_size = (width as usize) * (height as usize) * 4;
        let staging = ctx.allocate_host_visible_buffer(
            staging_size as u64,
            vk::BufferUsageFlags::TRANSFER_SRC,
        );
        unsafe {
            std::ptr::copy_nonoverlapping(
                pixels.as_ptr(),
                staging.as_mut_ptr(),
                staging_size,
            );
        }

        // One-shot transfer: barrier → copy → barrier.
        let img_handle = image.handle();
        let staging_handle = staging.handle();
        ctx.submit_oneshot(|cmd| unsafe {
            let to_dst = vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
                .src_access_mask(vk::AccessFlags2::empty())
                .dst_stage_mask(vk::PipelineStageFlags2::COPY)
                .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(img_handle)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1),
                );
            let barriers = [to_dst];
            let dep = vk::DependencyInfo::default().image_memory_barriers(&barriers);
            device.cmd_pipeline_barrier2(cmd, &dep);

            let region = vk::BufferImageCopy::default()
                .image_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .mip_level(0)
                        .base_array_layer(0)
                        .layer_count(1),
                )
                .image_extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                });
            device.cmd_copy_buffer_to_image(
                cmd,
                staging_handle,
                img_handle,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
            );

            let to_read = vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COPY)
                .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
                .dst_access_mask(vk::AccessFlags2::SHADER_READ)
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(img_handle)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1),
                );
            let barriers = [to_read];
            let dep = vk::DependencyInfo::default().image_memory_barriers(&barriers);
            device.cmd_pipeline_barrier2(cmd, &dep);
        });
        // Staging buffer drops here — submit_oneshot already waited.

        // Per-image descriptor pool + set.
        let pool_sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: 1,
        }];
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(&pool_sizes);
        let descriptor_pool = unsafe {
            device
                .create_descriptor_pool(&pool_info, None)
                .expect("create_descriptor_pool(image texture)")
        };
        let layouts = [descriptor_set_layout];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);
        let descriptor_set = unsafe {
            device
                .allocate_descriptor_sets(&alloc_info)
                .expect("allocate_descriptor_sets(image texture)")[0]
        };

        let image_info = vk::DescriptorImageInfo::default()
            .sampler(sampler)
            .image_view(image.view())
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        let infos = [image_info];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&infos);
        unsafe {
            device.update_descriptor_sets(&[write], &[]);
        }

        Self {
            image,
            descriptor_pool,
            descriptor_set,
            shared,
        }
    }
}

impl Drop for VulkanImageTexture {
    fn drop(&mut self) {
        unsafe {
            // Pool destruction frees the descriptor set; image drops
            // itself.
            self.shared
                .destroy_descriptor_pool(self.descriptor_pool, None);
        }
    }
}

// =======================================================================
// Geometry pipeline (per-vertex `Vertex` for non-quad draws)
// =======================================================================

fn build_geometry_pipeline(
    device: &ash::Device,
    pipeline_cache: vk::PipelineCache,
    layout: vk::PipelineLayout,
    color_format: vk::Format,
) -> vk::Pipeline {
    // Reuses the QUAD fragment shader — Vertex output structure is
    // intentionally identical to QuadInstance's vertex output.
    let vert = create_shader_module(device, GEOMETRY_VERT_SPV);
    let frag = create_shader_module(device, QUAD_FRAG_SPV);

    let entry = c"main";
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert)
            .name(entry),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag)
            .name(entry),
    ];

    // Per-vertex (NOT instanced) — matches `Vertex` (88 bytes).
    let bindings = [vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(std::mem::size_of::<Vertex>() as u32)
        .input_rate(vk::VertexInputRate::VERTEX)];
    let attrs = [
        // 0: pos vec3 @ 0
        vk::VertexInputAttributeDescription::default()
            .location(0)
            .binding(0)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(0),
        // 1: color vec4 @ 12
        vk::VertexInputAttributeDescription::default()
            .location(1)
            .binding(0)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(12),
        // 2: uv vec2 @ 28
        vk::VertexInputAttributeDescription::default()
            .location(2)
            .binding(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(28),
        // 3: layers ivec2 @ 36
        vk::VertexInputAttributeDescription::default()
            .location(3)
            .binding(0)
            .format(vk::Format::R32G32_SINT)
            .offset(36),
        // 4: corner_radii vec4 @ 44
        vk::VertexInputAttributeDescription::default()
            .location(4)
            .binding(0)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(44),
        // 5: rect_size vec2 @ 60
        vk::VertexInputAttributeDescription::default()
            .location(5)
            .binding(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(60),
        // 6: underline_style i32 @ 68
        vk::VertexInputAttributeDescription::default()
            .location(6)
            .binding(0)
            .format(vk::Format::R32_SINT)
            .offset(68),
        // 7: clip_rect vec4 @ 72
        vk::VertexInputAttributeDescription::default()
            .location(7)
            .binding(0)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(72),
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&bindings)
        .vertex_attribute_descriptions(&attrs);

    // TRIANGLE_LIST — emit path tessellates polygons / arcs / lines
    // into independent triangles (3 vertices each, no strip).
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    // Same blend as the quad pipeline — gamma-space SrcAlpha /
    // OneMinusSrcAlpha, matching every other sugarloaf pipeline.
    let blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(vk::ColorComponentFlags::RGBA);
    let blend_attachments = [blend_attachment];
    let color_blend =
        vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let color_attachment_formats = [color_format];
    let mut rendering = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&color_attachment_formats);

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .dynamic_state(&dynamic_state)
        .layout(layout)
        .push_next(&mut rendering);

    let pipeline = unsafe {
        device
            .create_graphics_pipelines(pipeline_cache, &[pipeline_info], None)
            .map_err(|(_, e)| e)
            .expect("create_graphics_pipelines(geometry)")[0]
    };
    unsafe {
        device.destroy_shader_module(vert, None);
        device.destroy_shader_module(frag, None);
    }
    pipeline
}
