// font_introspector was retired from https://github.com/dfrg/swash
// which is licensed under MIT license

pub mod cluster;

#[doc(hidden)]
pub mod partition;

mod aat;
mod at;
mod buffer;
mod cache;
mod engine;
mod feature;

use cluster::*;

use super::{
    cache::FontCache, charmap::Charmap, internal, metrics::Metrics, setting::Setting,
    FontRef, NormalizedCoord,
};
use crate::font_introspector::text::{
    cluster::{CharCluster, Parser, ShapeClass, Token},
    Language, Script,
};
use at::{FeatureMask, FeatureStore, FeatureStoreBuilder};
use buffer::*;
use cache::{FeatureCache, FontEntry};
use core::borrow::Borrow;
use engine::{Engine, EngineMode};

const DEFAULT_SIZE: usize = 16;

/// Text direction.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Direction {
    LeftToRight,
    RightToLeft,
}

/// Context that manages caches and transient buffers for shaping.
///
/// See the module level [documentation](index.html#building-the-shaper) for detail.
pub struct ShapeContext {
    font_cache: FontCache<FontEntry>,
    feature_cache: FeatureCache,
    coords: Vec<i16>,
    state: State,
}

impl ShapeContext {
    /// Creates a new shaping context.
    pub fn new() -> Self {
        Self::with_max_entries(DEFAULT_SIZE)
    }

    /// Creates a new shaping context with the specified maximum number of
    /// cache entries.
    pub fn with_max_entries(max_entries: usize) -> Self {
        let max_entries = max_entries.clamp(1, 64);
        Self {
            font_cache: FontCache::new(max_entries),
            feature_cache: FeatureCache::new(max_entries),
            coords: Vec::new(),
            state: State::new(),
        }
    }

    /// Creates a new builder for constructing a shaper with this context
    /// and the specified font.
    pub fn builder<'a>(&'a mut self, font: impl Into<FontRef<'a>>) -> ShaperBuilder<'a> {
        ShaperBuilder::new(self, font)
    }

    /// Creates a new builder for constructing a shaper with this context
    /// and the specified font.
    #[allow(unused)]
    pub fn builder_with_id<'a>(
        &'a mut self,
        font: impl Into<FontRef<'a>>,
        id: [u64; 2],
    ) -> ShaperBuilder<'a> {
        ShaperBuilder::new_with_id(self, font, id)
    }
}

impl Default for ShapeContext {
    fn default() -> Self {
        Self::new()
    }
}

struct State {
    buffer: Buffer,
    store_builder: FeatureStoreBuilder,
    order: Vec<usize>,
    glyphs: Vec<GlyphData>,
    disable_kern: bool,
    features: Vec<(u32, u16)>,
    selectors: Vec<(u16, u16)>,
}

impl State {
    pub fn new() -> Self {
        Self {
            buffer: Buffer::new(),
            store_builder: FeatureStoreBuilder::default(),
            order: Vec::new(),
            glyphs: Vec::new(),
            disable_kern: false,
            features: Vec::new(),
            selectors: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.features.clear();
        self.disable_kern = false;
    }
}

/// Builder for configuring a shaper.
///
/// See the module level [documentation](index.html#building-the-shaper) for more detail.
pub struct ShaperBuilder<'a> {
    state: &'a mut State,
    feature_cache: &'a mut FeatureCache,
    font: FontRef<'a>,
    font_id: [u64; 2],
    font_entry: &'a FontEntry,
    coords: &'a mut Vec<i16>,
    charmap: Charmap<'a>,
    dotted_circle: Option<u16>,
    retain_ignorables: bool,
    size: f32,
    script: Script,
    lang: Option<Language>,
    dir: Direction,
}

impl<'a> ShaperBuilder<'a> {
    /// Creates a new builder for configuring a shaper with the specified
    /// context and font.
    pub fn new(context: &'a mut ShapeContext, font: impl Into<FontRef<'a>>) -> Self {
        let font = font.into();
        let id = [font.key.value(), u64::MAX];
        Self::new_with_id(context, font, id)
    }

