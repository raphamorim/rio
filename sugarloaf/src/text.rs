// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! `text` — minimal immediate-mode text primitive for UI overlays.
//!
//! Replacement for sugarloaf's `Content` / `BuilderState` used by
//! tab titles, command palette, search input, assistant, etc.
//!
//! Public API (`draw` / `measure`) is identical across platforms.
//! On macOS the shape + rasterize backends are CoreText; everywhere
//! else they're swash (ShapeContext / ScaleContext + Render).
//! GPU backend is Metal on macOS and wgpu on other platforms.

use rustc_hash::FxHashMap;

use crate::font::FontLibrary;

//  GPU vertex data (platform-agnostic)

/// Per-instance GPU vertex data for a UI text glyph.
///
/// `pos` is **pixel-space top-left** of the glyph's text bounding box.
/// `bearings.x` shifts it right to the glyph bitmap's left edge;
/// `bearings.y` shifts it down to the bitmap top. The vertex shader
/// writes: `out_px = pos + bearings + quad_corner * glyph_size`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct TextInstance {
    pub pos: [f32; 2],
    pub glyph_pos: [u32; 2],
    pub glyph_size: [u32; 2],
    pub bearings: [i16; 2],
    pub color: [u8; 4],
    /// `0` = grayscale atlas; `1` = color atlas.
    pub atlas: u8,
    pub _pad: [u8; 3],
}

// 36 bytes (4-aligned). f32 pos (vs grid's u16 grid_pos) adds 4 bytes.
const _: () = assert!(std::mem::size_of::<TextInstance>() == 36);

//  Public draw options

#[derive(Clone, Copy, Debug)]
pub struct DrawOpts {
    /// **Logical** (unscaled) font size. Text multiplies by its
    /// stored `scale_factor` internally before shaping / rasterizing.
    pub font_size: f32,
    /// Non-premultiplied RGBA. Shader premultiplies.
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

//  Shape result — unified across platforms

/// One shaped glyph in a run. Same shape on macOS (CoreText) and
/// non-macOS (swash) so the emit loop doesn't care which backend
/// produced it. `cluster` is a UTF-8 byte offset into the run string —
/// held for a future ligature / multi-cell mapping pass (current emit
/// just walks glyphs linearly with a pen-x advance).
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
struct ShapedGlyph {
    id: u16,
    x: f32,
    y: f32,
    advance: f32,
    cluster: u32,
}

/// A fully-shaped run with everything the emit step needs.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Some fields only read from one cfg path.
struct ShapedRun {
    font_id: u32,
    size_u16: u16,
    size_bucket: u16,
    synthetic_bold: bool,
    synthetic_italic: bool,
    ascent_px: i16,
    glyphs: Vec<ShapedGlyph>,
}

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

//  Per-OS GPU state

#[cfg(target_os = "macos")]
struct TextMetalState {
    device: metal::Device,
    command_queue: metal::CommandQueue,
    atlas_grayscale: crate::grid::metal::MetalGlyphAtlas,
    atlas_color: crate::grid::metal::MetalGlyphAtlas,
    pipeline: metal::RenderPipelineState,
    instance_buffer: metal::Buffer,
    instance_capacity: usize,
}

#[cfg(feature = "wgpu")]
struct TextWgpuState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    atlas_grayscale: crate::grid::webgpu::WgpuGlyphAtlas,
    atlas_color: crate::grid::webgpu::WgpuGlyphAtlas,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    atlas_bind_group: wgpu::BindGroup,
    #[allow(dead_code)] // retained for future atlas-bind-group recreation on atlas grow
    atlas_bind_group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
}

/// Software backend state for the UI text overlay. Mirrors the
/// shape of `TextMetalState` / `TextWgpuState` / `TextVulkanState`
/// (two atlases — grayscale + color) but the atlases are RAM-backed.
/// Always available; populated by `Text::init_cpu` when the active
/// sugarloaf context is `ContextType::Cpu`.
struct TextCpuState {
    atlas_grayscale: crate::grid::cpu::CpuGridAtlas,
    atlas_color: crate::grid::cpu::CpuGridAtlas,
}

#[cfg(target_os = "linux")]
struct TextVulkanState {
    device: ash::Device,
    instance: ash::Instance,
    physical_device: ash::vk::PhysicalDevice,
    /// Independent atlases owned by the UI text overlay — separate
    /// from each `VulkanGridRenderer`'s atlases so an overlay glyph
    /// doesn't have to compete for grid atlas space (and vice versa).
    /// Same shape as the macOS / wgpu paths.
    atlas_grayscale: crate::grid::vulkan::VulkanGlyphAtlas,
    atlas_color: crate::grid::vulkan::VulkanGlyphAtlas,
    /// Single sampler shared by both atlases. We only `texelFetch`,
    /// so the sampler's filter / address mode don't matter — it just
    /// has to exist for the COMBINED_IMAGE_SAMPLER descriptor.
    sampler: ash::vk::Sampler,
    uniform_buffers:
        [crate::context::vulkan::VulkanBuffer; crate::context::vulkan::FRAMES_IN_FLIGHT],
    /// Per-slot ring of instance buffers. Grow on demand, never shrink.
    instance_buffers: [Option<crate::context::vulkan::VulkanBuffer>;
        crate::context::vulkan::FRAMES_IN_FLIGHT],
    instance_capacity: [usize; crate::context::vulkan::FRAMES_IN_FLIGHT],
    descriptor_pool: ash::vk::DescriptorPool,
    uniform_descriptor_set_layout: ash::vk::DescriptorSetLayout,
    atlas_descriptor_set_layout: ash::vk::DescriptorSetLayout,
    uniform_descriptor_sets:
        [ash::vk::DescriptorSet; crate::context::vulkan::FRAMES_IN_FLIGHT],
    atlas_descriptor_set: ash::vk::DescriptorSet,

    pipeline_layout: ash::vk::PipelineLayout,
    pipeline: ash::vk::Pipeline,
}

//  Text — the immediate-mode recorder owned by Sugarloaf

pub struct Text {
    /// Per-frame GPU instances, assembled inside `draw()` and drawn
    /// by the render-pass hook.
    instances: Vec<TextInstance>,

    /// Scale factor used to convert caller-supplied logical coords /
    /// font sizes to device pixels. Updated by `Sugarloaf::new` /
    /// `rescale`; defaults to 1.0.
    scale_factor: f32,

    // shared state across both OS paths
    font_library: FontLibrary,

    /// `(char, style_flags) → (font_id, is_emoji)` — first-char font
    /// resolution for a run.
    font_resolve: FxHashMap<(char, u8), (u32, bool)>,

    /// `font_id → (should_embolden, should_italicize)` from
    /// `FontData` load-time synthesis flags (parallel to the rich-text
    /// rasterizer's use of the same fields).
    synthesis_cache: FxHashMap<u32, (bool, bool)>,

    /// `(font_id, size_bucket) → ascent_px`. Used to compute
    /// `bearing_y` at rasterize time.
    ascent_cache: FxHashMap<(u32, u16), i16>,

