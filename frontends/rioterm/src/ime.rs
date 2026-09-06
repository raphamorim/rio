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
    /// Byte offsets are clamped to the text length but otherwise kept
    /// verbatim, including intra-cluster and non-char-boundary values
    /// (macOS reports UTF-16 ranges that can split a surrogate pair;
    /// jamo-level Korean IMEs report offsets inside an NFC syllable).
    /// Nothing downstream slices on the offset: the layout only
    /// compares it against cluster starts, snapping the caret to the
    /// cluster CONTAINING it.
    pub cursor: PreeditCursor,
}

impl Preedit {
    pub fn new(text: String, cursor: PreeditCursor) -> Self {
        let cursor = match cursor {
            PreeditCursor::Byte(offset) => PreeditCursor::Byte(offset.min(text.len())),
            PreeditCursor::Hidden => PreeditCursor::Hidden,
        };
        Self { text, cursor }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preedit_new_clamps_byte_offset() {
        // Intra-char offsets pass through verbatim: the layout snaps
        // them to the containing cluster and never slices on them.
        let preedit = Preedit::new("啊a".to_string(), PreeditCursor::Byte(1));
        assert_eq!(preedit.cursor, PreeditCursor::Byte(1));
        let preedit = Preedit::new("啊a".to_string(), PreeditCursor::Byte(3));
        assert_eq!(preedit.cursor, PreeditCursor::Byte(3));
        // Past-the-end offsets clamp to the length.
        let preedit = Preedit::new("啊a".to_string(), PreeditCursor::Byte(9));
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
