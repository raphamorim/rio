// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// This file was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

use super::index::*;
use super::index_data::*;
use super::library::FontLibrary;
use super::system::{Os, OS};
use super::types::*;
use crate::components::rich_text::util::string::SmallString;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::RwLock,
    time::SystemTime,
};
use swash::{
    Attributes, CacheKey, FontDataRef, FontRef, Stretch, StringId, Style, Weight,
};

/// Hint for specifying whether font files should be memory mapped.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MmapHint {
    /// Never memory map.
    Never,
    /// Always memory map.
    Always,
    /// Memory map when file size is greater than or equal to a
    /// threshold value.
    Threshold(usize),
}

impl Default for MmapHint {
    fn default() -> Self {
        Self::Never
    }
}

/// Builder for configuring a font library.
#[derive(Default)]
pub struct FontLibraryBuilder {
    inner: Inner,
    scanner: Scanner,
    all_names: bool,
    generics: bool,
    fallbacks: bool,
}

impl FontLibraryBuilder {
    /// Specifies whether all localized family names should be included
    /// in the context.
    pub fn all_names(&mut self, yes: bool) -> &mut Self {
        self.all_names = yes;
        self
    }

    /// Specifies a memory mapping hint.
    pub fn mmap(&mut self, hint: MmapHint) -> &mut Self {
        self.inner.mmap_hint = hint;
        self
    }

    /// Adds fonts from the specified directory to the library.
    pub fn add_dir(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.scanner.scan_dir(path, self.all_names, &mut self.inner);
        self
    }

    /// Adds a font file to the library.
    pub fn add_file(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.scanner
            .scan_file(path, self.all_names, &mut self.inner);
        self
    }

    /// Adds system fonts to the library.
    pub fn add_system_fonts(&mut self) -> &mut Self {
        match OS {
            Os::Windows => {
                if let Some(mut windir) = std::env::var_os("SYSTEMROOT") {
                    windir.push("\\Fonts\\");
                    self.add_dir(windir);
                } else {
                    self.add_dir("C:\\Windows\\Fonts\\");
                }
            }
            Os::MacOs => {
                self.add_dir("/System/Library/Fonts/");
                self.add_dir("/Library/Fonts/");
            }
            Os::Ios => {
                self.add_dir("/System/Library/Fonts/");
                self.add_dir("/Library/Fonts/");
            }
            Os::Android => {
                self.add_dir("/system/fonts/");
            }
            Os::Unix => {
                self.add_dir("/usr/share/fonts/");
                self.add_dir("/usr/local/share/fonts/");
            }
            Os::Other => {}
        }
        self
    }

    /// Adds user fonts to the library.
    pub fn add_user_fonts(&mut self) -> &mut Self {
        match OS {
            Os::Windows => {}
            Os::MacOs => {
                if let Some(mut homedir) = std::env::var_os("HOME") {
                    homedir.push("/Library/Fonts/");
                    self.add_dir(&homedir);
                }
            }
            Os::Ios => {}
            Os::Android => {}
            Os::Unix => {
                if let Some(mut homedir) = std::env::var_os("HOME") {
                    homedir.push("/.local/share/fonts/");
                    self.add_dir(&homedir);
                }
            }
            Os::Other => {}
        }
        self
    }

    /// Specifies whether default generic families should be mapped for the
    /// current platform.
    pub fn map_generic_families(&mut self, yes: bool) -> &mut Self {
        self.generics = yes;
        self
    }

    /// Specifies whether default fallbacks should be mapped for the current
    /// platform.
    pub fn map_fallbacks(&mut self, yes: bool) -> &mut Self {
        self.fallbacks = yes;
        self
    }

    /// Builds a library for the current configuration.
    pub fn build(&mut self) -> FontLibrary {
        let mut index = StaticIndex::default();
        core::mem::swap(&mut index, &mut self.inner.index);
        for family in index.families.iter_mut() {
            family
                .fonts
                .sort_unstable_by(|a, b| a.weight.cmp(&b.weight));
        }
        if self.generics {
            index.setup_default_generic();
        }
        if self.fallbacks {
            index.setup_default_fallbacks();
        }
        FontLibrary::new(index)
    }
}

struct Inner {
    path: PathBuf,
    mmap: bool,
    timestamp: SystemTime,
    source: SourceId,
    file_added: bool,
    mmap_hint: MmapHint,
    index: StaticIndex,
    lowercase_name: String,
}

impl Default for Inner {
    fn default() -> Self {
        Self::new()
    }
}

