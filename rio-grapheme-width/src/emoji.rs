// Forked verbatim (modulo module paths) from
// https://github.com/wezterm/wezterm/blob/main/wezterm-char-props/src/emoji.rs
// (MIT, Copyright (c) 2018-Present Wez Furlong).

use crate::emoji_variation::VARIATION_MAP;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Presentation {
    Text,
    Emoji,
}

impl Presentation {
    /// Returns the default presentation followed
    /// by the explicit presentation if specified
    /// by a variation selector
    pub fn for_grapheme(s: &str) -> (Self, Option<Self>) {
        if let Some((a, b)) = VARIATION_MAP.get(s) {
            return (*a, Some(*b));
        }
        let mut presentation = Self::Text;
        for c in s.chars() {
            if Self::for_char(c) == Self::Emoji {
                presentation = Self::Emoji;
                break;
            }
            // Note that `c` may be some other combining
            // sequence that doesn't definitively indicate
            // that we're text, so we only positively
            // change presentation when we identify an
            // emoji char.
        }
        (presentation, None)
    }

    pub fn for_char(c: char) -> Self {
        if crate::emoji_presentation::EMOJI_PRESENTATION.contains_u32(c as u32) {
            Self::Emoji
        } else {
            Self::Text
        }
    }
}
