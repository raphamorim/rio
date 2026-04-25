// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Native Vulkan backend for the grid renderer.
//!
//! Phase 4: bg + text passes. Mirrors `grid::metal::MetalGridRenderer`
//! in shape so the rioterm emit loop can drive both via the
//! `GridRenderer` enum without per-backend conditionals.
//!
//! Per-frame ring: bg + uniform + fg buffers are sized per
//! `FRAMES_IN_FLIGHT` slot, indexed by `VulkanFrame::slot`. The
//! `acquire_frame` fence wait already proved that slot's GPU work is
//! done, so writing into slot `N`'s buffers from the CPU is safe.
//!
//! Atlas uploads are deferred — `insert_glyph` records pending pixels
//! into a per-atlas queue, and `render` flushes the queue into the
//! frame's command buffer (one staging buffer per slot, copy +
//! barrier, then the text pass reads). Per-glyph synchronous uploads
//! would cost ~1ms/glyph (vkQueueSubmit + fence wait) — way too slow
//! for the first-frame burst of ~ASCII printables.

use ash::vk;
use rustc_hash::FxHashMap;

use super::atlas::{AtlasSlot, GlyphKey, RasterizedGlyph};
use super::cell::{CellBg, CellText, GridUniforms};
use crate::context::vulkan::{
    allocate_host_visible_buffer_raw, VulkanBuffer, VulkanContext, VulkanImage,
    FRAMES_IN_FLIGHT,
};
use crate::renderer::image_cache::atlas::AtlasAllocator;

// Compiled at build time by `sugarloaf/build.rs`. Source GLSL lives
// in `sugarloaf/src/grid/shaders/`; edit those, not the .spv.
const BG_VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/grid_bg.vert.spv"));
const BG_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/grid_bg.frag.spv"));
const TEXT_VERT_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/grid_text.vert.spv"));
const TEXT_FRAG_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/grid_text.frag.spv"));

/// Extra slots appended to `fg_rows` for cursor glyphs. Mirrors the
/// Metal layout so the CPU emit code is byte-identical.
const CURSOR_ROW_SLOTS: usize = 2;

/// Initial atlas side. 2048² @ R8 = 4 MiB; matches the Metal default.
const ATLAS_SIZE: u16 = 2048;

// =======================================================================
// Glyph atlas
// =======================================================================

/// One pending glyph upload — `bytes` were copied at insert time, so
/// the rasterizer's buffer can be reused immediately. Drained by
/// `flush_pending_uploads` on the next `render()`.
struct PendingUpload {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    bytes: Vec<u8>,
}

/// Glyph atlas: device-local image + slot allocator + key→slot map +
/// pending upload queue + per-slot staging ring.
///
/// One instance per atlas kind (R8 grayscale, RGBA8 color). Owned by
/// either `VulkanGridRenderer` (per-panel terminal grids) or
/// `sugarloaf::text::Text`'s Vulkan state (UI overlay text); the
/// caller drives uploads via `prepare_uploads(...)` before
/// `cmd_begin_rendering`.
pub struct VulkanGlyphAtlas {
    image: VulkanImage,
    allocator: AtlasAllocator,
    slots: FxHashMap<GlyphKey, AtlasSlot>,
    bytes_per_pixel: u32,
    pending: Vec<PendingUpload>,
    /// True once the image has been transitioned out of `UNDEFINED`.
    /// Until the first upload, the texture is still in `UNDEFINED`
    /// layout and reading from it would be UB — the descriptor set is
    /// bound but the text pipeline only reads when there are
    /// instances, and there are no instances until after at least one
    /// `insert_glyph + render` cycle.
    initialized: bool,
    /// Per-slot staging buffer ring. Sized on demand, never shrinks.
    /// Reused across frames within a slot — the `acquire_frame`
    /// fence wait inside `VulkanContext` proves the previous use of
    /// slot N's staging is GPU-complete before the next reuse.
    staging: [Option<crate::context::vulkan::VulkanBuffer>; FRAMES_IN_FLIGHT],
    staging_capacity: [usize; FRAMES_IN_FLIGHT],
}

impl VulkanGlyphAtlas {
    pub fn new_grayscale(ctx: &VulkanContext) -> Self {
        Self::new(ctx, vk::Format::R8_UNORM, 1)
    }

    pub fn new_color(ctx: &VulkanContext) -> Self {
        Self::new(ctx, vk::Format::R8G8B8A8_UNORM, 4)
    }

    fn new(ctx: &VulkanContext, format: vk::Format, bytes_per_pixel: u32) -> Self {
        let image = ctx.allocate_sampled_image(
            ATLAS_SIZE as u32,
            ATLAS_SIZE as u32,
            format,
            vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
        );
        Self {
            image,
            allocator: AtlasAllocator::new(ATLAS_SIZE, ATLAS_SIZE),
            slots: FxHashMap::default(),
            bytes_per_pixel,
            pending: Vec::new(),
            initialized: false,
            staging: std::array::from_fn(|_| None),
            staging_capacity: [0; FRAMES_IN_FLIGHT],
        }
    }

