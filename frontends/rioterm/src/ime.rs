pub use rio_grid::preedit::PreeditCursor;

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

#[derive(Debug, PartialEq, Eq)]
pub struct Preedit {
    /// The preedit text.
    pub text: String,

    /// The IME caret.
    ///
    /// Byte offsets that don't land on a char boundary (macOS reports
    /// UTF-16 ranges that can split a surrogate pair) snap to
    /// end-of-text rather than being trusted — slicing on one would
    /// panic downstream.
    pub cursor: PreeditCursor,
}

impl Preedit {
    pub fn new(text: String, cursor: PreeditCursor) -> Self {
        let cursor = match cursor {
            PreeditCursor::Byte(offset) if !text.is_char_boundary(offset) => {
                PreeditCursor::Byte(text.len())
            }
            other => other,
        };
        Self { text, cursor }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preedit_new_rejects_invalid_byte_offset() {
        // Byte 1 is inside 啊's UTF-8 encoding: not a char boundary.
        let preedit = Preedit::new("啊a".to_string(), PreeditCursor::Byte(1));
        assert_eq!(preedit.cursor, PreeditCursor::Byte(4));
        // Boundary offsets survive, including one-past-the-end.
        let preedit = Preedit::new("啊a".to_string(), PreeditCursor::Byte(3));
        assert_eq!(preedit.cursor, PreeditCursor::Byte(3));
        let preedit = Preedit::new("啊a".to_string(), PreeditCursor::Byte(4));
        assert_eq!(preedit.cursor, PreeditCursor::Byte(4));
        // A hidden caret stays hidden.
        let preedit = Preedit::new("啊a".to_string(), PreeditCursor::Hidden);
        assert_eq!(preedit.cursor, PreeditCursor::Hidden);
    }

    #[test]
    fn set_preedit_clears_on_none() {
        let mut ime = Ime::new();
        ime.set_preedit(Some(Preedit::new("a".to_string(), PreeditCursor::Byte(1))));
        assert!(ime.preedit().is_some());
        ime.set_preedit(None);
        assert!(ime.preedit().is_none());
    }
}
