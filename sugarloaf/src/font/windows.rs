//! Windows per-codepoint font discovery via font-kit walk.
//!
//! On Linux we get this via fontconfig's `FcFontSort` + `FcCharSet`
//! (a single C call returns sorted candidates). On Windows there's no
//! equivalent in our dep tree — `IDWriteFontFallback::MapCharacters`
//! would be the moral twin but lives in the `windows` / `dwrote` crates,
//! neither of which Debian ships, so we'd violate the workspace's
//! Debian-only constraint.
//!
//! The fallback approach here: ask font-kit for every installed font,
//! then mmap each and probe its CMAP for the codepoint via ttf-parser.
//! Slower per cold lookup than DirectWrite (we touch every font file
//! once until we hit a match), but cached at the font-cache layer so
//! the cost amortizes to zero after the first hit per codepoint per
//! session. Ghostty doesn't ship Windows discovery yet either, so this
//! still puts us ahead.
//!
//! Future replacement path: when either `windows` or `dwrote` lands in
//! Debian, swap the body of `discover_fallback` for a single
//! `IDWriteFontFallback::MapCharacters` call and delete the walk.

use std::path::PathBuf;
use std::sync::OnceLock;

use font_kit::handle::Handle;
use font_kit::source::SystemSource;
use memmap2::Mmap;

/// Cached list of every installed font path + collection index.
/// Built once on first miss; subsequent misses iterate in-memory only.
/// `Vec<(PathBuf, u32)>` rather than `Vec<Handle>` so we don't hold
/// onto font-kit's Memory variant (would defeat the lazy mmap).
static SYSTEM_FONTS: OnceLock<Vec<(PathBuf, u32)>> = OnceLock::new();

fn system_font_index() -> &'static [(PathBuf, u32)] {
    SYSTEM_FONTS
        .get_or_init(|| {
            let source = SystemSource::new();
            source
                .all_fonts()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|h| match h {
                    Handle::Path { path, font_index } => Some((path, font_index)),
                    Handle::Memory { .. } => None,
                })
                .collect()
        })
        .as_slice()
}

/// Find a font file installed on the system that contains `ch`.
///
/// The signature mirrors `font/linux.rs::discover_fallback` so the
/// cross-platform call site in `FontLibrary::resolve_font_for_char`
/// stays platform-uniform. Style hints are accepted but ignored on
/// this path — picking the right weight/italic among multiple matches
/// would require reading each candidate's `OS/2` table, and the
/// payoff is small for fallback glyphs (CJK, emoji, symbols are
/// usually single-weight families anyway).
pub fn discover_fallback(
    _primary_family: &str,
    ch: char,
    _want_mono: bool,
    _want_bold: bool,
    _want_italic: bool,
) -> Option<(PathBuf, u32)> {
    for (path, face_index) in system_font_index() {
        if face_contains_char(path, *face_index, ch) {
            return Some((path.clone(), *face_index));
        }
    }
    None
}

/// `true` if the font at `(path, face_index)` declares a glyph for
/// `ch`. mmap + parse + CMAP lookup; fast in practice (< 1 ms per
/// font) because ttf-parser only walks the table directory and the
/// CMAP, not the full font.
fn face_contains_char(path: &PathBuf, face_index: u32, ch: char) -> bool {
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let Ok(mmap) = (unsafe { Mmap::map(&file) }) else {
        return false;
    };
    let Ok(face) = ttf_parser::Face::parse(&mmap, face_index) else {
        return false;
    };
    face.glyph_index(ch).is_some()
}
