//! Line-level parsing of the control mode protocol.

/// One line of the protocol, outside of a `%begin` block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notification {
    Begin {
        number: u64,
        flags: u32,
    },
    End {
        number: u64,
    },
    Error {
        number: u64,
    },
    Output {
        pane: u32,
        data: Vec<u8>,
    },
    LayoutChange {
        window: u32,
        visible_layout: String,
    },
    WindowAdd(u32),
    WindowClose(u32),
    WindowRenamed {
        window: u32,
        name: String,
    },
    WindowPaneChanged {
        window: u32,
        pane: u32,
    },
    SessionChanged(u32),
    SessionWindowChanged {
        session: u32,
        window: u32,
    },
    SubscriptionChanged {
        name: String,
        value: String,
    },
    Exit(Option<String>),
    /// A well-formed notification nothing here acts on.
    Other,
}

/// Parse a line with its line ending already removed. `None` means the line
/// is not part of the protocol at all.
pub fn parse_line(line: &[u8]) -> Option<Notification> {
    if line.first() != Some(&b'%') {
        return None;
    }

    let (name, rest) = split_word(line);
    let notification = match name {
        b"%begin" => {
            let (_, rest) = split_word(rest);
            let (number, rest) = split_word(rest);
            let (flags, _) = split_word(rest);
            Notification::Begin {
                number: number_of(number)?,
                flags: number_of(flags)? as u32,
            }
        }
        b"%end" | b"%error" => {
            let (_, rest) = split_word(rest);
            let (number, _) = split_word(rest);
            let number = number_of(number)?;
            if name == b"%end" {
                Notification::End { number }
            } else {
                Notification::Error { number }
            }
        }
        b"%output" => {
            let (pane, data) = split_word(rest);
            Notification::Output {
                pane: id_of(pane, b'%')?,
                data: decode_output(data),
            }
        }
        // `%extended-output %<pane> <ms-behind> : <data>`, used once flow
        // control is enabled.
        b"%extended-output" => {
            let (pane, rest) = split_word(rest);
            let (_, rest) = split_word(rest);
            let (_, data) = split_word(rest);
            Notification::Output {
                pane: id_of(pane, b'%')?,
                data: decode_output(data),
            }
        }
        // `%layout-change @<window> <layout> <visible-layout> <flags>`; tmux
        // older than 2.2 sends only the first layout.
        b"%layout-change" => {
            let (window, rest) = split_word(rest);
            let (layout, rest) = split_word(rest);
            let (visible, _) = split_word(rest);
            let visible = if visible.is_empty() { layout } else { visible };
            Notification::LayoutChange {
                window: id_of(window, b'@')?,
                visible_layout: String::from_utf8_lossy(visible).into_owned(),
            }
        }
        b"%window-add" => Notification::WindowAdd(id_of(rest, b'@')?),
        // Closing a window of the attached session can surface as the
        // unlinked form: tmux unlinks it before notifying.
        b"%window-close" | b"%unlinked-window-close" => {
            Notification::WindowClose(id_of(rest, b'@')?)
        }
        b"%window-renamed" => {
            let (window, name) = split_word(rest);
            Notification::WindowRenamed {
                window: id_of(window, b'@')?,
                name: String::from_utf8_lossy(name).into_owned(),
            }
        }
        b"%window-pane-changed" => {
            let (window, pane) = split_word(rest);
            Notification::WindowPaneChanged {
                window: id_of(window, b'@')?,
                pane: id_of(pane, b'%')?,
            }
        }
        b"%session-changed" => {
            let (session, _) = split_word(rest);
            Notification::SessionChanged(id_of(session, b'$')?)
        }
        b"%session-window-changed" => {
            let (session, window) = split_word(rest);
            Notification::SessionWindowChanged {
                session: id_of(session, b'$')?,
                window: id_of(window, b'@')?,
            }
        }
        // `%subscription-changed <name> $<session> <window> <index> <pane> :
        // <value>`, where only the value can hold anything.
        b"%subscription-changed" => {
            let (name, rest) = split_word(rest);
            let colon = rest.windows(3).position(|bytes| bytes == b" : ")?;
            Notification::SubscriptionChanged {
                name: String::from_utf8_lossy(name).into_owned(),
                value: String::from_utf8_lossy(&rest[colon + 3..]).into_owned(),
            }
        }
        b"%exit" => Notification::Exit(
            (!rest.is_empty()).then(|| String::from_utf8_lossy(rest).into_owned()),
        ),
        _ => Notification::Other,
    };
    Some(notification)
}

/// Undo `%output` escaping: every byte below 0x20 and the backslash arrive
/// as three octal digits, everything else is the pane's raw output.
pub fn decode_output(data: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::with_capacity(data.len());
    let mut rest = data;
    while let Some(slash) = memchr::memchr(b'\\', rest) {
        decoded.extend_from_slice(&rest[..slash]);
        rest = &rest[slash..];
        match octal_escape(rest) {
            Some(byte) => {
                decoded.push(byte);
                rest = &rest[4..];
            }
            None => {
                decoded.push(b'\\');
                rest = &rest[1..];
            }
        }
    }
    decoded.extend_from_slice(rest);
    decoded
}

