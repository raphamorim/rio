pub use ttf_parser::Language;

use std::path::PathBuf;

#[cfg(not(target_arch = "wasm32"))]
use font_kit::source::SystemSource;

#[derive(Clone, Debug)]
pub struct ID {
    #[cfg(not(target_arch = "wasm32"))]
    handle: Option<font_kit::handle::Handle>,
    // TODO: Fix wasm32
    #[cfg(target_arch = "wasm32")]
    _dummy: u32,
}

impl ID {
    #[cfg(not(target_arch = "wasm32"))]
    fn from_handle(handle: font_kit::handle::Handle) -> Self {
        Self {
            handle: Some(handle),
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn from_handle(_handle: ()) -> Self {
        Self { _dummy: 0 }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn to_handle(&self) -> Option<font_kit::handle::Handle> {
        self.handle.clone()
    }
}

#[derive(Clone, Debug)]
pub enum Source {
    File(PathBuf),
    Binary(std::sync::Arc<Vec<u8>>),
}

/// Font query parameters
#[derive(Clone, Copy, Default, Debug)]
pub struct Query<'a> {
    pub families: &'a [Family<'a>],
    pub weight: Weight,
    pub stretch: Stretch,
    pub style: Style,
}

/// Font family
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family<'a> {
    Name(&'a str),
    Serif,
    SansSerif,
    Cursive,
    Fantasy,
    Monospace,
}

/// Font weight
#[derive(Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Debug, Hash)]
pub struct Weight(pub u16);

impl Default for Weight {
    fn default() -> Weight {
        Weight::NORMAL
    }
}

impl Weight {
    pub const THIN: Weight = Weight(100);
    pub const EXTRA_LIGHT: Weight = Weight(200);
    pub const LIGHT: Weight = Weight(300);
    pub const NORMAL: Weight = Weight(400);
    pub const MEDIUM: Weight = Weight(500);
    pub const SEMIBOLD: Weight = Weight(600);
    pub const BOLD: Weight = Weight(700);
    pub const EXTRA_BOLD: Weight = Weight(800);
    pub const BLACK: Weight = Weight(900);
}

/// Font stretch/width
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
pub enum Stretch {
    UltraCondensed,
    ExtraCondensed,
    Condensed,
    SemiCondensed,
    #[default]
    Normal,
    SemiExpanded,
    Expanded,
    ExtraExpanded,
    UltraExpanded,
}

/// Font style
#[derive(Clone, Default, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Style {
    #[default]
    Normal,
    Italic,
    Oblique,
}

pub struct Database {
    #[cfg(not(target_arch = "wasm32"))]
    system_source: SystemSource,
    #[cfg(not(target_arch = "wasm32"))]
    additional_sources: Vec<font_kit::sources::mem::MemSource>,
}

impl Database {
    pub fn new() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            system_source: SystemSource::new(),
            #[cfg(not(target_arch = "wasm32"))]
            additional_sources: Vec::new(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_fonts_dir<P: AsRef<std::path::Path>>(&mut self, path: P) {
        use font_kit::handle::Handle;
        use walkdir::WalkDir;

        // Scan directory for font files
        let mut fonts = Vec::new();
        for entry in WalkDir::new(path.as_ref())
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    let ext_lower = ext.to_string_lossy().to_lowercase();
                    if ext_lower == "ttf"
                        || ext_lower == "otf"
                        || ext_lower == "ttc"
                        || ext_lower == "otc"
                    {
                        // Create handle - font data will be loaded lazily when needed
                        fonts.push(Handle::from_path(path.to_path_buf(), 0));
                    }
                }
            }
        }

        // Create memory source from handles (stores paths, not data)
        if !fonts.is_empty() {
            if let Ok(mem_source) =
                font_kit::sources::mem::MemSource::from_fonts(fonts.into_iter())
            {
                self.additional_sources.push(mem_source);
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn load_fonts_dir<P: AsRef<std::path::Path>>(&mut self, _path: P) {
        // No-op for WASM
    }

    /// Query for a font matching the given criteria
    #[cfg(not(target_arch = "wasm32"))]
    pub fn query(&self, query: &Query) -> Option<ID> {
        use font_kit::family_name::FamilyName;
        use font_kit::properties::{
            Properties, Stretch as FKStretch, Style as FKStyle, Weight as FKWeight,
        };

        // Convert Rio's query to font-kit query
        for family in query.families {
            let family_name = match family {
                Family::Name(name) => FamilyName::Title(name.to_string()),
                Family::Serif => FamilyName::Serif,
                Family::SansSerif => FamilyName::SansSerif,
                Family::Cursive => FamilyName::Cursive,
                Family::Fantasy => FamilyName::Fantasy,
                Family::Monospace => FamilyName::Monospace,
            };

            // Convert properties
            let weight = FKWeight(query.weight.0 as f32);
            let stretch = match query.stretch {
                Stretch::UltraCondensed => FKStretch::ULTRA_CONDENSED,
                Stretch::ExtraCondensed => FKStretch::EXTRA_CONDENSED,
                Stretch::Condensed => FKStretch::CONDENSED,
                Stretch::SemiCondensed => FKStretch::SEMI_CONDENSED,
                Stretch::Normal => FKStretch::NORMAL,
                Stretch::SemiExpanded => FKStretch::SEMI_EXPANDED,
                Stretch::Expanded => FKStretch::EXPANDED,
                Stretch::ExtraExpanded => FKStretch::EXTRA_EXPANDED,
                Stretch::UltraExpanded => FKStretch::ULTRA_EXPANDED,
            };
            let style = match query.style {
                Style::Normal => FKStyle::Normal,
                Style::Italic => FKStyle::Italic,
                Style::Oblique => FKStyle::Oblique,
            };

            let properties = Properties {
                weight,
                stretch,
                style,
            };

            // First try additional sources (user-specified directories have priority)
            for additional_source in &self.additional_sources {
                if let Ok(handle) = additional_source
                    .select_best_match(std::slice::from_ref(&family_name), &properties)
                {
                    return Some(ID::from_handle(handle));
                }
            }

            // Then fallback to system fonts
            if let Ok(handle) = self
                .system_source
                .select_best_match(&[family_name], &properties)
            {
                return Some(ID::from_handle(handle));
            }
        }

        None
    }

    #[cfg(target_arch = "wasm32")]
    pub fn query(&self, _query: &Query) -> Option<ID> {
        None
    }

    /// Get face source (path and index) for a given ID
    #[cfg(not(target_arch = "wasm32"))]
    pub fn face_source(&self, id: ID) -> Option<(Source, u32)> {
        // Reconstruct handle from ID
        if let Some(handle) = id.to_handle() {
            match handle {
                font_kit::handle::Handle::Path { path, font_index } => {
                    return Some((Source::File(path), font_index));
                }
                font_kit::handle::Handle::Memory { bytes, font_index } => {
                    return Some((Source::Binary(bytes), font_index));
                }
            }
        }
        None
    }

    #[cfg(target_arch = "wasm32")]
    pub fn face_source(&self, _id: ID) -> Option<(Source, u32)> {
        None
    }
}

impl Default for Database {
    fn default() -> Self {
        Self::new()
    }
}
