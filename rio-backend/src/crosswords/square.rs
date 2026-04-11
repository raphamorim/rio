// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// In rio it has been rewritten as a packed `u64`. The previous design held
// `c`, `fg`, `bg`, `flags`, and an `Option<Arc<CellExtra>>` inline (24 bytes
// per cell + per-cell heap allocations for "extras"). All variable-sized
// data now lives in per-grid side tables (`StyleSet`, `ExtrasTable`); the
// cell itself is 8 bytes.

use crate::crosswords::grid::GridSquare;
use crate::crosswords::style::{StyleId, DEFAULT_STYLE_ID};
use crate::crosswords::Column;
use crate::crosswords::Row;
use bitflags::bitflags;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Bit layout for Square(u64)
//
//   bits  0..20 (21): codepoint (Unicode scalar value, max 0x10_FFFF)
//                     OR low bits of bg color when content_tag != Codepoint
//   bits 21..22 (2):  wide                              (Wide enum)
//   bits 23..29 (7):  per-cell flag bits (CellFlags), incl WRAPLINE at bit 0
//   bits 30..31 (2):  content_tag (NEW)
//                       0 = Codepoint        (text cell, use style_id below)
//                       1 = BgPalette        (bg-only cell, palette index in 32..39)
//                       2 = BgRgb            (bg-only cell, RGB packed in 32..55)
//                       3 = reserved
//   bits 32..47 (16): style_id      (when tag == Codepoint)
//                     bg palette idx in low 8 (when tag == BgPalette)
//                     bg RGB.r:g    in low 16  (when tag == BgRgb)
//   bits 48..63 (16): extras_id     (when tag == Codepoint)
//                     bg RGB.b      in low 8   (when tag == BgRgb)
//
// The bg-only encoding (BgPalette / BgRgb) is the Ghostty trick: cells that
// represent a colored background with no text don't need a style table
// lookup at all, which is the dominant cost for large filled regions
// (selection, padding, blank lines after `clear`, color blocks).
// ---------------------------------------------------------------------------

const CODEPOINT_SHIFT: u64 = 0;
const CODEPOINT_MASK: u64 = (1 << 21) - 1;

const WIDE_SHIFT: u64 = 21;
const WIDE_MASK: u64 = 0b11 << WIDE_SHIFT;

const CELL_FLAGS_SHIFT: u64 = 23;
const CELL_FLAGS_MASK: u64 = 0x7F << CELL_FLAGS_SHIFT; // 7 bits incl WRAPLINE

const CONTENT_TAG_SHIFT: u64 = 30;

const STYLE_ID_SHIFT: u64 = 32;
const STYLE_ID_MASK: u64 = 0xFFFF << STYLE_ID_SHIFT;

const EXTRAS_ID_SHIFT: u64 = 48;
const EXTRAS_ID_MASK: u64 = 0xFFFF << EXTRAS_ID_SHIFT;

// Bg-color slot reuse (only valid when content_tag != Codepoint).
const BG_PALETTE_SHIFT: u64 = 32;
const BG_PALETTE_MASK: u64 = 0xFF << BG_PALETTE_SHIFT;

const BG_RGB_R_SHIFT: u64 = 32;
const BG_RGB_G_SHIFT: u64 = 40;
const BG_RGB_B_SHIFT: u64 = 48;

/// Wide-character state for a cell. Encoded in 2 bits.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Wide {
    /// Normal single-cell character.
    Narrow = 0,
    /// First cell of a double-wide character.
    Wide = 1,
    /// Second cell of a double-wide character.
    Spacer = 2,
    /// Trailing spacer at end of a soft-wrapped line indicating a wide
    /// character continues on the next line.
    LeadingSpacer = 3,
}

impl Wide {
    #[inline]
    fn from_bits(bits: u64) -> Wide {
        match (bits >> WIDE_SHIFT) & 0b11 {
            0 => Wide::Narrow,
            1 => Wide::Wide,
            2 => Wide::Spacer,
            _ => Wide::LeadingSpacer,
        }
    }
}

/// Discriminator for what the cell's payload represents. Default is
/// `Codepoint`, the standard text cell. The bg-only variants are an
/// optimization for cells that carry only a background color (selection,
/// padding, color rectangles, blank lines after `clear`) — they encode the
/// color directly in the cell, skipping the style table entirely.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContentTag {
    /// Standard text cell. Codepoint in bits 0..20, style_id in bits 32..47.
    Codepoint = 0,
    /// Bg-only cell with a palette-indexed background.
    /// Palette index in bits 32..39.
    BgPalette = 1,
    /// Bg-only cell with an RGB background.
    /// RGB packed in bits 32..55 (R, G, B).
    BgRgb = 2,
}

