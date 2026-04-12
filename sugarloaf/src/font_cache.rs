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
/// terminal cells, and whether it lives in a Unicode private-use
/// area (Nerd Font icons, custom symbol fonts, etc.).
#[derive(Debug, Clone, Copy)]
pub struct ResolvedGlyph {
    pub font_id: usize,
    pub width: f32,
    pub is_pua: bool,
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

    let resolved = ResolvedGlyph {
        font_id,
        width,
        is_pua: is_private_user_area(&ch),
    };
    cache.insert((ch, attrs), resolved);
    resolved
}
