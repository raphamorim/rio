// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// This file was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

use super::family::parse_families;
use super::index::*;
use super::library::FontLibrary;
use super::types::{FamilyId, FamilyKey, FontId, FontKey, SourceId};
use super::{shared_data::SharedData, Font};
use crate::components::rich_text::util::fxhash::FxHashMap;
use std::sync::Arc;
use swash::proxy::CharmapProxy;
use swash::text::{
    cluster::{CharCluster, Status},
    Cjk, Language, Script,
};
use swash::{Attributes, Synthesis};
pub type FontGroupKey = (u64, Attributes);
type Epoch = u64;

/// Identifier for a cached font group.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct FontGroupId(pub u64);

const MAX_INLINE: usize = 6;

pub struct FontContext {
    library: FontLibrary,
    fonts: FontCache,
    groups: GroupCache,
}

impl FontContext {
    pub fn new(library: FontLibrary) -> Self {
        let index = library.inner.index.read().unwrap().clone();
        let fonts = FontCache {
            index,
            sources: FxHashMap::default(),
            epoch: 0,
        };
        Self {
            library,
            fonts,
            groups: GroupCache::default(),
        }
    }

    /// Returns the underlying font library.
    pub fn library(&self) -> &FontLibrary {
        &self.library
    }

    /// Resets font group caches and state. This should be called at the end
    /// of every layout session.
    pub fn reset_group_state(&mut self) {
        self.groups.reset();
    }

    /// Registers a font group.
    pub fn register_group(
        &mut self,
        families: &str,
        key: u64,
        attrs: Attributes,
    ) -> FontGroupId {
        self.groups.get(&self.fonts, families, key, attrs)
    }

    /// Selects a font group for subsequent cluster mapping operations.
    pub fn select_group(&mut self, descriptor: FontGroupId) {
        self.groups.select(descriptor);
    }

    /// Selects fallback fonts for the specified writing system.
    pub fn select_fallbacks(&mut self, script: Script, language: Option<&Language>) {
        self.groups
            .select_fallbacks(script, language.map(|l| l.cjk()).unwrap_or(Cjk::None))
    }

    /// Maps the characters in a cluster to nominal glyph identifiers and
    /// returns the most suitable font based on the currently selected
    /// descriptor and fallback configuration.
    pub fn map_cluster(
        &mut self,
        cluster: &mut CharCluster,
        synthesis: &mut Synthesis,
    ) -> Option<Font> {
        let mut best = None;
        let list = &self.groups.state.fonts;
        for entry in self.groups.fonts.get_mut(list.start..list.end)?.iter_mut() {
            match entry.map_cluster(&mut self.fonts, cluster, synthesis, best.is_none()) {
                Some((font, status)) => {
                    if status == Status::Complete {
                        return Some(font);
                    }
                    best = Some(font);
                }
                None => continue,
            }
        }
        let attrs = list.attributes;
        // We don't have a complete mapping at this point, so time to check
        // fallback fonts.
        if cluster.info().is_emoji() {
            if let Some(entry) = self.groups.emoji(&self.fonts, attrs) {
                match entry.map_cluster(
                    &mut self.fonts,
                    cluster,
                    synthesis,
                    best.is_none(),
                ) {
                    Some((font, status)) => {
                        if status == Status::Complete {
                            return Some(font);
                        }
                        best = Some(font);
                    }
                    None => {}
                }
            }
        }
        if !self.groups.state.fallbacks_ready {
            self.groups.fill_fallbacks(&self.fonts);
        }
        for &family in &self.groups.state.fallbacks {
            let entry = match self.groups.state.fallback_map.get_mut(&(family, attrs)) {
                Some(entry) => entry,
                _ => match self.fonts.query(family, attrs) {
                    Some(font) => {
                        self.groups
                            .state
                            .fallback_map
                            .insert((family, attrs), font.selector(attrs).into());
                        self.groups
                            .state
                            .fallback_map
                            .get_mut(&(family, attrs))
                            .unwrap()
                    }
                    _ => continue,
                },
            };
            match entry.map_cluster(&mut self.fonts, cluster, synthesis, best.is_none()) {
                Some((font, status)) => {
                    if status == Status::Complete {
                        return Some(font);
                    }
                    best = Some(font);
                }
                None => continue,
            }
        }
        best
    }
}

