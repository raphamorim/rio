// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! `text` — minimal immediate-mode text primitive for UI overlays.
//!
//! Intended replacement for sugarloaf's `Content` / `BuilderState` when
//! rendering tab titles, command palette entries, search input, etc.
//! See `memory/project_sugarloaf_content_drop.md` for the broader plan.
//!
//! # Status
//!
//! **Phase 1c-1/1c-2.** macOS: shape via CoreText, rasterize into
//! Text-owned atlases, emit `TextInstance`s directly from `draw()`.
//! `flush` still has no GPU pipeline yet (Phase 1c-3) — instances sit
//! in the vec waiting for a render hook.
//!
//! `measure()` shapes without rasterizing, so truncation binary
//! search is cheap.
//!
//! Non-macOS: stubs (swash port is Phase 4 of the content-drop plan).

use rustc_hash::FxHashMap;

use crate::font::FontLibrary;

/// Per-instance GPU vertex data for a UI text glyph.
///
/// `pos` is **pixel-space top-left** of the text's bounding box for
/// this glyph. `bearings.x` shifts it right to the glyph's bitmap
/// origin; `bearings.y` shifts it down from the box top to the
/// glyph's bitmap top. The vertex shader writes:
/// `out_px = pos + bearings + quad_corner * glyph_size`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct TextInstance {
    pub pos: [f32; 2],
    pub glyph_pos: [u32; 2],
    pub glyph_size: [u32; 2],
    pub bearings: [i16; 2],
    pub color: [u8; 4],
    /// `0` = grayscale atlas; `1` = color atlas. Same dispatch as
    /// `grid::cell::CellText::atlas`.
    pub atlas: u8,
    pub _pad: [u8; 3],
}

// 36 bytes (4-aligned): pos 8 + glyph_pos 8 + glyph_size 8 + bearings 4
// + color 4 + atlas 1 + _pad 3. Unlike `CellText` (32B, u16 grid_pos),
// UI text uses f32 positions to lay out in free pixel space — adds 4B.
const _: () = assert!(std::mem::size_of::<TextInstance>() == 36);

/// Atlas-relative glyph metadata held alongside the grid's
/// `AtlasSlot`, but with **UI-convention bearings** (top-of-bbox
/// relative) rather than cell-bottom relative. Kept separate so we
/// don't clobber the grid atlas's slot bookkeeping.
#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug)]
struct TextGlyphSlot {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    /// Distance from pos.x to glyph bitmap's left edge. Same as
    /// `raw.left` from `rasterize_glyph`.
    bearing_x: i16,
    /// Distance from text bbox TOP to glyph bitmap's top, positive
    /// down. Computed as `ascent_px - raw.top` at insert time.
    bearing_y: i16,
    /// `true` if the glyph lives in the color atlas.
    is_color: bool,
}

#[cfg(target_os = "macos")]
#[derive(Eq, Hash, PartialEq, Clone, Copy)]
struct TextGlyphKey {
    font_id: u32,
    glyph_id: u32,
    size_bucket: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct DrawOpts {
    /// **Logical** (unscaled) font size. Callers pass CSS-style font
    /// size; `Text` multiplies by its stored `scale_factor` before
    /// shaping / rasterization.
    pub font_size: f32,
    /// Premultiplied RGBA. Passed into `TextInstance.color` for
    /// grayscale glyphs; ignored for color (emoji).
    pub color: [u8; 4],
    pub bold: bool,
    pub italic: bool,
    /// `None` → primary font.
    pub font_id: Option<usize>,
}

impl Default for DrawOpts {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            color: [255, 255, 255, 255],
            bold: false,
            italic: false,
            font_id: None,
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct ShapedRun {
    font_id: u32,
    size_u16: u16,
    size_bucket: u16,
    synthetic_bold: bool,
    synthetic_italic: bool,
    glyphs: Vec<crate::font::macos::ShapedGlyph>,
    ascent_px: i16,
}

#[cfg(target_os = "macos")]
#[inline]
fn shape_hash(font_id: u32, size_bucket: u16, style_flags: u8, text: &str) -> u64 {
    use core::hash::Hasher;
    use rustc_hash::FxHasher;
    let mut h = FxHasher::default();
    h.write_u32(font_id);
    h.write_u16(size_bucket);
    h.write_u8(style_flags);
    h.write(text.as_bytes());
    h.finish()
}

/// Metal backend GPU state — atlases + device/queue + pipeline +
/// instance buffer. Lazily initialised on first `draw()` with a Metal
/// context so Text::new doesn't need a ctx handle.
#[cfg(target_os = "macos")]
struct TextMetalState {
    device: metal::Device,
    command_queue: metal::CommandQueue,
    atlas_grayscale: crate::grid::metal::MetalGlyphAtlas,
    atlas_color: crate::grid::metal::MetalGlyphAtlas,
    /// Slots keyed by `(font_id, glyph_id, size_bucket)` — one table
    /// covers both atlases (the slot carries `is_color`).
    slots: FxHashMap<TextGlyphKey, TextGlyphSlot>,
    /// Compiled text pipeline — `text_vertex` + reused
    /// `grid_text_fragment`.
    pipeline: metal::RenderPipelineState,
    /// Resident GPU instance buffer. Re-created when `instances.len()`
    /// exceeds its capacity.
    instance_buffer: metal::Buffer,
    instance_capacity: usize,
}

/// Immediate-mode UI text recorder. One instance owned by `Sugarloaf`.
pub struct Text {
    /// Per-frame GPU instances, consumed by the flush.
    instances: Vec<TextInstance>,

