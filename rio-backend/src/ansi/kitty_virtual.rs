// Kitty graphics protocol virtual placement encoding/decoding

use crate::config::colors::ColorRgb;

/// Diacritics used for row/column encoding in Kitty virtual placements
/// Index in array = the value being encoded
/// Derived from: https://sw.kovidgoyal.net/kitty/_downloads/f0a0de9ec8d9ff4456206db8e0814937/rowcolumn-diacritics.txt
pub const DIACRITICS: &[char] = &[
    '\u{0305}', '\u{030D}', '\u{030E}', '\u{0310}', '\u{0312}', '\u{033D}', '\u{033E}', '\u{033F}',
    '\u{0346}', '\u{034A}', '\u{034B}', '\u{034C}', '\u{0350}', '\u{0351}', '\u{0352}', '\u{0357}',
    '\u{035B}', '\u{0363}', '\u{0364}', '\u{0365}', '\u{0366}', '\u{0367}', '\u{0368}', '\u{0369}',
    '\u{036A}', '\u{036B}', '\u{036C}', '\u{036D}', '\u{036E}', '\u{036F}', '\u{0483}', '\u{0484}',
    '\u{0485}', '\u{0486}', '\u{0487}', '\u{0592}', '\u{0593}', '\u{0594}', '\u{0595}', '\u{0597}',
    '\u{0598}', '\u{0599}', '\u{059C}', '\u{059D}', '\u{059E}', '\u{059F}', '\u{05A0}', '\u{05A1}',
    '\u{05A8}', '\u{05A9}', '\u{05AB}', '\u{05AC}', '\u{05AF}', '\u{05C4}', '\u{0610}', '\u{0611}',
    '\u{0612}', '\u{0613}', '\u{0614}', '\u{0615}', '\u{0616}', '\u{0617}', '\u{0657}', '\u{0658}',
    '\u{0659}', '\u{065A}', '\u{065B}', '\u{065D}', '\u{065E}', '\u{06D6}', '\u{06D7}', '\u{06D8}',
    '\u{06D9}', '\u{06DA}', '\u{06DB}', '\u{06DC}', '\u{06DF}', '\u{06E0}', '\u{06E1}', '\u{06E2}',
    '\u{06E4}', '\u{06E7}', '\u{06E8}', '\u{06EA}', '\u{06EB}', '\u{06EC}', '\u{06ED}', '\u{0730}',
    '\u{0732}', '\u{0733}', '\u{0735}', '\u{0736}', '\u{073A}', '\u{073D}', '\u{073F}', '\u{0740}',
    '\u{0741}', '\u{0743}', '\u{0745}', '\u{0747}', '\u{0749}', '\u{074A}', '\u{07EB}', '\u{07EC}',
    '\u{07ED}', '\u{07EE}', '\u{07EF}', '\u{07F0}', '\u{07F1}', '\u{07F3}', '\u{0816}', '\u{0817}',
    '\u{0818}', '\u{0819}', '\u{081B}', '\u{081C}', '\u{081D}', '\u{081E}', '\u{081F}', '\u{0820}',
    '\u{0821}', '\u{0822}', '\u{0823}', '\u{0825}', '\u{0826}', '\u{0827}', '\u{0829}', '\u{082A}',
    '\u{082B}', '\u{082C}', '\u{082D}', '\u{0951}', '\u{0953}', '\u{0954}', '\u{0F82}', '\u{0F83}',
    '\u{0F86}', '\u{0F87}', '\u{135D}', '\u{135E}', '\u{135F}', '\u{17DD}', '\u{193A}', '\u{1A17}',
    '\u{1A75}', '\u{1A76}', '\u{1A77}', '\u{1A78}', '\u{1A79}', '\u{1A7A}', '\u{1A7B}', '\u{1A7C}',
    '\u{1B6B}', '\u{1B6D}', '\u{1B6E}', '\u{1B6F}', '\u{1B70}', '\u{1B71}', '\u{1B72}', '\u{1B73}',
    '\u{1CD0}', '\u{1CD1}', '\u{1CD2}', '\u{1CDA}', '\u{1CDB}', '\u{1CE0}', '\u{1DC0}', '\u{1DC1}',
    '\u{1DC3}', '\u{1DC4}', '\u{1DC5}', '\u{1DC6}', '\u{1DC7}', '\u{1DC8}', '\u{1DC9}', '\u{1DCB}',
    '\u{1DCC}', '\u{1DD1}', '\u{1DD2}', '\u{1DD3}', '\u{1DD4}', '\u{1DD5}', '\u{1DD6}', '\u{1DD7}',
    '\u{1DD8}', '\u{1DD9}', '\u{1DDA}', '\u{1DDB}', '\u{1DDC}', '\u{1DDD}', '\u{1DDE}', '\u{1DDF}',
    '\u{1DE0}', '\u{1DE1}', '\u{1DE2}', '\u{1DE3}', '\u{1DE4}', '\u{1DE5}', '\u{1DE6}', '\u{1DFE}',
    '\u{20D0}', '\u{20D1}', '\u{20D4}', '\u{20D5}', '\u{20D6}', '\u{20D7}', '\u{20DB}', '\u{20DC}',
    '\u{20E1}', '\u{20E7}', '\u{20E9}', '\u{20F0}', '\u{2CEF}', '\u{2CF0}', '\u{2CF1}', '\u{2DE0}',
    '\u{2DE1}', '\u{2DE2}', '\u{2DE3}', '\u{2DE4}', '\u{2DE5}', '\u{2DE6}', '\u{2DE7}', '\u{2DE8}',
    '\u{2DE9}', '\u{2DEA}', '\u{2DEB}', '\u{2DEC}', '\u{2DED}', '\u{2DEE}', '\u{2DEF}', '\u{2DF0}',
    '\u{2DF1}', '\u{2DF2}', '\u{2DF3}', '\u{2DF4}', '\u{2DF5}', '\u{2DF6}', '\u{2DF7}', '\u{2DF8}',
    '\u{2DF9}', '\u{2DFA}', '\u{2DFB}', '\u{2DFC}', '\u{2DFD}', '\u{2DFE}', '\u{2DFF}', '\u{A66F}',
    '\u{A67C}', '\u{A67D}', '\u{A6F0}', '\u{A6F1}', '\u{A8E0}', '\u{A8E1}', '\u{A8E2}', '\u{A8E3}',
    '\u{A8E4}', '\u{A8E5}', '\u{A8E6}', '\u{A8E7}', '\u{A8E8}', '\u{A8E9}', '\u{A8EA}', '\u{A8EB}',
    '\u{A8EC}', '\u{A8ED}', '\u{A8EE}', '\u{A8EF}', '\u{A8F0}', '\u{A8F1}', '\u{AAB0}', '\u{AAB2}',
    '\u{AAB3}', '\u{AAB7}', '\u{AAB8}', '\u{AABE}', '\u{AABF}', '\u{AAC1}', '\u{FE20}', '\u{FE21}',
    '\u{FE22}', '\u{FE23}', '\u{FE24}', '\u{FE25}', '\u{FE26}', '\u{10A0F}', '\u{10A38}', '\u{1D185}',
    '\u{1D186}', '\u{1D187}', '\u{1D188}', '\u{1D189}', '\u{1D1AA}', '\u{1D1AB}', '\u{1D1AC}', '\u{1D1AD}',
    '\u{1D242}', '\u{1D243}', '\u{1D244}',
];