    /// Drain `self.pending` into slot `slot`'s staging buffer (growing
    /// it if needed), then record `cmd_copy_buffer_to_image` +
    /// barriers into `cmd`. Caller MUST be outside a dynamic-rendering
    /// pass — Vulkan 1.3 spec
    /// `VUID-vkCmdCopyBufferToImage-renderpass` forbids transfer
    /// commands inside one. No-op when there are no pending uploads.
    ///
    /// We take `(device, instance, physical_device)` rather than
    /// `&VulkanContext` so the text overlay path can call this
    /// without holding an immutable borrow on the context
    /// (`Sugarloaf::render_vulkan` keeps `ctx: &mut VulkanContext`
    /// for the swapchain acquire/present cycle).
    pub fn flush_uploads(
        &mut self,
        device: &ash::Device,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        cmd: vk::CommandBuffer,
        slot: usize,
    ) {
        if self.pending.is_empty() {
            return;
        }
        let total_bytes: usize = self
            .pending
            .iter()
            .map(|p| (p.w as usize) * (p.h as usize) * self.bytes_per_pixel as usize)
            .sum();

        // Grow per-slot staging if needed. The `min(256K)` floor keeps
        // us from churning allocations during the first-frame burst.
        if total_bytes > self.staging_capacity[slot] {
            let new_cap = total_bytes.next_power_of_two().max(256 * 1024);
            self.staging[slot] =
                Some(crate::context::vulkan::allocate_host_visible_buffer_raw(
                    device,
                    instance,
                    physical_device,
                    new_cap as u64,
                    vk::BufferUsageFlags::TRANSFER_SRC,
                ));
            self.staging_capacity[slot] = new_cap;
        }
        let staging = self.staging[slot].as_ref().unwrap();
        let staging_ptr = staging.as_mut_ptr();
        let staging_handle = staging.handle();

        let bpp = self.bytes_per_pixel as usize;
        let mut offset: u64 = 0;
        let mut copies: Vec<vk::BufferImageCopy> = Vec::with_capacity(self.pending.len());
        unsafe {
            for upload in self.pending.drain(..) {
                let bytes = (upload.w as usize) * (upload.h as usize) * bpp;
                std::ptr::copy_nonoverlapping(
                    upload.bytes.as_ptr(),
                    staging_ptr.add(offset as usize),
                    bytes,
                );
                copies.push(image_copy_region(
                    offset, upload.x, upload.y, upload.w, upload.h,
                ));
                offset += bytes as u64;
            }
        }

        upload_to_atlas(device, cmd, staging_handle, self, &copies);
    }

    #[inline]
    pub fn lookup(&self, key: GlyphKey) -> Option<AtlasSlot> {
        self.slots.get(&key).copied()
    }

    /// Image view bound to this atlas. Used by callers to wire the
    /// atlas into their text-pipeline descriptor sets.
    #[inline]
    pub fn image_view(&self) -> vk::ImageView {
        self.image.view()
    }

    /// Pack + queue a glyph for upload. Returns `None` when the atlas
    /// is full. Bytes are copied into the pending queue, so the
    /// caller's `glyph.bytes` slice can be freed/reused immediately.
    /// Pixels reach the GPU on the next `render()` flush.
    pub fn insert(
        &mut self,
        key: GlyphKey,
        glyph: RasterizedGlyph<'_>,
    ) -> Option<AtlasSlot> {
        if glyph.width == 0 || glyph.height == 0 {
            // Whitespace / control glyphs — record an empty slot so
            // lookups don't keep retrying.
            let slot = AtlasSlot {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
                bearing_x: glyph.bearing_x,
                bearing_y: glyph.bearing_y,
            };
            self.slots.insert(key, slot);
            return Some(slot);
        }

        let (x, y) = self.allocator.allocate(glyph.width, glyph.height)?;
        let slot = AtlasSlot {
            x,
            y,
            w: glyph.width,
            h: glyph.height,
            bearing_x: glyph.bearing_x,
            bearing_y: glyph.bearing_y,
        };
        self.slots.insert(key, slot);
        self.pending.push(PendingUpload {
            x,
            y,
            w: glyph.width,
            h: glyph.height,
            bytes: glyph.bytes.to_vec(),
        });
        Some(slot)
    }
}

// =======================================================================
// Grid renderer
// =======================================================================

pub struct VulkanGridRenderer {
    device: ash::Device,
    /// Cached so `resize` (which only has `&mut self`) can allocate
    /// new bg buffers via `allocate_host_visible_buffer_raw` without
    /// needing a `&VulkanContext` borrow.
    instance: ash::Instance,
    physical_device: vk::PhysicalDevice,

    cols: u32,
    rows: u32,

    // ---------- bg state ----------
    bg_buffers: [VulkanBuffer; FRAMES_IN_FLIGHT],
    bg_dirty: [bool; FRAMES_IN_FLIGHT],
    bg_cpu: Vec<CellBg>,

    // ---------- shared uniform state ----------
    uniform_buffers: [VulkanBuffer; FRAMES_IN_FLIGHT],

    // ---------- bg pipeline ----------
    bg_descriptor_pool: vk::DescriptorPool,
    bg_descriptor_set_layout: vk::DescriptorSetLayout,
    bg_descriptor_sets: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    bg_pipeline_layout: vk::PipelineLayout,
    bg_pipeline: vk::Pipeline,

