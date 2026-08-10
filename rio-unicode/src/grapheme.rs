//! Grapheme cluster segmentation (UAX #29, extended grapheme clusters).
//!
//! The break decision is a pairwise state machine, the shape a terminal
//! needs: `is_break(prev, next, &mut state)` answers whether a cluster
//! boundary falls between two adjacent codepoints while the state
//! carries the three pieces of history single-pair rules can't see —
//! emoji ZWJ sequences (GB11), regional-indicator pair parity
//! (GB12/13), and Indic conjunct linkage (GB9c). Grid code appending
//! codepoints cell by cell can run it without ever materializing a
//! string; [`Graphemes`]/[`GraphemeIndices`] wrap the same machine for
//! string callers.
//!
//! Property data is generated from the same UCD release as the width
//! tables (`scripts/grapheme.py`), so segmentation and width can never
//! disagree about what Unicode says.

use crate::grapheme_tables::GRAPHEME_CLASS_RANGES;
pub use crate::grapheme_tables::GRAPHEME_UNICODE_VERSION;

/// Grapheme_Cluster_Break class, refined with the Extended_Pictographic
/// and InCB properties so every rule can run on classes alone. The
/// undocumented variants are the plain UAX #29 GCB classes.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum GraphemeClass {
    Other,
    CR,
    LF,
    Control,
    /// GCB=Extend without InCB=Extend.
    Extend,
    /// GCB=Extend with InCB=Extend: continues an Indic conjunct chain.
    ExtendIncb,
    /// GCB=Extend with InCB=Linker (viramas).
    Linker,
    Zwj,
    RegionalIndicator,
    Prepend,
    SpacingMark,
    L,
    V,
    T,
    LV,
    LVT,
    /// Extended_Pictographic (GCB=Other).
    ExtPic,
    /// InCB=Consonant (GCB=Other).
    Consonant,
}

/// Class lookup: binary search over the generated ranges.
pub fn grapheme_class(c: char) -> GraphemeClass {
    let cp = c as u32;
    match GRAPHEME_CLASS_RANGES.binary_search_by(|&(lo, hi, _)| {
        if cp < lo {
            core::cmp::Ordering::Greater
        } else if cp > hi {
            core::cmp::Ordering::Less
        } else {
            core::cmp::Ordering::Equal
        }
    }) {
        Ok(i) => GRAPHEME_CLASS_RANGES[i].2,
        Err(_) => GraphemeClass::Other,
    }
}

/// Emoji ZWJ sequence progress (GB11): `\p{ExtPic} Extend* ZWJ × \p{ExtPic}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum EmojiSeq {
    #[default]
    None,
    /// Saw Extended_Pictographic, possibly followed by Extends.
    Emoji,
    /// ...then a ZWJ: the very next ExtPic joins.
    EmojiZwj,
}

/// Indic conjunct progress (GB9c):
/// `Consonant [Extend Linker]* Linker [Extend Linker]* × Consonant`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Incb {
    #[default]
    None,
    /// Saw a consonant; no linker in the chain yet.
    Consonant,
    /// Consonant then a chain containing at least one linker: the next
    /// consonant joins.
    LinkerSeen,
}

/// Cross-pair segmentation state. Describes the codepoint run ending at
/// the `prev` side of the next [`is_break`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BreakState {
    emoji: EmojiSeq,
    incb: Incb,
    /// The regional-indicator run ending at `prev` has odd length.
    ri_odd: bool,
}

impl BreakState {
    /// State after the first codepoint of a sequence.
    pub fn start(first: GraphemeClass) -> Self {
        let mut state = Self::default();
        state.advance(first);
        state
    }

    fn advance(&mut self, class: GraphemeClass) {
        use GraphemeClass as G;
        self.emoji = match (self.emoji, class) {
            (_, G::ExtPic) => EmojiSeq::Emoji,
            (EmojiSeq::Emoji, G::Extend | G::ExtendIncb | G::Linker) => EmojiSeq::Emoji,
            (EmojiSeq::Emoji, G::Zwj) => EmojiSeq::EmojiZwj,
            _ => EmojiSeq::None,
        };
        self.incb = match (self.incb, class) {
            (_, G::Consonant) => Incb::Consonant,
            (Incb::Consonant, G::Linker) => Incb::LinkerSeen,
            (Incb::Consonant, G::ExtendIncb | G::Zwj) => Incb::Consonant,
            (Incb::LinkerSeen, G::Linker | G::ExtendIncb | G::Zwj) => Incb::LinkerSeen,
            _ => Incb::None,
        };
        self.ri_odd = match class {
            G::RegionalIndicator => !self.ri_odd,
            _ => false,
        };
    }
}