    /// Position-independent shape cache. Hash of
    /// `(font_id, size_bucket, style_flags, text)` → shaped run.
    shape_cache: FxHashMap<u64, ShapedRun>,

    #[cfg(target_os = "macos")]
    handle_cache: FxHashMap<u32, crate::font::macos::FontHandle>,
    #[cfg(target_os = "macos")]
    metal: Option<TextMetalState>,

    #[cfg(not(target_os = "macos"))]
    shape_ctx: swash::shape::ShapeContext,
    #[cfg(not(target_os = "macos"))]
    scale_ctx: swash::scale::ScaleContext,
    /// Cached `(shared_data, offset, cache_key)` per font_id so the
    /// `FontLibraryData` read-lock isn't re-acquired per shape.
    #[cfg(not(target_os = "macos"))]
    font_data_cache: FxHashMap<u32, (crate::font::SharedData, u32, swash::CacheKey)>,
    #[cfg(feature = "wgpu")]
    wgpu: Option<TextWgpuState>,
    #[cfg(target_os = "linux")]
    vulkan: Option<TextVulkanState>,
    /// Software backend. Initialised by `init_cpu` when the
    /// surrounding sugarloaf context is `ContextType::Cpu`.
    cpu: Option<TextCpuState>,
}

impl Text {
    pub fn new(font_library: &FontLibrary) -> Self {
        Self {
            instances: Vec::new(),
            scale_factor: 1.0,
            font_library: font_library.clone(),
            font_resolve: FxHashMap::default(),
            synthesis_cache: FxHashMap::default(),
            ascent_cache: FxHashMap::default(),
            shape_cache: FxHashMap::default(),
            #[cfg(target_os = "macos")]
            handle_cache: FxHashMap::default(),
            #[cfg(target_os = "macos")]
            metal: None,
            #[cfg(not(target_os = "macos"))]
            shape_ctx: swash::shape::ShapeContext::new(),
            #[cfg(not(target_os = "macos"))]
            scale_ctx: swash::scale::ScaleContext::new(),
            #[cfg(not(target_os = "macos"))]
            font_data_cache: FxHashMap::default(),
            #[cfg(feature = "wgpu")]
            wgpu: None,
            #[cfg(target_os = "linux")]
            vulkan: None,
            cpu: None,
        }
    }

    /// Idempotent CPU-state init. Call once before the first
    /// `draw()` on the CPU backend; subsequent calls are no-ops.
    pub fn init_cpu(&mut self) {
        if self.cpu.is_some() {
            return;
        }
        self.cpu = Some(TextCpuState {
            atlas_grayscale: crate::grid::cpu::CpuGridAtlas::new_grayscale(),
            atlas_color: crate::grid::cpu::CpuGridAtlas::new_color(),
        });
    }

    /// Update the scale factor used to convert caller-supplied
    /// logical coords / font sizes to device pixels.
    #[inline]
    pub fn set_scale_factor(&mut self, scale: f32) {
        self.scale_factor = scale.max(1.0);
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

    //  Public draw API

    /// Draw `text` at logical top-left `(x, y)` with `opts`. Returns
    /// rendered width in **logical** pixels.
    pub fn draw(&mut self, x: f32, y: f32, text: &str, opts: &DrawOpts) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let Some(shaped) = self.shape_for(text, opts) else {
            return 0.0;
        };
        let width_px = shaped_width(&shaped);
        self.emit_instances(x, y, &shaped, opts);
        width_px / self.scale_factor
    }

