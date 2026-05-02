// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Sugarloaf-side global state — font handle, root style, and a
//! single visual-bell overlay slot. The previous version owned the
//! `Content` registry that tracked per-panel layout; that bookkeeping
//! moved to rio's `ContextDimension` (see
//! `memory/project_sugarloaf_content_drop.md`). This module is now
//! mostly a thin holder.

use crate::font::FontLibrary;
use crate::layout::RootStyle;
use crate::renderer::Renderer;
use crate::Graphics;

pub struct SugarState {
    pub style: RootStyle,
    /// Live font handle. Cloned (Arc-shallow) into per-frame contexts.
    /// Replaces the previous indirection through `Content`.
    pub fonts: FontLibrary,
    pub visual_bell_overlay: Option<crate::sugarloaf::primitives::Rect>,
}

impl SugarState {
    pub fn new(
        style: RootStyle,
        font_library: &FontLibrary,
        _font_features: &Option<Vec<String>>,
    ) -> SugarState {
        // Font features used to be threaded through the rich-text
        // shaper; with that pipeline gone they're no longer applied
        // here. Grid-side shaping (`grid_emit`) and UI text shaping
        // (`sugarloaf::text`) handle features inline at shape time.
        SugarState {
            fonts: font_library.clone(),
            style,
            visual_bell_overlay: None,
        }
    }

    /// Compatibility shim used by the per-frame `compute_updates` —
    /// the old shaper kept font-feature settings here. Kept as a
    /// helper that returns an empty list so call sites don't need a
    /// rewrite while the rest of the pipeline is being torn down.
    pub fn found_font_features(
        _font_features: &Option<Vec<String>>,
    ) -> Vec<swash::Setting<u16>> {
        Vec::new()
    }

    /// Drive the per-frame `Renderer::prepare` step. The legacy
    /// transient-text shaping pass is gone; `advance_brush.prepare`
    /// remains because it still emits per-frame quads / clears the
    /// instance buffers that the immediate-mode `rect/quad/...`
    /// helpers fill.
    #[inline]
    pub fn compute_updates(
        &mut self,
        advance_brush: &mut Renderer,
        context: &mut super::Context,
        graphics: &mut Graphics,
        image_data: &mut rustc_hash::FxHashMap<u32, super::graphics::GraphicDataEntry>,
        image_overlays: &rustc_hash::FxHashMap<
            usize,
            Vec<super::graphics::GraphicOverlay>,
        >,
    ) {
        advance_brush.prepare(context, self, graphics, image_data, image_overlays);
    }

    /// `compute_dimensions` used to walk per-id Content states and
    /// recompute cell metrics on the `needs_repaint` flag. Per-panel
    /// dimensions live on rio's `ContextDimension` now and rio drives
    /// the recompute through `Sugarloaf::compute_cell_metrics`. This
    /// is preserved as a no-op stub so existing render-loop call
    /// sites stay shape-compatible until they're audited.
    #[inline]
    pub fn compute_dimensions(&mut self) {}

    /// Pre-frame cleanup hook. Was responsible for purging Content
    /// states marked for removal. With Content gone, sugarloaf has no
    /// per-id state of its own to GC; the only remaining sweep
    /// (per-frame text instances) lives on `crate::text::Text`.
    #[inline]
    pub fn reset(&mut self) {}

    /// Was responsible for marking every panel-text state for
    /// removal so the next frame's `reset` would drop them. Without
    /// Content there's nothing to mark — kept as a no-op so the
    /// `Sugarloaf::clear` path doesn't change shape.
    #[inline]
    pub fn clean_screen(&mut self) {}

    /// Refresh `RootStyle.scale_factor`. Per-panel `dimension` /
    /// `scaled_font_size` updates happen on rio's `ContextDimension`
    /// — this only touches sugarloaf's global default that new panels
    /// inherit from.
    #[inline]
    pub fn compute_layout_rescale(&mut self, scale: f32) {
        self.style.scale_factor = scale;
    }

    pub fn set_visual_bell_overlay(
        &mut self,
        overlay: Option<crate::sugarloaf::primitives::Rect>,
    ) {
        self.visual_bell_overlay = overlay;
    }

    /// Was a no-op even before Content was removed; kept for the
    /// `Sugarloaf::update_font` call site until that path is
    /// simplified to skip routing through state.
    #[inline]
    pub fn set_fonts(
        &mut self,
        font_library: &FontLibrary,
        _advance_brush: &mut Renderer,
    ) {
        self.fonts = font_library.clone();
    }
}
