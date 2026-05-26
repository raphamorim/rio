// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Metal backend for the grid renderer.
//!
//! Phase 1a: `bg` pass only. Fullscreen triangle + per-fragment cell
//! lookup from `bg_buffers[0]`. Triple-buffering of the bg buffer is
//! stubbed (the field is reserved) but not yet used — Phase 1c will
//! add a GPU completion handler + semaphore gate. For now slot 0 is
//! written and read on every frame.
//!
//! `ghostty/src/renderer/cell.zig` allocation model:
//! one flat `CellBg` buffer indexed `row * cols + col`, one
//! `ArrayList(CellText)` per row plus two cursor slots. The per-row
//! FG storage lands in Phase 1c alongside `cell_text` shader port.

use metal::{
    Buffer, CommandQueue, CompileOptions, Device, MTLBlendFactor, MTLBlendOperation,
    MTLPixelFormat, MTLPrimitiveType, MTLRegion, MTLResourceOptions, MTLTextureUsage,
    MTLVertexFormat, MTLVertexStepFunction, RenderCommandEncoderRef,
    RenderPipelineDescriptor, RenderPipelineState, Texture, TextureDescriptor,
    VertexDescriptor,
};
use parking_lot::{Condvar, Mutex};
use rustc_hash::FxHashMap;
use std::sync::Arc;

use super::atlas::{AtlasSlot, GlyphKey, RasterizedGlyph};
use super::cell::{CellBg, CellText, GridUniforms};
use crate::context::metal::MetalContext;
use crate::renderer::image_cache::atlas::AtlasAllocator;

/// Number of GPU buffer slots cycled through frame-by-frame so the
/// CPU can populate frame N+1 while the GPU is still drawing N. Sized
/// to match Metal `swap_chain_count` at
/// `ghostty/src/renderer/Metal.zig:37`.
const FRAMES_IN_FLIGHT: usize = 3;

/// Number of in-flight frames the swap chain owns. Re-export here
/// so the Renderer-level swap chain (which advances frame_index +
/// owns the permit pool) can size itself off the same constant the
/// per-grid buffer arrays use.
pub const FRAMES_IN_FLIGHT_PUB: usize = FRAMES_IN_FLIGHT;

/// Cross-thread semaphore — `FRAMES_IN_FLIGHT_PUB` permits,
/// decremented on render-start, restored in the command-buffer
/// completion handler. Mirrors ghostty's `frame_sema` at
/// `renderer/generic.zig:261` (`std.Thread.Semaphore` →
/// `dispatch_semaphore_t` on Darwin). We use parking_lot's
/// Mutex+Condvar instead of dispatch_semaphore because the rest of
/// sugarloaf already pulls parking_lot in and the behavior is
/// identical for a counting semaphore.
///
/// One pool is shared across all grids on a `Renderer`, so a render
/// only acquires one permit even with N split panels.
pub type FramePermits = Arc<(Mutex<usize>, Condvar)>;

/// Construct a permit pool with `FRAMES_IN_FLIGHT` initial permits.
pub fn new_frame_permits() -> FramePermits {
    Arc::new((Mutex::new(FRAMES_IN_FLIGHT), Condvar::new()))
}

/// Block until a permit is available, then take one. Called once per
/// frame at render start.
pub fn acquire_frame_permit(p: &FramePermits) {
    let (m, c) = &**p;
    let mut g = m.lock();
    while *g == 0 {
        c.wait(&mut g);
    }
    *g -= 1;
}

/// Release a permit, signaling any frame waiting on
/// `acquire_frame_permit`. Called from the command-buffer completion
/// handler — i.e. on a Metal-internal thread, not the main thread.
pub fn release_frame_permit(p: &FramePermits) {
    let (m, c) = &**p;
    let mut g = m.lock();
    *g += 1;
    c.notify_one();
}

/// Extra slots appended to the per-row fg storage for cursor glyphs.
/// `rows + 2` layout (block cursor at slot 0,
/// non-block-style cursor at the tail).
const CURSOR_ROW_SLOTS: usize = 2;

