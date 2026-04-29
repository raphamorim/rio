// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Persistent, row-indexed GPU cell buffer for the terminal grid.
//!
//! This is the target of a direct rewrite replacing sugarloaf's
//! rich-text-based terminal rendering (see
//! `memory/project_grid_gpu_renderer_plan.md`). The design mirrors
//! Metal renderer: one `bg_cells` flat buffer indexed by
//! `row * cols + col`, one `fg_rows` collection of per-row glyph lists
//! concatenated for GPU upload. Only dirty rows are rewritten between
//! frames; everything else stays resident in the GPU buffer.
//!
//! # Phase 0 status
//!
//! This module is scaffolding. Both backends allocate empty buffers,
//! accept writes, and expose a `render` no-op. Shaders land in Phase 1,
//! call sites in `rioterm::renderer` land in Phase 2.

pub mod atlas;
pub mod cell;
pub mod cpu;
#[cfg(target_os = "macos")]
pub mod metal;
#[cfg(target_os = "linux")]
pub mod vulkan;
#[cfg(feature = "wgpu")]
pub mod webgpu;

use crate::context::{Context, ContextType};

pub use atlas::{AtlasSlot, GlyphKey, RasterizedGlyph};
pub use cell::{CellBg, CellText, GridUniforms};

/// Backend-dispatching grid renderer. One of these lives per terminal
/// panel; it owns the per-panel cell buffers and submits grid draw
/// calls to the sugarloaf context's encoder / render pass.
///
/// Backend selection matches sugarloaf's existing `ContextType` —
/// there's no separate config knob for grid vs. rich-text, because
/// the rich-text terminal path is being removed.
///
/// The Vulkan variant is significantly larger than the others
/// because atlas and buffer-ring state lives inline, but only one
/// variant is ever constructed per panel; boxing the bigger one
/// would just trade stack size for one allocation per panel — not
/// worth it.
#[allow(clippy::large_enum_variant)]
pub enum GridRenderer {
    #[cfg(target_os = "macos")]
    Metal(metal::MetalGridRenderer),
    #[cfg(feature = "wgpu")]
    Wgpu(webgpu::WgpuGridRenderer),
    /// Native Vulkan grid renderer. Phase 3 = bg pass; text pass +
    /// atlases land in Phase 4.
    #[cfg(target_os = "linux")]
    Vulkan(vulkan::VulkanGridRenderer),
    /// Software grid renderer. Same `CellBg` / `CellText` storage as
    /// the GPU backends, blits into the softbuffer surface during
    /// `Sugarloaf::render_cpu` instead of recording GPU draws.
    Cpu(cpu::CpuGridRenderer),
}

impl GridRenderer {
    /// Construct a grid renderer matching `context`'s backend.
    pub fn new(context: &Context<'_>, cols: u32, rows: u32) -> Self {
        match &context.inner {
            #[cfg(target_os = "macos")]
            ContextType::Metal(ctx) => {
                GridRenderer::Metal(metal::MetalGridRenderer::new(ctx, cols, rows))
            }
            #[cfg(not(feature = "wgpu"))]
            ContextType::_Phantom(_) => unreachable!(),
            #[cfg(feature = "wgpu")]
            ContextType::Wgpu(ctx) => {
                GridRenderer::Wgpu(webgpu::WgpuGridRenderer::new(ctx, cols, rows))
            }
            #[cfg(target_os = "linux")]
            ContextType::Vulkan(ctx) => {
                GridRenderer::Vulkan(vulkan::VulkanGridRenderer::new(ctx, cols, rows))
            }
            ContextType::Cpu(_) => {
                GridRenderer::Cpu(cpu::CpuGridRenderer::new(cols, rows))
            }
        }
    }

    /// Reallocate buffers for a new grid size. Preserves nothing — the
    /// caller must rewrite every row after a resize.
    pub fn resize(&mut self, cols: u32, rows: u32) {
        match self {
            #[cfg(target_os = "macos")]
            GridRenderer::Metal(r) => r.resize(cols, rows),
            #[cfg(feature = "wgpu")]
            GridRenderer::Wgpu(r) => r.resize(cols, rows),
            #[cfg(target_os = "linux")]
            GridRenderer::Vulkan(r) => r.resize(cols, rows),
            GridRenderer::Cpu(r) => r.resize(cols, rows),
        }
    }

