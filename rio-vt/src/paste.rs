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

/// Whether [`encode`] sends the byte `b` as a space. Compared with each
/// of [`REPLACED`] rather than looked up, so a loop over bytes compiles to
/// SIMD compares.
#[inline(always)]
fn replaces(b: u8) -> bool {
    let mut hit = false;
    for &replaced in &REPLACED {
        hit |= b == replaced;
    }
    hit
}

/// Turn `bytes` into what xterm sends for them, in place: control bytes
/// (see [`REPLACED`]) become spaces either way, so the paste can't end
/// its bracket or act as tty input; a space keeps the words around them
/// apart. Bracketed, the line endings stay as they are: the program asked
/// for the paste as text. Otherwise the program can't tell it from typing,
/// so LF becomes CR, what Enter sends; CRLF becomes two CRs.
///
/// Every byte changed is ASCII and replaced by ASCII, so UTF-8 text stays
/// UTF-8. The choice is made without a branch, so the loop vectorizes.
pub fn encode_in_place(bytes: &mut [u8], bracketed: bool) {
    #[inline(always)]
    fn encode_byte(b: u8, lf_is_enter: bool) -> u8 {
        if replaces(b) {
            b' '
        } else if lf_is_enter & (b == b'\n') {
            b'\r'
        } else {
            b
        }
    }
    // One loop per mode, so neither tests the mode per byte.
    if bracketed {
        for b in bytes.iter_mut() {
            *b = encode_byte(*b, false);
        }
    } else {
        for b in bytes.iter_mut() {
            *b = encode_byte(*b, true);
        }
    }
}

/// A paste as written to the program, in order: the bracketed paste start
/// marker (empty unless bracketed), the text, and the end marker.
pub struct Encoded {
    pub start: &'static [u8],
    pub text: Vec<u8>,
    pub end: &'static [u8],
}

/// Encode a paste the caller owns, such as text taken from the clipboard:
/// its buffer is changed in place and handed back, so the text is never
/// copied, and the markers are separate writes.
pub fn encode_owned(text: String, bracketed: bool) -> Encoded {
    let mut text = text.into_bytes();
    encode_in_place(&mut text, bracketed);
    let (start, end): (&'static [u8], &'static [u8]) = if bracketed {
        (BRACKETED_START, BRACKETED_END)
    } else {
        (b"", b"")
    };
    Encoded { start, text, end }
}

/// The bytes to write for pasting `text`, as one buffer: the text copied
/// once and encoded in place (see [`encode_in_place`]), between the
/// bracketed paste markers when `bracketed`. When the caller owns the
/// text, [`encode_owned`] saves the copy.
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
    encode_in_place(&mut out[start..], bracketed);
    if bracketed {
        out.extend_from_slice(BRACKETED_END);
    }
    out
}

/// Text read from the Windows clipboard, with its line endings as text
/// has them. Windows ends every line on its clipboard with CRLF; the CR is
/// how it marks the end of a line, not part of the text. Pasted as is into
/// a shell, it doubles every line break: readline reads each CR in a
/// bracketed paste as a newline, so CRLF is two, and unbracketed it is two
/// Enters (#1669). CRLF becomes LF, so a paste reads the same on every
/// platform; a lone CR stays.
///
/// Text without a CR, the usual case, is handed back as is after one
/// memchr scan. Otherwise the text between CRs is copied as `&str`, so it
/// needs no new UTF-8 check: 32 MB in about 10 ms, half of `replace`.
pub fn from_windows_clipboard(text: String) -> String {
    let Some(first) = text.find('\r') else {
        return text;
    };
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..first]);
    let mut rest = &text[first..];
    while let Some(cr) = rest.find('\r') {
        let keep = if rest[cr + 1..].starts_with('\n') {
            cr
        } else {
            cr + 1
        };
        out.push_str(&rest[..keep]);
        rest = &rest[cr + 1..];
    }
    out.push_str(rest);
    out
}

/// Whether [`encode`] sends `c` as a space, for showing a paste the way
/// it will arrive.
pub fn is_replaced(c: char) -> bool {
    c.is_ascii() && replaces(c as u8)
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

    // An owned paste is encoded in its own buffer, not copied, and the
    // markers come as their own writes.
    #[test]
    fn an_owned_paste_is_not_copied() {
        let text = String::from("echo a\x1b[201~\nls\n");
        let buffer = text.as_ptr();
        let encoded = encode_owned(text, true);
        assert_eq!(encoded.text.as_ptr(), buffer);
        assert_eq!(encoded.start, BRACKETED_START);
        assert_eq!(encoded.end, BRACKETED_END);
        assert_eq!(
            [encoded.start, &encoded.text[..], encoded.end].concat(),
            encode("echo a\x1b[201~\nls\n", true)
        );

        let encoded = encode_owned(String::from("one\r\ntwo\n"), false);
        assert!(encoded.start.is_empty() && encoded.end.is_empty());
        assert_eq!(encoded.text, encode("one\r\ntwo\n", false));
    }

    // Windows clipboard line endings are line breaks, not text: CRLF is
    // one LF, so a bracketed paste has one line break per line and an
    // unbracketed one one Enter. A lone CR stays.
    #[test]
    fn windows_clipboard_line_endings_are_one_break() {
        let text = from_windows_clipboard("one\r\ntwo\r\nlone cr\r".to_owned());
        assert_eq!(text, "one\ntwo\nlone cr\r");
        assert_eq!(encode(&text, true), bracketed(b"one\ntwo\nlone cr\r"));
        assert_eq!(encode(&text, false), b"one\rtwo\rlone cr\r".to_vec());

        for (from, to) in [
            ("", ""),
            ("\r", "\r"),
            ("\r\n", "\n"),
            ("\r\r\n", "\r\n"),
            ("\r\n\r\n", "\n\n"),
            ("\r\n\n", "\n\n"),
            ("é\r\nü\r", "é\nü\r"),
        ] {
            assert_eq!(from_windows_clipboard(from.to_owned()), to, "{from:?}");
        }

        let plain = String::from("echo a\necho b");
        let buffer = plain.as_ptr();
        let kept = from_windows_clipboard(plain);
        assert_eq!(kept.as_ptr(), buffer, "nothing to change, nothing copied");
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