    // ---------- text state ----------
    fg_rows: Vec<Vec<CellText>>,
    fg_staging: Vec<CellText>,
    fg_buffers: [Option<VulkanBuffer>; FRAMES_IN_FLIGHT],
    fg_capacity: [usize; FRAMES_IN_FLIGHT],
    fg_live_count: [u32; FRAMES_IN_FLIGHT],
    fg_dirty: [bool; FRAMES_IN_FLIGHT],

    // ---------- text pipeline ----------
    text_uniform_descriptor_set_layout: vk::DescriptorSetLayout,
    text_atlas_descriptor_set_layout: vk::DescriptorSetLayout,
    text_descriptor_pool: vk::DescriptorPool,
    text_uniform_descriptor_sets: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    text_atlas_descriptor_set: vk::DescriptorSet,
    text_pipeline_layout: vk::PipelineLayout,
    text_pipeline: vk::Pipeline,
    sampler: vk::Sampler,

    // ---------- atlases ----------
    pub atlas_grayscale: VulkanGlyphAtlas,
    pub atlas_color: VulkanGlyphAtlas,

    needs_full_rebuild: bool,
}

impl VulkanGridRenderer {
    pub fn new(ctx: &VulkanContext, cols: u32, rows: u32) -> Self {
        let device = ctx.device().clone();
        let instance = ctx.instance().clone();
        let physical_device = ctx.physical_device();

        // ----- bg + uniforms -----
        let bg_buffers = std::array::from_fn(|_| alloc_bg_buffer(ctx, cols, rows));
        let uniform_buffers = std::array::from_fn(|_| {
            ctx.allocate_host_visible_buffer(
                std::mem::size_of::<GridUniforms>() as u64,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            )
        });

        let bg_descriptor_set_layout = create_bg_descriptor_set_layout(&device);
        let bg_descriptor_pool = create_bg_descriptor_pool(&device);
        let bg_descriptor_sets = allocate_descriptor_sets(
            &device,
            bg_descriptor_pool,
            bg_descriptor_set_layout,
        );
        for slot in 0..FRAMES_IN_FLIGHT {
            update_bg_descriptor_set(
                &device,
                bg_descriptor_sets[slot],
                &uniform_buffers[slot],
                &bg_buffers[slot],
            );
        }
        let bg_pipeline_layout =
            create_pipeline_layout(&device, &[bg_descriptor_set_layout]);
        let pipeline_cache = ctx.pipeline_cache();
        let bg_pipeline = create_bg_pipeline(
            &device,
            pipeline_cache,
            bg_pipeline_layout,
            ctx.swapchain_format(),
        );

        // ----- text -----
        let atlas_grayscale = VulkanGlyphAtlas::new_grayscale(ctx);
        let atlas_color = VulkanGlyphAtlas::new_color(ctx);
        let sampler = create_sampler(&device);

        let text_uniform_descriptor_set_layout =
            create_text_uniform_descriptor_set_layout(&device);
        let text_atlas_descriptor_set_layout =
            create_text_atlas_descriptor_set_layout(&device);
        // One pool that holds (FRAMES_IN_FLIGHT uniform sets) + (1 atlas set).
        let text_descriptor_pool = create_text_descriptor_pool(&device);

        let text_uniform_descriptor_sets = allocate_descriptor_sets(
            &device,
            text_descriptor_pool,
            text_uniform_descriptor_set_layout,
        );
        for slot in 0..FRAMES_IN_FLIGHT {
            update_text_uniform_descriptor_set(
                &device,
                text_uniform_descriptor_sets[slot],
                &uniform_buffers[slot],
            );
        }
        let text_atlas_descriptor_set = allocate_one_descriptor_set(
            &device,
            text_descriptor_pool,
            text_atlas_descriptor_set_layout,
        );
        update_text_atlas_descriptor_set(
            &device,
            text_atlas_descriptor_set,
            &atlas_grayscale.image,
            &atlas_color.image,
            sampler,
        );

        let text_pipeline_layout = create_pipeline_layout(
            &device,
            &[
                text_uniform_descriptor_set_layout,
                text_atlas_descriptor_set_layout,
            ],
        );
        let text_pipeline = create_text_pipeline(
            &device,
            pipeline_cache,
            text_pipeline_layout,
            ctx.swapchain_format(),
        );

        let bg_len = (cols as usize) * (rows as usize);
        Self {
            device,
            instance,
            physical_device,
            cols,
            rows,
            bg_buffers,
            bg_dirty: [true; FRAMES_IN_FLIGHT],
            bg_cpu: vec![CellBg::TRANSPARENT; bg_len],
            uniform_buffers,
            bg_descriptor_pool,
            bg_descriptor_set_layout,
            bg_descriptor_sets,
            bg_pipeline_layout,
            bg_pipeline,
            fg_rows: init_fg_rows(rows),
            fg_staging: Vec::new(),
            fg_buffers: std::array::from_fn(|_| None),
            fg_capacity: [0; FRAMES_IN_FLIGHT],
            fg_live_count: [0; FRAMES_IN_FLIGHT],
            fg_dirty: [true; FRAMES_IN_FLIGHT],
            text_uniform_descriptor_set_layout,
            text_atlas_descriptor_set_layout,
            text_descriptor_pool,
            text_uniform_descriptor_sets,
            text_atlas_descriptor_set,
            text_pipeline_layout,
            text_pipeline,
            sampler,
            atlas_grayscale,
            atlas_color,
            needs_full_rebuild: true,
        }
    }

