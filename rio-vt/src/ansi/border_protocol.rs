//! Experimental cell borders carried in a private APC namespace.
//!
//! Commands modify existing cells, independently of text and SGR state.

pub const PREFIX: &[u8] = b"rio-border;";
pub const SUPPORT_REPLY: &str = "\x1b_rio-border;1;ok\x1b\\";

/// A solid stroke. Width is in 1/256 of the cell width on both axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BorderStroke {
    pub color: [u8; 3],
    pub width: u8,
    /// 0: inside the cell; 1: centered on the edge.
    pub placement: u8,
    /// 0: below text; 1: above text (within the owning row).
    pub layer: u8,
}

/// Top, right, bottom, left, horizontal center, vertical center,
/// then left/right horizontal arms and top/bottom vertical arms.
pub type CellBorders = [Option<BorderStroke>; 10];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorderCommand {
    Query,
    Paint {
        mask: u16,
        stroke: Option<BorderStroke>,
        rows: u16,
        cols: u16,
    },
}

/// Parse a complete body; unknown versions and malformed bodies are ignored atomically.
pub fn parse(data: &[u8]) -> Option<BorderCommand> {
    if data.len() > 256 {
        return None;
    }
    let body = std::str::from_utf8(data).ok()?;
    let mut fields = body.split(';');
    if fields.next()? != "rio-border" || fields.next()? != "1" {
        return None;
    }
    let verb = fields.next()?;
    if verb == "q" {
        return fields.next().is_none().then_some(BorderCommand::Query);
    }
    if !matches!(verb, "set" | "clear") {
        return None;
    }
    let mask = decimal(fields.next()?, 1023)?;
    if mask == 0 {
        return None;
    }
    let stroke = if verb == "set" {
        let width = decimal(fields.next()?, 32)? as u8;
        if width == 0 {
            return None;
        }
        let rgb = fields.next()?;
        if rgb.len() != 6 || !rgb.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let color = [
            u8::from_str_radix(&rgb[0..2], 16).ok()?,
            u8::from_str_radix(&rgb[2..4], 16).ok()?,
            u8::from_str_radix(&rgb[4..6], 16).ok()?,
        ];
        Some(BorderStroke {
            color,
            width,
            placement: decimal(fields.next()?, 1)? as u8,
            layer: decimal(fields.next()?, 1)? as u8,
        })
    } else {
        None
    };
    let rows = decimal(fields.next()?, u16::MAX)?;
    let cols = decimal(fields.next()?, u16::MAX)?;
    if rows == 0 || cols == 0 || fields.next().is_some() {
        return None;
    }
    Some(BorderCommand::Paint {
        mask,
        stroke,
        rows,
        cols,
    })
}

fn decimal(field: &str, max: u16) -> Option<u16> {
    if field.is_empty() || !field.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    field.parse::<u16>().ok().filter(|&value| value <= max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_query_and_independent_strokes() {
        assert_eq!(parse(b"rio-border;1;q"), Some(BorderCommand::Query));
        assert_eq!(
            parse(b"rio-border;1;set;33;16;12AbEF;1;0;2;3"),
            Some(BorderCommand::Paint {
                mask: 33,
                stroke: Some(BorderStroke {
                    color: [0x12, 0xab, 0xef],
                    width: 16,
                    placement: 1,
                    layer: 0,
                }),
                rows: 2,
                cols: 3
            })
        );
        assert_eq!(
            parse(b"rio-border;1;clear;63;1;65535"),
            Some(BorderCommand::Paint {
                mask: 63,
                stroke: None,
                rows: 1,
                cols: 65535
            })
        );
    }

    #[test]
    fn accepts_half_centerline_corner_masks() {
        assert_eq!(
            parse(b"rio-border;1;set;640;16;abcdef;0;1;1;1"),
            Some(BorderCommand::Paint {
                mask: 640,
                stroke: Some(BorderStroke {
                    color: [0xab, 0xcd, 0xef],
                    width: 16,
                    placement: 0,
                    layer: 1,
                }),
                rows: 1,
                cols: 1,
            })
        );
        assert_eq!(
            parse(b"rio-border;1;clear;1023;1;1"),
            Some(BorderCommand::Paint {
                mask: 1023,
                stroke: None,
                rows: 1,
                cols: 1
            })
        );
    }

    #[test]
    fn rejects_malformed_commands_without_partial_effects() {
        for body in [
            "rio-border;2;q",
            "rio-border;1;q;extra",
            "rio-border;1;clear;0;1;1",
            "rio-border;1;clear;1024;1;1",
            "rio-border;1;clear;1;0;1",
            "rio-border;1;clear;1;1;65536",
            "rio-border;1;clear;+1;1;1",
            "rio-border;1;set;1;0;ffffff;0;0;1;1",
            "rio-border;1;set;1;33;ffffff;0;0;1;1",
            "rio-border;1;set;1;16;zzzzzz;0;0;1;1",
            "rio-border;1;set;1;16;ffffff;2;0;1;1",
            "rio-border;1;set;1;16;ffffff;0;2;1;1",
            "rio-border;1;set;1;16;ffffff;0;0;1;1;extra",
        ] {
            assert_eq!(parse(body.as_bytes()), None, "{body}");
        }
        assert_eq!(parse(&[b'0'; 257]), None);
        assert_eq!(parse(b"rio-border;1;\xff"), None);
    }
}