/// Initial square atlas texture side. 2048² @ R8 = 4 MiB, grown to
/// 4096² / 8192² on demand when the allocator reports full (see
/// `MetalGlyphAtlas::grow`). `atlas.grow` in
/// `ghostty/src/font/Atlas.zig`.
const ATLAS_SIZE: u16 = 2048;

/// Hard cap on atlas side — Metal textures support 16384² on Apple
/// Silicon but 8192² is the safe floor across Intel Mac + discrete
/// GPUs. Beyond this we'd need a multi-atlas strategy.
const ATLAS_MAX_SIZE: u16 = 8192;

/// Glyph atlas for grayscale OR color glyphs. A single instance
/// holds one `MTLTexture`, an allocator, and the key→slot map; the
/// `bytes_per_pixel` field lets the same struct serve both paths
/// (R8 for mask glyphs, RGBA8 for color emoji).
/// split between `atlas_grayscale` and `atlas_color`
/// — owned by the renderer rather
/// than the font subsystem.
pub struct MetalGlyphAtlas {
    pub(crate) texture: Texture,
    allocator: AtlasAllocator,
    slots: FxHashMap<GlyphKey, AtlasSlot>,
    bytes_per_pixel: u32,
    format: MTLPixelFormat,
    /// Persist for `set_label` on the grown texture so Xcode's GPU
    /// debugger still identifies it after a grow.
    label: &'static str,
}

impl MetalGlyphAtlas {
    pub fn new_grayscale(device: &Device) -> Self {
        Self::new(device, MTLPixelFormat::R8Unorm, 1, "grid.atlas_grayscale")
    }

    pub fn new_color(device: &Device) -> Self {
        // RGBA8Unorm because macOS `rasterize_glyph` returns RGBA
        // premultiplied bytes for color emoji. BGRA would need a
        // byte swap on upload; RGBA is the zero-cost path.
        Self::new(device, MTLPixelFormat::RGBA8Unorm, 4, "grid.atlas_color")
    }

    fn new(
        device: &Device,
        format: MTLPixelFormat,
        bytes_per_pixel: u32,
        label: &'static str,
    ) -> Self {
        let texture = create_atlas_texture(device, format, ATLAS_SIZE, label);

        Self {
            texture,
            allocator: AtlasAllocator::new(ATLAS_SIZE, ATLAS_SIZE),
            slots: FxHashMap::default(),
            bytes_per_pixel,
            format,
            label,
        }
    }

    /// Double the atlas texture + allocator dimensions, copying old
    /// texel data into the top-left of the new texture via a blit.
    /// Existing `AtlasSlot`s stay valid because their `(x, y)` fall
    /// inside the unchanged old region. Returns `false` if the atlas
    /// is already at `ATLAS_MAX_SIZE` (caller must handle the failure
    /// — there's no eviction).
    pub fn grow(&mut self, device: &Device, queue: &CommandQueue) -> bool {
        let (old_w, old_h) = self.allocator.dimensions();
        if old_w >= ATLAS_MAX_SIZE {
            return false;
        }
        let new_size = old_w.saturating_mul(2).min(ATLAS_MAX_SIZE);
        if new_size <= old_w {
            return false;
        }

        let new_texture = create_atlas_texture(device, self.format, new_size, self.label);

        // Blit the old texture into the top-left of the new one.
        // Slots are still addressed by their original (x, y) so we
        // don't touch the allocator's shelf layout, just its bounds.
        let cmd_buffer = queue.new_command_buffer();
        let blit = cmd_buffer.new_blit_command_encoder();
        blit.copy_from_texture(
            &self.texture,
            0,
            0,
            metal::MTLOrigin { x: 0, y: 0, z: 0 },
            metal::MTLSize {
                width: old_w as u64,
                height: old_h as u64,
                depth: 1,
            },
            &new_texture,
            0,
            0,
            metal::MTLOrigin { x: 0, y: 0, z: 0 },
        );
        blit.end_encoding();
        cmd_buffer.commit();
        // Wait so subsequent `replace_region` writes to the new
        // texture don't race the blit.
        cmd_buffer.wait_until_completed();

        self.texture = new_texture;
        self.allocator.grow_to(new_size, new_size);
        true
    }