    #[inline]
    pub fn needs_full_rebuild(&self) -> bool {
        self.needs_full_rebuild
    }

    #[inline]
    pub fn mark_full_rebuild_done(&mut self) {
        self.needs_full_rebuild = false;
    }

    pub fn resize(&mut self, cols: u32, rows: u32) {
        if cols == self.cols && rows == self.rows {
            return;
        }
        unsafe {
            let _ = self.device.device_wait_idle();
        }

        self.cols = cols;
        self.rows = rows;
        let bg_len = (cols as usize) * (rows as usize);
        self.bg_cpu = vec![CellBg::TRANSPARENT; bg_len];

        // Reallocate bg buffers via the cached (instance,
        // physical_device) pair and re-wire descriptor sets to the
        // new buffer handles.
        let bg_byte_size = (bg_len * std::mem::size_of::<CellBg>())
            .max(std::mem::size_of::<CellBg>()) as u64;
        self.bg_buffers = std::array::from_fn(|_| {
            allocate_host_visible_buffer_raw(
                &self.device,
                &self.instance,
                self.physical_device,
                bg_byte_size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )
        });
        for slot in 0..FRAMES_IN_FLIGHT {
            update_bg_descriptor_set(
                &self.device,
                self.bg_descriptor_sets[slot],
                &self.uniform_buffers[slot],
                &self.bg_buffers[slot],
            );
        }
        self.bg_dirty = [true; FRAMES_IN_FLIGHT];

        // Reset fg state — emit loop will re-populate after resize.
        self.fg_rows = init_fg_rows(rows);
        self.fg_dirty = [true; FRAMES_IN_FLIGHT];
        self.fg_live_count = [0; FRAMES_IN_FLIGHT];
        self.needs_full_rebuild = true;
    }

    pub fn write_row(&mut self, row: u32, bg: &[CellBg], fg: &[CellText]) {
        // FG: stash in CPU per-row vec, mark all slots dirty.
        let idx = (row as usize) + 1;
        if let Some(slot) = self.fg_rows.get_mut(idx) {
            slot.clear();
            slot.extend_from_slice(fg);
            self.fg_dirty = [true; FRAMES_IN_FLIGHT];
        }

        if row >= self.rows {
            return;
        }
        let row_start = (row as usize) * (self.cols as usize);
        let row_len = (self.cols as usize).min(bg.len());
        self.bg_cpu[row_start..row_start + row_len].copy_from_slice(&bg[..row_len]);
        for slot in &mut self.bg_cpu[row_start + row_len..row_start + self.cols as usize]
        {
            *slot = CellBg::TRANSPARENT;
        }
        self.bg_dirty = [true; FRAMES_IN_FLIGHT];
    }

    pub fn clear_row(&mut self, row: u32) {
        let idx = (row as usize) + 1;
        if let Some(slot) = self.fg_rows.get_mut(idx) {
            if !slot.is_empty() {
                self.fg_dirty = [true; FRAMES_IN_FLIGHT];
            }
            slot.clear();
        }
        if row >= self.rows {
            return;
        }
        let row_start = (row as usize) * (self.cols as usize);
        for slot in &mut self.bg_cpu[row_start..row_start + self.cols as usize] {
            *slot = CellBg::TRANSPARENT;
        }
        self.bg_dirty = [true; FRAMES_IN_FLIGHT];
    }

    pub fn set_block_cursor(&mut self, cells: &[CellText]) {
        if let Some(slot) = self.fg_rows.first_mut() {
            if slot.is_empty() && cells.is_empty() {
                return;
            }
            slot.clear();
            slot.extend_from_slice(cells);
            self.fg_dirty = [true; FRAMES_IN_FLIGHT];
        }
    }

    pub fn set_non_block_cursor(&mut self, cells: &[CellText]) {
        let idx = self.fg_rows.len().saturating_sub(1);
        if let Some(slot) = self.fg_rows.get_mut(idx) {
            if slot.is_empty() && cells.is_empty() {
                return;
            }
            slot.clear();
            slot.extend_from_slice(cells);
            self.fg_dirty = [true; FRAMES_IN_FLIGHT];
        }
    }

    pub fn clear_cursor(&mut self) {
        let mut changed = false;
        if let Some(slot) = self.fg_rows.first_mut() {
            if !slot.is_empty() {
                slot.clear();
                changed = true;
            }
        }
        let last = self.fg_rows.len().saturating_sub(1);
        if last > 0 {
            if let Some(slot) = self.fg_rows.get_mut(last) {
                if !slot.is_empty() {
                    slot.clear();
                    changed = true;
                }
            }
        }
        if changed {
            self.fg_dirty = [true; FRAMES_IN_FLIGHT];
        }
    }

    #[inline]
    pub fn lookup_glyph(&self, key: GlyphKey) -> Option<AtlasSlot> {
        self.atlas_grayscale.lookup(key)
    }

    #[inline]
    pub fn lookup_glyph_color(&self, key: GlyphKey) -> Option<AtlasSlot> {
        self.atlas_color.lookup(key)
    }

