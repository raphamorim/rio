// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// This file was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

//! Font index.

use super::index_data::*;
use super::types::*;
use super::{
    fallback::Fallbacks,
    types::{FamilyId, GenericFamily},
};
use crate::components::rich_text::util::{
    fxhash::FxHashMap,
    string::{LowercaseString, SmallString},
};
use std::path::Path;
use std::sync::Arc;
use swash::text::{Cjk, Script};
use swash::{Attributes, CacheKey};

/// Type alias for signatures to distinguish between inherent and
/// requested attributes.
pub type RequestedAttributes = Attributes;

#[derive(Default)]
pub struct BaseIndex {
    pub family_map: FxHashMap<SmallString, FamilyId>,
    pub fonts: Vec<FontData>,
    pub sources: Vec<SourceData>,
}

pub struct StaticIndex {
    pub base: BaseIndex,
    pub families: Vec<FamilyData>,
    pub script_map: FxHashMap<Script, Fallbacks>,
    pub cjk: [Fallbacks; 5],
    pub generic: [Option<FamilyId>; 13],
}

impl Default for StaticIndex {
    fn default() -> Self {
        let fallbacks = Fallbacks::new();
        Self {
            base: BaseIndex::default(),
            families: Vec::new(),
            script_map: Default::default(),
            cjk: [fallbacks; 5],
            generic: [None; 13],
        }
    }
}

impl StaticIndex {
    pub fn setup_default_fallbacks(&mut self) {
        use super::system::*;
        use Cjk::*;
        use Script::*;
        match OS {
            Os::Windows => {
                // Simplified Chinese
                self.cjk[Simplified as usize] =
                    self.find_fallbacks(&["microsoft yahei", "simsun", "simsun-extb"]);
                // Traditional Chinese
                self.cjk[Traditional as usize] = self.find_fallbacks(&[
                    "microsoft jhenghei",
                    "pmingliu",
                    "pmingliu-extb",
                ]);
                self.cjk[Cjk::None as usize] = self.cjk[Traditional as usize];
                // Japanese
                self.cjk[Japanese as usize] = self.find_fallbacks(&[
                    "meiryo",
                    "yu gothic",
                    "microsoft yahei",
                    "simsun",
                    "simsun-extb",
                ]);
                // Korean
                self.cjk[Korean as usize] = self.find_fallbacks(&[
                    "malgun gothic",
                    "gulim",
                    "microsoft yahei",
                    "simsun",
                    "simsun-extb",
                ]);
                self.map_script(Latin, &["times new roman"]);
                self.map_script(Arabic, &["tahoma", "segoe ui"]);
                self.map_script(Armenian, &["segoe ui", "sylfaen"]);
                self.map_script(Bengali, &["nirmala ui", "vrinda"]);
                self.map_script(Brahmi, &["segoe ui historic"]);
                self.map_script(Braille, &["segoe ui symbol"]);
                self.map_script(Buginese, &["leelawadee ui"]);
                self.map_script(CanadianAboriginal, &["gadugi", "euphemia"]);
                self.map_script(Carian, &["segoe ui historic"]);
                self.map_script(Devanagari, &["nirmala ui", "mangal"]);
                self.map_script(Hebrew, &["david", "segoe ui", "calibri"]);
                self.map_script(Hangul, &["malgun gothic", "gulim"]);
                self.map_script(Myanmar, &["myanmar text"]);
                self.map_script(Malayalam, &["nirmala ui", "kartika"]);
                self.map_script(Han, &["microsoft yahei", "simsun", "simsun-extb"]);
                self.map_script(
                    Hiragana,
                    &["meiryo", "yu gothic", "ms pgothic", "microsoft yahei"],
                );
                self.map_script(
                    Katakana,
                    &["meiryo", "yu gothic", "ms pgothic", "microsoft yahei"],
                );
                self.map_script(Kharoshthi, &["segoe ui historic"]);
                self.map_script(
                    Khmer,
                    &[
                        "leelawadee ui",
                        "khmer ui",
                        "khmer os",
                        "moolboran",
                        "daunpenh",
                    ],
                );
                self.map_script(
                    Lao,
                    &[
                        "leelawadee ui",
                        "lao ui",
                        "dokchampa",
                        "saysettha ot",
                        "phetsarath ot",
                        "code2000",
                    ],
                );
                self.map_script(Lisu, &["segoe ui"]);
                self.map_script(
                    Syriac,
                    &["estrangelo edessa", "estrangelo nisibin", "code2000"],
                );
                self.map_script(Thai, &["tahoma", "leelawadee ui", "leelawadee"]);
                self.map_script(
                    Tibetan,
                    &["microsoft himalaya", "jomolhari", "tibetan machine uni"],
                );
                self.map_script(Vai, &["ebrima"]);
                self.map_script(Yi, &["microsoft yi baiti", "nuosu sil", "code2000"]);
            }
            Os::MacOs => {
                // Simplified Chinese
                self.cjk[Simplified as usize] = self.find_fallbacks(&["pingfang sc"]);
                // Traditional Chinese
                self.cjk[Traditional as usize] = self.find_fallbacks(&["pingfang tc"]);
                self.cjk[Cjk::None as usize] = self.cjk[Traditional as usize];
                // Japanese
                self.cjk[Japanese as usize] =
                    self.find_fallbacks(&["hiragino kaku gothic pron w3"]);
                // Korean
                self.cjk[Korean as usize] = self.find_fallbacks(&["apple sd gothic neo"]);
                self.map_script(Latin, &["times", "times new roman"]);
                self.map_script(Arabic, &["geeza pro"]);
                self.map_script(
                    Devanagari,
                    &[
                        "itf devanagari",
                        "kohinoor devanagari",
                        "devanagari sangam mn",
                        "devanagari mt",
                    ],
                );
                self.map_script(Bengali, &[]);
                self.map_script(Myanmar, &["noto sans myanmar", "myanmar mn"]);
                self.map_script(Malayalam, &["malayalam mn"]);
                self.map_script(Hebrew, &["lucida grande", "arial hebrew"]);
            }
            _ => {
                self.map_script(
                    Latin,
                    &[
                        "liberation sans",
                        "dejavu sans",
                        "ubuntu",
                        "source sans pro",
                    ],
                );
                self.map_script(Arabic, &["noto sans arabic"]);
                self.map_script(Hebrew, &["noto sans hebrew", "noto serif hebrew"]);
                self.map_script(Bengali, &["noto sans bengali", "noto serif bengali"]);
                self.map_script(
                    Devanagari,
                    &["noto sans devanagari", "noto serif devanagari"],
                );
                self.map_script(
                    Malayalam,
                    &["noto sans malayalam", "noto serif malayalam"],
                );
                self.map_script(Myanmar, &["noto sans myanmar", "noto serif myanmar"]);
            }
        }
    }