/// Convert an index (0-based) to a diacritic character
/// Returns None if index is out of range
pub fn index_to_diacritic(index: u32) -> Option<char> {
    DIACRITICS.get(index as usize).copied()
}

/// Convert a diacritic character to an index (0-based)
/// Returns None if not a valid diacritic
pub fn diacritic_to_index(c: char) -> Option<u32> {
    DIACRITICS.iter().position(|&d| d == c).map(|i| i as u32)
}

/// Convert an image ID to RGB color (lower 24 bits)
pub fn id_to_rgb(id: u32) -> ColorRgb {
    ColorRgb {
        r: ((id >> 16) & 0xFF) as u8,
        g: ((id >> 8) & 0xFF) as u8,
        b: (id & 0xFF) as u8,
    }
}

/// Convert RGB color to image ID (24 bits)
pub fn rgb_to_id(rgb: ColorRgb) -> u32 {
    ((rgb.r as u32) << 16) | ((rgb.g as u32) << 8) | (rgb.b as u32)
}

/// Encode virtual placement data into a string with placeholder + diacritics
///
/// Encoding scheme (matching Ghostty/Kitty):
/// - Base character: U+10EEEE (placeholder)
/// - 1st diacritic: Row index (0-based)
/// - 2nd diacritic: Column index (0-based)
/// - 3rd diacritic: High 8 bits of image_id (optional, for IDs > 16M)
///
/// Image ID (lower 24 bits) is encoded in foreground color
/// Placement ID is encoded in underline color
pub fn encode_placeholder(row: u32, col: u32, image_id_high: Option<u8>) -> String {
    let mut result = String::from('\u{10EEEE}');

    // Add row diacritic
    if let Some(d) = index_to_diacritic(row) {
        result.push(d);
    }

    // Add column diacritic
    if let Some(d) = index_to_diacritic(col) {
        result.push(d);
    }

    // Add high byte diacritic if needed
    if let Some(high) = image_id_high {
        if let Some(d) = index_to_diacritic(high as u32) {
            result.push(d);
        }
    }

    result
}