    #[inline]
    pub fn lookup(&self, key: GlyphKey) -> Option<AtlasSlot> {
        self.slots.get(&key).copied()
    }

    /// Pack + upload a rasterized glyph. Returns `None` when the
    /// atlas is full. `glyph.bytes` length must be
    /// `glyph.width * glyph.height * bytes_per_pixel`.
    pub fn insert(
        &mut self,
        key: GlyphKey,
        glyph: RasterizedGlyph<'_>,
    ) -> Option<AtlasSlot> {
        if glyph.width == 0 || glyph.height == 0 {
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

        let region = MTLRegion {
            origin: metal::MTLOrigin {
                x: x as u64,
                y: y as u64,
                z: 0,
            },
            size: metal::MTLSize {
                width: glyph.width as u64,
                height: glyph.height as u64,
                depth: 1,
            },
        };
        self.texture.replace_region(
            region,
            0,
            glyph.bytes.as_ptr() as *const std::ffi::c_void,
            (glyph.width as u64) * (self.bytes_per_pixel as u64),
        );

        Some(slot)
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.allocator.clear();
        self.slots.clear();
    }
}

fn create_atlas_texture(
    device: &Device,
    format: MTLPixelFormat,
    size: u16,
    label: &str,
) -> Texture {
    let descriptor = TextureDescriptor::new();
    descriptor.set_width(size as u64);
    descriptor.set_height(size as u64);
    descriptor.set_pixel_format(format);
    // Apple Silicon + other UMA devices (`hasUnifiedMemory`) can back
    // the texture in shared memory — `replaceRegion` becomes a plain
    // memcpy with no CPU/GPU coherency sync. Discrete-GPU Macs
    // (pre-M1) still need `Managed` with an implicit sync on draw.
    // `src/renderer/Metal.zig:79-83`.
    descriptor.set_storage_mode(if device.has_unified_memory() {
        metal::MTLStorageMode::Shared
    } else {
        metal::MTLStorageMode::Managed
    });
    descriptor.set_usage(MTLTextureUsage::ShaderRead);
    let texture = device.new_texture(&descriptor);
    texture.set_label(label);
    texture
}

pub struct MetalGridRenderer {
    device: Device,
    /// Needed for atlas-grow blits. Keeping a handle lets us submit
    /// a one-off command buffer without threading the queue through
    /// every emit-time call site.
    command_queue: CommandQueue,

    /// Current grid size (cells).
    cols: u32,
    rows: u32,

    /// CPU-side shadow of the per-cell bg buffer. `write_row` /
    /// `clear_row` mutate this directly; `render_bg` flushes it into
    /// the active frame slot's GPU buffer when the slot is dirty.
    /// Decoupling the writes from the GPU buffer is what makes
    /// triple-buffering safe — the CPU can mutate `bg_cpu` while the
    /// GPU is still reading from a previous slot.
    bg_cpu: Vec<CellBg>,

    /// One GPU `CellBg` buffer per in-flight frame slot. The slot
    /// active for a given frame is selected by `next_frame()` and
    /// stored in `self.frame`.
    bg_buffers: [Buffer; FRAMES_IN_FLIGHT],

    /// Per-slot dirty flag. Any `write_row` / `clear_row` sets all
    /// `FRAMES_IN_FLIGHT` flags so each slot re-flushes from `bg_cpu`
    /// when it next becomes the active frame; the slot's flag is
    /// cleared after the per-slot flush in `render_bg`.
    bg_dirty: [bool; FRAMES_IN_FLIGHT],