    /// Measure `text` under `opts` without recording a draw. Returns
    /// logical-pixel width.
    pub fn measure(&mut self, text: &str, opts: &DrawOpts) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        self.shape_for(text, opts)
            .map(|r| shaped_width(&r) / self.scale_factor)
            .unwrap_or(0.0)
    }

    //  Shape pipeline — shared cache + cfg-gated backend call

    fn shape_for(&mut self, text: &str, opts: &DrawOpts) -> Option<ShapedRun> {
        use crate::{Attributes, SpanStyle, Stretch, Style as FontStyle, Weight};

        let scaled = opts.font_size * self.scale_factor;
        let size_bucket = (scaled * 4.0).round().clamp(0.0, u16::MAX as f32) as u16;
        let size_u16 = scaled.round().clamp(1.0, u16::MAX as f32) as u16;
        let style_flags =
            (if opts.bold { 1u8 } else { 0 }) | (if opts.italic { 2u8 } else { 0 });

        let first_ch = text.chars().next()?;
        let (font_id, _is_emoji) = match self.font_resolve.entry((first_ch, style_flags))
        {
            std::collections::hash_map::Entry::Occupied(e) => *e.get(),
            std::collections::hash_map::Entry::Vacant(e) => {
                let mut ss = SpanStyle::default();
                let weight = if opts.bold {
                    Weight::BOLD
                } else {
                    Weight::NORMAL
                };
                let fstyle = if opts.italic {
                    FontStyle::Italic
                } else {
                    FontStyle::Normal
                };
                ss.font_attrs = Attributes::new(Stretch::NORMAL, weight, fstyle);
                #[cfg(target_os = "macos")]
                let resolved = self.font_library.resolve_font_for_char(first_ch, &ss);

                #[cfg(not(target_os = "macos"))]
                let resolved = {
                    let lib = self.font_library.inner.read();
                    lib.find_best_font_match(first_ch, &ss)
                        .unwrap_or((0, false))
                };
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

        let (synthetic_bold, synthetic_italic) = match self.synthesis_cache.entry(font_id)
        {
            std::collections::hash_map::Entry::Occupied(e) => *e.get(),
            std::collections::hash_map::Entry::Vacant(e) => {
                let lib = self.font_library.inner.read();
                let fd = lib.get(&(font_id as usize));
                *e.insert((fd.should_embolden, fd.should_italicize))
            }
        };

        #[cfg(target_os = "macos")]
        let (glyphs, ascent_px) = {
            let handle = match self.handle_cache.entry(font_id) {
                std::collections::hash_map::Entry::Occupied(e) => e.into_mut().clone(),
                std::collections::hash_map::Entry::Vacant(e) => {
                    let h = self.font_library.ct_font(font_id as usize)?;
                    e.insert(h.clone());
                    h
                }
            };
            let ascent_px = *self
                .ascent_cache
                .entry((font_id, size_bucket))
                .or_insert_with(|| {
                    let m = crate::font::macos::font_metrics(&handle, size_u16 as f32);
                    m.ascent.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
                });
            let ct_glyphs =
                crate::font::macos::shape_text(&handle, text, size_u16 as f32);
            let glyphs: Vec<ShapedGlyph> = ct_glyphs
                .iter()
                .map(|g| ShapedGlyph {
                    id: g.id,
                    x: g.x,
                    y: g.y,
                    advance: g.advance,
                    cluster: g.cluster,
                })
                .collect();
            (glyphs, ascent_px)
        };

        #[cfg(not(target_os = "macos"))]
        let (glyphs, ascent_px) = {
            use swash::FontRef;

            // Pull (or cache) the font bytes + offset + key once per
            // font_id to avoid the RwLock read-lock per shape.
            let font_entry = self.font_data_cache.entry(font_id).or_insert_with(|| {
                let lib = self.font_library.inner.read();
                lib.get_data(&(font_id as usize)).expect(
                    "font id resolved but get_data returned None — cache invariant",
                )
            });
            let font_ref = FontRef {
                data: font_entry.0.as_ref(),
                offset: font_entry.1,
                key: font_entry.2,
            };

            // Ascent — via swash metrics scaled to device-px size.
            let ascent_px = *self
                .ascent_cache
                .entry((font_id, size_bucket))
                .or_insert_with(|| {
                    let m = font_ref.metrics(&[]).scale(size_u16 as f32);
                    m.ascent.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
                });

            // Shape with swash. Flatten clusters to a Vec<ShapedGlyph>
            // with UTF-8 byte offset as `cluster`.
            let mut shaper = self
                .shape_ctx
                .builder(font_ref)
                .size(size_u16 as f32)
                .build();
            shaper.add_str(text);
            let mut glyphs: Vec<ShapedGlyph> = Vec::new();
            shaper.shape_with(|cluster| {
                let byte_offset = cluster.source.start;
                for g in cluster.glyphs {
                    glyphs.push(ShapedGlyph {
                        id: g.id,
                        x: g.x,
                        y: g.y,
                        advance: g.advance,
                        cluster: byte_offset,
                    });
                }
            });
            (glyphs, ascent_px)
        };

        let run = ShapedRun {
            font_id,
            size_u16,
            size_bucket,
            synthetic_bold,
            synthetic_italic,
            ascent_px,
            glyphs,
        };
        self.shape_cache.insert(hash, run.clone());
        Some(run)
    }

    //  Emit pipeline — rasterize + push TextInstance

    fn emit_instances(&mut self, x: f32, y: f32, run: &ShapedRun, opts: &DrawOpts) {
        let scale = self.scale_factor;
        let mut pen_x = x * scale;
        let py = y * scale;
        let color = opts.color;

        for glyph in &run.glyphs {
            let Some((slot_x, slot_y, slot_w, slot_h, bearing_x, bearing_y, is_color)) =
                self.rasterize_slot(run, glyph.id)
            else {
                continue;
            };
            if slot_w == 0 || slot_h == 0 {
                pen_x += glyph.advance;
                continue;
            }

            let atlas_tag = if is_color { 1u8 } else { 0u8 };
            let instance_color = if is_color {
                [255u8, 255, 255, 255]
            } else {
                color
            };

            self.instances.push(TextInstance {
                pos: [pen_x + glyph.x, py + glyph.y.max(0.0)],
                glyph_pos: [slot_x as u32, slot_y as u32],
                glyph_size: [slot_w as u32, slot_h as u32],
                bearings: [bearing_x, bearing_y],
                color: instance_color,
                atlas: atlas_tag,
                _pad: [0; 3],
            });

            pen_x += glyph.advance;
        }
    }

    /// Lookup or rasterize-and-insert a glyph. Returns
    /// `(x, y, w, h, bearing_x, bearing_y, is_color)` from the atlas
    /// slot. Per-OS rasterize path; shared slot shape.
    #[allow(clippy::type_complexity)]
    fn rasterize_slot(
        &mut self,
        run: &ShapedRun,
        glyph_id: u16,
    ) -> Option<(u16, u16, u16, u16, i16, i16, bool)> {
        let key = crate::grid::GlyphKey {
            font_id: run.font_id,
            glyph_id: glyph_id as u32,
            size_bucket: run.size_bucket,
        };

        // CPU path takes precedence whenever it's initialized
        // The CPU atlases are RAM-resident; the GPU branches below
        // would either fail (no Metal/Vulkan/Wgpu state) or pointlessly
        // upload to a GPU we won't read from.
        if self.cpu.is_some() {
            return self.rasterize_slot_cpu(run, glyph_id, key);
        }

        // macOS (CoreText → MetalGlyphAtlas)
        #[cfg(target_os = "macos")]
        {
            let state = self.metal.as_mut()?;

            if let Some(s) = state.atlas_grayscale.lookup(key) {
                return Some((s.x, s.y, s.w, s.h, s.bearing_x, s.bearing_y, false));
            }
            if let Some(s) = state.atlas_color.lookup(key) {
                return Some((s.x, s.y, s.w, s.h, s.bearing_x, s.bearing_y, true));
            }

            let handle = self.handle_cache.get(&run.font_id)?.clone();
            let raw = crate::font::macos::rasterize_glyph(
                &handle,
                glyph_id,
                run.size_u16 as f32,
                /* is_emoji: */ false,
                run.synthetic_italic,
                run.synthetic_bold,
            )?;
            let is_color = raw.is_color;
            let raster = crate::grid::RasterizedGlyph {
                width: raw.width.min(u16::MAX as u32) as u16,
                height: raw.height.min(u16::MAX as u32) as u16,
                bearing_x: raw.left.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                bearing_y: {
                    let top_i16 = raw.top.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                    run.ascent_px.saturating_sub(top_i16)
                },
                bytes: &raw.bytes,
            };
            let slot = if is_color {
                match state.atlas_color.insert(key, raster) {
                    Some(s) => s,
                    None => {
                        if state.atlas_color.grow(&state.device, &state.command_queue) {
                            state.atlas_color.insert(key, raster)?
                        } else {
                            return None;
                        }
                    }
                }
            } else {
                match state.atlas_grayscale.insert(key, raster) {
                    Some(s) => s,
                    None => {
                        if state
                            .atlas_grayscale
                            .grow(&state.device, &state.command_queue)
                        {
                            state.atlas_grayscale.insert(key, raster)?
                        } else {
                            return None;
                        }
                    }
                }
            };
            Some((
                slot.x,
                slot.y,
                slot.w,
                slot.h,
                slot.bearing_x,
                slot.bearing_y,
                is_color,
            ))
        }

        // non-macOS (swash → VulkanGlyphAtlas or WgpuGlyphAtlas)
        #[cfg(not(target_os = "macos"))]
        {
            // Look up the slot first — backend-agnostic — and
            // rasterize+insert into whichever atlas is initialized.
            // Vulkan takes precedence on Linux when the Vulkan
            // backend is active; wgpu is the fallback.
            #[cfg(target_os = "linux")]
            if self.vulkan.is_some() {
                let state = self.vulkan.as_mut()?;
                if let Some(s) = state.atlas_grayscale.lookup(key) {
                    return Some((s.x, s.y, s.w, s.h, s.bearing_x, s.bearing_y, false));
                }
                if let Some(s) = state.atlas_color.lookup(key) {
                    return Some((s.x, s.y, s.w, s.h, s.bearing_x, s.bearing_y, true));
                }

                let font_entry = self.font_data_cache.get(&run.font_id)?.clone();
                let raw = rasterize_swash_glyph(
                    &mut self.scale_ctx,
                    &font_entry,
                    glyph_id,
                    run.size_u16 as f32,
                    run.synthetic_bold,
                    run.synthetic_italic,
                    self.font_library.inner.read().hinting,
                )?;
                let is_color = raw.is_color;
                let raster = crate::grid::RasterizedGlyph {
                    width: raw.width.min(u16::MAX as u32) as u16,
                    height: raw.height.min(u16::MAX as u32) as u16,
                    bearing_x: raw.left.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                    bearing_y: {
                        let top_i16 =
                            raw.top.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                        run.ascent_px.saturating_sub(top_i16)
                    },
                    bytes: &raw.bytes,
                };
                let slot = if is_color {
                    state.atlas_color.insert(key, raster)?
                } else {
                    state.atlas_grayscale.insert(key, raster)?
                };
                return Some((
                    slot.x,
                    slot.y,
                    slot.w,
                    slot.h,
                    slot.bearing_x,
                    slot.bearing_y,
                    is_color,
                ));
            }

            #[cfg(feature = "wgpu")]
            {
                let state = self.wgpu.as_mut()?;

                if let Some(s) = state.atlas_grayscale.lookup(key) {
                    return Some((s.x, s.y, s.w, s.h, s.bearing_x, s.bearing_y, false));
                }
                if let Some(s) = state.atlas_color.lookup(key) {
                    return Some((s.x, s.y, s.w, s.h, s.bearing_x, s.bearing_y, true));
                }

                let font_entry = self.font_data_cache.get(&run.font_id)?.clone();
                let raw = rasterize_swash_glyph(
                    &mut self.scale_ctx,
                    &font_entry,
                    glyph_id,
                    run.size_u16 as f32,
                    run.synthetic_bold,
                    run.synthetic_italic,
                    self.font_library.inner.read().hinting,
                )?;
                let is_color = raw.is_color;

                let raster = crate::grid::RasterizedGlyph {
                    width: raw.width.min(u16::MAX as u32) as u16,
                    height: raw.height.min(u16::MAX as u32) as u16,
                    bearing_x: raw.left.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                    bearing_y: {
                        let top_i16 =
                            raw.top.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                        run.ascent_px.saturating_sub(top_i16)
                    },
                    bytes: &raw.bytes,
                };
                let slot = if is_color {
                    state.atlas_color.insert(key, raster)?
                } else {
                    state.atlas_grayscale.insert(key, raster)?
                };
                Some((
                    slot.x,
                    slot.y,
                    slot.w,
                    slot.h,
                    slot.bearing_x,
                    slot.bearing_y,
                    is_color,
                ))
            }
            #[cfg(not(feature = "wgpu"))]
            {
                let _ = (run, glyph_id);
                None
            }
        }
    }

    /// Software-atlas variant of `rasterize_slot`. Same flow as the
    /// GPU branches: lookup, then OS-native (CoreText) or swash
    /// rasterize, then insert into the matching CPU atlas. Returns
    /// `None` when shaping caches haven't been primed for this
    /// font_id (shouldn't happen — `shape()` always runs first) or
    /// when the atlas is at `ATLAS_MAX_SIZE` and the glyph won't fit.
    #[allow(clippy::type_complexity)]
    fn rasterize_slot_cpu(
        &mut self,
        run: &ShapedRun,
        glyph_id: u16,
        key: crate::grid::GlyphKey,
    ) -> Option<(u16, u16, u16, u16, i16, i16, bool)> {
        // Lookup first — both atlases.
        {
            let state = self.cpu.as_ref()?;
            if let Some(s) = state.atlas_grayscale.lookup(key) {
                return Some((s.x, s.y, s.w, s.h, s.bearing_x, s.bearing_y, false));
            }
            if let Some(s) = state.atlas_color.lookup(key) {
                return Some((s.x, s.y, s.w, s.h, s.bearing_x, s.bearing_y, true));
            }
        }

        // Rasterize. macOS uses CoreText so emoji come out correctly;
        // every other target uses swash.
        #[cfg(target_os = "macos")]
        let (raw_w, raw_h, raw_left, raw_top, raw_is_color, raw_bytes) = {
            let handle = self.handle_cache.get(&run.font_id)?.clone();
            let raw = crate::font::macos::rasterize_glyph(
                &handle,
                glyph_id,
                run.size_u16 as f32,
                /* is_emoji: */ false,
                run.synthetic_italic,
                run.synthetic_bold,
            )?;
            (
                raw.width,
                raw.height,
                raw.left,
                raw.top,
                raw.is_color,
                raw.bytes,
            )
        };
        #[cfg(not(target_os = "macos"))]
        let (raw_w, raw_h, raw_left, raw_top, raw_is_color, raw_bytes) = {
            let font_entry = self.font_data_cache.get(&run.font_id)?.clone();
            let raw = rasterize_swash_glyph(
                &mut self.scale_ctx,
                &font_entry,
                glyph_id,
                run.size_u16 as f32,
                run.synthetic_bold,
                run.synthetic_italic,
                self.font_library.inner.read().hinting,
            )?;
            (
                raw.width,
                raw.height,
                raw.left,
                raw.top,
                raw.is_color,
                raw.bytes,
            )
        };

        let raster = crate::grid::RasterizedGlyph {
            width: raw_w.min(u16::MAX as u32) as u16,
            height: raw_h.min(u16::MAX as u32) as u16,
            bearing_x: raw_left.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
            bearing_y: {
                let top_i16 = raw_top.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                run.ascent_px.saturating_sub(top_i16)
            },
            bytes: &raw_bytes,
        };

        let state = self.cpu.as_mut()?;
        let slot = if raw_is_color {
            state.atlas_color.insert(key, raster).or_else(|| {
                if state.atlas_color.grow() {
                    state.atlas_color.insert(key, raster)
                } else {
                    None
                }
            })?
        } else {
            state.atlas_grayscale.insert(key, raster).or_else(|| {
                if state.atlas_grayscale.grow() {
                    state.atlas_grayscale.insert(key, raster)
                } else {
                    None
                }
            })?
        };
        Some((
            slot.x,
            slot.y,
            slot.w,
            slot.h,
            slot.bearing_x,
            slot.bearing_y,
            raw_is_color,
        ))
    }

    /// Paint the queued UI text instances into the caller-supplied
    /// `0x00RRGGBB` u32 buffer. Mirrors `text_vertex` /
    /// `grid_text_fragment`: glyph origin = `pos + bearings`; mask
    /// glyphs use `instance.color`, color glyphs sample directly.
    /// No-op when CPU state is absent or no instances were queued.
    pub fn render_cpu(&self, buf: &mut [u32], buf_w: u32, buf_h: u32) {
        if self.instances.is_empty() {
            return;
        }
        let Some(state) = self.cpu.as_ref() else {
            return;
        };
        let buf_w_i = buf_w as i32;
        let buf_h_i = buf_h as i32;
        let mask = state.atlas_grayscale.pixels();
        let mask_side = state.atlas_grayscale.side() as usize;
        let color_atlas = state.atlas_color.pixels();
        let color_side = state.atlas_color.side() as usize;

        for inst in &self.instances {
            let gw = inst.glyph_size[0] as i32;
            let gh = inst.glyph_size[1] as i32;
            if gw <= 0 || gh <= 0 {
                continue;
            }
            let glyph_x = (inst.pos[0] + inst.bearings[0] as f32) as i32;
            let glyph_y = (inst.pos[1] + inst.bearings[1] as f32) as i32;
            let ax = inst.glyph_pos[0] as usize;
            let ay = inst.glyph_pos[1] as usize;

            if inst.atlas == 1 {
                blit_text_color(
                    buf,
                    buf_w_i,
                    buf_h_i,
                    glyph_x,
                    glyph_y,
                    gw,
                    gh,
                    color_atlas,
                    color_side,
                    ax,
                    ay,
                );
            } else {
                blit_text_mask(
                    buf, buf_w_i, buf_h_i, glyph_x, glyph_y, gw, gh, mask, mask_side, ax,
                    ay, inst.color,
                );
            }
        }
    }

    //  macOS GPU backend

    #[cfg(target_os = "macos")]
    pub fn init_metal(
        &mut self,
        device: &metal::Device,
        command_queue: &metal::CommandQueue,
    ) {
        if self.metal.is_some() {
            return;
        }
        let pipeline = build_text_pipeline_metal(device);
        let instance_capacity: usize = 256;
        let instance_buffer = alloc_instance_buffer_metal(device, instance_capacity);
        self.metal = Some(TextMetalState {
            device: device.to_owned(),
            command_queue: command_queue.to_owned(),
            atlas_grayscale: crate::grid::metal::MetalGlyphAtlas::new_grayscale(device),
            atlas_color: crate::grid::metal::MetalGlyphAtlas::new_color(device),
            pipeline,
            instance_buffer,
            instance_capacity,
        });
    }

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

        if instance_count > state.instance_capacity {
            let new_cap = instance_count.next_power_of_two().max(256);
            state.instance_buffer = alloc_instance_buffer_metal(&state.device, new_cap);
            state.instance_capacity = new_cap;
        }

        unsafe {
            let dst = state.instance_buffer.contents() as *mut TextInstance;
            std::ptr::copy_nonoverlapping(self.instances.as_ptr(), dst, instance_count);
        }

        encoder.set_render_pipeline_state(&state.pipeline);
        encoder.set_vertex_buffer(0, Some(&state.instance_buffer), 0);
        let vp: [f32; 2] = viewport;
        encoder.set_vertex_bytes(
            1,
            std::mem::size_of::<[f32; 2]>() as u64,
            vp.as_ptr() as *const std::ffi::c_void,
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

    //  wgpu GPU backend

    #[cfg(feature = "wgpu")]
    pub fn init_wgpu(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) {
        if self.wgpu.is_some() {
            return;
        }
        let atlas_grayscale =
            crate::grid::webgpu::WgpuGlyphAtlas::new_grayscale(device, queue.clone());
        let atlas_color =
            crate::grid::webgpu::WgpuGlyphAtlas::new_color(device, queue.clone());

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sugarloaf.text.uniforms"),
            size: 16, // vec2<f32> viewport + vec2<f32> pad (WGSL min alignment)
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bgl =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("sugarloaf.text.uniform_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: std::num::NonZeroU64::new(16),
                    },
                    count: None,
                }],
            });
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sugarloaf.text.uniform_bg"),
            layout: &uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let atlas_bgl = create_text_atlas_bgl_wgpu(device);
        let atlas_bind_group = create_text_atlas_bg_wgpu(
            device,
            &atlas_bgl,
            atlas_grayscale.view(),
            atlas_color.view(),
        );

        let pipeline =
            build_text_pipeline_wgpu(device, format, &[&uniform_bgl, &atlas_bgl]);
        let instance_capacity: usize = 256;
        let instance_buffer = alloc_instance_buffer_wgpu(device, instance_capacity);

        self.wgpu = Some(TextWgpuState {
            device: device.to_owned(),
            queue: queue.to_owned(),
            atlas_grayscale,
            atlas_color,
            uniform_buffer,
            uniform_bind_group,
            atlas_bind_group,
            atlas_bind_group_layout: atlas_bgl,
            pipeline,
            instance_buffer,
            instance_capacity,
        });
    }

    /// Record the UI text pass into `render_pass`. No-op if wgpu state
    /// isn't initialised or there are no instances this frame.
    #[cfg(feature = "wgpu")]
    pub fn render_wgpu<'pass>(
        &'pass mut self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        viewport: [f32; 2],
    ) {
        let instance_count = self.instances.len();
        if instance_count == 0 {
            return;
        }
        let Some(state) = self.wgpu.as_mut() else {
            return;
        };

        // Upload uniforms (viewport + 8 bytes pad).
        let uniforms: [f32; 4] = [viewport[0], viewport[1], 0.0, 0.0];
        state.queue.write_buffer(
            &state.uniform_buffer,
            0,
            bytemuck::cast_slice(&uniforms),
        );

        // Grow instance buffer if necessary.
        if instance_count > state.instance_capacity {
            let new_cap = instance_count.next_power_of_two().max(256);
            state.instance_buffer = alloc_instance_buffer_wgpu(&state.device, new_cap);
            state.instance_capacity = new_cap;
        }

        // Upload instances.
        state.queue.write_buffer(
            &state.instance_buffer,
            0,
            bytemuck_instances(&self.instances),
        );

        render_pass.set_pipeline(&state.pipeline);
        render_pass.set_bind_group(0, &state.uniform_bind_group, &[]);
        render_pass.set_bind_group(1, &state.atlas_bind_group, &[]);
        render_pass.set_vertex_buffer(0, state.instance_buffer.slice(..));
        render_pass.draw(0..4, 0..instance_count as u32);
    }

    //  Vulkan GPU backend

    #[cfg(target_os = "linux")]
    pub fn init_vulkan(&mut self, ctx: &crate::context::vulkan::VulkanContext) {
        if self.vulkan.is_some() {
            return;
        }
        let state = build_text_vulkan_state(ctx);
        self.vulkan = Some(state);
    }

    /// Pre-pass hook: drain pending atlas uploads into `cmd`. MUST be
    /// called BEFORE `Sugarloaf::render_vulkan` opens its
    /// dynamic-rendering pass (matches `GridRenderer::prepare_vulkan`).
    /// No-op when neither atlas has pending uploads.
    #[cfg(target_os = "linux")]
    pub fn prepare_vulkan(
        &mut self,
        _ctx: &crate::context::vulkan::VulkanContext,
        cmd: ash::vk::CommandBuffer,
        slot: usize,
    ) {
        let Some(state) = self.vulkan.as_mut() else {
            return;
        };
        state.atlas_grayscale.flush_uploads(
            &state.device,
            &state.instance,
            state.physical_device,
            cmd,
            slot,
        );
        state.atlas_color.flush_uploads(
            &state.device,
            &state.instance,
            state.physical_device,
            cmd,
            slot,
        );
    }

    /// Record the UI text pass into `cmd`. Caller has already opened
    /// the dynamic-rendering pass and set viewport/scissor. No-op
    /// when no instances were recorded this frame or the Vulkan
    /// state isn't initialised.
    #[cfg(target_os = "linux")]
    pub fn render_vulkan(
        &mut self,
        cmd: ash::vk::CommandBuffer,
        slot: usize,
        viewport: [f32; 2],
    ) {
        let instance_count = self.instances.len();
        if instance_count == 0 {
            return;
        }
        let Some(state) = self.vulkan.as_mut() else {
            return;
        };

        // Upload uniforms (viewport + 8B pad — std140).
        let uniforms: [f32; 4] = [viewport[0], viewport[1], 0.0, 0.0];
        unsafe {
            let dst = state.uniform_buffers[slot].as_mut_ptr() as *mut [f32; 4];
            std::ptr::write(dst, uniforms);
        }

        // Grow per-slot instance buffer if needed.
        let needed_bytes = instance_count * std::mem::size_of::<TextInstance>();
        if instance_count > state.instance_capacity[slot] {
            let new_cap = instance_count.next_power_of_two().max(256);
            state.instance_buffers[slot] =
                Some(crate::context::vulkan::allocate_host_visible_buffer_raw(
                    &state.device,
                    &state.instance,
                    state.physical_device,
                    (new_cap * std::mem::size_of::<TextInstance>()) as u64,
                    ash::vk::BufferUsageFlags::VERTEX_BUFFER,
                ));
            state.instance_capacity[slot] = new_cap;
        }
        let instance_buf = state.instance_buffers[slot].as_ref().unwrap();
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.instances.as_ptr() as *const u8,
                instance_buf.as_mut_ptr(),
                needed_bytes,
            );
        }

        unsafe {
            state.device.cmd_bind_pipeline(
                cmd,
                ash::vk::PipelineBindPoint::GRAPHICS,
                state.pipeline,
            );
            state.device.cmd_bind_descriptor_sets(
                cmd,
                ash::vk::PipelineBindPoint::GRAPHICS,
                state.pipeline_layout,
                0,
                &[
                    state.uniform_descriptor_sets[slot],
                    state.atlas_descriptor_set,
                ],
                &[],
            );
            state
                .device
                .cmd_bind_vertex_buffers(cmd, 0, &[instance_buf.handle()], &[0]);
            state.device.cmd_draw(cmd, 4, instance_count as u32, 0, 0);
        }
    }
}

