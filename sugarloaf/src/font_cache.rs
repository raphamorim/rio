// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::font_introspector::Attributes;
use crate::sugarloaf::primitives::is_private_user_area;
use crate::SpanStyle;
use rustc_hash::FxHashMap;
use unicode_width::UnicodeWidthChar;

/// Unscaled horizontal advance for a glyph + the font's units-per-em,
/// stored together so callers can recover pixels at any font size:
/// `advance_units * font_size / units_per_em`. One cache entry survives
/// font-size changes.
#[derive(Debug, Clone, Copy)]
pub struct AdvanceInfo {
    pub advance_units: f32,
    pub units_per_em: u16,
}

impl AdvanceInfo {
    /// Scaled horizontal advance in pixels. Returns 0.0 when
    /// `units_per_em` is 0 (malformed/missing font metrics) so callers
    /// don't need a separate guard.
    #[inline]
    pub fn scaled(&self, font_size: f32) -> f32 {
        if self.units_per_em > 0 {
            self.advance_units * font_size / self.units_per_em as f32
        } else {
            0.0
        }
    }
}

/// Resolved glyph metadata: which font owns it, how wide it is in
/// terminal cells, whether it lives in a Unicode private-use area
/// (Nerd Font icons, custom symbol fonts, etc.), and — lazily — the
/// horizontal advance in the winning font's design units.
///
/// `advance` starts `None` and is filled only on the first
/// `char_advance` query for this `(char, attrs)`. The terminal grid
/// hot path doesn't need per-char pixel advances (it uses
/// `width` — cell count — for layout), so we don't pay for an hmtx /
/// upem read on every unique cell glyph.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedGlyph {
    pub font_id: usize,
    pub width: f32,
    pub is_pua: bool,
    pub advance: Option<AdvanceInfo>,
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

    /// Attach `advance` to the entry for `key`, if one exists. Used by
    /// `Sugarloaf::char_advance` after the first pixel-advance query
    /// for a `(char, attrs)` pair so subsequent queries can short-circuit.
    /// No-op when the entry hasn't been resolved yet (caller should
    /// call `resolve_with` first).
    #[inline]
    pub(crate) fn set_advance(&mut self, key: (char, Attributes), advance: AdvanceInfo) {
        if let Some(entry) = self.cache.get_mut(&key) {
            entry.advance = Some(advance);
        }
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

    let resolved = ResolvedGlyph {
        font_id,
        width,
        is_pua: is_private_user_area(&ch),
        advance: None,
    };
    cache.insert((ch, attrs), resolved);
    resolved
}

/// Compute the unscaled glyph advance for `ch` in the font registered
/// under `font_id`. Returns `None` when the font data isn't available
/// (font id unregistered or the SFNT bytes failed to parse); the
/// caller is responsible for picking a rendering fallback.
#[cfg(not(target_os = "macos"))]
pub(crate) fn compute_advance(
    font_ctx: &crate::font::FontLibraryData,
    font_id: usize,
    ch: char,
) -> Option<AdvanceInfo> {
    let (data, offset, _key) = font_ctx.get_data(&font_id)?;
    let font_ref = crate::font_introspector::FontRef::from_index(&data, offset as usize)?;
    let glyph_id = font_ref.charmap().map(ch as u32);
    let metrics = crate::font_introspector::GlyphMetrics::from_font(&font_ref, &[]);
    Some(AdvanceInfo {
        advance_units: metrics.advance_width(glyph_id),
        units_per_em: font_ref.metrics(&[]).units_per_em,
    })
}

/// macOS variant: derive the advance from CoreText without ever touching
/// the font's raw bytes. Matches Ghostty's bytes-free font handling on
/// mac.
#[cfg(target_os = "macos")]
pub(crate) fn compute_advance(
    font_ctx: &crate::font::FontLibraryData,
    font_id: usize,
    ch: char,
) -> Option<AdvanceInfo> {
    let font = font_ctx.inner.get(&font_id)?;
    let handle = if let Some(path) = font.path() {
        crate::font::macos::FontHandle::from_path(path)
    } else if let Some(bytes) = font.data() {
        crate::font::macos::FontHandle::from_bytes(bytes.as_ref())
    } else {
        None
    }?;
    let (advance_units, units_per_em) =
        crate::font::macos::advance_units_for_char(&handle, ch)?;
    Some(AdvanceInfo {
        advance_units,
        units_per_em,
    })
}