impl ContentTag {
    /// Decode the content tag from a raw `Square` u64. Public so render
    /// hot loops can read the cell once and dispatch on the tag without
    /// going through the `Square::content_tag()` method (which reloads
    /// the cell from memory if the optimizer can't prove it aliases).
    #[inline(always)]
    pub fn from_bits(bits: u64) -> ContentTag {
        match (bits >> CONTENT_TAG_SHIFT) & 0b11 {
            0 => ContentTag::Codepoint,
            1 => ContentTag::BgPalette,
            _ => ContentTag::BgRgb,
        }
    }
}

bitflags! {
    /// Per-cell flags that DON'T live in the style table. SGR-related
    /// attributes (bold, italic, underline, etc.) live in `StyleFlags`.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct CellFlags: u8 {
        /// Soft-wrap continuation marker on the last cell of a wrapped line.
        const WRAPLINE         = 1 << 0;
        /// Cell carries graphics data (sixel / iTerm2 inline image piece).
        /// Look up the actual graphic in `Grid::extras_table` via `extras_id`.
        const GRAPHICS         = 1 << 1;
        /// Cell carries hyperlink metadata. Lookup via extras_id.
        const HYPERLINK        = 1 << 2;
        /// Cell carries multi-codepoint grapheme cluster. Lookup via extras_id.
        const GRAPHEME         = 1 << 3;
    }
}

/// Counter for hyperlinks without explicit ID.
static HYPERLINK_ID_SUFFIX: AtomicU32 = AtomicU32::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Hyperlink {
    inner: Arc<HyperlinkInner>,
}

impl Hyperlink {
    pub fn new<T: ToString>(id: Option<T>, uri: T) -> Self {
        let inner = Arc::new(HyperlinkInner::new(id, uri));
        Self { inner }
    }

    pub fn id(&self) -> &str {
        &self.inner.id
    }

    pub fn uri(&self) -> &str {
        &self.inner.uri
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct HyperlinkInner {
    id: String,
    uri: String,
}

impl HyperlinkInner {
    pub fn new<T: ToString>(id: Option<T>, uri: T) -> Self {
        let id = match id {
            Some(id) => id.to_string(),
            None => {
                let mut id = HYPERLINK_ID_SUFFIX
                    .fetch_add(1, Ordering::Relaxed)
                    .to_string();
                id.push_str("_rio");
                id
            }
        };

        Self {
            id,
            uri: uri.to_string(),
        }
    }
}

/// Index into `Grid::extras_table`. `0` means "no extras".
pub type ExtrasId = u16;

/// Storage for the rare per-cell data that used to live inside `CellExtra`.
/// Allocated only for cells that need it; pooled in a `Vec` on the grid.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct Extras {
    pub zerowidth: Vec<char>,
    pub hyperlink: Option<Hyperlink>,
    pub graphic: Option<crate::ansi::graphics::GraphicsCell>,
}

impl Extras {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.zerowidth.is_empty()
            && self.hyperlink.is_none()
            && self.graphic.is_none()
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Square(u64);

impl Default for Square {
    #[inline]
    fn default() -> Square {
        Square(0)
    }
}

impl Square {
    /// Create a cell with the given codepoint and the default style/extras.
    #[inline]
    pub fn from_char(c: char) -> Self {
        let mut s = Square(0);
        s.set_c(c);
        s
    }

    /// Read the underlying packed bits. Used by render hot loops that want
    /// to extract multiple fields from a single cell load — calling the
    /// individual accessors would otherwise reload the cell from memory
    /// each time the optimizer can't prove they alias.
    #[inline(always)]
    pub fn raw(self) -> u64 {
        self.0
    }

    #[inline]
    pub fn c(self) -> char {
        let cp = ((self.0 >> CODEPOINT_SHIFT) & CODEPOINT_MASK) as u32;
        // Safety: we only ever store valid Unicode scalar values via set_c.
        char::from_u32(cp).unwrap_or('\0')
    }

    #[inline]
    pub fn set_c(&mut self, c: char) {
        let cp = c as u32 as u64;
        debug_assert!(cp <= CODEPOINT_MASK, "codepoint exceeds 21 bits");
        self.0 = (self.0 & !CODEPOINT_MASK) | ((cp & CODEPOINT_MASK) << CODEPOINT_SHIFT);
    }

