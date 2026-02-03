use unicode_width::UnicodeWidthChar;
#[derive(Debug, Default)]
pub struct Ime {
    /// Whether the IME is enabled.
    enabled: bool,

    /// Current IME preedit.
    preedit: Option<Preedit>,
}

impl Ime {
    pub fn new() -> Self {
        Default::default()
    }

    #[inline]
    pub fn set_enabled(&mut self, is_enabled: bool) {
        if is_enabled {
            self.enabled = is_enabled
        } else {
            // Clear state when disabling IME.
            *self = Default::default();
        }
    }

    #[inline]
    #[allow(unused)]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    #[inline]
    pub fn set_preedit(&mut self, preedit: Option<Preedit>) {
        self.preedit = preedit;
    }

    #[inline]
    pub fn preedit(&self) -> Option<&Preedit> {
        self.preedit.as_ref()
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Preedit {
    /// The preedit text.
    pub text: String,

    /// Byte offset for cursor start into the preedit text.
    ///
    /// `None` means that the cursor is invisible.
    pub cursor_byte_offset: Option<usize>,

    /// The cursor offset from the end of the preedit in char width.
    pub cursor_end_offset: Option<usize>,
}

impl Preedit {
    pub fn new(text: String, cursor_byte_offset: Option<usize>) -> Self {
        let cursor_byte_offset =
            cursor_byte_offset.filter(|&byte_offset| text.is_char_boundary(byte_offset));
        let cursor_end_offset = cursor_byte_offset.map(|byte_offset| {
            text[byte_offset..]
                .chars()
                .fold(0, |acc, ch| acc + ch.width().unwrap_or(1))
        });

        Self {
            text,
            cursor_byte_offset,
            cursor_end_offset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preedit_new_rejects_invalid_byte_offset() {
        let preedit = Preedit::new("啊a".to_string(), Some(1));
        assert!(preedit.cursor_byte_offset.is_none());
        assert!(preedit.cursor_end_offset.is_none());
    }

    #[test]
    fn preedit_new_computes_cursor_end_offset() {
        let preedit = Preedit::new("啊a".to_string(), Some(0));
        assert_eq!(preedit.cursor_byte_offset, Some(0));
        assert_eq!(preedit.cursor_end_offset, Some(3));
    }
}