    #[inline]
    pub fn insert_glyph(
        &mut self,
        key: GlyphKey,
        glyph: RasterizedGlyph<'_>,
    ) -> Option<AtlasSlot> {
        self.atlas_grayscale.insert(key, glyph)
    }

    #[inline]
    pub fn insert_glyph_color(
        &mut self,
        key: GlyphKey,
        glyph: RasterizedGlyph<'_>,
    ) -> Option<AtlasSlot> {
        self.atlas_color.insert(key, glyph)
    }

    /// Drain pending atlas uploads into `cmd`. MUST be called BEFORE
    /// `Sugarloaf::render_vulkan` opens its dynamic-rendering pass —
    /// `vkCmdCopyBufferToImage` is forbidden inside a render pass.
    /// No-op when both atlases have no pending entries.
    pub fn prepare(
        &mut self,
        ctx: &VulkanContext,
        cmd: vk::CommandBuffer,
        frame_slot: usize,
    ) {
        debug_assert!(frame_slot < FRAMES_IN_FLIGHT);
        if self.atlas_grayscale.pending.is_empty() && self.atlas_color.pending.is_empty()
        {
            return;
        }
        self.flush_pending_uploads(ctx, cmd, frame_slot);
    }

    /// Record the bg + text passes into `cmd`. Caller has already
    /// opened the dynamic-rendering pass and set viewport/scissor.
    /// `frame_slot` is the in-flight slot whose `in_flight` fence has
    /// been waited on. Atlas uploads must already have been flushed
    /// via `prepare()` before the pass opened.
    pub fn render(
        &mut self,
        ctx: &VulkanContext,
        cmd: vk::CommandBuffer,
        frame_slot: usize,
        uniforms: &GridUniforms,
    ) {
        debug_assert!(frame_slot < FRAMES_IN_FLIGHT);
        let slot = frame_slot;

        // ----- bg cells + uniforms upload -----
        if self.bg_dirty[slot] {
            unsafe {
                let dst = self.bg_buffers[slot].as_mut_ptr() as *mut CellBg;
                std::ptr::copy_nonoverlapping(
                    self.bg_cpu.as_ptr(),
                    dst,
                    self.bg_cpu.len(),
                );
            }
            self.bg_dirty[slot] = false;
        }
        unsafe {
            let dst = self.uniform_buffers[slot].as_mut_ptr() as *mut GridUniforms;
            std::ptr::write(dst, *uniforms);
        }

        // ----- bg pass (1 fullscreen triangle, fragment does cell lookup) -----
        unsafe {
            self.device.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.bg_pipeline,
            );
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.bg_pipeline_layout,
                0,
                &[self.bg_descriptor_sets[slot]],
                &[],
            );
            self.device.cmd_draw(cmd, 3, 1, 0, 0);
        }

        // ----- text pass (instanced quads, one per glyph) -----
        if self.fg_dirty[slot] {
            self.fg_staging.clear();
            for row in &self.fg_rows {
                self.fg_staging.extend_from_slice(row);
            }
            let needed = self.fg_staging.len();

            if needed > self.fg_capacity[slot] {
                let new_cap = needed.next_power_of_two().max(64);
                self.fg_buffers[slot] = Some(ctx.allocate_host_visible_buffer(
                    (new_cap * std::mem::size_of::<CellText>()) as u64,
                    vk::BufferUsageFlags::VERTEX_BUFFER,
                ));
                self.fg_capacity[slot] = new_cap;
            }

            if needed > 0 {
                let buf = self.fg_buffers[slot].as_ref().unwrap();
                unsafe {
                    let dst = buf.as_mut_ptr() as *mut CellText;
                    std::ptr::copy_nonoverlapping(self.fg_staging.as_ptr(), dst, needed);
                }
            }
            self.fg_live_count[slot] = needed as u32;
            self.fg_dirty[slot] = false;
        }

        let instance_count = self.fg_live_count[slot];
        if instance_count == 0 {
            return;
        }

        unsafe {
            self.device.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.text_pipeline,
            );
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.text_pipeline_layout,
                0,
                &[
                    self.text_uniform_descriptor_sets[slot],
                    self.text_atlas_descriptor_set,
                ],
                &[],
            );
            let buf = self.fg_buffers[slot].as_ref().unwrap();
            self.device
                .cmd_bind_vertex_buffers(cmd, 0, &[buf.handle()], &[0]);
            self.device.cmd_draw(cmd, 4, instance_count, 0, 0);
        }
    }

    /// Delegate to each atlas's own `flush_uploads`. Each atlas owns
    /// its own per-slot staging buffer ring now — see
    /// `VulkanGlyphAtlas::flush_uploads`.
    fn flush_pending_uploads(
        &mut self,
        _ctx: &VulkanContext,
        cmd: vk::CommandBuffer,
        slot: usize,
    ) {
        self.atlas_grayscale.flush_uploads(
            &self.device,
            &self.instance,
            self.physical_device,
            cmd,
            slot,
        );
        self.atlas_color.flush_uploads(
            &self.device,
            &self.instance,
            self.physical_device,
            cmd,
            slot,
        );
    }
}

