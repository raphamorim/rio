use std::path::PathBuf;

use crate::font::SharedData;
pub use ttf_parser::Language;

#[cfg(not(target_arch = "wasm32"))]
use font_kit::source::SystemSource;

#[derive(Clone, Debug)]
pub struct ID {
    #[cfg(not(target_arch = "wasm32"))]
    handle: Option<font_kit::handle::Handle>,
    /// What `Database::query` matched beyond the base face.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) matched: MatchedFace,
    // TODO: Fix wasm32
    #[cfg(target_arch = "wasm32")]
    _dummy: u32,
}

/// The named instance the query picked. Reopening a file by path+index
/// only gives its default, so `FontData::from_data` replays these values.
#[derive(Clone, Debug, Default)]
pub struct MatchedFace {
    /// Weight the match resolves to.
    pub weight: f32,
    /// Coordinates off their axis defaults (axis tag as big-endian `u32`).
    pub coords: Vec<(u32, f32)>,
}

impl ID {
    #[cfg(not(target_arch = "wasm32"))]
    fn from_handle(handle: font_kit::handle::Handle, matched: MatchedFace) -> Self {
        Self {
            handle: Some(handle),
            matched,
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
    /// Exact style name from `fonts.*.style = "Light"`; beats plain
    /// weight/style matching.
    pub style_name: Option<&'a str>,
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
    /// Instance coordinates the reopened base face cannot reproduce.
    instance: Option<Vec<(u32, f32)>>,
    postscript: Option<String>,
    full_name: String,
}

/// Load every face of `family_name` from `source` into `candidates`.
///
/// The direct `select_family_by_name` lookup runs first because it
/// matches every name a family carries, localized names and English
/// aliases alike, while `all_families` reports only the first name of
/// each family; CJK fonts like "Source Han Mono SC" were invisible
/// under their English name (#1466). The case-insensitive scan over
/// `all_families` stays as a fallback for sources whose direct lookup
/// is case-sensitive.
#[cfg(not(target_arch = "wasm32"))]
fn family_candidates(
    source: &dyn font_kit::source::Source,
    family_name: &str,
    family_name_lower: &str,
    candidates: &mut Vec<FontCandidate>,
) {
    let family_handle = source.select_family_by_name(family_name).ok().or_else(|| {
        let families = source.all_families().ok()?;
        let name = families
            .iter()
            .find(|f| f.to_lowercase() == family_name_lower)?;
        source.select_family_by_name(name).ok()
    });
    let Some(family_handle) = family_handle else {
        tracing::debug!("no family match for '{family_name}'");
        return;
    };
    // Patterns of one family share files; read each at most once here.
    let mut files: std::collections::HashMap<PathBuf, Option<Vec<u8>>> =
        std::collections::HashMap::new();
    for handle in family_handle.fonts() {
        if let Ok(font) = handle.load() {
            let props = font.properties();
            let mut weight = Weight(props.weight.0 as u16);
            let mut instance = None;
            // A reopened file only gives its default instance, so recover the
            // real one: fontconfig hides the 1-based `fvar` id in the high
            // bits of `FC_INDEX`; DirectWrite reports properties at index 0.
            if let font_kit::handle::Handle::Path { path, font_index } = handle.clone() {
                let data = files
                    .entry(path.clone())
                    .or_insert_with(|| read_font_file(family_name, &path));
                if let Some(data) = data.as_deref() {
                    let face = font_index & 0xFFFF;
                    let want_italic = props.style != font_kit::properties::Style::Normal;
                    match font_index >> 16 {
                        0 => {
                            instance =
                                match_instance(data, face, weight.0 as f32, want_italic)
                        }
                        instance_id => {
                            if let Some(coords) =
                                named_instance_coords(data, face, instance_id - 1)
                            {
                                if let Some(w) = fvar_wght(&coords) {
                                    weight = Weight(
                                        w.round().clamp(0.0, u16::MAX as f32) as u16,
                                    );
                                }
                                instance = pin_coords(&coords);
                            }
                        }
                    }
                }
            }
            tracing::debug!(
                "found candidate: weight={}, stretch={:?}, style={:?}",
                weight.0,
                props.stretch,
                props.style
            );
            candidates.push(FontCandidate {
                handle: handle.clone(),
                weight,
                stretch: Stretch::from_font_kit(props.stretch),
                style: Style::from_font_kit(props.style),
                instance,
                postscript: font.postscript_name(),
                full_name: font.full_name(),
            });
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn read_font_file(family_name: &str, path: &std::path::Path) -> Option<Vec<u8>> {
    std::fs::read(path)
        .inspect_err(|err| {
            tracing::debug!("cannot read '{family_name}' at {}: {err}", path.display())
        })
        .ok()
}

/// One instance axis: (tag big-endian `u32`, user coordinate, default).
type AxisTriple = (u32, f32, f32);

/// User-space coordinates of named instance `instance_index`, in axis
/// order; `None` for static fonts or a bad index.
#[cfg(not(target_arch = "wasm32"))]
fn named_instance_coords(
    data: &[u8],
    face_index: u32,
    instance_index: u32,
) -> Option<Vec<AxisTriple>> {
    let font = skrifa::FontRef::from_index(data, face_index).ok()?;
    let axes = skrifa::AxisCollection::new(&font);
    let instance =
        skrifa::NamedInstanceCollection::new(&font).get(instance_index as usize)?;
    Some(instance_triples(&axes, &instance))
}

#[cfg(not(target_arch = "wasm32"))]
fn instance_triples(
    axes: &skrifa::AxisCollection<'_>,
    instance: &skrifa::NamedInstance<'_>,
) -> Vec<AxisTriple> {
    axes.iter()
        .zip(instance.user_coords())
        .map(|(axis, coord)| {
            (
                u32::from_be_bytes(axis.tag().to_be_bytes()),
                coord,
                axis.default_value(),
            )
        })
        .collect()
}

/// The `wght` coordinate among `coords`, if the font has that axis.
#[cfg(not(target_arch = "wasm32"))]
fn fvar_wght(coords: &[AxisTriple]) -> Option<f32> {
    let wght = u32::from_be_bytes(*b"wght");
    coords
        .iter()
        .find(|(tag, ..)| *tag == wght)
        .map(|&(_, value, _)| value)
}

/// Only the coordinates off their axis default; `None` for the file
/// default itself.
#[cfg(not(target_arch = "wasm32"))]
fn pin_coords(coords: &[AxisTriple]) -> Option<Vec<(u32, f32)>> {
    let pinned: Vec<(u32, f32)> = coords
        .iter()
        .filter(|(_, value, default)| (value - default).abs() > 0.01)
        .map(|(tag, value, _)| (*tag, *value))
        .collect();
    (!pinned.is_empty()).then_some(pinned)
}

/// Whether coordinates lean on a slant/italic axis.
#[cfg(not(target_arch = "wasm32"))]
fn triple_italic(coords: &[AxisTriple]) -> bool {
    coords.iter().any(|&(tag, value, default)| {
        let tag = tag.to_be_bytes();
        (tag == *b"ital" && value > default + 0.5) || (tag == *b"slnt" && value < default)
    })
}

/// The instance `want_wght`/`want_italic` describe. DirectWrite reports
/// per-instance properties but keeps every instance at the base index.
#[cfg(not(target_arch = "wasm32"))]
fn match_instance(
    data: &[u8],
    face_index: u32,
    want_wght: f32,
    want_italic: bool,
) -> Option<Vec<(u32, f32)>> {
    let font = skrifa::FontRef::from_index(data, face_index).ok()?;
    let axes = skrifa::AxisCollection::new(&font);
    if axes.is_empty() {
        return None;
    }
    let instances = skrifa::NamedInstanceCollection::new(&font);
    let mut best: Option<(f32, Vec<AxisTriple>)> = None;
    for i in 0..instances.len() {
        let Some(instance) = instances.get(i) else {
            continue;
        };
        let coords = instance_triples(&axes, &instance);
        if triple_italic(&coords) != want_italic {
            continue;
        }
        let dist = (fvar_wght(&coords).unwrap_or(want_wght) - want_wght).abs();
        if best.as_ref().is_none_or(|(best_dist, _)| dist < *best_dist) {
            best = Some((dist, coords));
        }
    }
    pin_coords(best?.1.as_slice())
}

/// Exact style-name match: PostScript name suffix or trailing full-name
/// words, e.g. "JetBrainsMono-ExtraLight" answers to "ExtraLight".
#[cfg(not(target_arch = "wasm32"))]
fn style_name_matches(cand: &FontCandidate, want: &str) -> bool {
    fn normalize(s: &str) -> String {
        s.chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    }
    let want = normalize(want);
    if want.is_empty() {
        return false;
    }
    if let Some((_, suffix)) = cand
        .postscript
        .as_deref()
        .and_then(|ps| ps.rsplit_once('-'))
    {
        return normalize(suffix) == want;
    }
    let words: Vec<&str> = cand.full_name.split_whitespace().collect();
    (1..=words.len())
        .any(|tail| normalize(words[words.len() - tail..].join(" ").as_str()) == want)
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
            for additional_source in self.additional_sources.iter() {
                if candidates.is_empty() {
                    family_candidates(
                        additional_source,
                        family_name_str,
                        &family_name_lower,
                        &mut candidates,
                    );
                }
            }

            // step 2: system fonts
            if candidates.is_empty() {
                family_candidates(
                    &self.system_source,
                    family_name_str,
                    &family_name_lower,
                    &mut candidates,
                );
            }

            tracing::debug!("Total candidates found: {}", candidates.len());

            // Step 3: an exact style-name request wins over CSS matching.
            let chosen = query.style_name.and_then(|want| {
                let index = candidates
                    .iter()
                    .position(|c| style_name_matches(c, want))?;
                tracing::debug!("style name '{want}' matched candidate {index}");
                Some(index)
            });
            let chosen = chosen.or_else(|| find_best_match(&candidates, query));
            if let Some(index) = chosen {
                tracing::debug!("Best match selected at index {}", index);
                return Some(ID::from_handle(
                    candidates[index].handle.clone(),
                    MatchedFace {
                        weight: candidates[index].weight.0 as f32,
                        coords: candidates[index].instance.clone().unwrap_or_default(),
                    },
                ));
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
                    // Try to find the actual file path for this font
                    if let Some(path) = find_font_path_from_data(&bytes) {
                        tracing::debug!(
                            "face_source: Found file path for memory font: {}",
                            path.display()
                        );
                        return Some((Source::File(path), font_index));
                    }
                    // Fallback to binary if path not found
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

#[cfg(target_os = "macos")]
const SYSTEM_FONT_DIRS: &[&str] = &[
    "/Library/Fonts",
    "/System/Library/Fonts",
    "/System/Library/AssetsV2",
    "/Network/Library/Fonts",
];

#[cfg(target_os = "windows")]
const SYSTEM_FONT_DIRS: &[&str] = &[
    // Note: actual paths resolved at runtime using environment variables
];

#[cfg(all(unix, not(target_os = "macos")))]
const SYSTEM_FONT_DIRS: &[&str] = &["/usr/share/fonts", "/usr/local/share/fonts"];

#[cfg(not(target_arch = "wasm32"))]
fn get_font_name(face: &ttf_parser::Face) -> Option<String> {
    face.names()
        .into_iter()
        .find(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME && n.is_unicode())
        .and_then(|n| n.to_string())
        .or_else(|| {
            face.names()
                .into_iter()
                .find(|n| n.name_id == ttf_parser::name_id::FAMILY && n.is_unicode())
                .and_then(|n| n.to_string())
        })
}

#[cfg(not(target_arch = "wasm32"))]
fn get_font_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = SYSTEM_FONT_DIRS.iter().map(PathBuf::from).collect();

    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join("Library/Fonts"));
    }

    #[cfg(target_os = "windows")]
    {
        // System fonts
        if let Some(windir) = std::env::var_os("SYSTEMROOT") {
            dirs.push(PathBuf::from(windir).join("Fonts"));
        }
        // User fonts
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            let profile = PathBuf::from(profile);
            dirs.push(profile.join("AppData/Local/Microsoft/Windows/Fonts"));
            dirs.push(profile.join("AppData/Roaming/Microsoft/Windows/Fonts"));
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".fonts"));
        dirs.push(home.join(".local/share/fonts"));
    }

    dirs
}

// find the file path for a font given its binary data.
// parses the font to get its PostScript name, then searches system directories.
#[cfg(not(target_arch = "wasm32"))]
fn find_font_path_from_data(data: &[u8]) -> Option<PathBuf> {
    use memmap2::Mmap;
    use std::fs::File;
    use walkdir::WalkDir;

    // Parse font to get its name
    let face = ttf_parser::Face::parse(data, 0).ok()?;
    let target_name = get_font_name(&face)?;
    let target_name_lower = target_name.to_lowercase();

    tracing::debug!("find_font_path_from_data: searching for '{}'", target_name);

    // Search each directory for the font file
    for dir in get_font_dirs() {
        if !dir.exists() {
            continue;
        }

        for entry in WalkDir::new(&dir).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            // Check extension
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase());

            match ext.as_deref() {
                Some("ttf") | Some("otf") | Some("ttc") | Some("otc") => {}
                _ => continue,
            }

            // Use memory mapping for efficient file access
            let Ok(file) = File::open(path) else {
                continue;
            };
            let Ok(mmap) = (unsafe { Mmap::map(&file) }) else {
                continue;
            };

            // Handle font collections (TTC/OTC) - check all faces
            let face_count = ttf_parser::fonts_in_collection(&mmap).unwrap_or(1);
            for index in 0..face_count {
                if let Ok(file_face) = ttf_parser::Face::parse(&mmap, index) {
                    if let Some(file_name) = get_font_name(&file_face) {
                        if file_name.to_lowercase() == target_name_lower {
                            tracing::debug!(
                                "find_font_path_from_data: found match at {}",
                                path.display()
                            );
                            return Some(path.to_path_buf());
                        }
                    }
                }
            }
        }
    }

    tracing::debug!(
        "find_font_path_from_data: no file found for '{}'",
        target_name
    );
    None
}