    pub fn setup_default_generic(&mut self) {
        use super::system::*;
        use GenericFamily::*;
        match OS {
            Os::Windows => {
                self.generic[SansSerif as usize] = self.find_family(&["arial"]);
                self.generic[Serif as usize] = self.find_family(&["times new roman"]);
                self.generic[Monospace as usize] = self.find_family(&["courier new"]);
                self.generic[Fantasy as usize] = self.find_family(&["impact"]);
                self.generic[Cursive as usize] = self.find_family(&["comic sans ms"]);
                self.generic[SystemUI as usize] = self.find_family(&["segoe ui"]);
                self.generic[Emoji as usize] = self.find_family(&["segoe ui emoji"]);
            }
            Os::MacOs => {
                self.generic[SansSerif as usize] = self.find_family(&["helvetica"]);
                self.generic[Serif as usize] = self.find_family(&["times"]);
                self.generic[Monospace as usize] = self.find_family(&["courier"]);
                self.generic[Fantasy as usize] = self.find_family(&["papyrus"]);
                self.generic[Cursive as usize] = self.find_family(&["apple chancery"]);
                self.generic[SystemUI as usize] =
                    self.find_family(&["system font", "helvetica"]);
                self.generic[Emoji as usize] = self.find_family(&["apple color emoji"]);
            }
            Os::Ios => {
                self.generic[SansSerif as usize] = self.find_family(&["helvetica"]);
                self.generic[Serif as usize] = self.find_family(&["times new roman"]);
                self.generic[Monospace as usize] = self.find_family(&["courier"]);
                self.generic[Fantasy as usize] = self.find_family(&["papyrus"]);
                self.generic[Cursive as usize] = self.find_family(&["snell roundhand"]);
                self.generic[SystemUI as usize] =
                    self.find_family(&["system font", "helvetica"]);
                self.generic[Emoji as usize] = self.find_family(&["apple color emoji"]);
            }
            Os::Android => {
                self.generic[SansSerif as usize] = self.find_family(&["roboto"]);
                self.generic[Serif as usize] =
                    self.find_family(&["noto serif", "droid serif"]);
                self.generic[Monospace as usize] = self.find_family(&["droid sans mono"]);
                self.generic[Fantasy as usize] = self.find_family(&["noto serif"]);
                self.generic[Cursive as usize] = self.find_family(&["dancing script"]);
                self.generic[SystemUI as usize] = self.find_family(&["roboto"]);
                self.generic[Emoji as usize] = self.find_family(&["noto color emoji"]);
            }
            Os::Unix | Os::Other => {
                self.generic[SansSerif as usize] =
                    self.find_family(&["liberation sans", "dejavu sans"]);
                self.generic[Serif as usize] = self.find_family(&[
                    "liberation serif",
                    "dejavu serif",
                    "noto serif",
                    "times new roman",
                ]);
                self.generic[Monospace as usize] =
                    self.find_family(&["dejavu sans mono"]);
                self.generic[Fantasy as usize] =
                    self.find_family(&["liberation serif", "dejavu serif"]);
                self.generic[Cursive as usize] =
                    self.find_family(&["liberation serif", "dejavu serif"]);
                self.generic[SystemUI as usize] =
                    self.find_family(&["liberation sans", "dejavu sans"]);
                self.generic[Emoji as usize] =
                    self.find_family(&["noto color emoji", "emoji one"]);
            }
        }
    }