    #[inline]
    pub fn wide(self) -> Wide {
        Wide::from_bits(self.0)
    }

    #[inline]
    pub fn set_wide(&mut self, w: Wide) {
        self.0 = (self.0 & !WIDE_MASK) | ((w as u64) << WIDE_SHIFT);
    }

    #[inline]
    pub fn cell_flags(self) -> CellFlags {
        let bits = ((self.0 & CELL_FLAGS_MASK) >> CELL_FLAGS_SHIFT) as u8;
        CellFlags::from_bits_truncate(bits)
    }

    #[inline]
    pub fn set_cell_flags(&mut self, f: CellFlags) {
        self.0 =
            (self.0 & !CELL_FLAGS_MASK) | ((f.bits() as u64) << CELL_FLAGS_SHIFT);
    }

    #[inline]
    pub fn insert_cell_flag(&mut self, f: CellFlags) {
        let mut cur = self.cell_flags();
        cur.insert(f);
        self.set_cell_flags(cur);
    }

    #[inline]
    pub fn remove_cell_flag(&mut self, f: CellFlags) {
        let mut cur = self.cell_flags();
        cur.remove(f);
        self.set_cell_flags(cur);
    }

    #[inline]
    pub fn contains_cell_flag(self, f: CellFlags) -> bool {
        self.cell_flags().contains(f)
    }

    /// Read the raw style id bits.
    ///
    /// **The caller must check `content_tag()` first.** For non-Codepoint
    /// cells the upper 32 bits hold the bg color encoding, not a style id.
    /// We deliberately do not branch on the tag here so the renderer hot
    /// loop stays branchless when it has already established the cell is
    /// a Codepoint cell.
    #[inline(always)]
    pub fn style_id(self) -> StyleId {
        ((self.0 & STYLE_ID_MASK) >> STYLE_ID_SHIFT) as StyleId
    }

    #[inline]
    pub fn set_style_id(&mut self, id: StyleId) {
        self.0 = (self.0 & !STYLE_ID_MASK) | ((id as u64) << STYLE_ID_SHIFT);
    }

    /// Read the cell's extras id, if any.
    ///
    /// **The caller must check `content_tag()` first** if the cell might be
    /// a bg-only cell — those reuse the upper 32 bits for the bg color, so
    /// `extras_id()` would return garbage. For Codepoint cells (the
    /// overwhelming majority) this is a single bit extract.
    #[inline(always)]
    pub fn extras_id(self) -> Option<ExtrasId> {
        let id = ((self.0 & EXTRAS_ID_MASK) >> EXTRAS_ID_SHIFT) as ExtrasId;
        if id == 0 {
            None
        } else {
            Some(id)
        }
    }

    #[inline]
    pub fn set_extras_id(&mut self, id: Option<ExtrasId>) {
        let bits = id.unwrap_or(0) as u64;
        self.0 = (self.0 & !EXTRAS_ID_MASK) | (bits << EXTRAS_ID_SHIFT);
    }

    #[inline]
    pub fn content_tag(self) -> ContentTag {
        ContentTag::from_bits(self.0)
    }

    /// Convert this cell into a bg-only cell holding a palette-indexed
    /// background. Drops codepoint, wide, style_id, and extras_id; preserves
    /// per-cell flags (so e.g. `WRAPLINE` survives a clear-to-end-of-line).
    #[inline]
    pub fn set_bg_palette(&mut self, idx: u8) {
        let preserved = self.0 & CELL_FLAGS_MASK;
        self.0 = preserved
            | ((ContentTag::BgPalette as u64) << CONTENT_TAG_SHIFT)
            | ((idx as u64) << BG_PALETTE_SHIFT);
    }

    /// Convert this cell into a bg-only cell holding an RGB background.
    #[inline]
    pub fn set_bg_rgb(&mut self, r: u8, g: u8, b: u8) {
        let preserved = self.0 & CELL_FLAGS_MASK;
        self.0 = preserved
            | ((ContentTag::BgRgb as u64) << CONTENT_TAG_SHIFT)
            | ((r as u64) << BG_RGB_R_SHIFT)
            | ((g as u64) << BG_RGB_G_SHIFT)
            | ((b as u64) << BG_RGB_B_SHIFT);
    }