    /// Creates a new builder for configuring a shaper with the specified
    /// context and font.
    fn new_with_id(
        context: &'a mut ShapeContext,
        font: impl Into<FontRef<'a>>,
        id: [u64; 2],
    ) -> Self {
        let font = font.into();
        let (font_id, font_entry) =
            context.font_cache.get(&font, Some(id), FontEntry::new);
        context.state.reset();
        context.coords.clear();
        Self {
            state: &mut context.state,
            feature_cache: &mut context.feature_cache,
            font,
            font_id,
            font_entry,
            coords: &mut context.coords,
            charmap: font_entry.charmap.materialize(&font),
            dotted_circle: None,
            retain_ignorables: false,
            size: 0.,
            script: Script::Latin,
            lang: None,
            dir: Direction::LeftToRight,
        }
    }

    /// Specifies the script. The default value is [`Script::Latin`].
    pub fn script(mut self, script: Script) -> Self {
        self.script = script;
        self
    }

    /// Specifies the language. The default value is `None`.
    pub fn language(mut self, language: Option<Language>) -> Self {
        self.lang = language;
        self
    }

    /// Specifies the text direction. The default value is [`Direction::LeftToRight`].
    pub fn direction(mut self, direction: Direction) -> Self {
        self.dir = direction;
        self
    }

    /// Specifies the font size in pixels per em. The default value is `0`
    /// which will produce glyphs with offsets and advances in font units.
    pub fn size(mut self, ppem: f32) -> Self {
        self.size = ppem.max(0.);
        self
    }

    /// Adds feature settings to the shaper.
    pub fn features<I>(self, settings: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Setting<u16>>,
    {
        for feature in settings {
            let feature = feature.into();
            if feature.tag == feature::KERN {
                self.state.disable_kern = feature.value == 0;
            }
            self.state.features.push((feature.tag, feature.value));
        }
        self
    }

    /// Adds variation settings to the shaper.
    pub fn variations<I>(self, settings: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Setting<f32>>,
    {
        if self.font_entry.coord_count != 0 {
            let vars = self.font.variations();
            self.coords.resize(vars.len(), 0);
            for setting in settings {
                let setting = setting.into();
                for var in vars {
                    if var.tag() == setting.tag {
                        let value = var.normalize(setting.value);
                        if let Some(c) = self.coords.get_mut(var.index()) {
                            *c = value;
                        }
                    }
                }
            }
        }
        self
    }

    /// Specifies the variation settings in terms of normalized coordinates.
    #[allow(unused)]
    pub fn normalized_coords<I>(self, coords: I) -> Self
    where
        I: IntoIterator,
        I::Item: Borrow<NormalizedCoord>,
    {
        self.coords.clear();
        self.coords.extend(coords.into_iter().map(|c| *c.borrow()));
        self
    }

    /// Specifies whether to insert dotted circles for broken clusters. The
    /// default value is `false`.
    pub fn insert_dotted_circles(mut self, yes: bool) -> Self {
        if yes {
            let gid = self.charmap.map('\u{25cc}');
            if gid != 0 {
                self.dotted_circle = Some(gid);
            }
        } else {
            self.dotted_circle = None;
        }
        self
    }

    /// Specifies whether characters defined as default ignorable should be
    /// retained by the shaper. The default is `false`.
    #[allow(unused)]
    pub fn retain_ignorables(mut self, yes: bool) -> Self {
        self.retain_ignorables = yes;
        self
    }

    /// Builds a shaper for the current configuration.
    pub fn build(self) -> Shaper<'a> {
        let engine = Engine::new(
            &self.font_entry.metadata,
            self.font.data,
            &self.coords[..],
            self.script,
            self.lang,
        );
        self.state.buffer.dotted_circle = self.dotted_circle;
        let rtl = self.dir == Direction::RightToLeft;
        self.state.buffer.is_rtl = rtl;
        let (store, sub_mask, pos_mask) = if engine.use_ot {
            use cache::FeatureCacheEntry;
            let store = match self.feature_cache.entry(
                self.font_id,
                &self.coords[..],
                engine.has_feature_vars(),
                engine.tags(),
            ) {
                FeatureCacheEntry::Present(store) => store,
                FeatureCacheEntry::New(store) => {
                    engine.collect_features(&mut self.state.store_builder, store);
                    store
                }
            };
            let buf = &mut self.state.buffer;
            let (sub, pos) = store.custom_masks(
                &self.state.features[..],
                &mut buf.sub_args,
                &mut buf.pos_args,
                self.dir,
            );
            (Some(store as _), sub, pos)
        } else {
            (None, FeatureMask::default(), FeatureMask::default())
        };
        Shaper {
            state: self.state,
            font: self.font,
            font_entry: self.font_entry,
            charmap: self.charmap,
            retain_ignorables: self.retain_ignorables,
            size: self.size,
            script: self.script,
            joined: engine.use_ot && self.script.is_joined(),
            dir: self.dir,
            engine,
            store,
            sub_mask,
            pos_mask,
        }
    }
}

/// Maps character clusters to positioned glyph clusters according to
/// typographic rules and features.
///
/// See the module level [documentation](index.html#feeding-the-shaper) for detail.
pub struct Shaper<'a> {
    state: &'a mut State,
    font: FontRef<'a>,
    font_entry: &'a FontEntry,
    #[allow(unused)]
    charmap: Charmap<'a>,
    retain_ignorables: bool,
    size: f32,
    #[allow(unused)]
    script: Script,
    joined: bool,
    dir: Direction,
    engine: Engine<'a>,
    store: Option<&'a FeatureStore>,
    sub_mask: FeatureMask,
    pos_mask: FeatureMask,
}

