//! What a paste sends to the program, and whether it is safe to send
//! without asking first. Shared by Rio and librio hosts.

/// The bracketed paste (mode 2004) frame written around the text.
pub const BRACKETED_START: &[u8] = b"\x1b[200~";
pub const BRACKETED_END: &[u8] = b"\x1b[201~";

/// Bytes replaced by a space in any paste, bracketed or not, as xterm
/// does: a pasted control byte must not act as one. ESC could end the
/// bracket and let the rest run as typed. The others are what the tty
/// turns into signals or line editing (^C interrupts, ^Z suspends, ^\
/// quits, ^U and ^W erase, ^V quotes the next byte, ^S/^Q pause and
/// resume output, ^R reprints, ^O discards), plus NUL, BS, ENQ, EOT and
/// DEL. A program can rebind the tty keys with tcsetattr; these are their
/// defaults. Tab, LF and CR are not on the list: they are text.
const REPLACED: [u8; 16] = [
    0x00, // NUL
    0x08, // BS
    0x05, // ENQ
    0x04, // EOT
    0x1B, // ESC
    0x7F, // DEL
    0x03, // ^C, VINTR
    0x1C, // ^\, VQUIT
    0x15, // ^U, VKILL
    0x1A, // ^Z, VSUSP
    0x11, // ^Q, VSTART
    0x13, // ^S, VSTOP
    0x17, // ^W, VWERASE
    0x16, // ^V, VLNEXT
    0x12, // ^R, VREPRINT
    0x0F, // ^O, VDISCARD
];

/// [`REPLACED`] as a lookup table.
const fn replaced_table() -> [bool; 256] {
    let mut table = [false; 256];
    let mut i = 0;
    while i < REPLACED.len() {
        table[REPLACED[i] as usize] = true;
        i += 1;
    }
    table
}
const REPLACED_TABLE: [bool; 256] = replaced_table();

/// The bytes to write for pasting `text`, as xterm sends them.
///
/// Control bytes become spaces either way (see [`REPLACED`]), so the paste
/// can't end its bracket or act as tty input; a space keeps the words
/// around them apart. In bracketed paste mode the text then goes between
/// the markers with its line endings as they are: the program asked for
/// the paste as text. Otherwise the program can't tell it from typing, so
/// LF becomes CR, what Enter sends; CRLF becomes two CRs.
///
/// The text is copied once and changed in place: every byte replaced is
/// ASCII and replaced by ASCII, so the result is still UTF-8 text.
pub fn encode(text: &str, bracketed: bool) -> Vec<u8> {
    let frame = if bracketed {
        BRACKETED_START.len() + BRACKETED_END.len()
    } else {
        0
    };
    let mut out = Vec::with_capacity(text.len() + frame);
    if bracketed {
        out.extend_from_slice(BRACKETED_START);
    }
    let start = out.len();
    out.extend_from_slice(text.as_bytes());
    for b in &mut out[start..] {
        if REPLACED_TABLE[*b as usize] {
            *b = b' ';
        } else if !bracketed && *b == b'\n' {
            *b = b'\r';
        }
    }
    if bracketed {
        out.extend_from_slice(BRACKETED_END);
    }
    out
}

/// Whether [`encode`] sends `c` as a space, for showing a paste the way
/// it will arrive.
pub fn is_replaced(c: char) -> bool {
    c.is_ascii() && REPLACED_TABLE[c as usize]
}

/// Whether `text` holds the end of a bracketed paste. [`encode`] defuses
/// it, but text carrying it was most likely made to escape the bracket.
fn holds_bracket_end(text: &str) -> bool {
    text.contains("\x1b[201~")
}

/// Whether `text` looks safe to paste whatever the program asked for: no
/// line break, since CR is Enter just like LF and a paste that isn't
/// bracketed runs each line, and no bracket end. Anything else should be
/// confirmed first.
pub fn is_safe(text: &str) -> bool {
    !text.contains(['\r', '\n']) && !holds_bracket_end(text)
}