    /// Overwrite `row`'s background + foreground cells. `bg` must have
    /// exactly `cols` entries; `fg` is variable length (base glyph +
    /// decorations). Callers that want to clear a row should use
    /// `clear_row` instead — passing empty slices here is allowed but
    /// leaves the buffer in an inconsistent state.
    pub fn write_row(&mut self, row: u32, bg: &[CellBg], fg: &[CellText]) {
        match self {
            #[cfg(target_os = "macos")]
            GridRenderer::Metal(r) => r.write_row(row, bg, fg),
            #[cfg(feature = "wgpu")]
            GridRenderer::Wgpu(r) => r.write_row(row, bg, fg),
            #[cfg(target_os = "linux")]
            GridRenderer::Vulkan(r) => r.write_row(row, bg, fg),
            GridRenderer::Cpu(r) => r.write_row(row, bg, fg),
        }
    }

    /// Zero out `row`'s fg/bg slots. Corresponds to 's
    /// `self.cells.clear(y)`.
    pub fn clear_row(&mut self, row: u32) {
        match self {
            #[cfg(target_os = "macos")]
            GridRenderer::Metal(r) => r.clear_row(row),
            #[cfg(feature = "wgpu")]
            GridRenderer::Wgpu(r) => r.clear_row(row),
            #[cfg(target_os = "linux")]
            GridRenderer::Vulkan(r) => r.clear_row(row),
            GridRenderer::Cpu(r) => r.clear_row(row),
        }
    }

    /// Replace the block cursor sprite slot. Drawn FIRST in the text
    /// pass (`fg_rows[0]`) — sits BEHIND row glyphs so text inversion
    /// composites on top of the block.
    /// `Contents.setCursor` for the `.block` style.
    pub fn set_block_cursor(&mut self, cells: &[CellText]) {
        match self {
            #[cfg(target_os = "macos")]
            GridRenderer::Metal(r) => r.set_block_cursor(cells),
            #[cfg(feature = "wgpu")]
            GridRenderer::Wgpu(r) => r.set_block_cursor(cells),
            #[cfg(target_os = "linux")]
            GridRenderer::Vulkan(r) => r.set_block_cursor(cells),
            GridRenderer::Cpu(r) => r.set_block_cursor(cells),
        }
    }

    /// Replace the non-block cursor sprite slot. Drawn on top of all
    /// row glyphs in the text pass — used for hollow / bar /
    /// underline cursor sprites that overlay text. Pass `&[]` to
    /// clear. `Contents.setCursor` writing into
    /// `fg_rows[rows + 1]`.
    pub fn set_non_block_cursor(&mut self, cells: &[CellText]) {
        match self {
            #[cfg(target_os = "macos")]
            GridRenderer::Metal(r) => r.set_non_block_cursor(cells),
            #[cfg(feature = "wgpu")]
            GridRenderer::Wgpu(r) => r.set_non_block_cursor(cells),
            #[cfg(target_os = "linux")]
            GridRenderer::Vulkan(r) => r.set_non_block_cursor(cells),
            GridRenderer::Cpu(r) => r.set_non_block_cursor(cells),
        }
    }

    /// Empty both cursor slots (block + non-block). Call once per
    /// frame before deciding whether to emit a cursor sprite for
    /// this panel.
    pub fn clear_cursor(&mut self) {
        match self {
            #[cfg(target_os = "macos")]
            GridRenderer::Metal(r) => r.clear_cursor(),
            #[cfg(feature = "wgpu")]
            GridRenderer::Wgpu(r) => r.clear_cursor(),
            #[cfg(target_os = "linux")]
            GridRenderer::Vulkan(r) => r.clear_cursor(),
            GridRenderer::Cpu(r) => r.clear_cursor(),
        }
    }

    /// Record the cell-bg pass for this grid. `frame` is the swap
    /// chain slot index acquired from the Renderer's shared
    /// `FramePermits` pool — the grid uses it to pick the right
    /// per-slot GPU buffer. Caller composites `kitty_below_text`
    /// images between this call and `render_text_*` to match
    /// `renderer/generic.zig:1654-1668` ordering.
    #[cfg(target_os = "macos")]
    pub fn render_bg_metal(
        &mut self,
        encoder: &::metal::RenderCommandEncoderRef,
        frame: usize,
        uniforms: &GridUniforms,
    ) {
        if let GridRenderer::Metal(r) = self {
            r.render_bg(encoder, frame, uniforms);
        }
    }