impl<'a> Shaper<'a> {
    /// Adds a character cluster to the shaper.
    pub fn add_cluster(&mut self, cluster: &CharCluster) {
        let buf = &mut self.state.buffer;
        match self.engine.mode {
            EngineMode::Simple => {
                buf.push(cluster);
            }
            EngineMode::Myanmar => {
                let e = &mut self.engine;
                let s = self.store.unwrap();
                let chars = cluster.mapped_chars();
                reorder_myanmar(chars, &mut self.state.order);
                let range = buf.push_order(cluster, &self.state.order);
                e.set_classes(buf, Some(range.clone()));
                let start = range.start;
                e.gsub(s, s.groups.default, buf, Some(range));
                let end = buf.len();
                e.gsub(s, s.groups.reph, buf, Some(start..end));
                let end = buf.len();
                e.gsub(s, s.groups.pref, buf, Some(start..end));
                let end = buf.len();
                e.gsub(s, s.groups.stage1, buf, Some(start..end));
                let end = buf.len();
                e.gsub(s, s.groups.stage2, buf, Some(start..end));
            }
            EngineMode::Complex => {
                let e = &mut self.engine;
                let s = self.store.unwrap();
                let range = buf.push(cluster);
                e.set_classes(buf, Some(range.clone()));
                let start = range.start;
                // Default group
                e.gsub(s, s.groups.default, buf, Some(range.clone()));
                for g in &mut buf.glyphs[range] {
                    if g.char_class == ShapeClass::Halant && g.flags & SUBSTITUTED != 0 {
                        // Don't prevent reordering across a virama that has been substituted
                        g.char_class = ShapeClass::Other;
                    }
                }
                // Reph identification
                let len = 3.min(buf.glyphs.len() - start);
                let end = start + len;
                buf.clear_flags(buffer::SUBSTITUTED, Some(start..end));
                e.gsub(s, s.groups.reph, buf, Some(start..end));
                for g in &mut buf.glyphs[start..end] {
                    if g.flags & buffer::SUBSTITUTED != 0 {
                        g.char_class = ShapeClass::Reph;
                        break;
                    }
                }
                // Pref identification
                let end = buf.len();
                buf.clear_flags(buffer::SUBSTITUTED, Some(start..end));
                e.gsub(s, s.groups.pref, buf, Some(start..end));
                for g in &mut buf.glyphs[start..end] {
                    if g.flags & buffer::SUBSTITUTED != 0 {
                        g.char_class = ShapeClass::Pref;
                        break;
                    }
                }
                // Orthographic group
                let end = buf.len();
                e.gsub(s, s.groups.stage1, buf, Some(start..end));
                // Reordering
                let len = (buf.len() - start).min(64);
                let end = start + len;
                reorder_complex(
                    &mut buf.glyphs[start..end],
                    &mut self.state.glyphs,
                    &mut self.state.order,
                );
            }
        }
    }