/// Record an atlas upload: barrier image → `TRANSFER_DST_OPTIMAL`,
/// `cmd_copy_buffer_to_image`, barrier image → `SHADER_READ_ONLY_OPTIMAL`.
///
/// Both barriers are required: the first synchronizes any prior
/// fragment-shader read of the atlas (steady state) against the
/// upcoming transfer write; the second synchronizes the transfer
/// write against the *next* fragment-shader read (which happens in
/// the same command buffer, in the text pipeline draw a few hundred
/// instructions later). Without the trailing barrier the GPU is free
/// to start the fragment work before the copy completes, producing
/// transient garbage glyphs.
///
/// Caller (`flush_pending_uploads`) must ensure this is invoked
/// *outside* a dynamic-rendering pass — Vulkan 1.3 spec
/// VUID-vkCmdCopyBufferToImage-renderpass forbids transfer commands
/// inside a render pass. `Sugarloaf::render_vulkan` honours this by
/// calling `prepare_vulkan` before `cmd_begin_rendering`.
fn upload_to_atlas(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    staging: vk::Buffer,
    atlas: &mut VulkanGlyphAtlas,
    copies: &[vk::BufferImageCopy],
) {
    let old_layout = if atlas.initialized {
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
    } else {
        vk::ImageLayout::UNDEFINED
    };
    unsafe {
        // → TRANSFER_DST
        let to_transfer = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(if atlas.initialized {
                vk::PipelineStageFlags2::FRAGMENT_SHADER
            } else {
                vk::PipelineStageFlags2::TOP_OF_PIPE
            })
            .src_access_mask(if atlas.initialized {
                vk::AccessFlags2::SHADER_READ
            } else {
                vk::AccessFlags2::empty()
            })
            .dst_stage_mask(vk::PipelineStageFlags2::COPY)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .old_layout(old_layout)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(atlas.image.handle())
            .subresource_range(color_subresource_range());
        let barriers = [to_transfer];
        let dep = vk::DependencyInfo::default().image_memory_barriers(&barriers);
        device.cmd_pipeline_barrier2(cmd, &dep);

        // copy
        device.cmd_copy_buffer_to_image(
            cmd,
            staging,
            atlas.image.handle(),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            copies,
        );

        // → SHADER_READ
        let to_shader_read = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COPY)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
            .dst_access_mask(vk::AccessFlags2::SHADER_READ)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(atlas.image.handle())
            .subresource_range(color_subresource_range());
        let barriers = [to_shader_read];
        let dep = vk::DependencyInfo::default().image_memory_barriers(&barriers);
        device.cmd_pipeline_barrier2(cmd, &dep);
    }

    atlas.initialized = true;
}

impl Drop for VulkanGridRenderer {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_pipeline(self.text_pipeline, None);
            self.device
                .destroy_pipeline_layout(self.text_pipeline_layout, None);
            self.device
                .destroy_descriptor_pool(self.text_descriptor_pool, None);
            self.device.destroy_descriptor_set_layout(
                self.text_atlas_descriptor_set_layout,
                None,
            );
            self.device.destroy_descriptor_set_layout(
                self.text_uniform_descriptor_set_layout,
                None,
            );
            self.device.destroy_sampler(self.sampler, None);

            self.device.destroy_pipeline(self.bg_pipeline, None);
            self.device
                .destroy_pipeline_layout(self.bg_pipeline_layout, None);
            self.device
                .destroy_descriptor_pool(self.bg_descriptor_pool, None);
            self.device
                .destroy_descriptor_set_layout(self.bg_descriptor_set_layout, None);
            // Buffers + atlas images drop themselves.
        }
    }
}

// =======================================================================
// Helpers
// =======================================================================

fn alloc_bg_buffer(ctx: &VulkanContext, cols: u32, rows: u32) -> VulkanBuffer {
    let size = (cols as u64)
        .saturating_mul(rows as u64)
        .saturating_mul(std::mem::size_of::<CellBg>() as u64)
        .max(std::mem::size_of::<CellBg>() as u64);
    ctx.allocate_host_visible_buffer(size, vk::BufferUsageFlags::STORAGE_BUFFER)
}

fn init_fg_rows(rows: u32) -> Vec<Vec<CellText>> {
    (0..(rows as usize + CURSOR_ROW_SLOTS))
        .map(|_| Vec::new())
        .collect()
}

fn color_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
}

fn image_copy_region(
    buffer_offset: u64,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
) -> vk::BufferImageCopy {
    vk::BufferImageCopy::default()
        .buffer_offset(buffer_offset)
        .buffer_row_length(0) // tightly packed — same as bytes_per_row = w * bpp
        .buffer_image_height(0) // tightly packed
        .image_subresource(
            vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .mip_level(0)
                .base_array_layer(0)
                .layer_count(1),
        )
        .image_offset(vk::Offset3D {
            x: x as i32,
            y: y as i32,
            z: 0,
        })
        .image_extent(vk::Extent3D {
            width: w as u32,
            height: h as u32,
            depth: 1,
        })
}

// ----- descriptor / pipeline setup helpers -----

fn create_bg_descriptor_set_layout(device: &ash::Device) -> vk::DescriptorSetLayout {
    let bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    ];
    let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    unsafe {
        device
            .create_descriptor_set_layout(&info, None)
            .expect("create_descriptor_set_layout(grid.bg)")
    }
}