    /// Scale factor used to convert logical coords / sizes (the
    /// call-site unit for overlays) to device pixels. Updated by
    /// `Sugarloaf` at the top of each render; defaults to 1.0.
    scale_factor: f32,

    #[cfg(target_os = "macos")]
    font_library: FontLibrary,

    #[cfg(target_os = "macos")]
    font_resolve: FxHashMap<(char, u8), (u32, bool)>,
    #[cfg(target_os = "macos")]
    handle_cache: FxHashMap<u32, crate::font::macos::FontHandle>,
    #[cfg(target_os = "macos")]
    synthesis_cache: FxHashMap<u32, (bool, bool)>,
    #[cfg(target_os = "macos")]
    ascent_cache: FxHashMap<(u32, u16), i16>,

    /// Shape cache — `shape_hash` → shaped run.
    #[cfg(target_os = "macos")]
    shape_cache: FxHashMap<u64, ShapedRun>,

    #[cfg(target_os = "macos")]
    metal: Option<TextMetalState>,
}

impl Text {
    pub fn new(#[cfg(target_os = "macos")] font_library: &FontLibrary) -> Self {
        #[cfg(target_os = "macos")]
        let _fl = font_library;
        Self {
            instances: Vec::new(),
            scale_factor: 1.0,
            #[cfg(target_os = "macos")]
            font_library: _fl.clone(),
            #[cfg(target_os = "macos")]
            font_resolve: FxHashMap::default(),
            #[cfg(target_os = "macos")]
            handle_cache: FxHashMap::default(),
            #[cfg(target_os = "macos")]
            synthesis_cache: FxHashMap::default(),
            #[cfg(target_os = "macos")]
            ascent_cache: FxHashMap::default(),
            #[cfg(target_os = "macos")]
            shape_cache: FxHashMap::default(),
            #[cfg(target_os = "macos")]
            metal: None,
        }
    }

    /// Update the scale factor used to convert caller-supplied
    /// logical coords / font sizes to device pixels. Sugarloaf calls
    /// this at the top of each render so overlays can continue to
    /// pass logical values the way they do with the rich-text API.
    #[inline]
    pub fn set_scale_factor(&mut self, scale: f32) {
        self.scale_factor = scale.max(1.0);
    }

    /// Initialise the Metal backend (atlases + device/queue + pipeline +
    /// instance buffer). Called by sugarloaf's render path on the first
    /// frame with a Metal context. Re-callable — no-op after first init.
    #[cfg(target_os = "macos")]
    pub fn init_metal(
        &mut self,
        device: &metal::Device,
        command_queue: &metal::CommandQueue,
    ) {
        if self.metal.is_some() {
            return;
        }
        let pipeline = build_text_pipeline(device);
        let instance_capacity: usize = 256;
        let instance_buffer = alloc_instance_buffer(device, instance_capacity);
        self.metal = Some(TextMetalState {
            device: device.to_owned(),
            command_queue: command_queue.to_owned(),
            atlas_grayscale: crate::grid::metal::MetalGlyphAtlas::new_grayscale(device),
            atlas_color: crate::grid::metal::MetalGlyphAtlas::new_color(device),
            slots: FxHashMap::default(),
            pipeline,
            instance_buffer,
            instance_capacity,
        });
    }