struct FontCache {
    index: Arc<StaticIndex>,
    sources: FxHashMap<SourceId, Option<(SharedData, Epoch)>>,
    epoch: Epoch,
}

impl FontCache {
    /// Returns a font entry that matches the specified family and
    /// attributes.
    pub fn query<'a>(
        &'a self,
        family: impl Into<FamilyKey<'a>>,
        attributes: impl Into<Attributes>,
    ) -> Option<FontEntry<'a>> {
        self.index.query(family, attributes)
    }

    /// Returns a font entry for the specified identifier.
    pub fn font_by_id<'a>(&'a self, id: FontId) -> Option<FontEntry<'a>> {
        self.index.font_by_id(id)
    }

    /// Returns a font matching the specified key.
    pub fn get<'k>(&mut self, key: impl Into<FontKey<'k>>) -> Option<Font> {
        let (source_id, offset, attributes, key) = match key.into() {
            FontKey::Id(id) => {
                let font = self.font_by_id(id)?;
                (
                    font.source().id(),
                    font.offset(),
                    font.attributes(),
                    font.cache_key(),
                )
            }
            FontKey::Descriptor(family, attrs) => {
                let font = self.query(family, attrs)?;
                (
                    font.source().id(),
                    font.offset(),
                    font.attributes(),
                    font.cache_key(),
                )
            }
        };
        let epoch = self.epoch;
        match self.sources.get_mut(&source_id) {
            Some(data) => {
                return data.as_mut().map(|d| {
                    d.1 = epoch;
                    Font {
                        data: d.0.clone(),
                        offset,
                        attributes,
                        key,
                    }
                })
            }
            _ => {}
        }
        let source = self.index.base.sources.get(source_id.to_usize())?;
        match source.get() {
            Some(data) => {
                self.sources.insert(source_id, Some((data.clone(), epoch)));
                Some(Font {
                    data,
                    offset,
                    attributes,
                    key,
                })
            }
            _ => {
                self.sources.insert(source_id, None);
                None
            }
        }
    }
}

/// Internal cache of font groups.
///
/// The strategy here uses a two layer caching system that maps user font
/// groups to a list of resolved font identifiers and a group
/// identifier. The group identifier is then mapped to a transient
/// list of cached fonts. This structure provides reasonably fast lookup
/// while also allowing group invalidation and eviction without the
/// need for notifying user code or for a messy observer/listener style
/// system. Essentially, this is more complex than desired, but the complexity
/// is entirely encapsulated here.
#[derive(Default)]
struct GroupCache {
    /// Maps from a user font descriptor key to a list of font
    /// identifiers.
    key_map: FxHashMap<FontGroupKey, CachedGroup>,
    /// Temporary storage for parsing a user font descriptor.
    tmp: Vec<(FontId, Attributes)>,
    /// Next descriptor identifier.
    next_id: u64,
    /// Map from descriptor identifier to the list of cached fonts. This
    /// is semi-transient: usually per layout session.
    font_map: FxHashMap<FontGroupId, CachedFontList>,
    /// Fonts referenced by the ranges in `font_map`.
    fonts: Vec<CachedFont>,
    /// Currently selected descriptor/script/language state for mapping
    /// clusters.
    state: GroupCacheState,
}

/// Current mapping state for a descriptor cache.
struct GroupCacheState {
    /// Selected identifier.
    id: FontGroupId,
    /// Selected font list.
    fonts: CachedFontList,
    /// Fallback state.
    fallback: Option<(Script, Cjk)>,
    /// True if the fallbacks list is current.
    fallbacks_ready: bool,
    /// Transient fallback cache to avoid excessive queries.
    fallback_map: FxHashMap<(FamilyId, Attributes), CachedFont>,
    /// Current list of fallback families.
    fallbacks: Vec<FamilyId>,
    /// True if we've attempted to load an emoji font.
    emoji_ready: bool,
    /// Cached emoji font.
    emoji: Option<CachedFont>,
}