    /// Read the inline-encoded palette index. Only meaningful when
    /// `content_tag() == BgPalette` — call after the tag check.
    #[inline]
    pub fn bg_palette_index(self) -> u8 {
        ((self.0 & BG_PALETTE_MASK) >> BG_PALETTE_SHIFT) as u8
    }

    /// Read the inline-encoded RGB. Only meaningful when
    /// `content_tag() == BgRgb`.
    #[inline]
    pub fn bg_rgb(self) -> (u8, u8, u8) {
        (
            ((self.0 >> BG_RGB_R_SHIFT) & 0xFF) as u8,
            ((self.0 >> BG_RGB_G_SHIFT) & 0xFF) as u8,
            ((self.0 >> BG_RGB_B_SHIFT) & 0xFF) as u8,
        )
    }

    /// True if this cell encodes its background inline (no text content,
    /// no style table lookup needed). Used by the renderer hot loop.
    #[inline]
    pub fn is_bg_only(self) -> bool {
        !matches!(self.content_tag(), ContentTag::Codepoint)
    }

    /// Clear all per-cell state. Used by `clear_wide` and similar.
    #[inline]
    pub fn clear(&mut self) {
        *self = Square(0);
    }

    /// Reset the cell using a template. Preserves the template's style id;
    /// drops any extras the cell currently has (caller must free the slot).
    #[inline]
    pub fn reset(&mut self, template: Square) {
        let new = Square(0)
            .with_style_id(template.style_id());
        *self = new;
    }

    /// Builder helper for tests.
    #[inline]
    pub fn with_style_id(mut self, id: StyleId) -> Self {
        self.set_style_id(id);
        self
    }

    #[inline]
    pub fn is_default(self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub fn is_wide(self) -> bool {
        matches!(self.wide(), Wide::Wide)
    }

    #[inline]
    pub fn is_spacer(self) -> bool {
        matches!(self.wide(), Wide::Spacer)
    }

    #[inline]
    pub fn is_leading_spacer(self) -> bool {
        matches!(self.wide(), Wide::LeadingSpacer)
    }

    #[inline]
    pub fn wrapline(self) -> bool {
        self.contains_cell_flag(CellFlags::WRAPLINE)
    }

    #[inline]
    pub fn set_wrapline(&mut self, on: bool) {
        if on {
            self.insert_cell_flag(CellFlags::WRAPLINE);
        } else {
            self.remove_cell_flag(CellFlags::WRAPLINE);
        }
    }

    #[inline]
    pub fn has_extras(self) -> bool {
        self.extras_id().is_some()
    }

    #[inline]
    pub fn has_grapheme(self) -> bool {
        self.contains_cell_flag(CellFlags::GRAPHEME)
    }

    #[inline]
    pub fn has_hyperlink(self) -> bool {
        self.contains_cell_flag(CellFlags::HYPERLINK)
    }

    #[inline]
    pub fn has_graphics(self) -> bool {
        self.contains_cell_flag(CellFlags::GRAPHICS)
    }
}

impl GridSquare for Square {
    #[inline]
    fn is_empty(&self) -> bool {
        if self.0 == 0 {
            return true;
        }
        // A cell with an inline bg is NOT empty even if everything else
        // looks default.
        if self.is_bg_only() {
            return false;
        }
        (self.c() == '\0' || self.c() == '\t')
            && self.style_id() == DEFAULT_STYLE_ID
            && self.extras_id().is_none()
            && !self.contains_cell_flag(CellFlags::WRAPLINE)
            && matches!(self.wide(), Wide::Narrow)
    }

