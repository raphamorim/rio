// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Persistent, row-indexed GPU cell buffer for the terminal grid.
//!
//! This is the target of a direct rewrite replacing sugarloaf's
//! rich-text-based terminal rendering (see
//! `memory/project_grid_gpu_renderer_plan.md`). The design mirrors
//! Ghostty's Metal renderer: one `bg_cells` flat buffer indexed by
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
    /// CPU backend has no grid renderer — it falls back to
    /// rasterising via the existing cpu path. Terminal content won't
    /// use the grid path on CPU builds.
    Unsupported,
}

impl GridRenderer {
    /// Construct a grid renderer matching `context`'s backend. Returns
    /// `Unsupported` for CPU contexts (the CPU rasterizer keeps its
    /// current codepath).
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
            ContextType::Cpu(_) => GridRenderer::Unsupported,
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
            GridRenderer::Unsupported => {}
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
            GridRenderer::Unsupported => {}
        }
    }

    /// Zero out `row`'s fg/bg slots. Corresponds to Ghostty's
    /// `self.cells.clear(y)` (`generic.zig:2436`).
    pub fn clear_row(&mut self, row: u32) {
        match self {
            #[cfg(target_os = "macos")]
            GridRenderer::Metal(r) => r.clear_row(row),
            #[cfg(feature = "wgpu")]
            GridRenderer::Wgpu(r) => r.clear_row(row),
            #[cfg(target_os = "linux")]
            GridRenderer::Vulkan(r) => r.clear_row(row),
            GridRenderer::Unsupported => {}
        }
    }

    /// Record grid draw calls against a caller-supplied render pass /
    /// encoder. The caller owns the command buffer + drawable + pass
    /// descriptor so the grid composes with sugarloaf's UI overlays
    /// (island, assistant, etc.) in a single render pass.
    ///
    /// Phase 1a: Metal draws the bg pass; Wgpu is still a no-op.
    #[cfg(target_os = "macos")]
    pub fn render_metal(
        &mut self,
        encoder: &::metal::RenderCommandEncoderRef,
        uniforms: &GridUniforms,
    ) {
        if let GridRenderer::Metal(r) = self {
            r.render(encoder, uniforms);
        }
    }

    /// Wgpu counterpart of `render_metal`. Phase 1b will record a bg
    /// pass against the caller's `wgpu::RenderPass`.
    #[cfg(feature = "wgpu")]
    pub fn render_wgpu<'pass>(
        &'pass mut self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        uniforms: &GridUniforms,
    ) {
        if let GridRenderer::Wgpu(r) = self {
            r.render(render_pass, uniforms);
        }
    }

    /// Vulkan counterpart of `render_metal`. Records draws into
    /// `cmd_buffer`, which the caller (`Sugarloaf::render_vulkan`)
    /// has already wrapped in `cmd_begin_rendering` against the
    /// swapchain image. No-op when this renderer isn't a Vulkan
    /// variant — same shape as `render_wgpu` / `render_metal`.
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

    #[cfg(target_os = "linux")]
    pub fn render_vulkan(
        &mut self,
        ctx: &crate::context::vulkan::VulkanContext,
        cmd_buffer: ash::vk::CommandBuffer,
        frame_slot: usize,
        uniforms: &GridUniforms,
    ) {
        if let GridRenderer::Vulkan(r) = self {
            r.render(ctx, cmd_buffer, frame_slot, uniforms);
        }
    }

    /// Whether this backend actually renders grid cells. Call sites
    /// can fall back to the rich-text path when this returns false.
    pub fn is_active(&self) -> bool {
        !matches!(self, GridRenderer::Unsupported)
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
            GridRenderer::Unsupported => None,
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
            GridRenderer::Unsupported => None,
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
            GridRenderer::Unsupported => None,
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
            GridRenderer::Unsupported => None,
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
            GridRenderer::Unsupported => false,
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
            GridRenderer::Unsupported => {}
        }
    }
}