//  Helpers

#[inline]
fn shaped_width(run: &ShapedRun) -> f32 {
    run.glyphs.iter().map(|g| g.advance).sum()
}

#[cfg(feature = "wgpu")]
fn bytemuck_instances(insts: &[TextInstance]) -> &[u8] {
    // Safety: TextInstance is repr(C) with all-primitive fields (no
    // padding surprises thanks to 4-byte alignment + explicit _pad).
    // This is the same pattern sugarloaf uses for other instance
    // buffers (e.g. grid's CellBg upload).
    unsafe {
        std::slice::from_raw_parts(
            insts.as_ptr() as *const u8,
            std::mem::size_of_val(insts),
        )
    }
}

//  Swash rasterize — non-macOS

#[cfg(not(target_os = "macos"))]
struct SwashRawGlyph {
    width: u32,
    height: u32,
    left: i32,
    top: i32,
    is_color: bool,
    bytes: Vec<u8>,
}

#[cfg(not(target_os = "macos"))]
fn rasterize_swash_glyph(
    scale_ctx: &mut swash::scale::ScaleContext,
    font_entry: &(crate::font::SharedData, u32, swash::CacheKey),
    glyph_id: u16,
    size_px: f32,
    synthetic_bold: bool,
    synthetic_italic: bool,
    hint: bool,
) -> Option<SwashRawGlyph> {
    use swash::scale::{
        image::{Content, Image as GlyphImage},
        Render, Source, StrikeWith,
    };
    use swash::zeno::{Angle, Format, Transform};
    use swash::FontRef;

    let font_ref = FontRef {
        data: font_entry.0.as_ref(),
        offset: font_entry.1,
        key: font_entry.2,
    };

    let mut scaler = scale_ctx.builder(font_ref).hint(hint).size(size_px).build();

    let mut image = GlyphImage::new();
    let sources: &[Source] = &[
        Source::ColorOutline(0),
        Source::ColorBitmap(StrikeWith::BestFit),
        Source::Outline,
    ];
    let rendered = Render::new(sources)
        .format(Format::Alpha)
        .embolden(if synthetic_bold { 0.5 } else { 0.0 })
        .transform(if synthetic_italic {
            Some(Transform::skew(
                Angle::from_degrees(14.0),
                Angle::from_degrees(0.0),
            ))
        } else {
            None
        })
        .render_into(&mut scaler, glyph_id, &mut image);

    if !rendered {
        return None;
    }

    let is_color = image.content == Content::Color;
    Some(SwashRawGlyph {
        width: image.placement.width,
        height: image.placement.height,
        left: image.placement.left,
        top: image.placement.top,
        is_color,
        bytes: image.data,
    })
}