fn octal_escape(bytes: &[u8]) -> Option<u8> {
    let digits = bytes.get(1..4)?;
    let mut value: u16 = 0;
    for digit in digits {
        if !(b'0'..=b'7').contains(digit) {
            return None;
        }
        value = value * 8 + u16::from(digit - b'0');
    }
    u8::try_from(value).ok()
}

/// Split at the first space, dropping it.
fn split_word(bytes: &[u8]) -> (&[u8], &[u8]) {
    match memchr::memchr(b' ', bytes) {
        Some(space) => (&bytes[..space], &bytes[space + 1..]),
        None => (bytes, &[]),
    }
}

fn number_of(bytes: &[u8]) -> Option<u64> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

/// Parse an id such as `%3`, `@1` or `$0`.
pub(crate) fn id_of(bytes: &[u8], prefix: u8) -> Option<u32> {
    let (first, digits) = bytes.split_first()?;
    if *first != prefix {
        return None;
    }
    std::str::from_utf8(digits).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guards() {
        assert_eq!(
            parse_line(b"%begin 1793790355 262 1"),
            Some(Notification::Begin {
                number: 262,
                flags: 1
            })
        );
        assert_eq!(
            parse_line(b"%end 1793790355 262 1"),
            Some(Notification::End { number: 262 })
        );
        assert_eq!(
            parse_line(b"%error 1793790360 276 1"),
            Some(Notification::Error { number: 276 })
        );
    }

    #[test]
    fn output_keeps_raw_utf8_and_decodes_escapes() {
        assert_eq!(
            parse_line(b"%output %12 \\033[0m\xee\x82\xb0 a\\134b\\015\\012"),
            Some(Notification::Output {
                pane: 12,
                data: b"\x1b[0m\xee\x82\xb0 a\\b\r\n".to_vec(),
            })
        );
    }

    #[test]
    fn output_preserves_leading_space_and_empty_data() {
        assert_eq!(
            parse_line(b"%output %0  x"),
            Some(Notification::Output {
                pane: 0,
                data: b" x".to_vec(),
            })
        );
        assert_eq!(
            parse_line(b"%output %0 "),
            Some(Notification::Output {
                pane: 0,
                data: Vec::new(),
            })
        );
    }

    #[test]
    fn extended_output() {
        assert_eq!(
            parse_line(b"%extended-output %0 1234 : abc\\012"),
            Some(Notification::Output {
                pane: 0,
                data: b"abc\n".to_vec(),
            })
        );
    }

    #[test]
    fn malformed_escape_is_kept_literally() {
        assert_eq!(decode_output(b"a\\9"), b"a\\9");
        assert_eq!(decode_output(b"\\777"), b"\\777");
        assert_eq!(decode_output(b"\\01"), b"\\01");
    }

    #[test]
    fn layout_change_prefers_the_visible_layout() {
        assert_eq!(
            parse_line(b"%layout-change @2 aaaa,80x24,0,0,1 bbbb,80x24,0,0,2 *Z"),
            Some(Notification::LayoutChange {
                window: 2,
                visible_layout: "bbbb,80x24,0,0,2".into(),
            })
        );
        assert_eq!(
            parse_line(b"%layout-change @2 aaaa,80x24,0,0,1"),
            Some(Notification::LayoutChange {
                window: 2,
                visible_layout: "aaaa,80x24,0,0,1".into(),
            })
        );
    }

    #[test]
    fn window_and_session_notifications() {
        assert_eq!(
            parse_line(b"%window-add @4"),
            Some(Notification::WindowAdd(4))
        );
        assert_eq!(
            parse_line(b"%unlinked-window-close @1"),
            Some(Notification::WindowClose(1))
        );
        assert_eq!(
            parse_line(b"%window-renamed @1 two words"),
            Some(Notification::WindowRenamed {
                window: 1,
                name: "two words".into(),
            })
        );
        assert_eq!(
            parse_line(b"%window-pane-changed @0 %1"),
            Some(Notification::WindowPaneChanged { window: 0, pane: 1 })
        );
        assert_eq!(
            parse_line(b"%session-changed $0 cap"),
            Some(Notification::SessionChanged(0))
        );
        assert_eq!(
            parse_line(b"%session-window-changed $0 @1"),
            Some(Notification::SessionWindowChanged {
                session: 0,
                window: 1
            })
        );
    }

    #[test]
    fn exit_with_and_without_a_reason() {
        assert_eq!(parse_line(b"%exit"), Some(Notification::Exit(None)));
        assert_eq!(
            parse_line(b"%exit server exited"),
            Some(Notification::Exit(Some("server exited".into())))
        );
    }

    #[test]
    fn unknown_notifications_are_still_protocol() {
        assert_eq!(parse_line(b"%sessions-changed"), Some(Notification::Other));
        assert_eq!(
            parse_line(b"%some-future-thing 1 2"),
            Some(Notification::Other)
        );
    }

    #[test]
    fn anything_else_is_foreign() {
        assert_eq!(parse_line(b"Connection to host closed."), None);
        assert_eq!(parse_line(b""), None);
        // A protocol keyword with unusable arguments cannot be trusted either.
        assert_eq!(parse_line(b"%output nonsense"), None);
    }
}