    /// Per-row FG glyph storage (CPU-side, single copy across all
    /// frame slots). Slot 0 = block cursor cells, 1..=rows = content
    /// rows, last = non-block cursor cells.
    fg_rows: Vec<Vec<CellText>>,

    /// One per-instance `CellText` vertex buffer per in-flight frame
    /// slot. Re-uploaded from `fg_rows` on the slot's first frame
    /// after a write.
    fg_buffers: [Buffer; FRAMES_IN_FLIGHT],
    fg_capacity: [usize; FRAMES_IN_FLIGHT],

    /// Compiled bg render pipeline. Binds:
    /// buffer(0): `GridUniforms` (via `set_vertex_bytes` /
    /// `set_fragment_bytes`)
    /// buffer(1): `bg_buffers[frame]`
    bg_pipeline: RenderPipelineState,

    /// Compiled text render pipeline. Binds:
    /// buffer(0): per-instance `CellText` vertex buffer
    /// buffer(1): `GridUniforms`
    /// texture(0): `atlas_grayscale`
    /// texture(1): `atlas_color`
    text_pipeline: RenderPipelineState,

    /// Staging buffer for the concatenated fg instances. Rebuilt each
    /// frame by flattening `fg_rows` into a contiguous slice.
    fg_staging: Vec<CellText>,

    /// Per-slot live instance count from the most recent flush.
    fg_live_count: [u32; FRAMES_IN_FLIGHT],

    /// Per-slot dirty flag for the fg path. Any row-level write sets
    /// all flags; the slot's flag is cleared by `render_text` after a
    /// successful flush.
    fg_dirty: [bool; FRAMES_IN_FLIGHT],

    /// Grayscale (R8) glyph atlas — outline mask bitmaps from the
    /// monochrome rasterizer path.
    atlas_grayscale: MetalGlyphAtlas,

    /// Color (RGBA8) glyph atlas — premultiplied bitmaps from
    /// CoreText's color-emoji rasterizer. Same allocator + slot
    /// bookkeeping as the grayscale atlas; the text fragment
    /// shader picks between them via `CellText.atlas`
    /// (`ATLAS_GRAYSCALE` vs `ATLAS_COLOR`).
    atlas_color: MetalGlyphAtlas,