//  Metal pipeline construction

#[cfg(target_os = "macos")]
fn build_text_pipeline_metal(device: &metal::Device) -> metal::RenderPipelineState {
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

    let vd = VertexDescriptor::new();
    let attrs = vd.attributes();
    // attribute 0: pos [f32;2] @ 0
    let a = attrs.object_at(0).unwrap();
    a.set_format(MTLVertexFormat::Float2);
    a.set_buffer_index(0);
    a.set_offset(0);
    // attribute 1: glyph_pos [u32;2] @ 8
    let a = attrs.object_at(1).unwrap();
    a.set_format(MTLVertexFormat::UInt2);
    a.set_buffer_index(0);
    a.set_offset(8);
    // attribute 2: glyph_size [u32;2] @ 16
    let a = attrs.object_at(2).unwrap();
    a.set_format(MTLVertexFormat::UInt2);
    a.set_buffer_index(0);
    a.set_offset(16);
    // attribute 3: bearings [i16;2] @ 24
    let a = attrs.object_at(3).unwrap();
    a.set_format(MTLVertexFormat::Short2);
    a.set_buffer_index(0);
    a.set_offset(24);
    // attribute 4: color [u8;4] @ 28
    let a = attrs.object_at(4).unwrap();
    a.set_format(MTLVertexFormat::UChar4);
    a.set_buffer_index(0);
    a.set_offset(28);
    // attribute 5: atlas u8 @ 32
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

#[cfg(target_os = "macos")]
fn alloc_instance_buffer_metal(device: &metal::Device, capacity: usize) -> metal::Buffer {
    let size = (capacity.max(1) * std::mem::size_of::<TextInstance>()) as u64;
    device.new_buffer(size, metal::MTLResourceOptions::StorageModeShared)
}

//  wgpu pipeline construction

#[cfg(feature = "wgpu")]
fn create_text_atlas_bgl_wgpu(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("sugarloaf.text.atlas_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    })
}

#[cfg(feature = "wgpu")]
fn create_text_atlas_bg_wgpu(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    grayscale: &wgpu::TextureView,
    color: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("sugarloaf.text.atlas_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(grayscale),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(color),
            },
        ],
    })
}

