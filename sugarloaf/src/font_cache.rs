// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::font_introspector::Attributes;
use crate::sugarloaf::primitives::is_private_user_area;
use crate::SpanStyle;
use rustc_hash::FxHashMap;
use unicode_width::UnicodeWidthChar;

/// Resolved glyph metadata: which font owns it, how wide it is in
/// terminal cells, whether it lives in a Unicode private-use area
/// (Nerd Font icons, custom symbol fonts, etc.), and the horizontal
/// advance in the winning font's design units.
///
/// The advance is stored **unscaled** (font design units) together with
/// `units_per_em`, so one cache entry answers `char_advance` queries at
/// any font size via `advance_units * font_size / units_per_em`. Filled
/// at resolve time because `find_best_font_match` has already loaded
/// the winning font and consulted its cmap, so the extra hmtx lookup
/// is essentially free on top.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedGlyph {
    pub font_id: usize,
    pub width: f32,
    pub is_pua: bool,
    pub advance_units: f32,
    pub units_per_em: u16,
}

impl ResolvedGlyph {
    /// Scaled horizontal advance in pixels for the given font size.
    /// Returns 0.0 when `units_per_em` is 0 (malformed/missing font
    /// metrics) so callers don't need a separate guard.
    #[inline]
    pub fn scaled_advance(&self, font_size: f32) -> f32 {
        if self.units_per_em > 0 {
            self.advance_units * font_size / self.units_per_em as f32
        } else {
            0.0
        }
    }
}

/// Plain hash-map glyph cache. `get` is `&self` (no LRU promotion),
/// so callers can hold an immutable reference without borrow
/// conflicts. Unbounded — the working set of `(char, attrs)` pairs
/// in a terminal session is finite and small relative to memory.
pub(crate) struct FontCache {
    cache: FxHashMap<(char, Attributes), ResolvedGlyph>,
}

impl FontCache {
    pub fn new() -> Self {
        Self {
            cache: FxHashMap::default(),
        }
    }

    #[inline]
    pub fn get(&self, key: &(char, Attributes)) -> Option<&ResolvedGlyph> {
        self.cache.get(key)
    }

    #[inline]
    pub fn insert(&mut self, key: (char, Attributes), value: ResolvedGlyph) {
        self.cache.insert(key, value);
    }

    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

/// Resolve a single glyph: read from `cache` if present, otherwise
/// walk the fallback chain via `font_ctx` and store the result.
/// `font_ctx` is borrowed by the caller so multiple resolutions can
/// share one read-lock acquisition.
pub(crate) fn resolve_with(
    cache: &mut FontCache,
    font_ctx: &crate::font::FontLibraryData,
    ch: char,
    attrs: Attributes,
) -> ResolvedGlyph {
    if let Some(cached) = cache.get(&(ch, attrs)) {
        return *cached;
    }

    let style = SpanStyle {
        font_attrs: attrs,
        ..Default::default()
    };
    let mut width = ch.width().unwrap_or(1) as f32;
    let mut font_id = 0;
    if let Some((fid, is_emoji)) = font_ctx.find_best_font_match(ch, &style) {
        font_id = fid;
        if is_emoji {
            width = 2.0;
        }
    }

    // Pull the winning font's per-glyph advance and units-per-em so
    // proportional UI callers (tab titles, palette, hints) can answer
    // width queries at any font size via `ResolvedGlyph::scaled_advance`
    // with no extra font-data lookup. `find_best_font_match` has already
    // loaded the font and consulted its cmap — the added cost here is an
    // Arc clone + an hmtx read, dwarfed by the fallback walk.
    let (advance_units, units_per_em) =
        match font_ctx.get_data(&font_id) {
            Some((data, offset, _key)) => {
                let font_ref = crate::font_introspector::FontRef::from_index(
                    &data,
                    offset as usize,
                );
                match font_ref {
                    Some(font_ref) => {
                        let glyph_id = font_ref.charmap().map(ch as u32);
                        let metrics = crate::font_introspector::GlyphMetrics::from_font(
                            &font_ref,
                            &[],
                        );
                        let upem = font_ref.metrics(&[]).units_per_em;
                        (metrics.advance_width(glyph_id), upem)
                    }
                    None => (0.0, 0),
                }
            }
            None => (0.0, 0),
        };

    let resolved = ResolvedGlyph {
        font_id,
        width,
        is_pua: is_private_user_area(&ch),
        advance_units,
        units_per_em,
    };
    cache.insert((ch, attrs), resolved);
    resolved
}