    /// Adds a string to the shaper.
    pub fn add_str(&mut self, s: &str) {
        use crate::font_introspector::text::Codepoint;
        let mut cluster = CharCluster::new();
        let mut parser = Parser::new(
            self.script,
            s.char_indices().map(|(i, ch)| Token {
                ch,
                offset: i as u32,
                len: ch.len_utf8() as u8,
                info: ch.properties().into(),
                data: 0,
            }),
        );
        let charmap = self.charmap;
        while parser.next(&mut cluster) {
            cluster.map(|ch| charmap.map(ch));
            self.add_cluster(&cluster);
        }
    }

    /// Returns the current normalized variation coordinates in use by the
    /// shaper.
    pub fn normalized_coords(&self) -> &[NormalizedCoord] {
        self.engine.coords
    }

    /// Returns the current font metrics in use by the shaper.
    pub fn metrics(&self) -> Metrics {
        let scale = if self.size != 0. { self.size } else { 1. };
        self.font_entry
            .metrics
            .materialize_metrics(&self.font, self.engine.coords)
            .scale(scale)
    }

    /// Shapes the text and invokes the specified closure with each
    /// resulting glyph cluster.
    pub fn shape_with(mut self, mut f: impl FnMut(&GlyphCluster)) {
        self.finish();
        let buf = &mut self.state.buffer;
        buf.shaped_glyphs.clear();
        let mut sentinel = (
            buffer::GlyphData::default(),
            buffer::PositionData::default(),
        );
        sentinel.0.cluster = buf.ranges.len() as u32;
        let mut last_cluster = 0;
        for (g, p) in buf
            .glyphs
            .iter()
            .zip(&buf.positions)
            .chain(core::iter::once((&sentinel.0, &sentinel.1)))
        {
            if g.cluster != last_cluster {
                // Simple and common case: no ligatures and no empty clusters.
                if last_cluster > g.cluster || g.cluster - last_cluster == 1 {
                    let index = last_cluster as usize;
                    let info = &buf.infos[index];
                    let cluster = GlyphCluster {
                        source: buf.ranges[index],
                        info: info.0,
                        glyphs: &buf.shaped_glyphs,
                        components: &[],
                        data: info.2,
                    };
                    f(&cluster);
                    buf.shaped_glyphs.clear();
                } else {
                    // Collect the range for the non-empty cluster.
                    let end = g.cluster as usize;
                    let start = last_cluster as usize;
                    let mut group_end = start + 1;
                    while group_end < end && buf.infos[group_end].1 {
                        group_end += 1;
                    }
                    if !buf.shaped_glyphs.is_empty() {
                        // We have some glyphs. Emit the cluster.
                        let mut source = buf.ranges[start];
                        source.end = buf.ranges[group_end - 1].end;
                        // If the range spans more than one cluster, we have a ligature.
                        let components = if group_end > start + 1 {
                            &buf.ranges[start..group_end]
                        } else {
                            &[]
                        };
                        let info = &buf.infos[start];
                        let cluster = GlyphCluster {
                            source,
                            info: info.0,
                            glyphs: &buf.shaped_glyphs,
                            components,
                            data: info.2,
                        };
                        f(&cluster);
                        buf.shaped_glyphs.clear();
                    }
                    if end > group_end {
                        // We have a trailing sequence of empty clusters. Emit
                        // them one by one.
                        for (info, source) in buf.infos[group_end..end]
                            .iter()
                            .zip(&buf.ranges[group_end..end])
                        {
                            let cluster = GlyphCluster {
                                source: *source,
                                info: info.0,
                                glyphs: &[],
                                components: &[],
                                data: info.2,
                            };
                            f(&cluster);
                        }
                    }
                }
            }
            last_cluster = g.cluster;
            if self.retain_ignorables || g.flags & IGNORABLE == 0 {
                buf.shaped_glyphs.push(Glyph::new(g, p));
            }
        }
    }

    // FIXME: when writing docs, I realized that it's impossible
    // to use the result of this function correctly with RTL runs
    // that contain marks.

    // /// Shapes the text and invokes the specified closure with each
    // /// resulting glyph.
    // pub fn shape_glyphs_with(mut self, mut f: impl FnMut(&Glyph)) {
    //     self.finish();
    //     let buf = &self.state.buffer;
    //     for (g, p) in buf.glyphs.iter().zip(&buf.positions) {
    //         if g.flags & IGNORABLE == 0 {
    //             f(&Glyph::new(g, p))
    //         }
    //     }
    // }