    /// Record the cell-text pass for this grid. Caller composites
    /// `kitty_above_text` images after this call.
    #[cfg(target_os = "macos")]
    pub fn render_text_metal(
        &mut self,
        encoder: &::metal::RenderCommandEncoderRef,
        frame: usize,
        uniforms: &GridUniforms,
    ) {
        if let GridRenderer::Metal(r) = self {
            r.render_text(encoder, frame, uniforms);
        }
    }

    /// Wgpu cell-bg pass. Pair with `render_text_wgpu`.
    #[cfg(feature = "wgpu")]
    pub fn render_bg_wgpu(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        uniforms: &GridUniforms,
    ) {
        if let GridRenderer::Wgpu(r) = self {
            r.render_bg(render_pass, uniforms);
        }
    }

    /// Wgpu cell-text pass.
    #[cfg(feature = "wgpu")]
    pub fn render_text_wgpu(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        uniforms: &GridUniforms,
    ) {
        if let GridRenderer::Wgpu(r) = self {
            r.render_text(render_pass, uniforms);
        }
    }

    /// Pre-pass hook: flush atlas uploads before the caller opens
    /// dynamic rendering. Must be called BEFORE `cmd_begin_rendering`
    /// because `vkCmdCopyBufferToImage` is forbidden inside a render
    /// pass. No-op for non-Vulkan renderers (Metal handles uploads
    /// inside its own `replace_region`; wgpu handles them via
    /// `queue.write_texture`).
    #[cfg(target_os = "linux")]
    pub fn prepare_vulkan(
        &mut self,
        ctx: &crate::context::vulkan::VulkanContext,
        cmd_buffer: ash::vk::CommandBuffer,
        frame_slot: usize,
    ) {
        if let GridRenderer::Vulkan(r) = self {
            r.prepare(ctx, cmd_buffer, frame_slot);
        }
    }

    /// Vulkan cell-bg pass.
    #[cfg(target_os = "linux")]
    pub fn render_bg_vulkan(
        &mut self,
        ctx: &crate::context::vulkan::VulkanContext,
        cmd_buffer: ash::vk::CommandBuffer,
        frame_slot: usize,
        uniforms: &GridUniforms,
    ) {
        if let GridRenderer::Vulkan(r) = self {
            r.render_bg(ctx, cmd_buffer, frame_slot, uniforms);
        }
    }

    /// Vulkan cell-text pass.
    #[cfg(target_os = "linux")]
    pub fn render_text_vulkan(
        &mut self,
        ctx: &crate::context::vulkan::VulkanContext,
        cmd_buffer: ash::vk::CommandBuffer,
        frame_slot: usize,
        uniforms: &GridUniforms,
    ) {
        if let GridRenderer::Vulkan(r) = self {
            r.render_text(ctx, cmd_buffer, frame_slot, uniforms);
        }
    }

    /// Software cell-bg pass. Paints the grid bg into the
    /// caller-supplied `0x00RRGGBB` u32 buffer (typically
    /// softbuffer's `buffer_mut`). No-op for non-CPU variants.
    pub fn render_bg_cpu(
        &self,
        buf: &mut [u32],
        buf_w: u32,
        buf_h: u32,
        uniforms: &GridUniforms,
    ) {
        if let GridRenderer::Cpu(r) = self {
            r.render_bg(buf, buf_w, buf_h, uniforms);
        }
    }

    /// Software cell-text pass.
    pub fn render_text_cpu(
        &self,
        buf: &mut [u32],
        buf_w: u32,
        buf_h: u32,
        uniforms: &GridUniforms,
    ) {
        if let GridRenderer::Cpu(r) = self {
            r.render_text(buf, buf_w, buf_h, uniforms);
        }
    }

    /// Whether this backend actually renders grid cells. All backends
    /// — including CPU — render through the grid path now, so this
    /// always returns `true`. Kept for backwards compatibility with
    /// the previous `Unsupported` variant; remove once no caller
    /// branches on it.
    pub fn is_active(&self) -> bool {
        true
    }