    /// Record the UI text pass into `encoder`. No-op if Metal state
    /// isn't initialised or there are no instances this frame.
    /// `viewport` is the drawable's pixel size (width, height).
    ///
    /// Must be called inside a running `MTLRenderCommandEncoder` from
    /// the caller's frame — Text doesn't own its own drawable.
    #[cfg(target_os = "macos")]
    pub fn render_metal(
        &mut self,
        encoder: &metal::RenderCommandEncoderRef,
        viewport: [f32; 2],
    ) {
        let instance_count = self.instances.len();
        if instance_count == 0 {
            return;
        }
        let Some(state) = self.metal.as_mut() else {
            return;
        };

        // Grow the GPU instance buffer if this frame needs more capacity.
        if instance_count > state.instance_capacity {
            let new_cap = instance_count.next_power_of_two().max(256);
            state.instance_buffer = alloc_instance_buffer(&state.device, new_cap);
            state.instance_capacity = new_cap;
        }

        // Upload instances. `Shared` storage (from
        // `alloc_instance_buffer`) means we can write directly from
        // CPU and Metal sees the updates without a sync barrier —
        // fine for per-frame upload.
        unsafe {
            let dst = state.instance_buffer.contents() as *mut TextInstance;
            std::ptr::copy_nonoverlapping(
                self.instances.as_ptr(),
                dst,
                instance_count,
            );
        }

        encoder.set_render_pipeline_state(&state.pipeline);
        encoder.set_vertex_buffer(0, Some(&state.instance_buffer), 0);
        // buffer(1): viewport (pixel size). `set_vertex_bytes` is
        // efficient for tiny push-constant-like data.
        let vp_bytes: [f32; 2] = viewport;
        let vp_ptr = vp_bytes.as_ptr() as *const std::ffi::c_void;
        encoder.set_vertex_bytes(
            1,
            std::mem::size_of::<[f32; 2]>() as u64,
            vp_ptr,
        );

        encoder.set_fragment_texture(0, Some(&state.atlas_grayscale.texture));
        encoder.set_fragment_texture(1, Some(&state.atlas_color.texture));

        encoder.draw_primitives_instanced(
            metal::MTLPrimitiveType::TriangleStrip,
            0,
            4,
            instance_count as u64,
        );
    }

    /// Draw `text` at logical top-left `(x, y)` with `opts`. Returns
    /// the rendered width in **logical** pixels (for truncation
    /// bookkeeping). On macOS: shapes + rasterizes + pushes
    /// `TextInstance`s in device-pixel coords.
    pub fn draw(
        &mut self,
        #[allow(unused_variables)] x: f32,
        #[allow(unused_variables)] y: f32,
        text: &str,
        opts: &DrawOpts,
    ) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        #[cfg(target_os = "macos")]
        {
            let Some(shaped) = self.shape_for(text, opts) else {
                return 0.0;
            };
            let width_px = shaped_width(&shaped);
            if self.metal.is_some() {
                self.emit_instances(x, y, &shaped, opts);
            }
            width_px / self.scale_factor
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (x, y);
            estimate_width(text, opts.font_size)
        }
    }

    /// Measure `text` under `opts` without recording a draw. Returns
    /// logical-pixel width. Used by truncation / layout paths that
    /// try multiple candidates before committing.
    pub fn measure(&mut self, text: &str, opts: &DrawOpts) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        #[cfg(target_os = "macos")]
        {
            self.shape_for(text, opts)
                .map(|r| shaped_width(&r) / self.scale_factor)
                .unwrap_or(0.0)
        }
        #[cfg(not(target_os = "macos"))]
        {
            estimate_width(text, opts.font_size)
        }
    }

