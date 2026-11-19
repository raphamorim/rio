//! Fast codepoint width lookup.
//!
//! Flat table indexed by `u32` codepoint over the BMP plus plane 1
//! (`U+0000..U+20000`, so CJK and emoji both hit it), computed once at
//! first use, used for the bulk per-codepoint width queries the parser
//! emits via [`Handler::input_codepoints`]. The table is 128 KiB of `u8`
//! and amortises to zero on subsequent calls.
//!
//! For codepoints above plane 1 we fall back to a scalar
//! [`UnicodeWidthChar::width`] call; those are rare in real terminal
//! traffic.
//!
//! Encoding in the table:
//! - `0xFF` → width undefined (control / surrogate / unassigned).
//! - `0` / `1` / `2` → cell width.
//!
//! [`Handler::input_codepoints`]: crate::performer::handler::Handler::input_codepoints

use rio_unicode::UnicodeWidthChar;
use std::sync::OnceLock;

const TABLE_LEN: usize = 0x2_0000;
const SENTINEL_NONE: u8 = 0xFF;

static WIDTH_TABLE: OnceLock<Box<[u8]>> = OnceLock::new();

/// The width lookup table. Callers doing bulk lookups should grab this
/// once and use [`width_in`], hoisting the `OnceLock` access out of
/// their loop.
#[inline]
pub fn width_table() -> &'static [u8] {
    WIDTH_TABLE.get_or_init(build_table)
}

fn build_table() -> Box<[u8]> {
    let mut table = vec![SENTINEL_NONE; TABLE_LEN].into_boxed_slice();
    for cp in 0..TABLE_LEN as u32 {
        if let Some(c) = char::from_u32(cp) {
            if let Some(w) = UnicodeWidthChar::width(c) {
                table[cp as usize] = w as u8;
            }
        }
    }
    table
}

/// Lookup the cell width for a codepoint through an already-resolved
/// table reference (see [`width_table`]).
#[inline]
pub fn width_in(table: &[u8], cp: u32) -> Option<u8> {
    if (cp as usize) < TABLE_LEN {
        match table[cp as usize] {
            SENTINEL_NONE => None,
            w => Some(w),
        }
    } else {
        let c = char::from_u32(cp)?;
        UnicodeWidthChar::width(c).map(|w| w as u8)
    }
}

/// Lookup the cell width for a Unicode codepoint.
///
/// Returns `None` for codepoints with no defined width (controls,
/// unassigned, surrogates). Codepoints above plane 1 fall back to a
/// scalar `unicode-width` lookup; everything else is a single indexed
/// load from a 128 KiB table populated on first call.
#[inline]
pub fn codepoint_width(cp: u32) -> Option<u8> {
    width_in(width_table(), cp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_printable_is_one() {
        for cp in 0x20u32..=0x7E {
            assert_eq!(codepoint_width(cp), Some(1), "cp = U+{cp:04X}");
        }
    }

    #[test]
    fn ascii_control_matches_unicode_width() {
        // Whatever `unicode-width-16` decides for control bytes, our
        // table must agree with it. Currently `Some(0)` for ASCII
        // controls — they don't reach this code path in the parser
        // (controls are dispatched as `execute`, not `print`), but
        // table consistency matters for any caller that probes them.
        for cp in [0x00u32, 0x1B, 0x7F] {
            let scalar = char::from_u32(cp)
                .and_then(UnicodeWidthChar::width)
                .map(|w| w as u8);
            assert_eq!(codepoint_width(cp), scalar, "cp = U+{cp:04X}");
        }
    }

    #[test]
    fn cjk_ideograph_is_wide() {
        assert_eq!(codepoint_width(0x4E2D), Some(2)); // 中
        assert_eq!(codepoint_width(0x65E5), Some(2)); // 日
    }

    #[test]
    fn vs15_vs16_zero_width() {
        assert_eq!(codepoint_width(0xFE0E), Some(0));
        assert_eq!(codepoint_width(0xFE0F), Some(0));
    }

    #[test]
    fn supplementary_plane_emoji_wide() {
        // 🎉 U+1F389
        assert_eq!(codepoint_width(0x1F389), Some(2));
    }

    #[test]
    fn surrogate_is_none() {
        assert_eq!(codepoint_width(0xD800), None);
        assert_eq!(codepoint_width(0xDFFF), None);
    }

    #[test]
    fn invalid_codepoint_is_none() {
        assert_eq!(codepoint_width(0x11_0000), None);
    }

    #[test]
    fn matches_unicode_width_crate_for_bmp_sample() {
        // Spot-check that the table produces identical results to the
        // scalar crate across a range we care about (printable BMP).
        for cp in (0x20u32..0xFFFF).step_by(7) {
            let table = codepoint_width(cp);
            let scalar = char::from_u32(cp)
                .and_then(UnicodeWidthChar::width)
                .map(|w| w as u8);
            assert_eq!(table, scalar, "cp = U+{cp:04X}");
        }
    }
}