impl Default for GroupCacheState {
    fn default() -> Self {
        Self {
            id: FontGroupId(!0),
            fonts: CachedFontList::default(),
            fallback: None,
            fallbacks_ready: true,
            fallback_map: FxHashMap::default(),
            fallbacks: Vec::new(),
            emoji_ready: false,
            emoji: None,
        }
    }
}

impl GroupCacheState {
    fn reset(&mut self) {
        self.id = FontGroupId(!0);
        self.fonts = CachedFontList::default();
        self.fallback = None;
        self.fallbacks_ready = true;
        self.fallback_map.clear();
        self.fallbacks.clear();
        self.emoji_ready = false;
        self.emoji = None;
    }
}

impl GroupCache {
    /// Returns a font group identifier for the specified families and attributes.
    fn get(
        &mut self,
        fonts: &FontCache,
        names: &str,
        key: u64,
        attrs: Attributes,
    ) -> FontGroupId {
        use std::collections::hash_map::Entry;
        let key = (key, attrs);
        // Fast path for a descriptor we've already seen.
        match self.key_map.get_mut(&key) {
            Some(item) => {
                item.epoch = fonts.epoch;
                match self.font_map.entry(item.id) {
                    Entry::Occupied(..) => {}
                    Entry::Vacant(e) => {
                        let start = self.fonts.len();
                        self.fonts.extend(
                            item.data
                                .get()
                                .iter()
                                .map(|&sel| (sel.0, sel.1, attrs).into()),
                        );
                        let end = self.fonts.len();
                        e.insert(CachedFontList {
                            attributes: attrs,
                            start,
                            end,
                        });
                    }
                }
                return item.id;
            }
            _ => {}
        }
        // Parse the descriptor and collect the font identifiers.
        self.tmp.clear();
        for family in parse_families(names) {
            match fonts.query(family, attrs).map(|f| f.selector(attrs)) {
                Some(sel) => self.tmp.push((sel.0, sel.1)),
                _ => {}
            }
        }
        // Slow path: linear search.
        for (item_key, item) in &self.key_map {
            if item_key.1 != attrs {
                continue;
            }
            let existing = item.data.get();
            if existing == &self.tmp {
                match self.font_map.entry(item.id) {
                    Entry::Occupied(..) => {}
                    Entry::Vacant(e) => {
                        let start = self.fonts.len();
                        self.fonts.extend(
                            item.data
                                .get()
                                .iter()
                                .map(|&sel| (sel.0, sel.1, attrs).into()),
                        );
                        let end = self.fonts.len();
                        e.insert(CachedFontList {
                            attributes: attrs,
                            start,
                            end,
                        });
                    }
                }
                return item.id;
            }
        }
        // Insert a new entry.
        let id = FontGroupId(self.next_id);
        self.next_id += 1;
        let mut data =
            GroupData::Inline(0, [(FontId(0), Attributes::default()); MAX_INLINE]);
        for font in &self.tmp {
            data.push(font.0, font.1);
        }
        let start = self.fonts.len();
        self.fonts
            .extend(self.tmp.iter().map(|&sel| (sel.0, sel.1, attrs).into()));
        let end = self.fonts.len();
        self.font_map.insert(
            id,
            CachedFontList {
                attributes: attrs,
                start,
                end,
            },
        );
        let desc = CachedGroup {
            id,
            epoch: fonts.epoch,
            data,
        };
        self.key_map.insert(key, desc);
        id
    }

    /// Selects a descriptor for mapping clusters.
    fn select(&mut self, id: FontGroupId) {
        if self.state.id == id {
            return;
        }
        match self.font_map.get(&id) {
            Some(fonts) => self.state.fonts = *fonts,
            _ => self.state.fonts = CachedFontList::default(),
        }
        self.state.id = id;
    }

    /// Selects a fallback state.
    fn select_fallbacks(&mut self, script: Script, cjk: Cjk) {
        if self.state.fallback != Some((script, cjk)) {
            self.state.fallback = Some((script, cjk));
            self.state.fallbacks_ready = false;
            self.state.fallbacks.clear();
        }
    }