    #[inline]
    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.instances.clear();
    }

    #[inline]
    pub fn instances(&self) -> &[TextInstance] {
        &self.instances
    }

    // ===== macOS shape path =====

    #[cfg(target_os = "macos")]
    fn shape_for(&mut self, text: &str, opts: &DrawOpts) -> Option<ShapedRun> {
        use crate::{Attributes, SpanStyle, Stretch, Style as FontStyle, Weight};

        // Callers pass logical sizes; CoreText wants device pixels.
        let scaled = opts.font_size * self.scale_factor;
        let size_bucket = (scaled * 4.0).round().clamp(0.0, u16::MAX as f32) as u16;
        let size_u16 = scaled.round().clamp(1.0, u16::MAX as f32) as u16;
        let style_flags =
            (if opts.bold { 1u8 } else { 0 }) | (if opts.italic { 2u8 } else { 0 });

        let first_ch = text.chars().next()?;
        let (font_id, _is_emoji) =
            match self.font_resolve.entry((first_ch, style_flags)) {
                std::collections::hash_map::Entry::Occupied(e) => *e.get(),
                std::collections::hash_map::Entry::Vacant(e) => {
                    let mut ss = SpanStyle::default();
                    let weight = if opts.bold { Weight::BOLD } else { Weight::NORMAL };
                    let fstyle = if opts.italic {
                        FontStyle::Italic
                    } else {
                        FontStyle::Normal
                    };
                    ss.font_attrs = Attributes::new(Stretch::NORMAL, weight, fstyle);
                    let resolved =
                        self.font_library.resolve_font_for_char(first_ch, &ss);
                    let v = (resolved.0 as u32, resolved.1);
                    e.insert(v);
                    v
                }
            };
        let font_id = opts.font_id.map(|id| id as u32).unwrap_or(font_id);

        let hash = shape_hash(font_id, size_bucket, style_flags, text);
        if let Some(entry) = self.shape_cache.get(&hash) {
            return Some(entry.clone());
        }

        let handle = match self.handle_cache.entry(font_id) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut().clone(),
            std::collections::hash_map::Entry::Vacant(e) => {
                let h = self.font_library.ct_font(font_id as usize)?;
                e.insert(h.clone());
                h
            }
        };

        let (synthetic_bold, synthetic_italic) =
            match self.synthesis_cache.entry(font_id) {
                std::collections::hash_map::Entry::Occupied(e) => *e.get(),
                std::collections::hash_map::Entry::Vacant(e) => {
                    let lib = self.font_library.inner.read();
                    let fd = lib.get(&(font_id as usize));
                    *e.insert((fd.should_embolden, fd.should_italicize))
                }
            };

        let ascent_px = *self.ascent_cache.entry((font_id, size_bucket)).or_insert_with(
            || {
                let m =
                    crate::font::macos::font_metrics(&handle, size_u16 as f32);
                m.ascent.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
            },
        );

        let glyphs = crate::font::macos::shape_text(&handle, text, size_u16 as f32);
        let run = ShapedRun {
            font_id,
            size_u16,
            size_bucket,
            synthetic_bold,
            synthetic_italic,
            glyphs,
            ascent_px,
        };
        self.shape_cache.insert(hash, run.clone());
        Some(run)
    }

    /// Rasterize + emit instances for a shaped run at **logical**
    /// `(x, y)`. Multiplies by `scale_factor` to land in device
    /// pixels before pushing. Requires `metal` to be Some (caller
    /// checks).
    #[cfg(target_os = "macos")]
    fn emit_instances(
        &mut self,
        x: f32,
        y: f32,
        run: &ShapedRun,
        opts: &DrawOpts,
    ) {
        let scale = self.scale_factor;
        let mut pen_x = x * scale;
        let py = y * scale;
        let color = opts.color;

        let handle = match self.handle_cache.get(&run.font_id) {
            Some(h) => h.clone(),
            None => return,
        };

        for glyph in &run.glyphs {
            let key = TextGlyphKey {
                font_id: run.font_id,
                glyph_id: glyph.id as u32,
                size_bucket: run.size_bucket,
            };

            let slot = match Self::lookup_or_rasterize_slot(
                self.metal.as_mut().unwrap(),
                key,
                &handle,
                glyph.id,
                run.size_u16,
                run.ascent_px,
                run.synthetic_bold,
                run.synthetic_italic,
            ) {
                Some(s) => s,
                None => continue,
            };

            if slot.w == 0 || slot.h == 0 {
                pen_x += glyph.advance;
                continue;
            }

            let atlas_tag = if slot.is_color { 1u8 } else { 0u8 };
            let instance_color = if slot.is_color {
                [255u8, 255, 255, 255]
            } else {
                color
            };

            self.instances.push(TextInstance {
                pos: [pen_x + glyph.x, py + glyph.y.max(0.0)],
                glyph_pos: [slot.x as u32, slot.y as u32],
                glyph_size: [slot.w as u32, slot.h as u32],
                bearings: [slot.bearing_x, slot.bearing_y],
                color: instance_color,
                atlas: atlas_tag,
                _pad: [0; 3],
            });

            pen_x += glyph.advance;
        }
    }

    #[cfg(target_os = "macos")]
    fn lookup_or_rasterize_slot(
        metal_state: &mut TextMetalState,
        key: TextGlyphKey,
        handle: &crate::font::macos::FontHandle,
        glyph_id: u16,
        size_u16: u16,
        ascent_px: i16,
        synthetic_bold: bool,
        synthetic_italic: bool,
    ) -> Option<TextGlyphSlot> {
        if let Some(&s) = metal_state.slots.get(&key) {
            return Some(s);
        }

        // Rasterize. is_emoji passed to rasterize_glyph is derived
        // from the font's color-glyphs trait; we don't carry it on
        // the key, but we can read it from the handle's underlying
        // font. For Phase 1c we pass `false` and rely on
        // `raw.is_color` (set by CG-side detection) to route.
        let raw = crate::font::macos::rasterize_glyph(
            handle,
            glyph_id,
            size_u16 as f32,
            /* is_emoji: */ false,
            synthetic_italic,
            synthetic_bold,
        )?;
        let is_color = raw.is_color;

        let raster = crate::grid::RasterizedGlyph {
            width: raw.width.min(u16::MAX as u32) as u16,
            height: raw.height.min(u16::MAX as u32) as u16,
            bearing_x: raw.left.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
            // UI bearing: distance from text-box top to glyph top,
            // positive down. `raw.top` is distance from baseline up
            // to glyph top. Baseline sits `ascent_px` below text top.
            // → bearing_y = ascent_px - raw.top.
            bearing_y: {
                let top_i16 = raw.top.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                ascent_px.saturating_sub(top_i16)
            },
            bytes: &raw.bytes,
        };

        // Construct a GlyphKey the atlas understands. font_id +
        // glyph_id + size_bucket. Use our key directly.
        let grid_key = crate::grid::GlyphKey {
            font_id: key.font_id,
            glyph_id: key.glyph_id,
            size_bucket: key.size_bucket,
        };

        // Insert with atlas-grow retry. Mirrors
        // `MetalGridRenderer::insert_glyph`: on full, double the
        // texture (blit old → new top-left), retry once.
        let slot = if is_color {
            match metal_state.atlas_color.insert(grid_key, raster) {
                Some(s) => s,
                None => {
                    if metal_state
                        .atlas_color
                        .grow(&metal_state.device, &metal_state.command_queue)
                    {
                        metal_state.atlas_color.insert(grid_key, raster)?
                    } else {
                        return None;
                    }
                }
            }
        } else {
            match metal_state.atlas_grayscale.insert(grid_key, raster) {
                Some(s) => s,
                None => {
                    if metal_state
                        .atlas_grayscale
                        .grow(&metal_state.device, &metal_state.command_queue)
                    {
                        metal_state.atlas_grayscale.insert(grid_key, raster)?
                    } else {
                        return None;
                    }
                }
            }
        };

        let text_slot = TextGlyphSlot {
            x: slot.x,
            y: slot.y,
            w: slot.w,
            h: slot.h,
            bearing_x: slot.bearing_x,
            bearing_y: slot.bearing_y,
            is_color,
        };
        metal_state.slots.insert(key, text_slot);
        Some(text_slot)
    }
}

