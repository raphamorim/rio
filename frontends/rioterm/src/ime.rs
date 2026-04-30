use rio_backend::crosswords::pos::Pos;
use unicode_width::UnicodeWidthChar;
#[derive(Debug, Default)]
pub struct Ime {
    /// Whether the IME is enabled.
    enabled: bool,

    /// Current IME preedit.
    preedit: Option<Preedit>,

    /// Pinned cursor position for the active preedit session.
    ///
    /// The terminal cursor tracked by `terminal.cursor()` is updated
    /// asynchronously as the PTY thread parses shell output. When a
    /// composition begins, that cursor may still be catching up with the
    /// echo of a just-committed string or with the response to a
    /// cursor-movement key (e.g. Home/Ctrl-A) that the user pressed
    /// right before starting the next IME session. If we placed the
    /// overlay at whatever `terminal.cursor()` returned at render time,
    /// the preedit would briefly snap back to the previous line end
    /// until the PTY caught up — visible as the cursor "jumping" away
    /// from where the user is typing. Anchoring fixes this: the first
    /// frame that paints a non-empty preedit captures the cursor
    /// position the user actually saw, and every subsequent frame in
    /// the same composition reuses that anchor.
    preedit_anchor: Option<Pos>,
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
        if preedit.is_none() {
            self.preedit_anchor = None;
        }
        self.preedit = preedit;
    }

    #[inline]
    pub fn preedit(&self) -> Option<&Preedit> {
        self.preedit.as_ref()
    }

    /// Return the anchor for the current preedit session, seeding it with
    /// `fallback` on the first call of the session.
    #[inline]
    pub fn preedit_anchor_or_init(&mut self, fallback: Pos) -> Pos {
        *self.preedit_anchor.get_or_insert(fallback)
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

    #[test]
    fn preedit_anchor_seeds_on_first_call_and_stays_pinned() {
        use rio_backend::crosswords::pos::{Column, Line, Pos};

        let mut ime = Ime::new();
        ime.set_preedit(Some(Preedit::new("a".to_string(), Some(1))));

        let seen = Pos::new(Line(5), Column(0));
        assert_eq!(ime.preedit_anchor_or_init(seen), seen);

        // A later snapshot pointing somewhere else (e.g. PTY echo advancing
        // the cursor past a just-committed run) must not drag the anchor.
        let stale_snapshot = Pos::new(Line(5), Column(12));
        assert_eq!(ime.preedit_anchor_or_init(stale_snapshot), seen);
    }

    #[test]
    fn preedit_anchor_clears_when_preedit_cleared() {
        use rio_backend::crosswords::pos::{Column, Line, Pos};

        let mut ime = Ime::new();
        ime.set_preedit(Some(Preedit::new("a".to_string(), Some(1))));
        let first = Pos::new(Line(5), Column(0));
        ime.preedit_anchor_or_init(first);

        // Clearing preedit (synthetic empty Preedit / commit) ends the
        // session. The next composition must reseed from the new fallback.
        ime.set_preedit(None);

        let next = Pos::new(Line(5), Column(4));
        ime.set_preedit(Some(Preedit::new("b".to_string(), Some(1))));
        assert_eq!(ime.preedit_anchor_or_init(next), next);
    }
}