fn create_bg_descriptor_pool(device: &ash::Device) -> vk::DescriptorPool {
    let sizes = [
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: FRAMES_IN_FLIGHT as u32,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: FRAMES_IN_FLIGHT as u32,
        },
    ];
    let info = vk::DescriptorPoolCreateInfo::default()
        .max_sets(FRAMES_IN_FLIGHT as u32)
        .pool_sizes(&sizes);
    unsafe {
        device
            .create_descriptor_pool(&info, None)
            .expect("create_descriptor_pool(grid.bg)")
    }
}

fn create_text_uniform_descriptor_set_layout(
    device: &ash::Device,
) -> vk::DescriptorSetLayout {
    let bindings = [vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)];
    let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    unsafe {
        device
            .create_descriptor_set_layout(&info, None)
            .expect("create_descriptor_set_layout(grid.text uniform)")
    }
}

fn create_text_atlas_descriptor_set_layout(
    device: &ash::Device,
) -> vk::DescriptorSetLayout {
    let bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    ];
    let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    unsafe {
        device
            .create_descriptor_set_layout(&info, None)
            .expect("create_descriptor_set_layout(grid.text atlas)")
    }
}

fn create_text_descriptor_pool(device: &ash::Device) -> vk::DescriptorPool {
    let sizes = [
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: FRAMES_IN_FLIGHT as u32,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: 2,
        },
    ];
    let info = vk::DescriptorPoolCreateInfo::default()
        .max_sets((FRAMES_IN_FLIGHT + 1) as u32)
        .pool_sizes(&sizes);
    unsafe {
        device
            .create_descriptor_pool(&info, None)
            .expect("create_descriptor_pool(grid.text)")
    }
}

fn allocate_descriptor_sets(
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
            .expect("allocate_descriptor_sets")
    };
    let mut out = [vk::DescriptorSet::null(); FRAMES_IN_FLIGHT];
    out.copy_from_slice(&sets);
    out
}

fn allocate_one_descriptor_set(
    device: &ash::Device,
    pool: vk::DescriptorPool,
    layout: vk::DescriptorSetLayout,
) -> vk::DescriptorSet {
    let layouts = [layout];
    let info = vk::DescriptorSetAllocateInfo::default()
        .descriptor_pool(pool)
        .set_layouts(&layouts);
    unsafe {
        device
            .allocate_descriptor_sets(&info)
            .expect("allocate_descriptor_sets(one)")[0]
    }
}

fn update_bg_descriptor_set(
    device: &ash::Device,
    set: vk::DescriptorSet,
    uniform: &VulkanBuffer,
    cells: &VulkanBuffer,
) {
    let uniform_info = vk::DescriptorBufferInfo::default()
        .buffer(uniform.handle())
        .offset(0)
        .range(uniform.size());
    let uniform_infos = [uniform_info];
    let cells_info = vk::DescriptorBufferInfo::default()
        .buffer(cells.handle())
        .offset(0)
        .range(cells.size());
    let cells_infos = [cells_info];

    let writes = [
        vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(&uniform_infos),
        vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(1)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&cells_infos),
    ];
    unsafe {
        device.update_descriptor_sets(&writes, &[]);
    }
}

fn update_text_uniform_descriptor_set(
    device: &ash::Device,
    set: vk::DescriptorSet,
    uniform: &VulkanBuffer,
) {
    let uniform_info = vk::DescriptorBufferInfo::default()
        .buffer(uniform.handle())
        .offset(0)
        .range(uniform.size());
    let uniform_infos = [uniform_info];
    let writes = [vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(0)
        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
        .buffer_info(&uniform_infos)];
    unsafe {
        device.update_descriptor_sets(&writes, &[]);
    }
}

fn update_text_atlas_descriptor_set(
    device: &ash::Device,
    set: vk::DescriptorSet,
    grayscale: &VulkanImage,
    color: &VulkanImage,
    sampler: vk::Sampler,
) {
    let gray_info = vk::DescriptorImageInfo::default()
        .sampler(sampler)
        .image_view(grayscale.view())
        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
    let gray_infos = [gray_info];
    let color_info = vk::DescriptorImageInfo::default()
        .sampler(sampler)
        .image_view(color.view())
        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
    let color_infos = [color_info];
    let writes = [
        vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&gray_infos),
        vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(1)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&color_infos),
    ];
    unsafe {
        device.update_descriptor_sets(&writes, &[]);
    }
}

fn create_pipeline_layout(
    device: &ash::Device,
    set_layouts: &[vk::DescriptorSetLayout],
) -> vk::PipelineLayout {
    let info = vk::PipelineLayoutCreateInfo::default().set_layouts(set_layouts);
    unsafe {
        device
            .create_pipeline_layout(&info, None)
            .expect("create_pipeline_layout(grid)")
    }
}

fn create_sampler(device: &ash::Device) -> vk::Sampler {
    // Nearest filter + clamp-to-edge — matches Metal's
    // `filter::nearest, address::clamp_to_edge`. Not used for
    // sampling per se (we use `texelFetch` in the fragment shader),
    // but the COMBINED_IMAGE_SAMPLER descriptor still requires a
    // sampler object.
    let info = vk::SamplerCreateInfo::default()
        .mag_filter(vk::Filter::NEAREST)
        .min_filter(vk::Filter::NEAREST)
        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE);
    unsafe {
        device
            .create_sampler(&info, None)
            .expect("create_sampler(grid.text)")
    }
}

