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

use std::sync::OnceLock;
use unicode_width::grapheme::{grapheme_class, is_break, BreakState, GraphemeClass};

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every table entry must agree with the reference implementation
    /// it was built from — including the class table's fallback seam.
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

    #[test]
    fn start_state_matches_reference() {
        for class in 0..CLASSES as u8 {
            let reference = BreakState::start(GraphemeClass::from_u8(class).unwrap());
            assert_eq!(start_state(class), reference.pack(), "start({class})");
        }
    }
}