/// [`is_safe`] for the way the paste will be sent. Bracketed, the program
/// gets the text as one paste, so line breaks are fine and only a bracket
/// end is suspect; otherwise the same rule as [`is_safe`].
pub fn is_safe_with(text: &str, bracketed: bool) -> bool {
    if bracketed {
        !holds_bracket_end(text)
    } else {
        is_safe(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bracketed(inner: &[u8]) -> Vec<u8> {
        [BRACKETED_START, inner, BRACKETED_END].concat()
    }

    #[test]
    fn plain_text_is_sent_as_is() {
        assert_eq!(encode("hello", true), bracketed(b"hello"));
        assert_eq!(encode("hello", false), b"hello".to_vec());
        assert_eq!(encode("", true), bracketed(b""));
        assert!(encode("", false).is_empty());
        // Multi-byte text is untouched, the 8-bit CSI character included.
        assert_eq!(encode("café ©°", true), bracketed("café ©°".as_bytes()));
        assert_eq!(encode("a\u{9b}b", true), bracketed("a\u{9b}b".as_bytes()));
    }

    // A pasted control byte becomes a space in either mode, so the paste
    // can't end its bracket or act as a signal or a line edit.
    #[test]
    fn control_bytes_become_spaces() {
        assert_eq!(
            encode("hel\x1blo\x00world", true),
            bracketed(b"hel lo world")
        );
        assert_eq!(encode("hel\x03lo", false), b"hel lo".to_vec());
        assert_eq!(encode("\x00\x08\x7f", true), bracketed(b"   "));
        for &b in &REPLACED {
            let text = format!("a{}b", b as char);
            assert_eq!(encode(&text, false), b"a b".to_vec(), "{b:#04x}");
        }
        assert_eq!(encode("a\tb", false), b"a\tb".to_vec(), "tab is text");
    }

    // is_replaced says exactly what encode turns into a space, for every
    // ASCII character; nothing beyond ASCII is replaced.
    #[test]
    fn is_replaced_matches_what_encode_turns_into_spaces() {
        for code in 0u8..128 {
            let c = char::from(code);
            if c == ' ' {
                continue;
            }
            let replaced = encode(&c.to_string(), true) == bracketed(b" ");
            assert_eq!(is_replaced(c), replaced, "{code:#04x}");
        }
        assert_eq!(
            REPLACED
                .iter()
                .filter(|&&b| is_replaced(char::from(b)))
                .count(),
            16
        );
        for c in ['é', '\u{9b}', '\u{2028}'] {
            assert!(!is_replaced(c), "{c:?}");
        }
    }

    #[test]
    fn a_paste_cannot_end_its_bracket() {
        assert_eq!(
            encode("a\x1b[201~rm -rf /\n", true),
            bracketed(b"a [201~rm -rf /\n")
        );
    }

    // Bracketed, line endings are the text's: the program asked for it as
    // a paste.
    #[test]
    fn bracketed_line_endings_are_kept() {
        assert_eq!(
            encode("crlf\r\nlf\nlone cr\r", true),
            bracketed(b"crlf\r\nlf\nlone cr\r")
        );
    }

    // Unbracketed, LF is Enter; CRLF is two, as xterm sends it.
    #[test]
    fn unbracketed_lf_is_enter() {
        assert_eq!(
            encode("one\r\ntwo\nthree\r", false),
            b"one\r\rtwo\rthree\r".to_vec()
        );
    }

    #[test]
    fn line_breaks_and_bracket_ends_are_not_safe() {
        assert!(is_safe("hello"));
        assert!(is_safe("hello\tworld"));
        for unsafe_text in [
            "hello\n",
            "hello\nworld",
            "hello\r",
            "hello\r\nworld",
            "\r",
            "he\x1b[201~llo",
        ] {
            assert!(!is_safe(unsafe_text), "{unsafe_text:?}");
        }
    }

    #[test]
    fn bracketed_line_breaks_are_safe() {
        for text in ["hello", "hello\nworld", "hello\rworld", "hello\r\nworld"] {
            assert!(is_safe_with(text, true), "{text:?}");
        }
        assert!(!is_safe_with("he\x1b[201~llo", true));
        assert!(!is_safe_with("hello\n\x1b[201~", true));
        assert!(is_safe_with("hello", false));
        assert!(!is_safe_with("hello\nworld", false));
        assert!(!is_safe_with("hello\r", false));
    }
}
