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
        // One-hot underline kind in bits 6-10; at most one is set.
        const UNDERLINE        = 1 << 6;
        const DOUBLE_UNDERLINE = 1 << 7;
        const UNDERCURL        = 1 << 8;
        const DOTTED_UNDERLINE = 1 << 9;
        const DASHED_UNDERLINE = 1 << 10;
        const SLOW_BLINK       = 1 << 11;
        const RAPID_BLINK      = 1 << 12;
        const ALL_BLINK        = Self::SLOW_BLINK.bits() | Self::RAPID_BLINK.bits();
        const ALL_UNDERLINES   = Self::UNDERLINE.bits()
                               | Self::DOUBLE_UNDERLINE.bits()
                               | Self::UNDERCURL.bits()
                               | Self::DOTTED_UNDERLINE.bits()
                               | Self::DASHED_UNDERLINE.bits();
        // Combined intensity for shaping decisions.
        const DIM_BOLD         = Self::DIM.bits() | Self::BOLD.bits();
    }
}

/// The five one-hot underline kinds behind `StyleFlags` bits 6-10.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnderlineKind {
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

impl UnderlineKind {
    /// The one-hot `StyleFlags` bit for this kind.
    #[inline]
    pub fn flag(self) -> StyleFlags {
        match self {
            UnderlineKind::Single => StyleFlags::UNDERLINE,
            UnderlineKind::Double => StyleFlags::DOUBLE_UNDERLINE,
            UnderlineKind::Curly => StyleFlags::UNDERCURL,
            UnderlineKind::Dotted => StyleFlags::DOTTED_UNDERLINE,
            UnderlineKind::Dashed => StyleFlags::DASHED_UNDERLINE,
        }
    }
}

impl StyleFlags {
    /// Replace the underline kind (`None` removes it), keeping the kind
    /// bits one-hot. The write side of `underline_kind`.
    #[inline]
    pub fn set_underline(&mut self, kind: Option<UnderlineKind>) {
        self.remove(StyleFlags::ALL_UNDERLINES);
        if let Some(kind) = kind {
            self.insert(kind.flag());
        }
    }

