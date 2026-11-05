//! Terminal replies from panes.
//!
//! `%output` carries what a program wrote before tmux interpreted it, so a
//! pane's terminal sees its queries too. tmux answers some of them itself,
//! and a second answer would reach the program as typed input. Which ones
//! depends on its version, so this lets through what tmux 3.2a, 3.4 and 3.6
//! were seen to leave unanswered with only a control client attached.
//! Anything else stays silent, which programs have to cope with anyway.

/// Whether to send `reply` to the program that asked. `before_3_3` is for a
/// tmux known to be older than 3.3, which answers less: that's where colours,
/// cell sizes and sixel limits start being answered.
pub fn forwards_reply(reply: &[u8], before_3_3: bool) -> bool {
    if reply.starts_with(b"\x1b_") {
        // Kitty graphics and the glyph protocol.
        true
    } else if let Some(body) = reply.strip_prefix(b"\x1b[") {
        forwards_csi(body, before_3_3)
    } else if let Some(body) = reply.strip_prefix(b"\x1b]") {
        forwards_osc(body, before_3_3)
    } else if let Some(body) = reply.strip_prefix(b"\x1bP") {
        // XTGETTCAP.
        body.starts_with(b"1+r") || body.starts_with(b"0+r")
    } else {
        false
    }
}

fn forwards_csi(body: &[u8], before_3_3: bool) -> bool {
    let private = body.first() == Some(&b'?');
    match body.last() {
        // DECRPM, the state of a mode.
        Some(b'y') => body.ends_with(b"$y"),
        // The kitty keyboard flags, and the cursor position with DECXCPR.
        Some(b'u' | b'R') => private,
        // XTSMGRAPHICS.
        Some(b'S') => private && before_3_3,
        // Text area and cell size in pixels. tmux always answers `8;`.
        Some(b't') => (body.starts_with(b"4;") || body.starts_with(b"6;")) && before_3_3,
        _ => false,
    }
}

fn forwards_osc(body: &[u8], before_3_3: bool) -> bool {
    let number = body.split(|byte| *byte == b';').next().unwrap_or_default();
    match number {
        // The palette and the cursor colour.
        b"4" | b"12" => true,
        // Foreground and background.
        b"10" | b"11" => before_3_3,
        // Not the clipboard, OSC 52: tmux keeps that from programs unless
        // `set-clipboard` is `on`, and then it answers from its own buffers.
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_every_tmux_answers_is_dropped() {
        for reply in [
            &b"\x1b[?62;4;6;22;52c"[..],
            b"\x1b[>0;0;1c",
            b"\x1b[0n",
            b"\x1b[3;4R",
            b"\x1bP>|Rio 0.5\x1b\\",
            b"\x1b[8;24;80t",
        ] {
            assert!(!forwards_reply(reply, true), "{reply:?}");
            assert!(!forwards_reply(reply, false), "{reply:?}");
        }
    }

    #[test]
    fn what_no_tmux_answers_is_forwarded() {
        for reply in [
            &b"\x1b_Gi=31;OK\x1b\\"[..],
            b"\x1b_25a1;q;cp=e0b0;status=2\x1b\\",
            b"\x1b]4;1;rgb:cccc/0000/0000\x07",
            b"\x1b]12;rgb:ffff/ffff/ffff\x1b\\",
            b"\x1b[?2026;2$y",
            b"\x1b[4;2$y",
            b"\x1b[?0u",
            b"\x1b[?3;4;1R",
            b"\x1bP1+r544e=726976\x1b\\",
            b"\x1bP0+r\x1b\\",
        ] {
            assert!(forwards_reply(reply, true), "{reply:?}");
            assert!(forwards_reply(reply, false), "{reply:?}");
        }
    }

    #[test]
    fn what_newer_tmux_answers_needs_an_older_one() {
        for reply in [
            &b"\x1b]10;rgb:ffff/ffff/ffff\x07"[..],
            b"\x1b]11;rgb:0000/0000/0000\x07",
            b"\x1b[?1;0;1024S",
            b"\x1b[?2;0;1000;1000S",
            b"\x1b[4;768;1280t",
            b"\x1b[6;32;16t",
        ] {
            assert!(forwards_reply(reply, true), "{reply:?}");
            assert!(!forwards_reply(reply, false), "{reply:?}");
        }
    }

    #[test]
    fn programs_cannot_read_the_clipboard() {
        let reply = b"\x1b]52;c;c2VjcmV0\x07";
        assert!(!forwards_reply(reply, true));
        assert!(!forwards_reply(reply, false));
    }

    #[test]
    fn anything_unknown_stays_silent() {
        for reply in [&b""[..], b"text", b"\x1b[", b"\x1b]", b"\x1bP1$r0m\x1b\\"] {
            assert!(!forwards_reply(reply, true), "{reply:?}");
        }
    }
}