/// Whether an extended-grapheme-cluster boundary falls between `prev`
/// and `next`. `state` must describe the sequence ending at `prev`
/// (see [`BreakState::start`]); it is advanced past `next` before
/// returning, ready for the following pair.
// One branch per numbered UAX #29 rule, in spec order — merging the
// identical-bodied arms would make the machine harder to audit against
// the standard for zero behavioral gain.
#[allow(clippy::if_same_then_else)]
pub fn is_break(
    prev: GraphemeClass,
    next: GraphemeClass,
    state: &mut BreakState,
) -> bool {
    use GraphemeClass as G;
    let decision = {
        // GB3: CR × LF.
        if prev == G::CR && next == G::LF {
            false
        // GB4 / GB5: controls break on both sides.
        } else if matches!(prev, G::Control | G::CR | G::LF)
            || matches!(next, G::Control | G::CR | G::LF)
        {
            true
        // GB6-8: Hangul jamo.
        } else if prev == G::L && matches!(next, G::L | G::V | G::LV | G::LVT) {
            false
        } else if matches!(prev, G::LV | G::V) && matches!(next, G::V | G::T) {
            false
        } else if matches!(prev, G::LVT | G::T) && next == G::T {
            false
        // GB9 / GB9a / GB9b: extenders, spacing marks, prepends.
        } else if matches!(next, G::Extend | G::ExtendIncb | G::Linker | G::Zwj) {
            false
        } else if next == G::SpacingMark {
            false
        } else if prev == G::Prepend {
            false
        // GB9c: Indic conjuncts join across a linker chain.
        } else if next == G::Consonant && state.incb == Incb::LinkerSeen {
            false
        // GB11: emoji ZWJ sequences.
        } else if prev == G::Zwj && next == G::ExtPic && state.emoji == EmojiSeq::EmojiZwj
        {
            false
        // GB12/GB13: regional indicators pair up.
        } else if prev == G::RegionalIndicator
            && next == G::RegionalIndicator
            && state.ri_odd
        {
            false
        // GB999.
        } else {
            true
        }
    };
    state.advance(next);
    decision
}

/// Iterator over the extended grapheme clusters of a string.
#[derive(Debug, Clone)]
pub struct Graphemes<'a> {
    text: &'a str,
    offset: usize,
}

impl<'a> Graphemes<'a> {
    /// Segment `text` from its start.
    pub fn new(text: &'a str) -> Self {
        Self { text, offset: 0 }
    }
}

impl<'a> Iterator for Graphemes<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        let rest = &self.text[self.offset..];
        let mut chars = rest.char_indices();
        let (_, first) = chars.next()?;
        let mut prev = grapheme_class(first);
        let mut state = BreakState::start(prev);
        let end = loop {
            match chars.next() {
                Some((i, c)) => {
                    let class = grapheme_class(c);
                    if is_break(prev, class, &mut state) {
                        break i;
                    }
                    prev = class;
                }
                None => break rest.len(),
            }
        };
        let start = self.offset;
        self.offset += end;
        Some(&self.text[start..start + end])
    }
}

/// Like [`Graphemes`], yielding `(byte_offset, cluster)` pairs.
#[derive(Debug, Clone)]
pub struct GraphemeIndices<'a> {
    inner: Graphemes<'a>,
}

impl<'a> GraphemeIndices<'a> {
    /// Segment `text` from its start.
    pub fn new(text: &'a str) -> Self {
        Self {
            inner: Graphemes::new(text),
        }
    }
}

impl<'a> Iterator for GraphemeIndices<'a> {
    type Item = (usize, &'a str);

    fn next(&mut self) -> Option<(usize, &'a str)> {
        let offset = self.inner.offset;
        self.inner.next().map(|g| (offset, g))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec::Vec;

    fn clusters(s: &str) -> Vec<&str> {
        Graphemes::new(s).collect()
    }

    #[test]
    fn versions_stay_in_lockstep() {
        // The whole point of this crate: one Unicode version.
        assert_eq!(GRAPHEME_UNICODE_VERSION, crate::UNICODE_VERSION);
    }

    #[test]
    fn ascii_and_combining() {
        assert_eq!(clusters("abc"), ["a", "b", "c"]);
        assert_eq!(clusters("e\u{301}x"), ["e\u{301}", "x"]);
    }

    #[test]
    fn zwj_emoji_is_one_cluster() {
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        assert_eq!(clusters(family), [family]);
    }

    #[test]
    fn regional_indicators_pair() {
        // Two flags: BR + AR = four RIs, two clusters.
        let flags = "\u{1F1E7}\u{1F1F7}\u{1F1E6}\u{1F1F7}";
        assert_eq!(
            clusters(flags),
            ["\u{1F1E7}\u{1F1F7}", "\u{1F1E6}\u{1F1F7}"]
        );
    }

    #[test]
    fn crlf_and_controls() {
        assert_eq!(clusters("a\r\nb"), ["a", "\r\n", "b"]);
    }

    #[test]
    fn devanagari_conjunct_joins() {
        // ka + virama + ssa: GB9c keeps the conjunct together.
        assert_eq!(clusters("\u{915}\u{94D}\u{937}"), ["\u{915}\u{94D}\u{937}"]);
    }

    #[test]
    fn indices_report_byte_offsets() {
        let pairs: Vec<(usize, &str)> = GraphemeIndices::new("aé日").collect();
        assert_eq!(pairs, [(0, "a"), (1, "é"), (3, "日")]);
    }
}
