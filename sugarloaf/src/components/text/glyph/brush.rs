// glyph module code along with comments was originally retired from glyph-brush
// https://github.com/alexheretic/glyph-brush
// glyph-brush was originally written Alex Butler (https://github.com/alexheretic)
// and licensed under Apache-2.0 license.

mod builder;

pub use self::builder::*;
use super::cache::{CachedBy, DrawCache};
use super::calculator::{GlyphCruncher, GlyphedSection};
use super::{
    DefaultSectionHasher, FontId, GlyphChange, GlyphPositioner, Rectangle, Section,
    SectionGeometry, SectionGlyph, SectionGlyphIter,
};
use ab_glyph::{point, Font, FontArc, Glyph, Rect};

use super::extra::Extra;
use rustc_hash::{FxHashMap, FxHashSet};
use std::{
    borrow::Cow,
    fmt,
    hash::{BuildHasher, Hash, Hasher},
    mem,
};

/// A hash of `Section` data
type SectionHash = u64;

/// Object allowing glyph drawing, containing cache state. Manages glyph positioning caching,
/// glyph draw caching & efficient GPU texture cache updating.
///
/// Build using a [`GlyphBrushBuilder`].
///
/// Also see [`GlyphCruncher`] trait which providers extra functionality,
/// such as [`GlyphCruncher::glyph_bounds`].
///
/// # Generic types
/// * **`V`** A single glyph's vertex data type that matches, or is inferred by, the `to_vertex`
///   function given to the [`GlyphBrush::process_queued`] call.
/// * **`X`** Extra non-layout data for use in vertex generation. _Default [`Extra`]_.
/// * **`F`** _ab-glyph_ font type. Generally inferred by builder usage. _Default [`FontArc`]_.
/// * **`H`** Section hasher used for cache matching. See [`GlyphBrushBuilder::section_hasher`].
///   _Default [`DefaultSectionHasher`]_.
///
/// # Caching behaviour
/// Calls to [`GlyphBrush::queue`], [`GlyphBrush::glyph_bounds`], [`GlyphBrush::glyphs`]
/// calculate the positioned glyphs for a section.
/// This is cached so future calls to any of the methods for the same section are much
/// cheaper. In the case of [`GlyphBrush::queue`] the calculations will also be
/// used for actual drawing.
///
/// The cache for a section will be **cleared** after a
/// [`GlyphBrush::process_queued`] call when that section has not been used
/// since the previous call.
///
/// # Texture caching behaviour
/// Note the gpu/draw cache may contain multiple versions of the same glyph at different
/// subpixel positions.
/// This is required for high quality text as a glyph's positioning is always exactly aligned
/// to it's draw positioning.
///
/// This behaviour can be adjusted with [`GlyphBrushBuilder::draw_cache_position_tolerance`].
pub struct GlyphBrush<V, X = Extra, F = FontArc, H = DefaultSectionHasher> {
    fonts: Vec<F>,
    texture_cache: DrawCache,
    last_draw: LastDrawInfo,

    // cache of section-layout hash -> computed glyphs, this avoid repeated glyph computation
    // for identical layout/sections common to repeated frame rendering
    calculate_glyph_cache: FxHashMap<SectionHash, Glyphed<V, X>>,

    last_frame_seq_id_sections: Vec<SectionHashDetail>,
    frame_seq_id_sections: Vec<SectionHashDetail>,

    // buffer of section-layout hashs (that must exist in the calculate_glyph_cache)
    // to be used on the next `process_queued` call
    section_buffer: Vec<SectionHash>,

    // Set of section hashs to keep in the glyph cache this frame even if they haven't been drawn
    keep_in_cache: FxHashSet<SectionHash>,

    // config
    cache_glyph_positioning: bool,
    cache_redraws: bool,

    section_hasher: H,

    last_pre_positioned: Vec<Glyphed<V, X>>,
    pre_positioned: Vec<Glyphed<V, X>>,
}

impl<F, V, X, H> fmt::Debug for GlyphBrush<V, X, F, H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GlyphBrush")
    }
}