    pub fn emoji_family(&self) -> Option<FamilyId> {
        self.generic[GenericFamily::Emoji as usize]
    }

    pub fn fallbacks(&self, script: Script, cjk: Cjk) -> &[FamilyId] {
        if script == Script::Han {
            self.cjk[cjk as usize].get()
        } else {
            self.script_map.get(&script).map(|f| f.get()).unwrap_or(&[])
        }
    }

    fn map_script(&mut self, script: Script, families: &[&str]) {
        let fallbacks = self.find_fallbacks(families);
        if fallbacks.len() != 0 {
            self.script_map.insert(script, fallbacks);
        }
    }

    fn find_family(&self, families: &[&str]) -> Option<FamilyId> {
        for family in families {
            if let Some(id) = self.base.family_map.get(*family) {
                return Some(*id);
            }
        }
        None
    }

    fn find_fallbacks(&self, families: &[&str]) -> Fallbacks {
        let mut fallbacks = Fallbacks::new();
        for family in families {
            if let Some(id) = self.base.family_map.get(*family) {
                if !fallbacks.push(*id) {
                    break;
                }
            }
        }
        fallbacks
    }
}

impl StaticIndex {
    /// Returns a font entry that matches the specified family and
    /// attributes.
    pub fn query<'a>(
        &'a self,
        family: impl Into<FamilyKey<'a>>,
        attributes: impl Into<Attributes>,
    ) -> Option<FontEntry<'a>> {
        let family = self.family_by_key(family)?;
        let attrs = attributes.into();
        let font_id = family.data.query(attrs)?;
        let data = self.base.fonts.get(font_id.to_usize())?;
        Some(FontEntry {
            index: &self.base,
            family: family.data,
            data,
        })
    }

    /// Returns a font family entry for the specified family key.
    pub fn family_by_key<'a>(
        &'a self,
        key: impl Into<FamilyKey<'a>>,
    ) -> Option<FamilyEntry<'a>> {
        match key.into() {
            FamilyKey::Id(id) => self.family_by_id(id),
            FamilyKey::Name(name) => self.family_by_name(name),
            FamilyKey::Generic(generic) => {
                self.family_by_id(self.generic.get(generic as usize).copied()??)
            }
        }
    }

    /// Returns a font family entry for the specified name.
    pub fn family_by_name(&self, name: &str) -> Option<FamilyEntry<'_>> {
        let mut s = LowercaseString::new();
        let lowercase_name = s.get(name)?;
        let id = *self.base.family_map.get(lowercase_name)?;
        self.family_by_id(id)
    }

    /// Returns a font family entry for the specified identifier.
    pub fn family_by_id(&self, id: FamilyId) -> Option<FamilyEntry<'_>> {
        let data = self.families.get(id.to_usize())?;
        Some(FamilyEntry {
            index: &self.base,
            data,
        })
    }

    /// Returns a font entry for the specified identifier.
    pub fn font_by_id(&self, id: FontId) -> Option<FontEntry<'_>> {
        let data = self.base.fonts.get(id.to_usize())?;
        let family = self.families.get(data.family.to_usize())?;
        Some(FontEntry {
            index: &self.base,
            family,
            data,
        })
    }
}

