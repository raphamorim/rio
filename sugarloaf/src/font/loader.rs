use std::path::PathBuf;

use crate::font::SharedData;
pub use ttf_parser::Language;

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
    Binary(SharedData),
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
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Default, PartialOrd, Ord)]
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

impl Stretch {
    fn to_number(self) -> u16 {
        match self {
            Stretch::UltraCondensed => 50,
            Stretch::ExtraCondensed => 62,
            Stretch::Condensed => 75,
            Stretch::SemiCondensed => 87,
            Stretch::Normal => 100,
            Stretch::SemiExpanded => 112,
            Stretch::Expanded => 125,
            Stretch::ExtraExpanded => 150,
            Stretch::UltraExpanded => 200,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn from_font_kit(fk_stretch: font_kit::properties::Stretch) -> Self {
        use font_kit::properties::Stretch as FKS;
        let val = fk_stretch.0;
        if val <= FKS::ULTRA_CONDENSED.0 {
            Stretch::UltraCondensed
        } else if val <= FKS::EXTRA_CONDENSED.0 {
            Stretch::ExtraCondensed
        } else if val <= FKS::CONDENSED.0 {
            Stretch::Condensed
        } else if val <= FKS::SEMI_CONDENSED.0 {
            Stretch::SemiCondensed
        } else if val <= FKS::NORMAL.0 {
            Stretch::Normal
        } else if val <= FKS::SEMI_EXPANDED.0 {
            Stretch::SemiExpanded
        } else if val <= FKS::EXPANDED.0 {
            Stretch::Expanded
        } else if val <= FKS::EXTRA_EXPANDED.0 {
            Stretch::ExtraExpanded
        } else {
            Stretch::UltraExpanded
        }
    }
}

/// Font style
#[derive(Clone, Default, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Style {
    #[default]
    Normal,
    Italic,
    Oblique,
}

impl Style {
    #[cfg(not(target_arch = "wasm32"))]
    fn from_font_kit(fk_style: font_kit::properties::Style) -> Self {
        use font_kit::properties::Style as FKStyle;
        match fk_style {
            FKStyle::Normal => Style::Normal,
            FKStyle::Italic => Style::Italic,
            FKStyle::Oblique => Style::Oblique,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct FontCandidate {
    handle: font_kit::handle::Handle,
    weight: Weight,
    stretch: Stretch,
    style: Style,
}

/// CSS-spec compliant font matching algorithm
/// Based on https://www.w3.org/TR/css-fonts-4/#font-matching-algorithm
#[cfg(not(target_arch = "wasm32"))]
fn find_best_match(candidates: &[FontCandidate], query: &Query) -> Option<usize> {
    if candidates.is_empty() {
        return None;
    }

    // Step 4a: Match font-stretch
    let mut matching_set: Vec<usize> = (0..candidates.len()).collect();

    let matches = matching_set
        .iter()
        .any(|&index| candidates[index].stretch == query.stretch);

    let matching_stretch = if matches {
        query.stretch
    } else if query.stretch <= Stretch::Normal {
        // closest stretch, first checking narrower values and then wider values
        let stretch = matching_set
            .iter()
            .filter(|&&index| candidates[index].stretch < query.stretch)
            .min_by_key(|&&index| {
                query.stretch.to_number() - candidates[index].stretch.to_number()
            });

        match stretch {
            Some(&matching_index) => candidates[matching_index].stretch,
            None => {
                let matching_index = *matching_set.iter().min_by_key(|&&index| {
                    candidates[index]
                        .stretch
                        .to_number()
                        .abs_diff(query.stretch.to_number())
                })?;
                candidates[matching_index].stretch
            }
        }
    } else {
        // closest stretch, first checking wider values and then narrower values
        let stretch = matching_set
            .iter()
            .filter(|&&index| candidates[index].stretch > query.stretch)
            .min_by_key(|&&index| {
                candidates[index].stretch.to_number() - query.stretch.to_number()
            });

        match stretch {
            Some(&matching_index) => candidates[matching_index].stretch,
            None => {
                let matching_index = *matching_set.iter().min_by_key(|&&index| {
                    query
                        .stretch
                        .to_number()
                        .abs_diff(candidates[index].stretch.to_number())
                })?;
                candidates[matching_index].stretch
            }
        }
    };
    matching_set.retain(|&index| candidates[index].stretch == matching_stretch);

    // Step 4b: Match font-style
    let style_preference = match query.style {
        Style::Italic => [Style::Italic, Style::Oblique, Style::Normal],
        Style::Oblique => [Style::Oblique, Style::Italic, Style::Normal],
        Style::Normal => [Style::Normal, Style::Oblique, Style::Italic],
    };

    let matching_style = *style_preference.iter().find(|&query_style| {
        matching_set
            .iter()
            .any(|&index| candidates[index].style == *query_style)
    })?;

    matching_set.retain(|&index| candidates[index].style == matching_style);

    // Step 4c: Match font-weight
    let weight = query.weight.0;

    let matching_weight = if matching_set
        .iter()
        .any(|&index| candidates[index].weight.0 == weight)
    {
        Weight(weight)
    } else if (400..450).contains(&weight)
        && matching_set
            .iter()
            .any(|&index| candidates[index].weight.0 == 500)
    {
        Weight::MEDIUM
    } else if (450..=500).contains(&weight)
        && matching_set
            .iter()
            .any(|&index| candidates[index].weight.0 == 400)
    {
        Weight::NORMAL
    } else if weight <= 500 {
        // Closest weight, first checking thinner values and then fatter ones
        let idx = matching_set
            .iter()
            .filter(|&&index| candidates[index].weight.0 <= weight)
            .min_by_key(|&&index| weight - candidates[index].weight.0);

        match idx {
            Some(&matching_index) => candidates[matching_index].weight,
            None => {
                let matching_index = *matching_set
                    .iter()
                    .min_by_key(|&&index| candidates[index].weight.0.abs_diff(weight))?;
                candidates[matching_index].weight
            }
        }
    } else {
        // Closest weight, first checking fatter values and then thinner ones
        let idx = matching_set
            .iter()
            .filter(|&&index| candidates[index].weight.0 >= weight)
            .min_by_key(|&&index| candidates[index].weight.0 - weight);

        match idx {
            Some(&matching_index) => candidates[matching_index].weight,
            None => {
                let matching_index = *matching_set
                    .iter()
                    .min_by_key(|&&index| weight.abs_diff(candidates[index].weight.0))?;
                candidates[matching_index].weight
            }
        }
    };
    matching_set.retain(|&index| candidates[index].weight == matching_weight);

    matching_set.into_iter().next()
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
    /// Gets ALL faces from the family, then applies CSS-spec matching
    #[cfg(not(target_arch = "wasm32"))]
    pub fn query(&self, query: &Query) -> Option<ID> {
        use font_kit::family_name::FamilyName;

        tracing::debug!("Query starting: {:?}", query);

        // Convert query to font-kit family name
        for family in query.families {
            let family_name = match family {
                Family::Name(name) => FamilyName::Title(name.to_string()),
                Family::Serif => FamilyName::Serif,
                Family::SansSerif => FamilyName::SansSerif,
                Family::Cursive => FamilyName::Cursive,
                Family::Fantasy => FamilyName::Fantasy,
                Family::Monospace => FamilyName::Monospace,
            };

            // Get the family name string
            let family_name_str = match &family_name {
                FamilyName::Title(s) => s.as_str(),
                FamilyName::Serif => "serif",
                FamilyName::SansSerif => "sans-serif",
                FamilyName::Cursive => "cursive",
                FamilyName::Fantasy => "fantasy",
                FamilyName::Monospace => "monospace",
            };
            let family_name_lower = family_name_str.to_lowercase();

            tracing::debug!(
                "Searching for family: '{}' (lowercase: '{}')",
                family_name_str,
                family_name_lower
            );

            let mut candidates = Vec::new();

            // Step 1: collect all font faces from additional sources (user directories)
            tracing::debug!(
                "checking {} additional sources",
                self.additional_sources.len()
            );
            for (idx, additional_source) in self.additional_sources.iter().enumerate() {
                if candidates.is_empty() {
                    tracing::debug!(
                        "additional source {}: trying case-insensitive match",
                        idx
                    );
                    if let Ok(families) = additional_source.all_families() {
                        tracing::debug!(
                            "additional source {}: has {} families",
                            idx,
                            families.len()
                        );
                        for system_family_name in families {
                            if system_family_name.to_lowercase() == family_name_lower {
                                if let Ok(family_handle) = additional_source
                                    .select_family_by_name(&system_family_name)
                                {
                                    for handle in family_handle.fonts() {
                                        if let Ok(font) = handle.load() {
                                            let props = font.properties();
                                            tracing::debug!("found candidate: weight={}, stretch={:?}, style={:?}",
                                                props.weight.0, props.stretch, props.style);
                                            candidates.push(FontCandidate {
                                                handle: handle.clone(),
                                                weight: Weight(props.weight.0 as u16),
                                                stretch: Stretch::from_font_kit(
                                                    props.stretch,
                                                ),
                                                style: Style::from_font_kit(props.style),
                                            });
                                        }
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
            }

            // step 2: try case-insensitive on system fonts
            if candidates.is_empty() {
                tracing::debug!("System fonts: trying case-insensitive match");
                if let Ok(families) = self.system_source.all_families() {
                    tracing::debug!("System has {} families total", families.len());
                    for system_family_name in families {
                        if system_family_name.to_lowercase() == family_name_lower {
                            tracing::debug!(
                                "  Found case-insensitive system match: '{}'",
                                system_family_name
                            );
                            if let Ok(family_handle) = self
                                .system_source
                                .select_family_by_name(&system_family_name)
                            {
                                for handle in family_handle.fonts() {
                                    if let Ok(font) = handle.load() {
                                        let props = font.properties();
                                        tracing::debug!("    Found system candidate: weight={}, stretch={:?}, style={:?}",
                                            props.weight.0, props.stretch, props.style);
                                        candidates.push(FontCandidate {
                                            handle: handle.clone(),
                                            weight: Weight(props.weight.0 as u16),
                                            stretch: Stretch::from_font_kit(
                                                props.stretch,
                                            ),
                                            style: Style::from_font_kit(props.style),
                                        });
                                    }
                                }
                            }
                            break;
                        }
                    }
                }
            }

            tracing::debug!("Total candidates found: {}", candidates.len());

            // Step 3: apply CSS-spec matching algorithm to select the best face
            if let Some(index) = find_best_match(&candidates, query) {
                tracing::debug!("Best match selected at index {}", index);
                return Some(ID::from_handle(candidates[index].handle.clone()));
            } else {
                tracing::debug!("No best match found from candidates");
            }
        }

        tracing::debug!("Query failed: no fonts found");
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
        tracing::debug!("face_source: getting source for ID");
        if let Some(handle) = id.to_handle() {
            tracing::debug!("face_source: handle retrieved");
            match handle {
                font_kit::handle::Handle::Path {
                    ref path,
                    font_index,
                } => {
                    tracing::debug!(
                        "face_source: Path source - {}, index {}",
                        path.display(),
                        font_index
                    );
                    return Some((Source::File(path.clone()), font_index));
                }
                font_kit::handle::Handle::Memory { bytes, font_index } => {
                    tracing::debug!(
                        "face_source: Memory source, {} bytes, index {}",
                        bytes.len(),
                        font_index
                    );
                    return Some((
                        Source::Binary(SharedData::new(bytes.to_vec())),
                        font_index,
                    ));
                }
            }
        } else {
            tracing::debug!("face_source: handle is None!");
        }
        tracing::debug!("face_source: returning None");
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