impl<F, V, X, H> GlyphCruncher<F, X> for GlyphBrush<V, X, F, H>
where
    X: Clone + Hash,
    F: Font,
    V: Clone + 'static,
    H: BuildHasher,
{
    fn glyphs_custom_layout<'a, 'b, S, L>(
        &'b mut self,
        section: S,
        custom_layout: &L,
    ) -> SectionGlyphIter<'b>
    where
        X: 'a,
        L: GlyphPositioner + Hash,
        S: Into<Cow<'a, Section<'a, X>>>,
    {
        let section_hash = self.cache_glyphs(&section.into(), custom_layout);
        self.keep_in_cache.insert(section_hash);
        self.calculate_glyph_cache[&section_hash]
            .positioned
            .glyphs()
    }

    fn glyph_bounds_custom_layout<'a, S, L>(
        &mut self,
        section: S,
        custom_layout: &L,
    ) -> Option<Rect>
    where
        X: 'a,
        L: GlyphPositioner + Hash,
        S: Into<Cow<'a, Section<'a, X>>>,
    {
        let section = section.into();
        let geometry = SectionGeometry::from(section.as_ref());

        let section_hash = self.cache_glyphs(&section, custom_layout);
        self.keep_in_cache.insert(section_hash);
        self.calculate_glyph_cache[&section_hash]
            .positioned
            .glyphs()
            .fold(None, |b: Option<Rect>, sg| {
                let bounds = self.fonts[sg.font_id.0].glyph_bounds(&sg.glyph);
                b.map(|b| {
                    let min_x = b.min.x.min(bounds.min.x);
                    let max_x = b.max.x.max(bounds.max.x);
                    let min_y = b.min.y.min(bounds.min.y);
                    let max_y = b.max.y.max(bounds.max.y);
                    Rect {
                        min: point(min_x, min_y),
                        max: point(max_x, max_y),
                    }
                })
                .or(Some(bounds))
            })
            .map(|mut b| {
                // cap the glyph bounds to the layout specified max bounds
                let Rect { min, max } = custom_layout.bounds_rect(&geometry);
                b.min.x = b.min.x.max(min.x);
                b.min.y = b.min.y.max(min.y);
                b.max.x = b.max.x.min(max.x);
                b.max.y = b.max.y.min(max.y);
                b
            })
    }

    #[inline]
    fn fonts(&self) -> &[F] {
        &self.fonts
    }
}

impl<F, V, X, H: BuildHasher> GlyphBrush<V, X, F, H> {
    /// Adds an additional font to the one(s) initially added on build.
    ///
    /// Returns a new [`FontId`](struct.FontId.html) to reference this font.
    pub fn add_font<I: Into<F>>(&mut self, font_data: I) -> FontId {
        self.fonts.push(font_data.into());
        FontId(self.fonts.len() - 1)
    }
}

