//! Fast codepoint width lookup.
//!
//! Flat BMP table indexed by `u32` codepoint, computed once at first use,
//! used for the bulk per-codepoint width queries the parser emits via
//! [`Handler::input_codepoints`]. The table is 64 KiB of `u8` and
//! amortises to zero on subsequent calls.
//!
//! For supplementary-plane codepoints (`U+10000..=U+10FFFF`) we fall
//! back to a scalar [`UnicodeWidthChar::width`] call — those are rare
//! in real terminal traffic (modern emoji, CJK extensions) and the
//! 1 MiB flat table that would cover them is not worth the working set.
//!
//! Encoding in the BMP table:
//! - `0xFF` → width undefined (control / surrogate / unassigned).
//! - `0` / `1` / `2` → cell width.
//!
//! [`Handler::input_codepoints`]: crate::handler::Handler::input_codepoints

use std::sync::OnceLock;
use unicode_width::UnicodeWidthChar;

const BMP_LEN: usize = 0x1_0000;
const SENTINEL_NONE: u8 = 0xFF;

static BMP_TABLE: OnceLock<Box<[u8]>> = OnceLock::new();

#[inline]
fn bmp_table() -> &'static [u8] {
    BMP_TABLE.get_or_init(build_bmp_table)
}

fn build_bmp_table() -> Box<[u8]> {
    let mut table = vec![SENTINEL_NONE; BMP_LEN].into_boxed_slice();
    for cp in 0..BMP_LEN as u32 {
        if let Some(c) = char::from_u32(cp) {
            if let Some(w) = UnicodeWidthChar::width(c) {
                table[cp as usize] = w as u8;
            }
        }
    }
    table
}

/// Lookup the cell width for a Unicode codepoint.
///
/// Returns `None` for codepoints with no defined width (controls,
/// unassigned, surrogates). For supplementary-plane codepoints this
/// falls back to a scalar `unicode-width` lookup; for BMP codepoints
/// it's a single indexed load from a 64 KiB table populated on first
/// call.
#[inline]
pub fn codepoint_width(cp: u32) -> Option<u8> {
    if cp < BMP_LEN as u32 {
        let w = bmp_table()[cp as usize];
        if w == SENTINEL_NONE {
            None
        } else {
            Some(w)
        }
    } else {
        let c = char::from_u32(cp)?;
        UnicodeWidthChar::width(c).map(|w| w as u8)
    }
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