/// Decode placement information from a placeholder character string with diacritics
///
/// Returns (row_index, col_index, image_id_high) if successfully decoded
/// The base character should be U+10EEEE
pub fn decode_placeholder(s: &str) -> Option<(u32, u32, Option<u8>)> {
    let mut chars = s.chars();

    // First character should be the placeholder
    if chars.next()? != '\u{10EEEE}' {
        return None;
    }

    // 1st diacritic: row index
    let row = chars.next().and_then(diacritic_to_index)?;

    // 2nd diacritic: column index
    let col = chars.next().and_then(diacritic_to_index)?;

    // 3rd diacritic (optional): high 8 bits of image_id
    let high = chars.next().and_then(|c| {
        diacritic_to_index(c).and_then(|idx| {
            if idx <= 255 {
                Some(idx as u8)
            } else {
                None
            }
        })
    });

    Some((row, col, high))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diacritic_conversion() {
        // First diacritic should encode 0
        assert_eq!(index_to_diacritic(0), Some('\u{0305}'));
        assert_eq!(diacritic_to_index('\u{0305}'), Some(0));

        // Last diacritic
        let last_idx = (DIACRITICS.len() - 1) as u32;
        assert_eq!(index_to_diacritic(last_idx), Some('\u{1D244}'));
        assert_eq!(diacritic_to_index('\u{1D244}'), Some(last_idx));
    }

    #[test]
    fn test_rgb_id_conversion() {
        let rgb = ColorRgb { r: 0x12, g: 0x34, b: 0x56 };
        let id = rgb_to_id(rgb);
        assert_eq!(id, 0x123456);
        assert_eq!(id_to_rgb(id), rgb);
    }

    #[test]
    fn test_encode_placeholder() {
        // Encode row=0, col=0
        let s = encode_placeholder(0, 0, None);
        assert!(s.starts_with('\u{10EEEE}'));
        assert_eq!(s.chars().count(), 3); // placeholder + 2 diacritics

        // With high byte
        let s = encode_placeholder(0, 0, Some(1));
        assert_eq!(s.chars().count(), 4); // placeholder + 3 diacritics
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        // Test encoding and decoding
        let encoded = encode_placeholder(5, 10, None);
        let decoded = decode_placeholder(&encoded).unwrap();
        assert_eq!(decoded, (5, 10, None));

        // With high byte
        let encoded = encode_placeholder(5, 10, Some(42));
        let decoded = decode_placeholder(&encoded).unwrap();
        assert_eq!(decoded, (5, 10, Some(42)));
    }
}