#[cfg(feature = "wgpu")]
fn alloc_instance_buffer_wgpu(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    let size = (capacity.max(1) * std::mem::size_of::<TextInstance>()) as u64;
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sugarloaf.text.instances"),
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[cfg(feature = "wgpu")]
fn build_text_pipeline_wgpu(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    bind_group_layouts: &[&wgpu::BindGroupLayout],
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("sugarloaf.text.wgsl"),
        source: wgpu::ShaderSource::Wgsl(include_str!("text_shader.wgsl").into()),
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("sugarloaf.text.pipeline_layout"),
        bind_group_layouts,
        immediate_size: 0,
    });

    let stride = std::mem::size_of::<TextInstance>() as u64;
    let attrs = [
        // location 0: pos [f32;2] @ 0
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        },
        // location 1: glyph_pos [u32;2] @ 8
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32x2,
            offset: 8,
            shader_location: 1,
        },
        // location 2: glyph_size [u32;2] @ 16
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32x2,
            offset: 16,
            shader_location: 2,
        },
        // location 3: bearings [i16;2] @ 24 → Sint16x2 (sign-ext)
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Sint16x2,
            offset: 24,
            shader_location: 3,
        },
        // location 4: color [u8;4] @ 28 → Unorm8x4
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Unorm8x4,
            offset: 28,
            shader_location: 4,
        },
        // location 5: atlas u8 + _pad[3] @ 32 → Uint8x4 (we use .x only)
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint8x4,
            offset: 32,
            shader_location: 5,
        },
    ];
    let vbuf = wgpu::VertexBufferLayout {
        array_stride: stride,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &attrs,
    };

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sugarloaf.text.pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("text_vertex"),
            buffers: &[vbuf],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("text_fragment"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(premul_blend_wgpu()),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(feature = "wgpu")]