impl<F, V, X, H> GlyphBrush<V, X, F, H>
where
    F: Font,
    X: Clone + Hash,
    V: Clone + 'static,
    H: BuildHasher,
{
    /// Queues a section/layout to be processed by the next call of
    /// [`process_queued`](struct.GlyphBrush.html#method.process_queued). Can be called multiple
    /// times to queue multiple sections for drawing.
    ///
    /// Used to provide custom `GlyphPositioner` logic, if using built-in
    /// [`Layout`](enum.Layout.html) simply use [`queue`](struct.GlyphBrush.html#method.queue)
    ///
    /// Benefits from caching, see [caching behaviour](#caching-behaviour).
    #[inline]
    pub fn queue_custom_layout<'a, S, G>(&mut self, section: S, custom_layout: &G)
    where
        G: GlyphPositioner,
        X: 'a,
        S: Into<Cow<'a, Section<'a, X>>>,
    {
        let section = section.into();
        if cfg!(debug_assertions) {
            for text in &section.text {
                assert!(self.fonts.len() > text.font_id.0, "Invalid font id");
            }
        }
        let section_hash = self.cache_glyphs(&section, custom_layout);
        self.section_buffer.push(section_hash);
        self.keep_in_cache.insert(section_hash);
    }

    /// Queues a section/layout to be processed by the next call of
    /// [`process_queued`](struct.GlyphBrush.html#method.process_queued). Can be called multiple
    /// times to queue multiple sections for drawing.
    ///
    /// Benefits from caching, see [caching behaviour](#caching-behaviour).
    ///
    /// # use glyph_brush::{ab_glyph::*, *};
    /// # let font: FontArc = unimplemented!();
    /// # let mut glyph_brush: GlyphBrush<()> = GlyphBrushBuilder::using_font(font).build();
    /// glyph_brush.queue(Section::default().add_text(Text::new("Hello glyph_brush")));
    #[inline]
    pub fn queue<'a, S>(&mut self, section: S)
    where
        X: 'a,
        S: Into<Cow<'a, Section<'a, X>>>,
    {
        let section = section.into();
        let layout = section.layout;
        self.queue_custom_layout(section, &layout)
    }

    /// Queues pre-positioned glyphs to be processed by the next call of
    /// [`process_queued`](struct.GlyphBrush.html#method.process_queued). Can be called multiple
    /// times.
    pub fn queue_pre_positioned(
        &mut self,
        glyphs: Vec<SectionGlyph>,
        extra: Vec<X>,
        bounds: Rect,
    ) {
        self.pre_positioned.push(Glyphed::new(GlyphedSection {
            bounds,
            glyphs,
            extra,
        }));
    }

    /// Returns the calculate_glyph_cache key for this sections glyphs
    #[allow(clippy::map_entry)] // further borrows are required after the contains_key check
    #[inline]
    fn cache_glyphs<L>(&mut self, section: &Section<'_, X>, layout: &L) -> SectionHash
    where
        L: GlyphPositioner,
    {
        let section_hash = SectionHashDetail::new(&self.section_hasher, section, layout);
        // section id used to find a similar calculated layout from last frame
        let frame_seq_id = self.frame_seq_id_sections.len();
        self.frame_seq_id_sections.push(section_hash);

        if self.cache_glyph_positioning {
            if !self.calculate_glyph_cache.contains_key(&section_hash.full) {
                let geometry = SectionGeometry::from(section);

                let recalculated_glyphs = self
                    .last_frame_seq_id_sections
                    .get(frame_seq_id)
                    .cloned()
                    .and_then(|hash| {
                        let change = hash.layout_diff(section_hash);
                        if let Some(GlyphChange::Unknown) = change {
                            return None;
                        }

                        if self.keep_in_cache.contains(&hash.full) {
                            let cached = self.calculate_glyph_cache.get(&hash.full)?;
                            match change {
                                None => Some(cached.positioned.glyphs.clone()),
                                Some(change) => Some(layout.recalculate_glyphs(
                                    cached.positioned.glyphs.iter().cloned(),
                                    change,
                                    &self.fonts,
                                    &geometry,
                                    &section.text,
                                )),
                            }
                        } else {
                            let old = self.calculate_glyph_cache.remove(&hash.full)?;
                            match change {
                                None => Some(old.positioned.glyphs),
                                Some(change) => Some(layout.recalculate_glyphs(
                                    old.positioned.glyphs,
                                    change,
                                    &self.fonts,
                                    &geometry,
                                    &section.text,
                                )),
                            }
                        }
                    });

                self.calculate_glyph_cache.insert(
                    section_hash.full,
                    Glyphed::new(GlyphedSection {
                        bounds: layout.bounds_rect(&geometry),
                        glyphs: recalculated_glyphs.unwrap_or_else(|| {
                            layout.calculate_glyphs(&self.fonts, &geometry, &section.text)
                        }),
                        extra: section.clone_extras(),
                    }),
                );
            }
        } else {
            let geometry = SectionGeometry::from(section);
            let glyphs = layout.calculate_glyphs(&self.fonts, &geometry, &section.text);
            self.calculate_glyph_cache.insert(
                section_hash.full,
                Glyphed::new(GlyphedSection {
                    bounds: layout.bounds_rect(&geometry),
                    glyphs,
                    extra: section.text.iter().map(|s| s.extra.clone()).collect(),
                }),
            );
        }
        section_hash.full
    }

    pub fn resize_texture(&mut self, new_width: u32, new_height: u32) {
        self.texture_cache
            .to_builder()
            .dimensions(new_width, new_height)
            .rebuild(&mut self.texture_cache);

        self.last_draw = LastDrawInfo::default();

        // invalidate any previous cache position data
        for glyphed in self.calculate_glyph_cache.values_mut() {
            glyphed.invalidate_texture_positions();
        }
    }

    /// Returns the logical texture cache pixel dimensions `(width, height)`.
    pub fn texture_dimensions(&self) -> (u32, u32) {
        self.texture_cache.dimensions()
    }

    #[inline]
    fn cleanup_frame(&mut self) {
        if self.cache_glyph_positioning {
            // clear section_buffer & trim calculate_glyph_cache to active sections
            let active = mem::take(&mut self.keep_in_cache);
            self.calculate_glyph_cache
                .retain(|key, _| active.contains(key));

            self.keep_in_cache = active;
            self.keep_in_cache.clear();

            self.section_buffer.clear();
        } else {
            self.section_buffer.clear();
            self.calculate_glyph_cache.clear();
            self.keep_in_cache.clear();
        }

        mem::swap(
            &mut self.last_frame_seq_id_sections,
            &mut self.frame_seq_id_sections,
        );
        self.frame_seq_id_sections.clear();

        mem::swap(&mut self.last_pre_positioned, &mut self.pre_positioned);
        self.pre_positioned.clear();
    }

    /// Retains the section in the cache as if it had been used in the last draw-frame.
    ///
    /// Should not generally be necessary, see [caching behaviour](#caching-behaviour).
    pub fn keep_cached_custom_layout<'a, S, G>(&mut self, section: S, custom_layout: &G)
    where
        S: Into<Cow<'a, Section<'a, X>>>,
        G: GlyphPositioner,
        X: 'a,
    {
        if !self.cache_glyph_positioning {
            return;
        }
        let section = section.into();
        if cfg!(debug_assertions) {
            for text in &section.text {
                assert!(self.fonts.len() > text.font_id.0, "Invalid font id");
            }
        }

        let section_hash =
            SectionHashDetail::new(&self.section_hasher, &section, custom_layout);
        self.keep_in_cache.insert(section_hash.full);
    }

    /// Retains the section in the cache as if it had been used in the last draw-frame.
    ///
    /// Should not generally be necessary, see [caching behaviour](#caching-behaviour).
    pub fn keep_cached<'a, S>(&mut self, section: S)
    where
        S: Into<Cow<'a, Section<'a, X>>>,
        X: 'a,
    {
        let section = section.into();
        let layout = section.layout;
        self.keep_cached_custom_layout(section, &layout);
    }
}

// `Font + Sync` stuff
impl<F, V, X, H> GlyphBrush<V, X, F, H>
where
    F: Font + Sync,
    X: Clone + Hash + PartialEq,
    V: Clone + 'static,
    H: BuildHasher,
{
    /// Processes all queued sections, calling texture update logic when necessary &
    /// returning a `BrushAction`.
    /// See [`queue`](struct.GlyphBrush.html#method.queue).
    ///
    /// Two closures are required:
    /// * `update_texture` is called when new glyph texture data has been drawn for update in the
    ///   actual texture.
    ///   The arguments are the rect position of the data in the texture & the byte data itself
    ///   which is a single `u8` alpha value per pixel.
    /// * `to_vertex` maps a single glyph's `GlyphVertex` data into a generic vertex type. The
    ///   mapped vertices are returned in an `Ok(BrushAction::Draw(vertices))` result.
    ///   It's recommended to use a single vertex per glyph quad for best performance.
    ///
    /// Trims the cache, see [caching behaviour](#caching-behaviour).
    ///
    /// # use glyph_brush::{ab_glyph::*, *};
    /// # fn main() -> Result<(), BrushError> {
    /// # let dejavu = FontArc::try_from_slice(include_bytes!("../../fonts/DejaVuSans.ttf")).unwrap();
    /// # let mut glyph_brush = GlyphBrushBuilder::using_font(dejavu).build();
    /// # fn update_texture(_: Rectangle<u32>, _: &[u8]) {}
    /// # fn into_vertex(v: glyph_brush::GlyphVertex) { () }
    /// glyph_brush.process_queued(
    ///     |rect, tex_data| update_texture(rect, tex_data),
    ///     |vertex_data| into_vertex(vertex_data),
    /// )?
    /// # ;
    /// # Ok(())
    /// # }
    pub fn process_queued<Up, VF>(
        &mut self,
        update_texture: Up,
        to_vertex: VF,
    ) -> Result<BrushAction<V>, BrushError>
    where
        Up: FnMut(Rectangle<u32>, &[u8]),
        VF: Fn(GlyphVertex<X>) -> V + Copy,
    {
        let draw_info = LastDrawInfo {
            text_state: { self.section_hasher.hash_one(&self.section_buffer) },
        };

        let result = if !self.cache_redraws
            || self.last_draw != draw_info
            || self.last_pre_positioned != self.pre_positioned
        {
            let mut some_text = false;
            // Everything in the section_buffer should also be here. The extras should also
            // be retained in the texture cache avoiding cache thrashing if they are rendered
            // in a 2-draw per frame style.
            for section_hash in &self.keep_in_cache {
                for sg in self
                    .calculate_glyph_cache
                    .get(section_hash)
                    .iter()
                    .flat_map(|gs| &gs.positioned.glyphs)
                {
                    self.texture_cache
                        .queue_glyph(sg.font_id.0, sg.glyph.clone());
                    some_text = true;
                }
            }

            for sg in self
                .pre_positioned
                .iter()
                .flat_map(|p| &p.positioned.glyphs)
            {
                self.texture_cache
                    .queue_glyph(sg.font_id.0, sg.glyph.clone());
                some_text = true;
            }

            if some_text {
                match self.texture_cache.cache_queued(&self.fonts, update_texture) {
                    Ok(CachedBy::Adding) => {}
                    Ok(CachedBy::Reordering) => {
                        for glyphed in self.calculate_glyph_cache.values_mut() {
                            glyphed.invalidate_texture_positions();
                        }
                    }
                    Err(_) => {
                        let (width, height) = self.texture_cache.dimensions();
                        return Err(BrushError::TextureTooSmall {
                            suggested: (width * 2, height * 2),
                        });
                    }
                }
            }

            self.last_draw = draw_info;

            BrushAction::Draw({
                let mut verts = Vec::new();

                for hash in &self.section_buffer {
                    let glyphed = self.calculate_glyph_cache.get_mut(hash).unwrap();
                    glyphed.ensure_vertices(&self.texture_cache, to_vertex);
                    verts.extend(glyphed.vertices.iter().cloned());
                }

                for glyphed in &mut self.pre_positioned {
                    // pre-positioned glyph vertices can't be cached so
                    // generate & move straight into draw vec
                    glyphed.ensure_vertices(&self.texture_cache, to_vertex);
                    verts.append(&mut glyphed.vertices);
                }

                verts
            })
        } else {
            BrushAction::ReDraw
        };

        self.cleanup_frame();
        Ok(result)
    }

    /// Returns `true` if this glyph is currently present in the draw cache texture.
    ///
    /// So `false` means either this glyph is invisible, like `' '`, or hasn't been queued &
    /// processed yet.
    #[inline]
    pub fn is_draw_cached(&self, font_id: FontId, glyph: &Glyph) -> bool {
        self.texture_cache.rect_for(font_id.0, glyph).is_some()
    }
}

impl<F: Font + Clone, V, X, H: BuildHasher + Clone> GlyphBrush<V, X, F, H> {
    /// Return a [`GlyphBrushBuilder`](struct.GlyphBrushBuilder.html) prefilled with the
    /// properties of this `GlyphBrush`.
    ///
    /// # Example
    ///
    /// # use glyph_brush::{*, ab_glyph::*};
    /// # type Vertex = ();
    /// # let sans = FontArc::try_from_slice(include_bytes!("../../fonts/DejaVuSans.ttf")).unwrap();
    /// let glyph_brush: GlyphBrush<Vertex> = GlyphBrushBuilder::using_font(sans)
    ///     .initial_cache_size((128, 128))
    ///     .build();
    ///
    /// let new_brush: GlyphBrush<Vertex> = glyph_brush.to_builder().build();
    /// assert_eq!(new_brush.texture_dimensions(), (128, 128));
    pub fn to_builder(&self) -> GlyphBrushBuilder<F, H> {
        let mut builder = GlyphBrushBuilder::using_fonts(self.fonts.clone())
            .cache_glyph_positioning(self.cache_glyph_positioning)
            .cache_redraws(self.cache_redraws)
            .section_hasher(self.section_hasher.clone());
        builder.draw_cache_builder = self.texture_cache.to_builder();
        builder
    }
}

#[derive(Debug, Default, PartialEq)]
struct LastDrawInfo {
    text_state: u64,
}

/// Data used to generate vertex information for a single glyph
#[derive(Debug)]
pub struct GlyphVertex<'x, X = Extra> {
    pub tex_coords: Rect,
    pub pixel_coords: Rect,
    pub bounds: Rect,
    pub extra: &'x X,
}