    /// Cached lookup for a previously-rasterized glyph. Returns the
    /// atlas slot (position + metrics) without touching the GPU.
    pub fn lookup_glyph(&self, key: atlas::GlyphKey) -> Option<atlas::AtlasSlot> {
        match self {
            #[cfg(target_os = "macos")]
            GridRenderer::Metal(r) => r.lookup_glyph(key),
            #[cfg(feature = "wgpu")]
            GridRenderer::Wgpu(r) => r.lookup_glyph(key),
            #[cfg(target_os = "linux")]
            GridRenderer::Vulkan(r) => r.lookup_glyph(key),
            GridRenderer::Cpu(r) => r.lookup_glyph(key),
        }
    }

    /// Color-atlas lookup (RGBA emoji glyphs). Mirrors `lookup_glyph`
    /// but hits the color atlas instead of the grayscale one.
    pub fn lookup_glyph_color(&self, key: atlas::GlyphKey) -> Option<atlas::AtlasSlot> {
        match self {
            #[cfg(target_os = "macos")]
            GridRenderer::Metal(r) => r.lookup_glyph_color(key),
            #[cfg(feature = "wgpu")]
            GridRenderer::Wgpu(r) => r.lookup_glyph_color(key),
            #[cfg(target_os = "linux")]
            GridRenderer::Vulkan(r) => r.lookup_glyph_color(key),
            GridRenderer::Cpu(r) => r.lookup_glyph_color(key),
        }
    }

    /// Pack + upload a rasterized glyph. Returns the assigned
    /// `AtlasSlot` or `None` if the atlas is full.
    pub fn insert_glyph(
        &mut self,
        key: atlas::GlyphKey,
        glyph: atlas::RasterizedGlyph<'_>,
    ) -> Option<atlas::AtlasSlot> {
        match self {
            #[cfg(target_os = "macos")]
            GridRenderer::Metal(r) => r.insert_glyph(key, glyph),
            #[cfg(feature = "wgpu")]
            GridRenderer::Wgpu(r) => r.insert_glyph(key, glyph),
            #[cfg(target_os = "linux")]
            GridRenderer::Vulkan(r) => r.insert_glyph(key, glyph),
            GridRenderer::Cpu(r) => r.insert_glyph(key, glyph),
        }
    }

    /// Color-atlas insert. RGBA glyph bytes go into the RGBA8Unorm
    /// color texture (slot 1 in the text fragment shader).
    pub fn insert_glyph_color(
        &mut self,
        key: atlas::GlyphKey,
        glyph: atlas::RasterizedGlyph<'_>,
    ) -> Option<atlas::AtlasSlot> {
        match self {
            #[cfg(target_os = "macos")]
            GridRenderer::Metal(r) => r.insert_glyph_color(key, glyph),
            #[cfg(feature = "wgpu")]
            GridRenderer::Wgpu(r) => r.insert_glyph_color(key, glyph),
            #[cfg(target_os = "linux")]
            GridRenderer::Vulkan(r) => r.insert_glyph_color(key, glyph),
            GridRenderer::Cpu(r) => r.insert_glyph_color(key, glyph),
        }
    }

    /// `true` on the first frame after `new` or `resize`. Callers
    /// should treat this as "force full rebuild regardless of
    /// per-row damage" since the underlying CPU buffers are zeroed.
    pub fn needs_full_rebuild(&self) -> bool {
        match self {
            #[cfg(target_os = "macos")]
            GridRenderer::Metal(r) => r.needs_full_rebuild(),
            #[cfg(feature = "wgpu")]
            GridRenderer::Wgpu(r) => r.needs_full_rebuild(),
            #[cfg(target_os = "linux")]
            GridRenderer::Vulkan(r) => r.needs_full_rebuild(),
            GridRenderer::Cpu(r) => r.needs_full_rebuild(),
        }
    }

    /// Clear the force-full flag after the emission loop has done a
    /// full rebuild.
    pub fn mark_full_rebuild_done(&mut self) {
        match self {
            #[cfg(target_os = "macos")]
            GridRenderer::Metal(r) => r.mark_full_rebuild_done(),
            #[cfg(feature = "wgpu")]
            GridRenderer::Wgpu(r) => r.mark_full_rebuild_done(),
            #[cfg(target_os = "linux")]
            GridRenderer::Vulkan(r) => r.mark_full_rebuild_done(),
            GridRenderer::Cpu(r) => r.mark_full_rebuild_done(),
        }
    }
}