fn premul_blend_wgpu() -> wgpu::BlendState {
    wgpu::BlendState {
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
    }
}

//  Vulkan pipeline + state construction

// Compiled at build time by `sugarloaf/build.rs`. The fragment
// shader is shared with the grid text pass — same atlas sampling,
// same inputs.
#[cfg(target_os = "linux")]
const UI_TEXT_VERT_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/ui_text.vert.spv"));
#[cfg(target_os = "linux")]
const UI_TEXT_FRAG_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/grid_text.frag.spv"));

#[cfg(target_os = "linux")]
fn build_text_vulkan_state(
    ctx: &crate::context::vulkan::VulkanContext,
) -> TextVulkanState {
    use crate::context::vulkan::FRAMES_IN_FLIGHT;
    use ash::vk;

    let device = ctx.device().clone();
    let instance = ctx.instance().clone();
    let physical_device = ctx.physical_device();

    let atlas_grayscale = crate::grid::vulkan::VulkanGlyphAtlas::new_grayscale(ctx);
    let atlas_color = crate::grid::vulkan::VulkanGlyphAtlas::new_color(ctx);
    let sampler = create_text_sampler(&device);

    let uniform_buffers = std::array::from_fn(|_| {
        ctx.allocate_host_visible_buffer(16, vk::BufferUsageFlags::UNIFORM_BUFFER)
    });

    // Layouts: set 0 = uniform (per slot), set 1 = atlases (shared).
    let uniform_descriptor_set_layout = unsafe {
        let bindings = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX)];
        let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        device
            .create_descriptor_set_layout(&info, None)
            .expect("create_descriptor_set_layout(ui_text uniform)")
    };
    let atlas_descriptor_set_layout = unsafe {
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
        device
            .create_descriptor_set_layout(&info, None)
            .expect("create_descriptor_set_layout(ui_text atlas)")
    };

    // Pool sized for FRAMES_IN_FLIGHT uniform sets + 1 atlas set.
    let descriptor_pool = unsafe {
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
        device
            .create_descriptor_pool(&info, None)
            .expect("create_descriptor_pool(ui_text)")
    };

    let uniform_descriptor_sets = unsafe {
        let layouts = [uniform_descriptor_set_layout; FRAMES_IN_FLIGHT];
        let info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);
        let sets = device
            .allocate_descriptor_sets(&info)
            .expect("allocate_descriptor_sets(ui_text uniform)");
        let mut out = [vk::DescriptorSet::null(); FRAMES_IN_FLIGHT];
        out.copy_from_slice(&sets);
        out
    };
    let atlas_descriptor_set = unsafe {
        let layouts = [atlas_descriptor_set_layout];
        let info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);
        device
            .allocate_descriptor_sets(&info)
            .expect("allocate_descriptor_sets(ui_text atlas)")[0]
    };

    // Update descriptor sets.
    for slot in 0..FRAMES_IN_FLIGHT {
        let uniform_info = vk::DescriptorBufferInfo::default()
            .buffer(uniform_buffers[slot].handle())
            .offset(0)
            .range(uniform_buffers[slot].size());
        let infos = [uniform_info];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(uniform_descriptor_sets[slot])
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(&infos);
        unsafe {
            device.update_descriptor_sets(&[write], &[]);
        }
    }
    {
        let gray_info = vk::DescriptorImageInfo::default()
            .sampler(sampler)
            .image_view(atlas_grayscale.image_view())
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        let gray_infos = [gray_info];
        let color_info = vk::DescriptorImageInfo::default()
            .sampler(sampler)
            .image_view(atlas_color.image_view())
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        let color_infos = [color_info];
        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(atlas_descriptor_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&gray_infos),
            vk::WriteDescriptorSet::default()
                .dst_set(atlas_descriptor_set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&color_infos),
        ];
        unsafe {
            device.update_descriptor_sets(&writes, &[]);
        }
    }

    let pipeline_layout = unsafe {
        let set_layouts = [uniform_descriptor_set_layout, atlas_descriptor_set_layout];
        let info = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
        device
            .create_pipeline_layout(&info, None)
            .expect("create_pipeline_layout(ui_text)")
    };
    let pipeline = build_ui_text_pipeline_vulkan(
        &device,
        ctx.pipeline_cache(),
        pipeline_layout,
        ctx.swapchain_format(),
    );

    TextVulkanState {
        device,
        instance,
        physical_device,
        atlas_grayscale,
        atlas_color,
        sampler,
        uniform_buffers,
        instance_buffers: std::array::from_fn(|_| None),
        instance_capacity: [0; FRAMES_IN_FLIGHT],
        descriptor_pool,
        uniform_descriptor_set_layout,
        atlas_descriptor_set_layout,
        uniform_descriptor_sets,
        atlas_descriptor_set,
        pipeline_layout,
        pipeline,
    }
}

#[cfg(target_os = "linux")]
fn create_text_sampler(device: &ash::Device) -> ash::vk::Sampler {
    use ash::vk;
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
            .expect("create_sampler(ui_text)")
    }
}

