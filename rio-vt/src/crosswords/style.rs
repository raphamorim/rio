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
    /// Bumped whenever the id→style mapping changes (new intern, slot
    /// reuse, sweep). Only meaningful within this instance: two
    /// `StyleSet`s can carry equal revisions with different content, so
    /// a consumer caching a copy keyed by revision must re-copy on any
    /// frame that can follow a switch of the visible instance.
    /// `snapshot_visible` does that by copying unconditionally on
    /// full-damage frames; every instance switch (`swap_alt`,
    /// `reset_state`, resize) marks fully damaged.
    revision: u64,
}

const MEMO_BITS: u32 = 10;
const MEMO_SLOTS: usize = 1 << MEMO_BITS;

/// `pack_style` output never sets bits above 111 (flags occupy 96..112),
/// so this can never collide with a real key. Freed slots hold it so a
/// stale memo candidate verifies against it and misses.
const TOMBSTONE_KEY: u128 = 1 << 127;

/// One `Grid::reclaim_styles` mark-and-sweep per this many novel
/// interns. The mark walks every cell of the ring (rows carry no
/// has-styles hint), so the cadence is set high enough that the
/// amortized cost per novel style stays at a few cell reads; sessions
/// that never intern this many distinct styles never sweep. Counting
/// misses that failed at the id cap keeps the cadence — and therefore
/// recovery — alive even while interns fall back to the default style.
const STYLE_SWEEP_CADENCE: usize = 16384;

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
            revision: 1,
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
        self.revision += 1;
        id
    }

    /// Whether the caller should run a mark-and-sweep before the next
    /// intern: the novel-intern cadence elapsed. Purely cadence-driven so
    /// a sweep that frees nothing (every id genuinely live) cannot
    /// re-trigger on the very next intern.
    #[inline]
    pub fn should_sweep(&self) -> bool {
        self.novel_since_sweep >= STYLE_SWEEP_CADENCE
    }

    /// Free every non-default id whose bit is unset in `live`, making it
    /// reusable by later interns. `live` is a bitset indexed by id;
    /// callers mark every id still referenced by a cell or cursor
    /// template before calling. Freed slots keep the default style so a
    /// stale read renders defensively rather than garbage.
    pub fn sweep_unmarked(&mut self, live: &[u64]) {
        self.novel_since_sweep = 0;
        let mut freed = false;
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
            freed = true;
        }
        if freed {
            self.revision += 1;
        }
    }

    /// Current revision of the id→style mapping. Stable while no new
    /// style is interned and no sweep runs. Per-instance only — see the
    /// field doc for the cross-instance contract.
    #[inline]
    pub fn revision(&self) -> u64 {
        self.revision
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
        let red_id = set.intern(red);
        let blue_id = set.intern(blue);
        let rev = set.revision();

        // Keep red alive, sweep blue.
        let mut live = vec![0u64; (u16::MAX as usize + 1).div_ceil(64)];
        live[0] |= 1;
        live[red_id as usize / 64] |= 1u64 << (red_id % 64);
        set.sweep_unmarked(&live);
        assert_ne!(set.revision(), rev);

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
    fn sweep_cadence_counts_novel_interns() {
        use crate::config::colors::ColorRgb;
        let mut set = StyleSet::new();
        assert!(!set.should_sweep());
        for i in 0..STYLE_SWEEP_CADENCE {
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
        let live = vec![u64::MAX; (u16::MAX as usize + 1).div_ceil(64)];
        set.sweep_unmarked(&live);
        assert!(!set.should_sweep());
    }

    #[test]
    fn revision_changes_only_on_mutation() {
        let mut set = StyleSet::new();
        let s = Style {
            fg: AnsiColor::Named(NamedColor::Red),
            ..Style::default()
        };
        let rev0 = set.revision();
        set.intern(s);
        let rev1 = set.revision();
        assert_ne!(rev0, rev1);
        set.intern(s);
        assert_eq!(set.revision(), rev1);
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
