//! What a paste sends to the program, shared by Rio and librio hosts.

/// The bytes to write for pasting `text`.
///
/// When the program asked for bracketed paste (mode 2004) the text goes
/// between `ESC [200~` and `ESC [201~`, minus ESC, ETX and the 8-bit
/// CSI, so the payload can never close the bracket early and inject
/// keystrokes (some shells also end a bracketed paste at ETX). CRLF
/// collapses to CR there too: the Windows clipboard ends every line with
/// `\r\n`, which shells read as two breaks, doubling every newline
/// pasted into WSL or over ssh. A lone LF is left as is, so a Unix
/// multi-line paste into an editor is unchanged.
///
/// Otherwise the program can't tell a paste from typing, so line breaks
/// of either style become CR, what the Enter key sends.
pub fn encode(text: &str, bracketed: bool) -> Vec<u8> {
    if bracketed {
        let filtered = text
            .replace(['\x1b', '\x03', '\u{9b}'], "")
            .replace("\r\n", "\r");
        format!("\x1b[200~{filtered}\x1b[201~").into_bytes()
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::encode;

    // The payload can't close the bracket and inject keystrokes: ESC,
    // ETX and the 8-bit CSI are dropped.
    #[test]
    fn a_bracketed_paste_cannot_end_its_bracket() {
        assert_eq!(
            encode("a\x1b[201~rm -rf /\x03", true),
            b"\x1b[200~a[201~rm -rf /\x1b[201~".to_vec()
        );
        assert_eq!(
            encode("a\u{9b}201~oops", true),
            b"\x1b[200~a201~oops\x1b[201~".to_vec()
        );
    }

    // CRLF is one line break, not two (#1669); a lone LF stays.
    #[test]
    fn a_bracketed_paste_collapses_crlf_and_keeps_lf() {
        assert_eq!(
            encode("one\r\ntwo\r\n", true),
            b"\x1b[200~one\rtwo\r\x1b[201~".to_vec()
        );
        assert_eq!(
            encode("one\ntwo", true),
            b"\x1b[200~one\ntwo\x1b[201~".to_vec()
        );
        assert_eq!(
            encode("crlf\r\nlf\nlone cr\r", true),
            b"\x1b[200~crlf\rlf\nlone cr\r\x1b[201~".to_vec()
        );
    }

    // Unbracketed, every line break is what Enter sends.
    #[test]
    fn an_unbracketed_paste_sends_cr_for_every_break() {
        assert_eq!(
            encode("one\r\ntwo\nthree", false),
            b"one\rtwo\rthree".to_vec()
        );
    }
}