#[cfg(target_os = "linux")]
fn build_ui_text_pipeline_vulkan(
    device: &ash::Device,
    pipeline_cache: ash::vk::PipelineCache,
    layout: ash::vk::PipelineLayout,
    color_format: ash::vk::Format,
) -> ash::vk::Pipeline {
    use ash::vk;

    let vert = load_shader_module_vulkan(device, UI_TEXT_VERT_SPV);
    let frag = load_shader_module_vulkan(device, UI_TEXT_FRAG_SPV);
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

    // Vertex input mirrors `TextInstance` (36 bytes).
    let bindings = [vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(std::mem::size_of::<TextInstance>() as u32)
        .input_rate(vk::VertexInputRate::INSTANCE)];
    let attrs = [
        // 0: pos vec2 @ 0
        vk::VertexInputAttributeDescription::default()
            .location(0)
            .binding(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(0),
        // 1: glyph_pos uvec2 @ 8
        vk::VertexInputAttributeDescription::default()
            .location(1)
            .binding(0)
            .format(vk::Format::R32G32_UINT)
            .offset(8),
        // 2: glyph_size uvec2 @ 16
        vk::VertexInputAttributeDescription::default()
            .location(2)
            .binding(0)
            .format(vk::Format::R32G32_UINT)
            .offset(16),
        // 3: bearings ivec2 @ 24
        vk::VertexInputAttributeDescription::default()
            .location(3)
            .binding(0)
            .format(vk::Format::R16G16_SINT)
            .offset(24),
        // 4: color vec4 @ 28
        vk::VertexInputAttributeDescription::default()
            .location(4)
            .binding(0)
            .format(vk::Format::R8G8B8A8_UNORM)
            .offset(28),
        // 5: atlas u8 @ 32
        vk::VertexInputAttributeDescription::default()
            .location(5)
            .binding(0)
            .format(vk::Format::R8_UINT)
            .offset(32),
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

    // Premultiplied-over-from-one — fragment returns premultiplied.
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
            .expect("create_graphics_pipelines(ui_text)")[0]
    };
    unsafe {
        device.destroy_shader_module(vert, None);
        device.destroy_shader_module(frag, None);
    }
    pipeline
}

#[cfg(target_os = "linux")]
fn load_shader_module_vulkan(
    device: &ash::Device,
    bytes: &[u8],
) -> ash::vk::ShaderModule {
    use ash::vk;
    let code = ash::util::read_spv(&mut std::io::Cursor::new(bytes))
        .expect("read_spv (embedded ui_text shader is valid)");
    let info = vk::ShaderModuleCreateInfo::default().code(&code);
    unsafe {
        device
            .create_shader_module(&info, None)
            .expect("create_shader_module(ui_text)")
    }
}

#[cfg(target_os = "linux")]
impl Drop for TextVulkanState {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_pipeline(self.pipeline, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
            self.device
                .destroy_descriptor_set_layout(self.atlas_descriptor_set_layout, None);
            self.device
                .destroy_descriptor_set_layout(self.uniform_descriptor_set_layout, None);
            self.device.destroy_sampler(self.sampler, None);
            // Buffers + atlas images drop themselves.
        }
    }
}

//  CPU blit helpers for `Text::render_cpu`. Same blend model as
//  `grid::cpu`: premultiplied source-over against an opaque
//  `0x00RRGGBB` destination.

#[inline]
fn pack_opaque(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

#[inline]
fn blend_premul_over(src: [u8; 4], dst: u32) -> u32 {
    let sa = src[3] as u32;
    if sa == 0 {
        return dst;
    }
    if sa == 255 {
        return pack_opaque(src[0], src[1], src[2]);
    }
    let inv = 255 - sa;
    let dr = (dst >> 16) & 0xff;
    let dg = (dst >> 8) & 0xff;
    let db = dst & 0xff;
    let or = src[0] as u32 + (dr * inv + 127) / 255;
    let og = src[1] as u32 + (dg * inv + 127) / 255;
    let ob = src[2] as u32 + (db * inv + 127) / 255;
    pack_opaque(or.min(255) as u8, og.min(255) as u8, ob.min(255) as u8)
}

#[allow(clippy::too_many_arguments)]
fn blit_text_mask(
    buf: &mut [u32],
    buf_w: i32,
    buf_h: i32,
    glyph_x: i32,
    glyph_y: i32,
    gw: i32,
    gh: i32,
    atlas: &[u8],
    atlas_side: usize,
    ax: usize,
    ay: usize,
    color: [u8; 4],
) {
    if color[3] == 0 {
        return;
    }
    let stride = buf_w as usize;
    let x_start = glyph_x.max(0);
    let y_start = glyph_y.max(0);
    let x_end = (glyph_x + gw).min(buf_w);
    let y_end = (glyph_y + gh).min(buf_h);
    if x_end <= x_start || y_end <= y_start {
        return;
    }
    let r = color[0] as u32;
    let g = color[1] as u32;
    let b = color[2] as u32;
    let ca = color[3] as u32;

    for dst_y in y_start..y_end {
        let src_y = (dst_y - glyph_y) as usize + ay;
        if src_y >= atlas_side {
            continue;
        }
        let atlas_row = src_y * atlas_side;
        let buf_row = (dst_y as usize) * stride;
        for dst_x in x_start..x_end {
            let src_x = (dst_x - glyph_x) as usize + ax;
            if src_x >= atlas_side {
                continue;
            }
            let m = atlas[atlas_row + src_x] as u32;
            if m == 0 {
                continue;
            }
            let a = (m * ca + 127) / 255;
            if a == 0 {
                continue;
            }
            let pr = (r * a + 127) / 255;
            let pg = (g * a + 127) / 255;
            let pb = (b * a + 127) / 255;
            let src = [pr as u8, pg as u8, pb as u8, a as u8];
            let idx = buf_row + (dst_x as usize);
            buf[idx] = blend_premul_over(src, buf[idx]);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_text_color(
    buf: &mut [u32],
    buf_w: i32,
    buf_h: i32,
    glyph_x: i32,
    glyph_y: i32,
    gw: i32,
    gh: i32,
    atlas: &[u8],
    atlas_side: usize,
    ax: usize,
    ay: usize,
) {
    let stride = buf_w as usize;
    let x_start = glyph_x.max(0);
    let y_start = glyph_y.max(0);
    let x_end = (glyph_x + gw).min(buf_w);
    let y_end = (glyph_y + gh).min(buf_h);
    if x_end <= x_start || y_end <= y_start {
        return;
    }
    for dst_y in y_start..y_end {
        let src_y = (dst_y - glyph_y) as usize + ay;
        if src_y >= atlas_side {
            continue;
        }
        let atlas_row = src_y * atlas_side * 4;
        let buf_row = (dst_y as usize) * stride;
        for dst_x in x_start..x_end {
            let src_x = (dst_x - glyph_x) as usize + ax;
            if src_x >= atlas_side {
                continue;
            }
            let off = atlas_row + src_x * 4;
            let r = atlas[off];
            let g = atlas[off + 1];
            let b = atlas[off + 2];
            let a = atlas[off + 3];
            if a == 0 {
                continue;
            }
            let src = [r, g, b, a];
            let idx = buf_row + (dst_x as usize);
            buf[idx] = blend_premul_over(src, buf[idx]);
        }
    }
}