#[derive(Default)]
pub struct DynamicIndex {
    pub base: BaseIndex,
    pub families: Vec<Arc<FamilyData>>,
}

/// Font family entry in a library.
#[derive(Copy, Clone)]
pub struct FamilyEntry<'a> {
    index: &'a BaseIndex,
    data: &'a FamilyData,
}

impl<'a> FamilyEntry<'a> {
    /// Returns the family identifier.
    pub fn id(&self) -> FamilyId {
        self.data.id
    }

    /// Returns the name of the family.
    pub fn name(&self) -> &str {
        self.data.name.as_str()
    }

    /// Returns an iterator over the fonts in the family.
    pub fn fonts(&'a self) -> impl Iterator<Item = FontEntry<'a>> + 'a {
        self.data.fonts.iter().filter_map(move |f| {
            let data = self.index.fonts.get(f.id.to_usize())?;
            Some(FontEntry {
                index: self.index,
                family: self.data,
                data,
            })
        })
    }
}

/// Font entry in a library.
#[derive(Copy, Clone)]
pub struct FontEntry<'a> {
    index: &'a BaseIndex,
    family: &'a FamilyData,
    data: &'a FontData,
}

impl<'a> FontEntry<'a> {
    /// Returns the font identifier.
    pub fn id(&self) -> FontId {
        self.data.id
    }

    /// Returns the font source.
    pub fn source(&self) -> SourceEntry<'a> {
        SourceEntry {
            index: self.index,
            data: &self.index.sources[self.data.source.to_usize()],
        }
    }

    /// Returns the index of the font in the source.
    pub fn index(&self) -> u32 {
        self.data.index
    }

    /// Returns the offset to the font table directory in the source.
    pub fn offset(&self) -> u32 {
        self.data.offset
    }

    /// Returns the family entry.
    pub fn family(&self) -> FamilyEntry<'a> {
        FamilyEntry {
            index: self.index,
            data: self.family,
        }
    }

    /// Returns the family name.
    pub fn family_name(&self) -> &str {
        self.family.name.as_str()
    }

    /// Returns the font attributes.
    pub fn attributes(&self) -> Attributes {
        self.data.attributes
    }

    pub(crate) fn cache_key(&self) -> CacheKey {
        self.data.key
    }

    pub(crate) fn selector(
        &self,
        attrs: RequestedAttributes,
    ) -> (FontId, Attributes, RequestedAttributes) {
        (self.data.id, self.data.attributes, attrs)
    }
}

/// Source entry in a library.
#[derive(Copy, Clone)]
pub struct SourceEntry<'a> {
    #[allow(unused)]
    index: &'a BaseIndex,
    data: &'a SourceData,
}

impl<'a> SourceEntry<'a> {
    /// Returns the source identifier.
    pub fn id(&self) -> SourceId {
        self.data.id
    }

    /// Returns the path of the source, if it is represented by a file.
    pub fn path(&self) -> Option<&Path> {
        match &self.data.kind {
            SourceKind::Memory(..) => None,
            SourceKind::File(data) => Some(&data.path),
        }
    }
}