#[cfg(target_os = "macos")]
#[inline]
fn shaped_width(run: &ShapedRun) -> f32 {
    run.glyphs.iter().map(|g| g.advance).sum()
}

#[cfg(not(target_os = "macos"))]
#[inline]
fn estimate_width(text: &str, font_size_px: f32) -> f32 {
    text.chars().count() as f32 * font_size_px * 0.6
}

// ===== Metal pipeline construction =====

#[cfg(target_os = "macos")]
fn build_text_pipeline(device: &metal::Device) -> metal::RenderPipelineState {
    use metal::{
        MTLBlendFactor, MTLBlendOperation, MTLPixelFormat, MTLVertexFormat,
        MTLVertexStepFunction, RenderPipelineDescriptor, VertexDescriptor,
    };

    let shader_source = include_str!("grid/shaders/grid.metal");
    let library = device
        .new_library_with_source(shader_source, &metal::CompileOptions::new())
        .expect("grid.metal failed to compile (text)");

    let vertex_fn = library
        .get_function("text_vertex", None)
        .expect("text_vertex not found");
    let fragment_fn = library
        .get_function("grid_text_fragment", None)
        .expect("grid_text_fragment not found");

    // Per-instance vertex descriptor for `TextInstance`.
    // Offsets / attribute indices match the `[[attribute(N)]]` tags
    // in `text_vertex` in grid.metal.
    let vd = VertexDescriptor::new();
    let attrs = vd.attributes();
    // attribute 0: pos: [f32; 2] @ offset 0
    let a = attrs.object_at(0).unwrap();
    a.set_format(MTLVertexFormat::Float2);
    a.set_buffer_index(0);
    a.set_offset(0);
    // attribute 1: glyph_pos: [u32; 2] @ offset 8
    let a = attrs.object_at(1).unwrap();
    a.set_format(MTLVertexFormat::UInt2);
    a.set_buffer_index(0);
    a.set_offset(8);
    // attribute 2: glyph_size: [u32; 2] @ offset 16
    let a = attrs.object_at(2).unwrap();
    a.set_format(MTLVertexFormat::UInt2);
    a.set_buffer_index(0);
    a.set_offset(16);
    // attribute 3: bearings: [i16; 2] @ offset 24 → Short2 (sign-ext to int2)
    let a = attrs.object_at(3).unwrap();
    a.set_format(MTLVertexFormat::Short2);
    a.set_buffer_index(0);
    a.set_offset(24);
    // attribute 4: color: [u8; 4] @ offset 28 → UChar4
    let a = attrs.object_at(4).unwrap();
    a.set_format(MTLVertexFormat::UChar4);
    a.set_buffer_index(0);
    a.set_offset(28);
    // attribute 5: atlas: u8 @ offset 32 → UChar
    let a = attrs.object_at(5).unwrap();
    a.set_format(MTLVertexFormat::UChar);
    a.set_buffer_index(0);
    a.set_offset(32);

    let layout = vd.layouts().object_at(0).unwrap();
    layout.set_stride(std::mem::size_of::<TextInstance>() as u64);
    layout.set_step_function(MTLVertexStepFunction::PerInstance);
    layout.set_step_rate(1);

    let descriptor = RenderPipelineDescriptor::new();
    descriptor.set_label("sugarloaf.text");
    descriptor.set_vertex_function(Some(&vertex_fn));
    descriptor.set_fragment_function(Some(&fragment_fn));
    descriptor.set_vertex_descriptor(Some(vd));

    let color = descriptor
        .color_attachments()
        .object_at(0)
        .expect("color attachment 0 missing");
    color.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
    color.set_blending_enabled(true);
    // Premultiplied-over, same as grid text — the fragment returns
    // premultiplied RGBA.
    color.set_source_rgb_blend_factor(MTLBlendFactor::One);
    color.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    color.set_rgb_blend_operation(MTLBlendOperation::Add);
    color.set_source_alpha_blend_factor(MTLBlendFactor::One);
    color.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    color.set_alpha_blend_operation(MTLBlendOperation::Add);

    device
        .new_render_pipeline_state(&descriptor)
        .expect("sugarloaf.text pipeline state creation failed")
}

/// Allocate a CPU-writable Metal buffer sized for `capacity`
/// `TextInstance`s. `Shared` storage mode = CPU+GPU coherent; we
/// write from CPU every frame without a blit.
#[cfg(target_os = "macos")]
fn alloc_instance_buffer(device: &metal::Device, capacity: usize) -> metal::Buffer {
    let size = (capacity.max(1) * std::mem::size_of::<TextInstance>()) as u64;
    device.new_buffer(size, metal::MTLResourceOptions::StorageModeShared)
}
