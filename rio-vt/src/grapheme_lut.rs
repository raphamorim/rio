//! Flat lookup tables for mode-2027 grapheme clustering.
//!
//! The ghostty devlog-006 treatment: the segmentation hot path must
//! not evaluate rule chains or binary-search property ranges per
//! codepoint. Two tables replace both:
//!
//! - a flat class table over `U+0000..U+20000` (the same BMP+plane-1
//!   window as [`codepoint_width`](crate::codepoint_width), covering
//!   CJK and emoji), falling back to the range search above it;
//! - a transition table folding the whole break decision into one
//!   index: `(prev_class, next_class, state) → (break?, next_state)`.
//!
//! Both are built at first use *by driving rio-unicode's reference
//! implementation*, so they are correct by construction and can never
//! drift from the conformance-tested rules.

use rio_unicode::grapheme::{grapheme_class, is_break, BreakState, GraphemeClass};
use std::sync::OnceLock;

const TABLE_LEN: usize = 0x2_0000;

static CLASS_TABLE: OnceLock<Box<[u8]>> = OnceLock::new();
static TRANSITIONS: OnceLock<Box<[u8]>> = OnceLock::new();

const CLASSES: usize = GraphemeClass::COUNT;
const STATES: usize = BreakState::COUNT;
/// Transition entry: bit 7 = break, bits 0..6 = packed next state.
const BREAK_BIT: u8 = 0x80;

fn class_table() -> &'static [u8] {
    CLASS_TABLE.get_or_init(|| {
        let mut table = vec![0u8; TABLE_LEN].into_boxed_slice();
        for (cp, slot) in table.iter_mut().enumerate() {
            if let Some(c) = char::from_u32(cp as u32) {
                *slot = grapheme_class(c) as u8;
            }
        }
        table
    })
}

fn transitions() -> &'static [u8] {
    TRANSITIONS.get_or_init(|| {
        let mut table = vec![0u8; CLASSES * CLASSES * STATES].into_boxed_slice();
        for prev in 0..CLASSES {
            let prev_class = GraphemeClass::from_u8(prev as u8).unwrap();
            for next in 0..CLASSES {
                let next_class = GraphemeClass::from_u8(next as u8).unwrap();
                for state in 0..STATES {
                    let mut break_state = BreakState::unpack(state as u8).unwrap();
                    let breaks = is_break(prev_class, next_class, &mut break_state);
                    table[(prev * CLASSES + next) * STATES + state] =
                        break_state.pack() | if breaks { BREAK_BIT } else { 0 };
                }
            }
        }
        table
    })
}

/// The grapheme class of `c` as a table index, one lookup for the
/// common window.
#[inline]
pub fn class_of(c: char) -> u8 {
    let cp = c as usize;
    if cp < TABLE_LEN {
        class_table()[cp]
    } else {
        grapheme_class(c) as u8
    }
}

/// One-index break decision: whether a boundary falls between a
/// codepoint of class `prev` and one of class `next` given the packed
/// `state`, which is advanced past `next` in place.
#[inline]
pub fn is_break_lut(prev: u8, next: u8, state: &mut u8) -> bool {
    let entry = transitions()
        [(prev as usize * CLASSES + next as usize) * STATES + *state as usize];
    *state = entry & !BREAK_BIT;
    entry & BREAK_BIT != 0
}

/// Packed state after the first codepoint of a sequence
/// (`BreakState::start` in table form): the advance half of any
/// transition depends only on the incoming class and prior state.
#[inline]
pub fn start_state(first: u8) -> u8 {
    let entry = transitions()[(first as usize) * STATES]; // prev=0, state=0
    entry & !BREAK_BIT
}