    fn finish(&mut self) {
        use engine::{PosMode, SubMode};
        if self.state.buffer.glyphs.is_empty() {
            return;
        }
        let e = &mut self.engine;
        let buf = &mut self.state.buffer;
        match e.mode {
            EngineMode::Simple => match e.sub_mode {
                SubMode::Gsub => {
                    let s = self.store.unwrap();
                    e.set_classes(buf, None);
                    if self.joined {
                        buf.set_join_masks();
                    }
                    e.gsub(s, self.sub_mask, buf, None);
                }
                SubMode::Morx => {
                    e.collect_selectors(&self.state.features, &mut self.state.selectors);
                    e.morx(buf, &self.state.selectors);
                }
                _ => {}
            },
            EngineMode::Myanmar => {
                let s = self.store.unwrap();
                e.gsub(s, self.sub_mask | s.groups.stage2, buf, None);
            }
            EngineMode::Complex => {
                let s = self.store.unwrap();
                if self.joined {
                    buf.set_join_masks();
                    e.gsub(s, s.groups.stage2 | self.sub_mask, buf, None);
                } else {
                    e.gsub(s, self.sub_mask, buf, None);
                }
            }
        }
        buf.setup_positions(e.sub_mode == SubMode::Morx);
        match e.pos_mode {
            PosMode::Gpos => {
                let s = self.store.unwrap();
                e.gpos(s, self.pos_mask, buf, None);
            }
            PosMode::Kerx => {
                e.kerx(buf, self.state.disable_kern);
            }
            PosMode::Kern => {
                if !self.state.disable_kern {
                    e.kern(buf);
                }
            }
            _ => {}
        }
        // let metrics = self
        //     .font_entry
        //     .metrics
        //     .materialize_metrics(self.font.data, self.engine.coords);
        let glyph_metrics = self
            .font_entry
            .metrics
            .materialize_glyph_metrics(&self.font, self.engine.coords);
        for (g, p) in buf.glyphs.iter_mut().zip(buf.positions.iter_mut()) {
            if g.flags & MARK_ATTACH == 0 {
                p.advance += glyph_metrics.advance_width(g.id);
            }
            g.flags |= p.flags;
        }
        if buf.has_cursive {
            if self.dir == Direction::RightToLeft {
                for (i, g) in buf.glyphs.iter().enumerate().rev() {
                    if g.flags & buffer::CURSIVE_ATTACH != 0 {
                        let base_offset = buf.positions[i].base as usize;
                        if base_offset != 0 {
                            let (x, y) = {
                                let base = &buf.positions[i + base_offset];
                                (base.x, base.y)
                            };
                            let pos = &mut buf.positions[i];
                            pos.x += x;
                            pos.y += y;
                        }
                    }
                }
            } else {
                for (i, g) in buf.glyphs.iter().enumerate() {
                    if g.flags & buffer::CURSIVE_ATTACH != 0 {
                        let base_offset = buf.positions[i].base as usize;
                        if base_offset != 0 {
                            let (x, y) = {
                                let base = &buf.positions[i + base_offset];
                                (base.x, base.y)
                            };
                            let pos = &mut buf.positions[i];
                            pos.x += x;
                            pos.y += y;
                        }
                    }
                }
            }
        }
        if buf.has_marks {
            fn round_f32(f: f32) -> f32 {
                f
            }
            for (i, g) in buf.glyphs.iter().enumerate() {
                if g.flags & buffer::MARK_ATTACH != 0 {
                    let base_offset = buf.positions[i].base as usize;
                    if base_offset != 0 {
                        let (x, y) = {
                            let base = &buf.positions[i - base_offset];
                            (base.x - round_f32(base.advance), base.y)
                        };
                        let pos = &mut buf.positions[i];
                        pos.x += x;
                        pos.y += y;
                    }
                }
            }
        }
        let upem = glyph_metrics.units_per_em();
        if self.size != 0. && upem != 0 {
            let s = self.size / upem as f32;
            for p in buf.positions.iter_mut() {
                p.x *= s;
                p.y *= s;
                p.advance *= s;
            }
        }
    }
}