    fn fill_fallbacks(&mut self, fonts: &FontCache) {
        self.state.fallbacks.clear();
        self.state.fallbacks_ready = true;
        match self.state.fallback {
            Some((script, cjk)) => {
                self.state
                    .fallbacks
                    .extend_from_slice(fonts.index.fallbacks(script, cjk));
            }
            _ => {}
        }
    }

    fn emoji(&mut self, fonts: &FontCache, attrs: Attributes) -> Option<&mut CachedFont> {
        if !self.state.emoji_ready {
            self.state.emoji_ready = true;
            let family = fonts.index.emoji_family()?;
            let sel = fonts.query(family, ())?.selector(attrs);
            self.state.emoji = Some(sel.into());
        }
        self.state.emoji.as_mut()
    }

    /// Clears all transient state.
    fn reset(&mut self) {
        self.state.reset();
        self.font_map.clear();
        self.fonts.clear();
    }

    fn prune(&mut self, epoch: Epoch, target_size: usize) {
        if self.key_map.len() <= target_size {
            return;
        }
        let mut count = self.key_map.len() - target_size;
        self.key_map.retain(|_, v| {
            if count != 0 && v.epoch < epoch {
                count -= 1;
                false
            } else {
                true
            }
        });
        if count != 0 {
            self.key_map.retain(|_, _| {
                if count != 0 {
                    count -= 1;
                    false
                } else {
                    true
                }
            });
        }
    }
}

struct CachedGroup {
    id: FontGroupId,
    epoch: Epoch,
    data: GroupData,
}

#[derive(Clone)]
enum GroupData {
    Inline(u8, [(FontId, Attributes); MAX_INLINE]),
    Heap(Vec<(FontId, Attributes)>),
}

impl GroupData {
    fn clear(&mut self) {
        match self {
            Self::Inline(len, _) => {
                *len = 0;
            }
            Self::Heap(vec) => {
                vec.clear();
            }
        }
    }

    fn push(&mut self, font: FontId, attrs: Attributes) {
        match self {
            Self::Inline(len, ids) => {
                if *len as usize == ids.len() {
                    let mut vec = Vec::from(&ids[..]);
                    vec.push((font, attrs));
                    *self = Self::Heap(vec);
                } else {
                    ids[*len as usize] = (font, attrs);
                    *len += 1;
                }
            }
            Self::Heap(vec) => {
                vec.push((font, attrs));
            }
        }
    }

    fn get(&self) -> &[(FontId, Attributes)] {
        match self {
            Self::Inline(len, ids) => &ids[..*len as usize],
            Self::Heap(vec) => &vec,
        }
    }
}

#[derive(Copy, Clone, Default)]
struct CachedFontList {
    /// Attributes are necessary for fallback font selection.
    attributes: Attributes,
    /// Range of cached fonts.
    start: usize,
    /// ... ditto
    end: usize,
}

struct CachedFont {
    id: FontId,
    font: Option<Font>,
    charmap: CharmapProxy,
    synth: Synthesis,
    error: bool,
}

impl CachedFont {
    #[inline]
    fn map_cluster(
        &mut self,
        fonts: &mut FontCache,
        cluster: &mut CharCluster,
        synth: &mut Synthesis,
        first: bool,
    ) -> Option<(Font, Status)> {
        if self.error {
            return None;
        }
        let font = match &self.font {
            Some(font) => font,
            None => {
                let font = fonts.get(self.id);
                let font = match font {
                    Some(f) => f,
                    _ => {
                        self.error = true;
                        return None;
                    }
                };
                self.charmap = CharmapProxy::from_font(&font.as_ref());
                self.font = Some(font);
                self.font.as_ref().unwrap()
            }
        };
        let charmap = self.charmap.materialize(&font.as_ref());
        let status = cluster.map(|ch| charmap.map(ch));
        if status != Status::Discard || first {
            *synth = self.synth;
            Some((font.clone(), status))
        } else {
            None
        }
    }
}

impl From<(FontId, Attributes, Attributes)> for CachedFont {
    fn from(v: (FontId, Attributes, Attributes)) -> Self {
        let synth = v.1.synthesize(v.2);
        Self {
            id: v.0,
            font: None,
            charmap: CharmapProxy::default(),
            synth,
            error: false,
        }
    }
}