impl Inner {
    fn new() -> Self {
        Self {
            path: PathBuf::new(),
            mmap: false,
            timestamp: SystemTime::UNIX_EPOCH,
            source: SourceId(0),
            file_added: false,
            mmap_hint: MmapHint::default(),
            index: StaticIndex::default(),
            lowercase_name: String::default(),
        }
    }
}

impl ScannerSink for Inner {
    fn enter_file(&mut self, path: PathBuf, timestamp: SystemTime, size: u64) {
        let mmap = match self.mmap_hint {
            MmapHint::Never => false,
            MmapHint::Always => true,
            MmapHint::Threshold(value) => (value as u64) < size,
        };
        self.path = path;
        self.mmap = mmap;
        self.timestamp = timestamp;
        self.source = SourceId(self.index.base.sources.len() as u32);
        self.file_added = false;
    }

    fn add_font(&mut self, font: &FontInfo) {
        self.lowercase_name.clear();
        self.lowercase_name
            .extend(font.name.chars().flat_map(|c| c.to_lowercase()));
        let index = &mut self.index;
        let family = if let Some(family_id) =
            index.base.family_map.get(self.lowercase_name.as_str())
        {
            let family = &mut index.families[family_id.to_usize()];
            if family.contains(font.stretch, font.weight, font.style) {
                return;
            }
            family
        } else {
            let family_id = FamilyId(index.families.len() as u32);
            let family = FamilyData {
                id: family_id,
                name: SmallString::new(&font.name),
                fonts: Vec::new(),
                has_stretch: true,
            };
            index.families.push(family);
            index
                .base
                .family_map
                .insert(SmallString::new(&self.lowercase_name), family_id);
            &mut index.families[family_id.to_usize()]
        };
        if !self.file_added {
            self.file_added = true;
            let mut path2 = PathBuf::new();
            core::mem::swap(&mut path2, &mut self.path);
            index.base.sources.push(SourceData {
                id: self.source,
                kind: SourceKind::File(FileData {
                    path: path2,
                    mmap: self.mmap,
                    timestamp: self.timestamp,
                    status: RwLock::new(FileDataStatus::Empty),
                }),
            });
        }
        let font_id = FontId(index.base.fonts.len() as u32);
        let family_id = family.id;
        let font_data = FontData {
            id: font_id,
            family: family_id,
            source: self.source,
            index: font.index,
            offset: font.offset,
            attributes: font.attrs,
            key: CacheKey::new(),
        };
        index.base.fonts.push(font_data);
        family.fonts.push(FamilyFontData {
            id: font_id,
            stretch: font.stretch,
            weight: font.weight,
            style: font.style,
        });
        if font.stretch != Stretch::NORMAL {
            family.has_stretch = true;
        }
        for name in font.all_names() {
            if !index.base.family_map.contains_key(name.as_str()) {
                index
                    .base
                    .family_map
                    .insert(SmallString::new(name.as_str()), family_id);
            }
        }
    }
}

#[derive(Default)]
pub struct FontInfo {
    pub offset: u32,
    pub index: u32,
    pub name: String,
    pub attrs: Attributes,
    pub stretch: Stretch,
    pub weight: Weight,
    pub style: Style,
    all_names: Vec<String>,
    name_count: usize,
}

impl FontInfo {
    pub fn all_names(&self) -> &[String] {
        &self.all_names[..self.name_count]
    }
}

pub trait ScannerSink {
    fn enter_file(&mut self, path: PathBuf, timestamp: SystemTime, size: u64);
    fn add_font(&mut self, font: &FontInfo);
}

#[derive(Default)]
pub struct Scanner {
    font: FontInfo,
    name: String,
}

impl Scanner {
    pub fn scan_dir(
        &mut self,
        path: impl AsRef<Path>,
        all_names: bool,
        sink: &mut impl ScannerSink,
    ) -> Option<()> {
        self.scan_dir_impl(path, all_names, sink, 0)
    }

    pub fn scan_file(
        &mut self,
        path: impl AsRef<Path>,
        all_names: bool,
        sink: &mut impl ScannerSink,
    ) -> Option<()> {
        let file = fs::File::open(path.as_ref()).ok()?;
        let metadata = file.metadata().ok()?;
        let timestamp = metadata.modified().ok()?;
        let size = metadata.len();
        let data = unsafe { memmap2::Mmap::map(&file).ok()? };
        sink.enter_file(path.as_ref().into(), timestamp, size);
        self.scan_data(&data, all_names, |f| sink.add_font(f))
    }

