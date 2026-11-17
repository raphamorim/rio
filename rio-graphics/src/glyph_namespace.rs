// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Glyph Protocol registration block (spec §4). Plain data, outside
//! the `glyph` feature, because the wire parser checks it even in
//! builds without the registry.

/// First codepoint of the registration block, the start of
/// Supplementary Private Use Area-B.
pub const REGISTRATION_BLOCK_START: u32 = 0x10_0000;

/// Last codepoint of the registration block.
pub const REGISTRATION_BLOCK_END: u32 = 0x10_03FF;

/// Maximum simultaneous registrations per session: one slot per block
/// codepoint, regardless of payload type.
pub const GLOSSARY_CAPACITY: usize =
    (REGISTRATION_BLOCK_END - REGISTRATION_BLOCK_START + 1) as usize;

/// Is `cp` in the registration block? `r` and `c` accept only these
/// codepoints. The rest of the Private Use Areas is where Nerd Fonts
/// and Powerline put the icons the user's font draws, and a
/// registration there would replace them for the whole session (§9).
#[inline]
pub fn in_registration_block(cp: u32) -> bool {
    (REGISTRATION_BLOCK_START..=REGISTRATION_BLOCK_END).contains(&cp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_block_matches_spec() {
        assert!(in_registration_block(0x10_0000));
        assert!(in_registration_block(0x10_03FF));
        assert!(!in_registration_block(0x10_0400));
        assert!(!in_registration_block(0x10_FFFD)); // rest of PUA-B
        assert!(!in_registration_block(0xE0A0)); // Nerd Fonts git branch
        assert!(!in_registration_block(0xF8FF)); // end of basic PUA
        assert!(!in_registration_block(0xF_0000)); // PUA-A
        assert!(!in_registration_block(0x61)); // 'a'
        assert!(!in_registration_block(0x1F600)); // emoji
        assert_eq!(GLOSSARY_CAPACITY, 1024);
    }
}