    /// The underline kind these flags carry, or `None` without one. The
    /// kind bits are one-hot (`set_underline` keeps them so); the
    /// priority order here settles any corrupt state.
    #[inline]
    pub fn underline_kind(self) -> Option<UnderlineKind> {
        if self.contains(StyleFlags::UNDERLINE) {
            Some(UnderlineKind::Single)
        } else if self.contains(StyleFlags::DOUBLE_UNDERLINE) {
            Some(UnderlineKind::Double)
        } else if self.contains(StyleFlags::UNDERCURL) {
            Some(UnderlineKind::Curly)
        } else if self.contains(StyleFlags::DOTTED_UNDERLINE) {
            Some(UnderlineKind::Dotted)
        } else if self.contains(StyleFlags::DASHED_UNDERLINE) {
            Some(UnderlineKind::Dashed)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Style {
    pub fg: AnsiColor,
    pub bg: AnsiColor,
    pub underline_color: Option<AnsiColor>,
    pub flags: StyleFlags,
}

impl Style {
    /// Whether this style renders anything on a BLANK cell: a
    /// non-default background, an underline, a strikeout, or inverse
    /// (which paints the fg color as the cell's ground). Fg-only
    /// attributes (color, bold, dim, italic, hidden, blink) draw
    /// nothing without a glyph. The single authority for
    /// serialization trimming; renderers adding a new visible-on-blank
    /// attribute (an overline, say) must extend this, or snapshots
    /// will silently trim cells that render.
    #[inline]
    pub fn renders_on_blank(&self) -> bool {
        self.bg != AnsiColor::Named(NamedColor::Background)
            || self.flags.underline_kind().is_some()
            || self
                .flags
                .intersects(StyleFlags::STRIKEOUT | StyleFlags::INVERSE)
    }
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
    /// Lossless 128-bit packing of each interned style, parallel to
    /// `styles`. Memo verification and hashmap lookups compare/hash this
    /// single word instead of walking `Style`'s enum fields.
    packed: Vec<u128>,
    lookup: FxHashMap<u128, StyleId>,
    /// Direct-mapped cache of candidate ids fronting `lookup`; slots are
    /// verified against `packed`, so a stale slot misses, never lies.
    memo: [StyleId; MEMO_SLOTS],
    /// Ids freed by `sweep_unmarked`, reused before growing `styles`.
    free: Vec<StyleId>,
    /// Novel interns (lookup misses, including ones that failed at the
    /// id cap) since the last sweep. Drives the sweep cadence.
    novel_since_sweep: usize,
    /// Novel interns required before the next sweep. Re-armed by each
    /// sweep to the number of ids it freed (with a floor), so a sweep
    /// that reclaimed plenty allows the next one as soon as those ids
    /// are used up, instead of stranding novel styles on the default
    /// fallback for the remainder of a fixed cadence.
    novel_needed: usize,
}

const MEMO_BITS: u32 = 10;
const MEMO_SLOTS: usize = 1 << MEMO_BITS;

/// `pack_style` output never sets bits above 111 (flags occupy 96..112),
/// so this can never collide with a real key. Freed slots hold it so a
/// stale memo candidate verifies against it and misses.
const TOMBSTONE_KEY: u128 = 1 << 127;

/// Lower bound for the novel-intern count between sweeps. The mark
/// walks every cell of the ring's styled rows (`Row::has_styles`), so
/// a style-heavy scrollback can still mean a large walk per sweep; the
/// floor bounds how often a near-fruitless sweep can recur. Counting
/// misses that failed at the id cap keeps sweeps, and therefore
/// recovery, coming even while interns fall back to the default style.
const SWEEP_MIN_NOVEL: usize = 4096;

/// No sweep runs while the table is smaller than this, no matter the
/// cadence: sessions with modest style sets never pay the ring walk,
/// and after a spike the trailing-tombstone truncation can drop the
/// table back below it.
const SWEEP_HIGH_WATER: usize = 32768;

/// Pack a style into a unique `u128`: 32 bits per color (tag + payload),
/// 32 for the optional underline color, the flag bits on top. Injective,
/// so equality of packings IS equality of styles.
#[inline]
fn pack_style(style: &Style) -> u128 {
    #[inline]
    fn color_key(c: AnsiColor) -> u32 {
        match c {
            AnsiColor::Named(n) => 0x0100_0000 | (n as u32),
            AnsiColor::Indexed(i) => 0x0200_0000 | (i as u32),
            AnsiColor::Spec(rgb) => {
                0x0400_0000 | ((rgb.r as u32) << 16 | (rgb.g as u32) << 8 | rgb.b as u32)
            }
        }
    }
    let underline = match style.underline_color {
        None => 0u32,
        Some(c) => 0x0800_0000 | color_key(c),
    };
    (color_key(style.fg) as u128)
        | (color_key(style.bg) as u128) << 32
        | (underline as u128) << 64
        | (style.flags.bits() as u128) << 96
}

/// Direct-mapped slot for a packed style: a cheap multiply-fold.
#[inline]
fn memo_index(key: u128) -> usize {
    let folded = (key as u64) ^ (key >> 64) as u64;
    let h = (folded as u32 ^ (folded >> 32) as u32).wrapping_mul(0x9E37_79B9);
    (h >> (32 - MEMO_BITS)) as usize
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
        let key = pack_style(&default_style);
        let mut lookup = FxHashMap::default();
        lookup.insert(key, DEFAULT_STYLE_ID);
        Self {
            styles: vec![default_style],
            packed: vec![key],
            lookup,
            memo: [DEFAULT_STYLE_ID; MEMO_SLOTS],
            free: Vec::new(),
            novel_since_sweep: 0,
            novel_needed: SWEEP_MIN_NOVEL,
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

    /// Intern a style and return its id. If the style already exists,
    /// returns the existing id. If not, inserts it, reusing a swept id
    /// when one is free.
    ///
    /// Saturates at `u16::MAX` styles per grid: any attempt to intern beyond
    /// that returns `DEFAULT_STYLE_ID`. `Grid::reclaim_styles` sweeps
    /// unreferenced ids before the cap is reached, so hitting it means the
    /// grid genuinely displays 65k distinct styles at once.
    pub fn intern(&mut self, style: Style) -> StyleId {
        let key = pack_style(&style);
        let slot = memo_index(key);
        let cand = self.memo[slot];
        if let Some(&packed) = self.packed.get(cand as usize) {
            if packed == key {
                return cand;
            }
        }
        let id = self.intern_slow(style, key);
        self.memo[slot] = id;
        id
    }

    fn intern_slow(&mut self, style: Style, key: u128) -> StyleId {
        if let Some(&id) = self.lookup.get(&key) {
            return id;
        }
        self.novel_since_sweep += 1;
        let id = match self.free.pop() {
            Some(id) => {
                self.styles[id as usize] = style;
                self.packed[id as usize] = key;
                id
            }
            None => {
                if self.styles.len() >= u16::MAX as usize {
                    tracing::warn!(
                        "StyleSet hit u16::MAX styles ({}); falling back to default",
                        self.styles.len()
                    );
                    return DEFAULT_STYLE_ID;
                }
                let id = self.styles.len() as StyleId;
                self.styles.push(style);
                self.packed.push(key);
                id
            }
        };
        self.lookup.insert(key, id);
        id
    }

    /// Whether the caller should run a mark-and-sweep before the next
    /// intern: enough novel interns since the last sweep, freed ids ran
    /// out, and the table is past the high-water mark. The re-armed
    /// `novel_needed` floor keeps a sweep that frees nothing (every id
    /// genuinely live) from re-triggering on the very next intern; at
    /// full id-cap saturation the pressure escape retries on the
    /// smaller fixed floor instead, so a large scaled floor cannot
    /// stretch the window where novel styles fall back to the default.
    #[inline]
    pub fn should_sweep(&self) -> bool {
        if !self.free.is_empty() {
            return false;
        }
        let under_pressure = self.styles.len() >= u16::MAX as usize
            && self.novel_since_sweep >= SWEEP_MIN_NOVEL;
        under_pressure
            || (self.novel_since_sweep >= self.novel_needed
                && self.styles.len() >= SWEEP_HIGH_WATER)
    }

    /// Free every non-default id whose bit is unset in `live`, making it
    /// reusable by later interns. `live` is a bitset indexed by id;
    /// callers mark every id still referenced by a cell or cursor
    /// template before calling, and pass `min_novel` to scale the
    /// re-arm floor with the cost of the mark walk. Freed slots keep
    /// the default style so a stale read renders defensively rather
    /// than garbage.
    pub fn sweep_unmarked(&mut self, live: &[u64], min_novel: usize) {
        self.novel_since_sweep = 0;
        let mut freed = 0usize;
        for id in 1..self.styles.len() {
            if live[id / 64] & (1 << (id % 64)) != 0 {
                continue;
            }
            let key = self.packed[id];
            if key == TOMBSTONE_KEY {
                continue;
            }
            self.lookup.remove(&key);
            self.packed[id] = TOMBSTONE_KEY;
            self.styles[id] = Style::default();
            self.free.push(id as StyleId);
            freed += 1;
        }
        self.novel_needed = freed.max(min_novel).max(SWEEP_MIN_NOVEL);
        // Drop trailing tombstoned slots so a one-off style spike doesn't
        // permanently inflate every later table copy. Ids past the new
        // length resolve through `get`'s default fallback, and a stale
        // memo candidate past the length fails the `packed.get` verify.
        // Trailing tombstones can only be this sweep's own frees (each
        // sweep truncates all of them), so `freed > 0` covers the pops.
        while self.styles.len() > 1 && *self.packed.last().unwrap() == TOMBSTONE_KEY {
            self.styles.pop();
            self.packed.pop();
        }
        if freed > 0 {
            self.free.retain(|&id| (id as usize) < self.styles.len());
        }
    }

    #[inline]
    pub fn styles(&self) -> &[Style] {
        &self.styles
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
    fn sweep_frees_and_reuses_ids() {
        let mut set = StyleSet::new();
        let red = Style {
            fg: AnsiColor::Named(NamedColor::Red),
            ..Style::default()
        };
        let blue = Style {
            fg: AnsiColor::Named(NamedColor::Blue),
            ..Style::default()
        };
        // Blue first so the swept slot is interior (red stays live at the
        // tail) and exercises tombstone + reuse rather than truncation.
        let blue_id = set.intern(blue);
        let red_id = set.intern(red);

        // Keep red alive, sweep blue.
        let mut live = vec![0u64; crate::crosswords::grid::ID_BITSET_WORDS];
        live[red_id as usize / 64] |= 1u64 << (red_id % 64);
        set.sweep_unmarked(&live, 0);

        // Red survives with its id; blue's slot reads as default.
        assert_eq!(set.intern(red), red_id);
        assert_eq!(set.get(blue_id), Style::default());

        // The freed id is reused before the table grows.
        let len = set.len();
        let green = Style {
            fg: AnsiColor::Named(NamedColor::Green),
            ..Style::default()
        };
        assert_eq!(set.intern(green), blue_id);
        assert_eq!(set.get(blue_id), green);
        assert_eq!(set.len(), len);

        // Re-interning the swept style allocates a fresh id.
        assert_ne!(set.intern(blue), blue_id);
    }

    #[test]
    fn sweep_truncates_trailing_tombstones() {
        let mut set = StyleSet::new();
        let red = Style {
            fg: AnsiColor::Named(NamedColor::Red),
            ..Style::default()
        };
        let blue = Style {
            fg: AnsiColor::Named(NamedColor::Blue),
            ..Style::default()
        };
        let red_id = set.intern(red);
        let blue_id = set.intern(blue);

        // Keep red alive; blue is the trailing slot and gets truncated.
        let mut live = vec![0u64; crate::crosswords::grid::ID_BITSET_WORDS];
        live[red_id as usize / 64] |= 1u64 << (red_id % 64);
        set.sweep_unmarked(&live, 0);
        assert_eq!(set.len(), 2);
        assert_eq!(set.get(blue_id), Style::default());
        assert_eq!(set.get(red_id), red);

        // Fresh interns regrow from the truncated end.
        let green = Style {
            fg: AnsiColor::Named(NamedColor::Green),
            ..Style::default()
        };
        assert_eq!(set.intern(green), blue_id);
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn sweep_cadence_counts_novel_interns() {
        use crate::config::colors::ColorRgb;
        let mut set = StyleSet::new();
        assert!(!set.should_sweep());
        // Past both gates: enough novel interns AND past the high-water
        // table size, with no freed ids available.
        for i in 0..SWEEP_HIGH_WATER {
            set.intern(Style {
                fg: AnsiColor::Spec(ColorRgb {
                    r: (i >> 8) as u8,
                    g: (i & 0xFF) as u8,
                    b: 0,
                }),
                ..Style::default()
            });
        }
        assert!(set.should_sweep());

        // A sweep resets the cadence even when nothing is freed, so a
        // fully-live table can't re-trigger a sweep per intern.
        let live = vec![u64::MAX; crate::crosswords::grid::ID_BITSET_WORDS];
        set.sweep_unmarked(&live, 0);
        assert!(!set.should_sweep());
    }

    #[test]
    fn sweep_rearm_respects_min_novel_floor() {
        use crate::config::colors::ColorRgb;
        fn distinct(i: usize) -> Style {
            Style {
                fg: AnsiColor::Spec(ColorRgb {
                    r: (i & 0xFF) as u8,
                    g: ((i >> 8) & 0xFF) as u8,
                    b: ((i >> 16) & 0xFF) as u8,
                }),
                ..Style::default()
            }
        }

        let mut set = StyleSet::new();
        for i in 0..SWEEP_HIGH_WATER {
            set.intern(distinct(i));
        }

        // Everything live, nothing freed: the caller-provided floor
        // re-arms the cadence.
        let live = vec![u64::MAX; crate::crosswords::grid::ID_BITSET_WORDS];
        set.sweep_unmarked(&live, 9_999);
        assert!(!set.should_sweep());
        for i in 0..9_998 {
            set.intern(distinct(SWEEP_HIGH_WATER + i));
        }
        assert!(!set.should_sweep());
        set.intern(distinct(SWEEP_HIGH_WATER + 9_998));
        assert!(set.should_sweep());
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