/// Measure the first grapheme cluster in `codepoints`: how many
/// codepoints it spans and how many terminal cells it occupies.
/// Returns `(len, width)`; `(0, 0)` for an empty slice.
///
/// Segmentation and width follow the same rules the mode-2027 input
/// path applies when printing: a variation selector flips the width
/// of a valid emoji base (U+FE0F wide, U+FE0E narrow) and is consumed
/// without effect anywhere else; any other width-bearing continuation
/// makes the whole cluster wide; zero-width continuations change
/// nothing.
///
/// This is not a streaming call: the slice must contain a complete
/// first cluster or the logical end of the text, since a continuation
/// arriving later would have joined it.
///
/// Values that are not Unicode scalars (surrogates, above U+10FFFF)
/// measure as one single-width codepoint when first and terminate the
/// cluster when later, so untrusted FFI input cannot wedge the walk.
pub fn cluster_width(codepoints: &[u32]) -> (usize, u8) {
    let Some(&first) = codepoints.first() else {
        return (0, 0);
    };
    let Some(base) = char::from_u32(first) else {
        return (1, 1);
    };
    let mut width = crate::codepoint_width::codepoint_width(first).unwrap_or(1);
    let mut prev = class_of(base);
    let mut state = start_state(prev);
    let mut last_cp = base;
    let mut len = 1;
    while len < codepoints.len() {
        let Some(c) = char::from_u32(codepoints[len]) else {
            break;
        };
        let class = class_of(c);
        let state_before = state;
        if is_break_lut(prev, class, &mut state) {
            break;
        }
        match c {
            '\u{FE0F}' | '\u{FE0E}' => {
                if crate::crosswords::vs_is_valid_base(last_cp, c) {
                    width = if c == '\u{FE0F}' { 2 } else { 1 };
                    prev = class;
                    last_cp = c;
                } else {
                    // Ignored selector: consumed, but it neither
                    // advances the state nor becomes the codepoint
                    // a later selector would judge against.
                    state = state_before;
                }
            }
            _ => {
                if crate::codepoint_width::codepoint_width(codepoints[len]).unwrap_or(0)
                    > 0
                {
                    width = 2;
                }
                prev = class;
                last_cp = c;
            }
        }
        len += 1;
    }
    (len, width)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every table entry must agree with the reference implementation
    /// it was built from, including the class table's fallback seam.
    #[test]
    fn tables_match_reference() {
        for cp in (0..TABLE_LEN as u32 + 0x100).step_by(7) {
            if let Some(c) = char::from_u32(cp) {
                assert_eq!(class_of(c), grapheme_class(c) as u8, "class of U+{cp:04X}");
            }
        }
        for prev in 0..CLASSES as u8 {
            for next in 0..CLASSES as u8 {
                for state in 0..STATES as u8 {
                    let mut reference = BreakState::unpack(state).unwrap();
                    let expected = is_break(
                        GraphemeClass::from_u8(prev).unwrap(),
                        GraphemeClass::from_u8(next).unwrap(),
                        &mut reference,
                    );
                    let mut packed = state;
                    let got = is_break_lut(prev, next, &mut packed);
                    assert_eq!(got, expected, "break ({prev},{next},{state})");
                    assert_eq!(packed, reference.pack(), "state ({prev},{next},{state})");
                }
            }
        }
    }

    /// One assertion per width rule: plain narrow/wide, combining
    /// marks, both selector directions, selector validity, ZWJ,
    /// modifiers, regional indicator pairing and FFI garbage.
    #[test]
    fn cluster_width_measures_first_cluster() {
        // (input, expected len, expected width)
        let cases: &[(&[u32], usize, u8)] = &[
            (&[], 0, 0),
            (&[0x61, 0x62], 1, 1),           // "ab": narrow, breaks
            (&[0x4E00], 1, 2),               // lone CJK: wide
            (&[0x65, 0x301, 0x62], 2, 1),    // e + combining acute
            (&[0x2764, 0xFE0F], 2, 2),       // text heart forced emoji
            (&[0x231A, 0xFE0E], 2, 1),       // emoji watch forced text
            (&[0x61, 0xFE0F], 2, 1),         // selector off base: ignored
            (&[0x61, 0xFE0F, 0x301], 3, 1),  // ignored selector keeps joining
            (&[0x31, 0xFE0F, 0x20E3], 3, 2), // keycap sequence
            (&[0x1F468, 0x200D, 0x1F33E], 3, 2), // ZWJ farmer
            (&[0x1F44D, 0x1F3FB], 2, 2),     // thumbs up + skin tone
            (&[0x1F1E7, 0x1F1F7, 0x1F1E7, 0x1F1F7], 2, 2), // flag pairs split
            (&[0x110000, 0x61], 1, 1),       // invalid first: one narrow cell
            (&[0x65, 0x301, 0xD800], 2, 1),  // invalid later: terminates
        ];
        for &(cps, len, width) in cases {
            assert_eq!(cluster_width(cps), (len, width), "input {cps:04X?}");
        }
    }

    #[test]
    fn start_state_matches_reference() {
        for class in 0..CLASSES as u8 {
            let reference = BreakState::start(GraphemeClass::from_u8(class).unwrap());
            assert_eq!(start_state(class), reference.pack(), "start({class})");
        }
    }
}