/// Actions that should be taken after processing queue data
#[derive(Debug)]
pub enum BrushAction<V> {
    /// Draw new/changed vertex data.
    Draw(Vec<V>),
    /// Re-draw last frame's vertices unmodified.
    ReDraw,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrushError {
    /// Texture is too small to cache queued glyphs
    ///
    /// A larger suggested size is included.
    TextureTooSmall { suggested: (u32, u32) },
}
impl fmt::Display for BrushError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TextureTooSmall { .. } => write!(f, "TextureTooSmall"),
        }
    }
}
impl std::error::Error for BrushError {
    fn description(&self) -> &str {
        match self {
            BrushError::TextureTooSmall { .. } => {
                "Texture is too small to cache queued glyphs"
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct SectionHashDetail {
    /// hash of text (- extra - geo)
    text: SectionHash,
    /// hash of text + extra + geo
    full: SectionHash,
    /// copy of geometry for later comparison
    geometry: SectionGeometry,
}

impl SectionHashDetail {
    #[inline]
    fn new<X, H, L>(build_hasher: &H, section: &Section<'_, X>, layout: &L) -> Self
    where
        X: Clone + Hash,
        H: BuildHasher,
        L: GlyphPositioner,
    {
        let parts = section.to_hashable_parts();

        let mut s = build_hasher.build_hasher();
        layout.hash(&mut s);
        parts.hash_text_no_extra(&mut s);
        let text = s.finish();

        parts.hash_extra(&mut s);
        parts.hash_geometry(&mut s);
        let full = s.finish();

        Self {
            text,
            full,
            geometry: SectionGeometry::from(section),
        }
    }

    /// Hash layout diff, if any (None implies no change or extra-only change)
    fn layout_diff(self, other: SectionHashDetail) -> Option<GlyphChange> {
        if self.text == other.text {
            if self.geometry == other.geometry {
                None
            } else {
                Some(GlyphChange::Geometry(self.geometry))
            }
        } else {
            Some(GlyphChange::Unknown)
        }
    }
}

/// Container for positioned glyphs which can generate and cache vertices
struct Glyphed<V, X> {
    positioned: GlyphedSection<X>,
    vertices: Vec<V>,
}

impl<V, X: PartialEq> PartialEq for Glyphed<V, X> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.positioned == other.positioned
    }
}

impl<V, X> Glyphed<V, X> {
    #[inline]
    fn new(gs: GlyphedSection<X>) -> Self {
        Self {
            positioned: gs,
            vertices: Vec::new(),
        }
    }

    /// Mark previous texture positions as no longer valid (vertices require re-generation)
    fn invalidate_texture_positions(&mut self) {
        self.vertices.clear();
    }

    /// Calculate vertices if not already done
    fn ensure_vertices<F>(&mut self, texture_cache: &DrawCache, to_vertex: F)
    where
        F: Fn(GlyphVertex<X>) -> V,
    {
        if !self.vertices.is_empty() {
            return;
        }

        let GlyphedSection {
            bounds,
            ref extra,
            ref glyphs,
        } = self.positioned;

        self.vertices.reserve(glyphs.len());
        self.vertices.extend(glyphs.iter().filter_map(|sg| {
            match texture_cache.rect_for(sg.font_id.0, &sg.glyph) {
                None => None,
                Some((tex_coords, pixel_coords)) => {
                    if pixel_coords.min.x > bounds.max.x
                        || pixel_coords.min.y > bounds.max.y
                        || bounds.min.x > pixel_coords.max.x
                        || bounds.min.y > pixel_coords.max.y
                    {
                        // glyph is totally outside the bounds
                        None
                    } else {
                        Some(to_vertex(GlyphVertex {
                            tex_coords,
                            pixel_coords,
                            bounds,
                            extra: &extra[sg.section_index],
                        }))
                    }
                }
            }
        }));
    }
}

#[cfg(test)]
mod hash_diff_test {
    use super::*;
    use crate::components::text::glyph::layout::Layout;
    use crate::components::text::glyph::{PxScale, Text};

    fn section() -> Section<'static> {
        Section {
            text: vec![
                Text {
                    text: "Hello, ",
                    scale: PxScale::from(20.0),
                    font_id: FontId(0),
                    extra: Extra {
                        color: [1.0, 0.9, 0.8, 0.7],
                        z: 0.444,
                    },
                },
                Text {
                    text: "World",
                    scale: PxScale::from(22.0),
                    font_id: FontId(1),
                    extra: Extra {
                        color: [0.6, 0.5, 0.4, 0.3],
                        z: 0.444,
                    },
                },
            ],
            bounds: (55.5, 66.6),
            layout: Layout::default(),
            screen_position: (999.99, 888.88),
        }
    }

    #[test]
    fn change_screen_position() {
        let build_hasher = DefaultSectionHasher::default();
        let mut section = section();
        let hash_deets = SectionHashDetail::new(&build_hasher, &section, &section.layout);

        section.screen_position.1 += 0.1;

        let diff = hash_deets.layout_diff(SectionHashDetail::new(
            &build_hasher,
            &section,
            &section.layout,
        ));

        match diff {
            Some(GlyphChange::Geometry(geo)) => assert_eq!(geo, hash_deets.geometry),
            _ => assert!(matches!(diff, Some(GlyphChange::Geometry(..)))),
        }
    }

    #[test]
    fn change_extra() {
        let build_hasher = DefaultSectionHasher::default();
        let mut section = section();
        let hash_deets = SectionHashDetail::new(&build_hasher, &section, &section.layout);

        section.text[1].extra.color[2] -= 0.1;

        let diff = hash_deets.layout_diff(SectionHashDetail::new(
            &build_hasher,
            &section,
            &section.layout,
        ));

        assert!(diff.is_none());
    }

    #[test]
    fn change_text() {
        let build_hasher = DefaultSectionHasher::default();
        let mut section = section();
        let hash_deets = SectionHashDetail::new(&build_hasher, &section, &section.layout);

        section.text[1].text = "something else";

        let diff = hash_deets.layout_diff(SectionHashDetail::new(
            &build_hasher,
            &section,
            &section.layout,
        ));

        assert!(matches!(diff, Some(GlyphChange::Unknown)));
    }
}

#[cfg(test)]
mod test {
    use crate::components::text::glyph::*;

    #[test]
    fn is_draw_cached() {
        let font_a = FontRef::try_from_slice(include_bytes!(
            "../../../../resources/test-fonts/DejaVuSans.ttf"
        ))
        .unwrap();
        let font_b = FontRef::try_from_slice(include_bytes!(
            "../../../../resources/test-fonts/Exo2-Light.otf"
        ))
        .unwrap();
        let unqueued_glyph = font_a.glyph_id('c').with_scale(50.0);

        let mut brush = GlyphBrushBuilder::using_fonts(vec![font_a, font_b]).build();

        let section = Section::default()
            .add_text(Text::new("a "))
            .add_text(Text::new("b ").with_font_id(FontId(1)));

        brush.queue(&section);
        let glyphs: Vec<_> = brush.glyphs(section).map(|sg| sg.glyph.clone()).collect();

        assert_eq!(glyphs.len(), 4);

        // nothing was cached because `process_queued` has not been called yet.
        assert!(!brush.is_draw_cached(FontId(0), &glyphs[0]));
        assert!(!brush.is_draw_cached(FontId(0), &glyphs[1]));
        assert!(!brush.is_draw_cached(FontId(1), &glyphs[2]));
        assert!(!brush.is_draw_cached(FontId(1), &glyphs[3]));
        assert!(!brush.is_draw_cached(FontId(0), &unqueued_glyph));

        brush.process_queued(|_, _| {}, |_| ()).unwrap();

        // visible glyphs that were queued have now been cached.
        assert!(brush.is_draw_cached(FontId(0), &glyphs[0]));
        assert!(!brush.is_draw_cached(FontId(0), &glyphs[1]));
        assert!(brush.is_draw_cached(FontId(1), &glyphs[2]));
        assert!(!brush.is_draw_cached(FontId(1), &glyphs[3]));
        assert!(!brush.is_draw_cached(FontId(0), &unqueued_glyph));
    }
}