    #[inline]
    fn reset(&mut self, template: &Self) {
        let style_id = template.style_id();
        *self = Square(0).with_style_id(style_id);
    }
}

pub trait LineLength {
    /// Calculate the occupied line length.
    fn line_length(&self) -> Column;
}

impl LineLength for Row<Square> {
    fn line_length(&self) -> Column {
        let mut length = Column(0);

        if self[Column(self.len() - 1)].wrapline() {
            return Column(self.len());
        }

        for (index, cell) in self[..].iter().rev().enumerate() {
            if cell.c() != '\0' || cell.has_extras() {
                length = Column(self.len() - index);
                break;
            }
        }

        length
    }
}

pub trait ResetDiscriminant<T> {
    /// Value based on which equality for the reset will be determined.
    fn discriminant(&self) -> T;
}

impl<T: Copy> ResetDiscriminant<T> for T {
    fn discriminant(&self) -> T {
        *self
    }
}

impl ResetDiscriminant<StyleId> for Square {
    fn discriminant(&self) -> StyleId {
        self.style_id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::mem;

    use crate::crosswords::grid::row::Row;
    use crate::crosswords::pos::Column;

    #[test]
    fn square_is_eight_bytes() {
        // The whole point of this rewrite.
        assert_eq!(mem::size_of::<Square>(), 8);
    }

    #[test]
    fn codepoint_round_trip() {
        let mut s = Square(0);
        s.set_c('🦀');
        assert_eq!(s.c(), '🦀');
        s.set_c('a');
        assert_eq!(s.c(), 'a');
        s.set_c('\0');
        assert_eq!(s.c(), '\0');
    }

    #[test]
    fn style_id_round_trip() {
        let mut s = Square(0);
        s.set_style_id(42);
        assert_eq!(s.style_id(), 42);
        s.set_style_id(0xFFFF);
        assert_eq!(s.style_id(), 0xFFFF);
    }

    #[test]
    fn extras_id_round_trip() {
        let mut s = Square(0);
        assert_eq!(s.extras_id(), None);
        s.set_extras_id(Some(7));
        assert_eq!(s.extras_id(), Some(7));
        s.set_extras_id(None);
        assert_eq!(s.extras_id(), None);
    }

    #[test]
    fn wide_round_trip() {
        let mut s = Square(0);
        for w in [Wide::Narrow, Wide::Wide, Wide::Spacer, Wide::LeadingSpacer] {
            s.set_wide(w);
            assert_eq!(s.wide(), w);
        }
    }

    #[test]
    fn cell_flags_round_trip() {
        let mut s = Square(0);
        s.insert_cell_flag(CellFlags::WRAPLINE | CellFlags::GRAPHEME);
        assert!(s.wrapline());
        assert!(s.has_grapheme());
        assert!(!s.has_hyperlink());
        s.remove_cell_flag(CellFlags::WRAPLINE);
        assert!(!s.wrapline());
        assert!(s.has_grapheme());
    }

    #[test]
    fn fields_are_independent() {
        let mut s = Square(0);
        s.set_c('Z');
        s.set_style_id(0x1234);
        s.set_extras_id(Some(0x5678));
        s.set_wide(Wide::Wide);
        s.insert_cell_flag(CellFlags::WRAPLINE);
        assert_eq!(s.c(), 'Z');
        assert_eq!(s.style_id(), 0x1234);
        assert_eq!(s.extras_id(), Some(0x5678));
        assert_eq!(s.wide(), Wide::Wide);
        assert!(s.wrapline());
    }

    #[test]
    fn bg_palette_round_trip() {
        let mut s = Square(0);
        s.set_bg_palette(42);
        assert_eq!(s.content_tag(), ContentTag::BgPalette);
        assert!(s.is_bg_only());
        assert_eq!(s.bg_palette_index(), 42);
        // bg-only cells implicitly use the default style
        assert_eq!(s.style_id(), DEFAULT_STYLE_ID);
        // and have no codepoint / extras
        assert_eq!(s.c(), '\0');
        assert_eq!(s.extras_id(), None);
    }

    #[test]
    fn bg_rgb_round_trip() {
        let mut s = Square(0);
        s.set_bg_rgb(0x12, 0x34, 0x56);
        assert_eq!(s.content_tag(), ContentTag::BgRgb);
        assert!(s.is_bg_only());
        assert_eq!(s.bg_rgb(), (0x12, 0x34, 0x56));
        assert_eq!(s.style_id(), DEFAULT_STYLE_ID);
        assert_eq!(s.c(), '\0');
    }

    #[test]
    fn bg_only_preserves_wrapline() {
        let mut s = Square(0);
        s.set_wrapline(true);
        s.set_bg_rgb(1, 2, 3);
        assert!(s.wrapline());
        assert_eq!(s.bg_rgb(), (1, 2, 3));
    }

    #[test]
    fn bg_only_cells_are_not_empty() {
        let mut s = Square(0);
        s.set_bg_palette(7);
        assert!(!<Square as crate::crosswords::grid::GridSquare>::is_empty(&s));
    }

    #[test]
    fn line_length_works() {
        let mut row = Row::<Square>::new(10);
        row[Column(5)].set_c('a');
        assert_eq!(row.line_length(), Column(6));
    }

    #[test]
    fn line_length_works_with_wrapline() {
        let mut row = Row::<Square>::new(10);
        row[Column(9)].set_wrapline(true);
        assert_eq!(row.line_length(), Column(10));
    }
}