    pub fn scan_data(
        &mut self,
        data: &[u8],
        all_names: bool,
        mut f: impl FnMut(&FontInfo),
    ) -> Option<()> {
        self.font.name.clear();
        let font_data = FontDataRef::new(data)?;
        for i in 0..font_data.len() {
            if let Some(font) = font_data.get(i) {
                self.scan_font(font, i as u32, all_names, &mut f);
            }
        }
        Some(())
    }

    fn scan_dir_impl(
        &mut self,
        path: impl AsRef<Path>,
        all_names: bool,
        sink: &mut impl ScannerSink,
        recurse: u32,
    ) -> Option<()> {
        if recurse > 4 {
            return Some(());
        }
        let mut lower_ext = [0u8; 3];
        for entry in (fs::read_dir(path).ok()?).flatten() {
            let path = entry.path();
            if path.is_file() {
                let mut is_dfont = false;
                match path.extension().and_then(|e| e.to_str()) {
                    Some("dfont") => is_dfont = true,
                    Some(ext) => {
                        let ext = ext.as_bytes();
                        if ext.len() != 3 {
                            continue;
                        }
                        for i in 0..3 {
                            lower_ext[i] = ext[i].to_ascii_lowercase();
                        }
                    }
                    None => continue,
                };
                if !is_dfont {
                    match &lower_ext {
                        b"ttf" | b"otf" | b"ttc" | b"otc" => {}
                        _ => continue,
                    }
                }
                if let Ok(file) = fs::File::open(&path) {
                    if let Ok(metadata) = entry.metadata() {
                        if let Ok(timestamp) = metadata.modified() {
                            if let Ok(data) = unsafe { memmap2::Mmap::map(&file) } {
                                sink.enter_file(path, timestamp, metadata.len());
                                self.scan_data(&data, all_names, |f| sink.add_font(f));
                            }
                        }
                    }
                }
            } else if path.is_dir() {
                self.scan_dir_impl(&path, all_names, sink, recurse + 1);
            }
        }
        Some(())
    }

    fn scan_font(
        &mut self,
        font: FontRef,
        index: u32,
        all_names: bool,
        f: &mut impl FnMut(&FontInfo),
    ) -> Option<()> {
        self.font.name_count = 0;
        let strings = font.localized_strings();
        let vars = font.variations();
        let var_count = vars.len();
        self.font.name.clear();
        // Use typographic family for variable fonts that tend to encode the
        // full style in the standard family name.
        let mut nid = if var_count != 0 {
            StringId::TypographicFamily
        } else {
            StringId::Family
        };
        if let Some(name) = strings.find_by_id(nid, Some("en")) {
            self.font.name.extend(name.chars());
        } else if let Some(name) = strings.find_by_id(nid, None) {
            self.font.name.extend(name.chars());
        }
        if self.font.name.is_empty() {
            nid = if nid == StringId::Family {
                StringId::TypographicFamily
            } else {
                StringId::Family
            };
            if let Some(name) = strings.find_by_id(nid, Some("en")) {
                self.name.extend(name.chars());
            } else if let Some(name) = strings.find_by_id(nid, None) {
                self.name.extend(name.chars());
            }
        }
        if !self.name.is_empty() && self.name.len() < self.font.name.len() {
            core::mem::swap(&mut self.name, &mut self.font.name);
        }
        if self.font.name.is_empty() {
            if let Some(name) = strings.find_by_id(nid, Some("en")) {
                self.font.name.extend(name.chars());
            } else if let Some(name) = strings.find_by_id(nid, None) {
                self.font.name.extend(name.chars());
            }
        }
        if self.font.name.is_empty() {
            return None;
        }
        self.font.attrs = font.attributes();
        let (stretch, weight, style) = self.font.attrs.parts();
        self.font.stretch = stretch;
        self.font.weight = weight;
        self.font.style = style;
        self.font.index = index;
        self.font.offset = font.offset;
        let mut count = 0;
        if all_names {
            for name in strings.filter(|name| name.id() == nid && name.is_unicode()) {
                if count >= self.font.all_names.len() {
                    self.font.all_names.push(String::default());
                }
                let name_buf = &mut self.font.all_names[count];
                count += 1;
                name_buf.clear();
                for ch in name.chars() {
                    name_buf.extend(ch.to_lowercase());
                }
            }
        }
        f(&self.font);
        Some(())
    }
}