fn create_bg_pipeline(
    device: &ash::Device,
    pipeline_cache: vk::PipelineCache,
    layout: vk::PipelineLayout,
    color_format: vk::Format,
) -> vk::Pipeline {
    build_pipeline(
        device,
        pipeline_cache,
        layout,
        color_format,
        BG_VERT_SPV,
        BG_FRAG_SPV,
        &[], // no vertex bindings
        &[],
        vk::PrimitiveTopology::TRIANGLE_LIST,
        BlendMode::Premultiplied, // bg uses src=SRC_ALPHA
    )
}

fn create_text_pipeline(
    device: &ash::Device,
    pipeline_cache: vk::PipelineCache,
    layout: vk::PipelineLayout,
    color_format: vk::Format,
) -> vk::Pipeline {
    let bindings = [vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(std::mem::size_of::<CellText>() as u32)
        .input_rate(vk::VertexInputRate::INSTANCE)];
    let attrs = [
        // 0: glyph_pos uvec2 @ 0
        vk::VertexInputAttributeDescription::default()
            .location(0)
            .binding(0)
            .format(vk::Format::R32G32_UINT)
            .offset(0),
        // 1: glyph_size uvec2 @ 8
        vk::VertexInputAttributeDescription::default()
            .location(1)
            .binding(0)
            .format(vk::Format::R32G32_UINT)
            .offset(8),
        // 2: bearings ivec2 @ 16 (stored as i16x2)
        vk::VertexInputAttributeDescription::default()
            .location(2)
            .binding(0)
            .format(vk::Format::R16G16_SINT)
            .offset(16),
        // 3: grid_pos uvec2 @ 20 (stored as u16x2)
        vk::VertexInputAttributeDescription::default()
            .location(3)
            .binding(0)
            .format(vk::Format::R16G16_UINT)
            .offset(20),
        // 4: color vec4 @ 24 (UNORM8)
        vk::VertexInputAttributeDescription::default()
            .location(4)
            .binding(0)
            .format(vk::Format::R8G8B8A8_UNORM)
            .offset(24),
        // 5: atlas u8 @ 28 → uint
        vk::VertexInputAttributeDescription::default()
            .location(5)
            .binding(0)
            .format(vk::Format::R8_UINT)
            .offset(28),
        // 6: bools u8 @ 29 → uint
        vk::VertexInputAttributeDescription::default()
            .location(6)
            .binding(0)
            .format(vk::Format::R8_UINT)
            .offset(29),
    ];
    build_pipeline(
        device,
        pipeline_cache,
        layout,
        color_format,
        TEXT_VERT_SPV,
        TEXT_FRAG_SPV,
        &bindings,
        &attrs,
        vk::PrimitiveTopology::TRIANGLE_STRIP,
        BlendMode::PremultipliedOverFromOne, // text fragment returns premultiplied
    )
}

#[derive(Copy, Clone)]
enum BlendMode {
    /// Source RGB factor = `SRC_ALPHA`. For shaders that return
    /// non-premultiplied RGBA + alpha (the bg pass).
    Premultiplied,
    /// Source RGB factor = `ONE`. For shaders that return
    /// already-premultiplied RGBA (the text pass — `in.color * mask_a`
    /// and the color atlas sample are both premultiplied).
    PremultipliedOverFromOne,
}

#[allow(clippy::too_many_arguments)]
fn build_pipeline(
    device: &ash::Device,
    pipeline_cache: vk::PipelineCache,
    layout: vk::PipelineLayout,
    color_format: vk::Format,
    vert_spv: &[u8],
    frag_spv: &[u8],
    vertex_bindings: &[vk::VertexInputBindingDescription],
    vertex_attrs: &[vk::VertexInputAttributeDescription],
    topology: vk::PrimitiveTopology,
    blend: BlendMode,
) -> vk::Pipeline {
    let vert = load_shader_module(device, vert_spv);
    let frag = load_shader_module(device, frag_spv);

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

    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(vertex_bindings)
        .vertex_attribute_descriptions(vertex_attrs);

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(topology)
        .primitive_restart_enable(false);

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

    let (src_rgb, dst_rgb) = match blend {
        BlendMode::Premultiplied => (
            vk::BlendFactor::SRC_ALPHA,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        ),
        BlendMode::PremultipliedOverFromOne => {
            (vk::BlendFactor::ONE, vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        }
    };
    let blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(src_rgb)
        .dst_color_blend_factor(dst_rgb)
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
            .expect("create_graphics_pipelines(grid)")[0]
    };

    unsafe {
        device.destroy_shader_module(vert, None);
        device.destroy_shader_module(frag, None);
    }
    pipeline
}

fn load_shader_module(device: &ash::Device, bytes: &[u8]) -> vk::ShaderModule {
    let code = ash::util::read_spv(&mut std::io::Cursor::new(bytes))
        .expect("read_spv (embedded grid shader is valid)");
    let info = vk::ShaderModuleCreateInfo::default().code(&code);
    unsafe {
        device
            .create_shader_module(&info, None)
            .expect("create_shader_module(grid)")
    }
}
