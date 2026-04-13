// Per-grid style intern table for cells.
//
// Each unique combination of (fg, bg, underline_color, sgr_flags) is hashed
// and assigned a `StyleId: u16`. The cell stores only the id; the actual
// style data lives in `StyleSet::styles`. This means:
//
//   - Most cells share `style_id == 0` (the default style) and require no
//     lookup at render time.
//   - A row of 200 characters with the same SGR state shares one id.
//   - The `Square` struct stays at u64.

use crate::config::colors::{AnsiColor, NamedColor};
use bitflags::bitflags;
use rustc_hash::FxHashMap;

/// Index into the per-grid `StyleSet`. Id `0` is always the default style
/// (`Style::default()`), so a freshly-zeroed cell renders correctly without
/// any lookup.
pub type StyleId = u16;

/// The id of the default style. Always present.
pub const DEFAULT_STYLE_ID: StyleId = 0;

bitflags! {
    /// SGR-related cell attributes that live inside the style table.
    /// Distinct from the per-cell flags that stay on `Square` (wide,
    /// wrapline, presence bits).
    #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
    pub struct StyleFlags: u16 {
        const INVERSE          = 1 << 0;
        const BOLD             = 1 << 1;
        const ITALIC           = 1 << 2;
        const DIM              = 1 << 3;
        const HIDDEN           = 1 << 4;
        const STRIKEOUT        = 1 << 5;
        // 3-bit underline kind packed into bits 6-8.
        const UNDERLINE        = 1 << 6;
        const DOUBLE_UNDERLINE = 1 << 7;
        const UNDERCURL        = 1 << 8;
        const DOTTED_UNDERLINE = 1 << 9;
        const DASHED_UNDERLINE = 1 << 10;
        const ALL_UNDERLINES   = Self::UNDERLINE.bits()
                               | Self::DOUBLE_UNDERLINE.bits()
                               | Self::UNDERCURL.bits()
                               | Self::DOTTED_UNDERLINE.bits()
                               | Self::DASHED_UNDERLINE.bits();
        // Combined intensity for shaping decisions.
        const DIM_BOLD         = Self::DIM.bits() | Self::BOLD.bits();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Style {
    pub fg: AnsiColor,
    pub bg: AnsiColor,
    pub underline_color: Option<AnsiColor>,
    pub flags: StyleFlags,
}

impl Default for Style {
    #[inline]
    fn default() -> Self {
        Self {
            fg: AnsiColor::Named(NamedColor::Foreground),
            bg: AnsiColor::Named(NamedColor::Background),
            underline_color: None,
            flags: StyleFlags::empty(),
        }
    }
}

/// Interning table for `Style` values, owned per-grid.
#[derive(Clone, Debug)]
pub struct StyleSet {
    styles: Vec<Style>,
    lookup: FxHashMap<Style, StyleId>,
}

impl PartialEq for StyleSet {
    fn eq(&self, other: &Self) -> bool {
        // Two style sets are considered equal if they intern the same set
        // of styles in the same id order. Used by snapshot diffing.
        self.styles == other.styles
    }
}

impl StyleSet {
    /// Create a new style set with the default style pre-interned at id 0.
    pub fn new() -> Self {
        let default_style = Style::default();
        let mut lookup = FxHashMap::default();
        lookup.insert(default_style, DEFAULT_STYLE_ID);
        Self {
            styles: vec![default_style],
            lookup,
        }
    }

    /// Look up the style for an id. Returns the default style for unknown
    /// ids (defensive — should never happen in practice).
    ///
    /// Hot path note: this is called once per cell during rendering, so the
    /// `id == 0` (default style) check is intentionally inlined first. The
    /// overwhelming majority of cells in a typical terminal use the default
    /// style; that branch becomes a single compare + copy of a constant.
    #[inline(always)]
    pub fn get(&self, id: StyleId) -> Style {
        if id == DEFAULT_STYLE_ID {
            return Style::default();
        }
        // Safety: ids are only ever produced by `intern`, which guarantees
        // they index into `self.styles`. The bounds check is provably dead
        // on the hot path but the optimizer doesn't always remove it.
        // We still fall back to the default if the slot is somehow gone.
        self.styles
            .get(id as usize)
            .copied()
            .unwrap_or_else(Style::default)
    }

    /// Unchecked variant of `get`. Skips both the default-style early
    /// return AND the bounds check on `self.styles`. Used by the renderer
    /// hot loop after the caller has already verified the id is non-zero
    /// and in range (which is always true for ids produced by `intern`).
    ///
    /// # Safety
    /// `id` must be a valid index into `self.styles` (i.e. less than
    /// `self.len()`). Ids returned by `intern` always satisfy this.
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, id: StyleId) -> Style {
        debug_assert!(
            (id as usize) < self.styles.len(),
            "StyleSet::get_unchecked called with out-of-range id {} (len {})",
            id,
            self.styles.len(),
        );
        *self.styles.get_unchecked(id as usize)
    }

    /// Intern a style and return its id. If the style already exists,
    /// returns the existing id. If not, inserts it.
    ///
    /// Saturates at `u16::MAX` styles per grid: any attempt to intern beyond
    /// that returns `DEFAULT_STYLE_ID`. In practice rio sessions use < 100
    /// distinct styles so this is purely defensive.
    pub fn intern(&mut self, style: Style) -> StyleId {
        if let Some(&id) = self.lookup.get(&style) {
            return id;
        }
        if self.styles.len() >= u16::MAX as usize {
            tracing::warn!(
                "StyleSet hit u16::MAX styles ({}); falling back to default",
                self.styles.len()
            );
            return DEFAULT_STYLE_ID;
        }
        let id = self.styles.len() as StyleId;
        self.styles.push(style);
        self.lookup.insert(style, id);
        id
    }

    /// Number of distinct styles currently interned.
    #[inline]
    pub fn len(&self) -> usize {
        self.styles.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.styles.is_empty()
    }
}

impl Default for StyleSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_style_at_id_zero() {
        let set = StyleSet::new();
        assert_eq!(set.get(DEFAULT_STYLE_ID), Style::default());
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn intern_returns_existing_id() {
        let mut set = StyleSet::new();
        let s = Style {
            fg: AnsiColor::Named(NamedColor::Red),
            ..Style::default()
        };
        let id1 = set.intern(s);
        let id2 = set.intern(s);
        assert_eq!(id1, id2);
        assert_ne!(id1, DEFAULT_STYLE_ID);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn distinct_styles_get_distinct_ids() {
        let mut set = StyleSet::new();
        let red = Style {
            fg: AnsiColor::Named(NamedColor::Red),
            ..Style::default()
        };
        let blue = Style {
            fg: AnsiColor::Named(NamedColor::Blue),
            ..Style::default()
        };
        assert_ne!(set.intern(red), set.intern(blue));
        assert_eq!(set.len(), 3);
    }
}