    /// Set to `true` on construction + `resize()`. The emission
    /// path checks this to force a full rebuild (every row) on the
    /// next frame, regardless of whether `TerminalDamage` is
    /// `Noop`. `grid_size_diff` gate at
    /// `ghostty/src/renderer/generic.zig:2353`. Cleared via
    /// `mark_full_rebuild_done` after the emission loop runs.
    needs_full_rebuild: bool,
}

impl MetalGridRenderer {
    pub fn new(ctx: &MetalContext, cols: u32, rows: u32) -> Self {
        let device = ctx.device.to_owned();
        let command_queue = ctx.command_queue.to_owned();
        let bg_buffers = std::array::from_fn(|_| alloc_bg_buffer(&device, cols, rows));
        let initial_fg_capacity = (cols as usize) * (rows as usize).max(1);
        let fg_buffers =
            std::array::from_fn(|_| alloc_fg_buffer(&device, initial_fg_capacity));
        let fg_capacity = [initial_fg_capacity; FRAMES_IN_FLIGHT];

        let bg_pipeline = build_bg_pipeline(&device);
        let text_pipeline = build_text_pipeline(&device);
        let atlas_grayscale = MetalGlyphAtlas::new_grayscale(&device);
        let atlas_color = MetalGlyphAtlas::new_color(&device);

        let bg_cpu_len = (cols as usize) * (rows as usize);
        let bg_cpu = vec![CellBg::TRANSPARENT; bg_cpu_len];

        Self {
            device,
            command_queue,
            cols,
            rows,
            bg_cpu,
            bg_buffers,
            bg_dirty: [true; FRAMES_IN_FLIGHT],
            fg_rows: init_fg_rows(rows),
            fg_buffers,
            fg_capacity,
            bg_pipeline,
            text_pipeline,
            fg_staging: Vec::new(),
            fg_live_count: [0; FRAMES_IN_FLIGHT],
            fg_dirty: [true; FRAMES_IN_FLIGHT],
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

    /// Lookup a glyph in the grayscale atlas.
    pub fn lookup_glyph(&self, key: GlyphKey) -> Option<AtlasSlot> {
        self.atlas_grayscale.lookup(key)
    }

    /// Pack + upload a grayscale rasterized glyph. On atlas-full,
    /// grows the atlas (doubles the texture, blits old texels into
    /// the top-left) and retries once. Returns `None` only if the
    /// atlas is at `ATLAS_MAX_SIZE` and still can't fit the glyph.
    pub fn insert_glyph(
        &mut self,
        key: GlyphKey,
        glyph: RasterizedGlyph<'_>,
    ) -> Option<AtlasSlot> {
        if let Some(slot) = self.atlas_grayscale.insert(key, glyph) {
            return Some(slot);
        }
        if self.atlas_grayscale.grow(&self.device, &self.command_queue) {
            self.atlas_grayscale.insert(key, glyph)
        } else {
            None
        }
    }

    /// Lookup a glyph in the color atlas.
    pub fn lookup_glyph_color(&self, key: GlyphKey) -> Option<AtlasSlot> {
        self.atlas_color.lookup(key)
    }

    /// Pack + upload a color (RGBA8-premultiplied) rasterized glyph.
    /// Same grow-on-full behaviour as `insert_glyph`.
    pub fn insert_glyph_color(
        &mut self,
        key: GlyphKey,
        glyph: RasterizedGlyph<'_>,
    ) -> Option<AtlasSlot> {
        if let Some(slot) = self.atlas_color.insert(key, glyph) {
            return Some(slot);
        }
        if self.atlas_color.grow(&self.device, &self.command_queue) {
            self.atlas_color.insert(key, glyph)
        } else {
            None
        }
    }

    pub fn resize(&mut self, cols: u32, rows: u32) {
        if cols == self.cols && rows == self.rows {
            return;
        }
        self.cols = cols;
        self.rows = rows;
        self.bg_cpu = vec![CellBg::TRANSPARENT; (cols as usize) * (rows as usize)];
        self.bg_buffers =
            std::array::from_fn(|_| alloc_bg_buffer(&self.device, cols, rows));
        self.fg_rows = init_fg_rows(rows);
        let initial_fg_capacity = (cols as usize) * (rows as usize).max(1);
        self.fg_buffers =
            std::array::from_fn(|_| alloc_fg_buffer(&self.device, initial_fg_capacity));
        self.fg_capacity = [initial_fg_capacity; FRAMES_IN_FLIGHT];
        // Fresh buffers = zero contents; emission path must rewrite
        // every row on the next frame even if no damage came in. All
        // 3 in-flight slots are now stale so each one needs a flush
        // when it next becomes the active frame.
        self.needs_full_rebuild = true;
        self.bg_dirty = [true; FRAMES_IN_FLIGHT];
        self.fg_dirty = [true; FRAMES_IN_FLIGHT];
        self.fg_live_count = [0; FRAMES_IN_FLIGHT];
    }

    pub fn write_row(&mut self, row: u32, bg: &[CellBg], fg: &[CellText]) {
        // FG: stash in the CPU-side per-row vec. All in-flight slots
        // need re-flushing — set every dirty flag so each slot
        // re-uploads when it becomes the active frame.
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
        let dst = &mut self.bg_cpu[row_start..row_start + self.cols as usize];
        dst[..row_len].copy_from_slice(&bg[..row_len]);
        for slot in &mut dst[row_len..] {
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
        let dst = &mut self.bg_cpu[row_start..row_start + self.cols as usize];
        let needs_flush = dst.iter().any(|c| *c != CellBg::TRANSPARENT);
        for slot in dst {
            *slot = CellBg::TRANSPARENT;
        }
        if needs_flush {
            self.bg_dirty = [true; FRAMES_IN_FLIGHT];
        }
    }

    /// Replace the block cursor sprite slot. Drawn FIRST in the text
    /// pass (slot 0) — sits BEHIND row glyphs so text inversion can
    /// composite on top of the block. "block" cursor
    /// slot at `fg_rows[0]`.
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

    /// Replace the non-block cursor sprite slot. Drawn LAST in the
    /// text pass — sits on top of all row glyphs. Used for hollow /
    /// bar / underline cursor sprites that should overlay text. Pass
    /// `&[]` to clear. "non-block" cursor slot at
    /// `fg_rows[rows + 1]`.
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

    /// Empty both cursor slots (block + non-block). Call once per
    /// frame before deciding whether to emit a cursor sprite for
    /// this panel — without this, the previous frame's sprite stays
    /// resident in fg_rows.
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

    /// Record the cell-bg pass against the caller's `encoder`. Drawn
    /// as a fullscreen triangle; the fragment shader resolves each
    /// pixel back to its owning cell and reads the per-cell color
    /// from `bg_buffers[frame]`. Caller owns command buffer +
    /// drawable + pass descriptor; `frame` comes from the Renderer's
    /// shared swap chain (acquired via `acquire_frame_permit` once
    /// per render). Pair with `render_text` after compositing any
    /// `kitty_below_text` images in between (matches
    /// `renderer/generic.zig:1654-1668`).
    pub fn render_bg(
        &mut self,
        encoder: &RenderCommandEncoderRef,
        frame: usize,
        uniforms: &GridUniforms,
    ) {
        // Flush the CPU shadow into this slot's GPU buffer if it
        // hasn't been flushed since the last `write_row`. Each slot
        // tracks its own dirty bit so all 3 in-flight slots stay
        // consistent without re-uploading on every frame.
        if self.bg_dirty[frame] {
            let bytes = bytemuck::cast_slice::<CellBg, u8>(&self.bg_cpu);
            unsafe {
                let dst = self.bg_buffers[frame].contents() as *mut u8;
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
            }
            self.bg_dirty[frame] = false;
        }

        let uniforms_bytes = bytemuck::bytes_of(uniforms);
        encoder.set_render_pipeline_state(&self.bg_pipeline);
        encoder.set_vertex_bytes(
            0,
            uniforms_bytes.len() as u64,
            uniforms_bytes.as_ptr() as *const std::ffi::c_void,
        );
        encoder.set_fragment_bytes(
            0,
            uniforms_bytes.len() as u64,
            uniforms_bytes.as_ptr() as *const std::ffi::c_void,
        );
        encoder.set_fragment_buffer(1, Some(&self.bg_buffers[frame]), 0);
        encoder.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
    }

    /// Record the cell-text pass. One instanced quad per `CellText`
    /// in `fg_rows`. Lazily flushes the per-row CPU vecs into
    /// `fg_buffers[frame]` only when the slot is dirty — on a
    /// Noop/CursorOnly damage frame (blink tick, scrollbar fade,
    /// etc.) this is the whole difference between ~0 µs and ~(rows ×
    /// cols × 32 B) of wasted CPU work per frame.
    pub fn render_text(
        &mut self,
        encoder: &RenderCommandEncoderRef,
        frame: usize,
        uniforms: &GridUniforms,
    ) {
        if self.fg_dirty[frame] {
            // Flatten per-row fg_rows into the staging vec. Order matters
            // for z: slot 0 (block cursor) first, content rows next,
            // non-block-cursor slot last — same approach's ordering.
            self.fg_staging.clear();
            for row in &self.fg_rows {
                self.fg_staging.extend_from_slice(row);
            }

            if self.fg_staging.len() > self.fg_capacity[frame] {
                let new_cap = self.fg_staging.len().next_power_of_two();
                self.fg_buffers[frame] = alloc_fg_buffer(&self.device, new_cap);
                self.fg_capacity[frame] = new_cap;
            }

            // Upload staging → GPU buffer. Shared storage mode means the
            // CPU pointer is the GPU pointer.
            let fg_bytes = bytemuck::cast_slice::<CellText, u8>(&self.fg_staging);
            unsafe {
                let dst = self.fg_buffers[frame].contents() as *mut u8;
                std::ptr::copy_nonoverlapping(fg_bytes.as_ptr(), dst, fg_bytes.len());
            }

            self.fg_live_count[frame] = self.fg_staging.len() as u32;
            self.fg_dirty[frame] = false;
        }

        let instance_count = self.fg_live_count[frame] as usize;
        if instance_count == 0 {
            return;
        }

        let uniforms_bytes = bytemuck::bytes_of(uniforms);
        encoder.set_render_pipeline_state(&self.text_pipeline);
        encoder.set_vertex_buffer(0, Some(&self.fg_buffers[frame]), 0);
        encoder.set_vertex_bytes(
            1,
            uniforms_bytes.len() as u64,
            uniforms_bytes.as_ptr() as *const std::ffi::c_void,
        );
        encoder.set_fragment_texture(0, Some(&self.atlas_grayscale.texture));
        encoder.set_fragment_texture(1, Some(&self.atlas_color.texture));

        encoder.draw_primitives_instanced(
            MTLPrimitiveType::TriangleStrip,
            0,
            4,
            instance_count as u64,
        );
    }
}

fn build_text_pipeline(device: &Device) -> RenderPipelineState {
    let shader_source = include_str!("shaders/grid.metal");
    let library = device
        .new_library_with_source(shader_source, &CompileOptions::new())
        .expect("grid.metal failed to compile (text)");

    let vertex_fn = library
        .get_function("grid_text_vertex", None)
        .expect("grid_text_vertex not found");
    let fragment_fn = library
        .get_function("grid_text_fragment", None)
        .expect("grid_text_fragment not found");

    // Per-instance vertex descriptor for `CellText`. Offsets match
    // `CellText` in cell.rs; attribute indices match the
    // `[[attribute(N)]]` tags in `grid_text_vertex` in grid.metal.
    let vd = VertexDescriptor::new();
    let attrs = vd.attributes();
    // attribute 0: glyph_pos: [u32; 2] @ offset 0
    let a = attrs.object_at(0).unwrap();
    a.set_format(MTLVertexFormat::UInt2);
    a.set_buffer_index(0);
    a.set_offset(0);
    // attribute 1: glyph_size: [u32; 2] @ offset 8
    let a = attrs.object_at(1).unwrap();
    a.set_format(MTLVertexFormat::UInt2);
    a.set_buffer_index(0);
    a.set_offset(8);
    // attribute 2: bearings: [i16; 2] @ offset 16 → Short2 (sign-ext to int2)
    let a = attrs.object_at(2).unwrap();
    a.set_format(MTLVertexFormat::Short2);
    a.set_buffer_index(0);
    a.set_offset(16);
    // attribute 3: grid_pos: [u16; 2] @ offset 20 → UShort2 (zero-ext to ushort2)
    let a = attrs.object_at(3).unwrap();
    a.set_format(MTLVertexFormat::UShort2);
    a.set_buffer_index(0);
    a.set_offset(20);
    // attribute 4: color: [u8; 4] @ offset 24 → UChar4
    let a = attrs.object_at(4).unwrap();
    a.set_format(MTLVertexFormat::UChar4);
    a.set_buffer_index(0);
    a.set_offset(24);
    // attribute 5: atlas: u8 @ offset 28 → UChar
    let a = attrs.object_at(5).unwrap();
    a.set_format(MTLVertexFormat::UChar);
    a.set_buffer_index(0);
    a.set_offset(28);
    // attribute 6: bools: u8 @ offset 29 → UChar
    let a = attrs.object_at(6).unwrap();
    a.set_format(MTLVertexFormat::UChar);
    a.set_buffer_index(0);
    a.set_offset(29);
    // Layout: per-instance, stride = sizeof(CellText) = 32.
    let layout = vd.layouts().object_at(0).unwrap();
    layout.set_stride(std::mem::size_of::<CellText>() as u64);
    layout.set_step_function(MTLVertexStepFunction::PerInstance);
    layout.set_step_rate(1);

    let descriptor = RenderPipelineDescriptor::new();
    descriptor.set_label("grid.text");
    descriptor.set_vertex_function(Some(&vertex_fn));
    descriptor.set_fragment_function(Some(&fragment_fn));
    descriptor.set_vertex_descriptor(Some(vd));

    let color = descriptor
        .color_attachments()
        .object_at(0)
        .expect("color attachment 0 missing");
    color.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
    color.set_blending_enabled(true);
    // Premultiplied-over, matching .
    // The text fragment returns `in.color * mask_a` (grayscale path)
    // or the color-atlas sample directly (emoji) — both premultiplied
    // already, so source RGB factor must be `One`, not `SourceAlpha`.
    color.set_source_rgb_blend_factor(MTLBlendFactor::One);
    color.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    color.set_rgb_blend_operation(MTLBlendOperation::Add);
    color.set_source_alpha_blend_factor(MTLBlendFactor::One);
    color.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    color.set_alpha_blend_operation(MTLBlendOperation::Add);

    device
        .new_render_pipeline_state(&descriptor)
        .expect("grid.text pipeline state creation failed")
}

fn build_bg_pipeline(device: &Device) -> RenderPipelineState {
    let shader_source = include_str!("shaders/grid.metal");
    let library = device
        .new_library_with_source(shader_source, &CompileOptions::new())
        .expect("grid.metal failed to compile");

    let vertex_fn = library
        .get_function("grid_bg_vertex", None)
        .expect("grid_bg_vertex not found");
    let fragment_fn = library
        .get_function("grid_bg_fragment", None)
        .expect("grid_bg_fragment not found");

    let descriptor = RenderPipelineDescriptor::new();
    descriptor.set_label("grid.bg");
    descriptor.set_vertex_function(Some(&vertex_fn));
    descriptor.set_fragment_function(Some(&fragment_fn));
    // No vertex descriptor: the fullscreen triangle derives positions
    // from `[[vertex_id]]`, and the fragment samples the bg buffer by
    // screen position + uniforms.

    let color = descriptor
        .color_attachments()
        .object_at(0)
        .expect("color attachment 0 missing");
    // Must match the drawable format configured in
    // `sugarloaf/src/context/metal.rs:79`.
    color.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
    color.set_blending_enabled(true);
    // Premultiplied-over blend, matching sugarloaf's rich-text pipeline.
    color.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
    color.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    color.set_rgb_blend_operation(MTLBlendOperation::Add);
    color.set_source_alpha_blend_factor(MTLBlendFactor::One);
    color.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    color.set_alpha_blend_operation(MTLBlendOperation::Add);

    device
        .new_render_pipeline_state(&descriptor)
        .expect("grid.bg pipeline state creation failed")
}

fn alloc_bg_buffer(device: &Device, cols: u32, rows: u32) -> Buffer {
    let size = (cols as u64)
        .saturating_mul(rows as u64)
        .saturating_mul(std::mem::size_of::<CellBg>() as u64)
        .max(std::mem::size_of::<CellBg>() as u64);
    device.new_buffer(size, MTLResourceOptions::StorageModeShared)
}

fn alloc_fg_buffer(device: &Device, capacity: usize) -> Buffer {
    let size = (capacity as u64)
        .saturating_mul(std::mem::size_of::<CellText>() as u64)
        .max(std::mem::size_of::<CellText>() as u64);
    device.new_buffer(size, MTLResourceOptions::StorageModeShared)
}

fn init_fg_rows(rows: u32) -> Vec<Vec<CellText>> {
    (0..(rows as usize + CURSOR_ROW_SLOTS))
        .map(|_| Vec::new())
        .collect()
}
