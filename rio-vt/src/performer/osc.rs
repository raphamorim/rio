//! Typed parsing helpers for OSC (Operating System Command) sequences.
//!
//! The Williams parser hands the dispatcher a `&[&[u8]]` of separator-split
//! parameter slices. Each helper here takes those raw slices and returns a
//! typed result for the corresponding OSC command, leaving the dispatcher in
//! `handler.rs` as a thin glue layer.

use std::str::FromStr;

use cursor_icon::CursorIcon;

use crate::ansi::CursorShape;
use crate::config::colors::{ColorRgb, NamedColor};
use crate::crosswords::square::Hyperlink;
use crate::event::{ProgressReport, ProgressState};
use crate::simd_utf8;

/// Either a concrete color value or a query for the current value.
pub(super) enum ColorSpec {
    Set(ColorRgb),
    Query,
}

pub(super) struct PaletteEntry {
    pub index: u8,
    pub spec: ColorSpec,
}

pub(super) struct DynamicColorEntry {
    pub index: NamedColor,
    pub dynamic_code: u16,
    pub spec: ColorSpec,
}

pub(super) enum ClipboardOp<'a> {
    Load { kind: u8 },
    Store { kind: u8, payload: &'a [u8] },
}

pub(super) enum PaletteReset {
    All,
    Indices(Vec<u8>),
}

/// Parse an OSC 133 semantic prompt sequence into the row mark it
/// should set, if any. `A` and `P` mark the cursor row as a prompt
/// (`P;k=c` / `P;k=s` as a continuation); the remaining subcommands
/// (`B`, `C`, `D`, `I`, `L`, `N`) and all `key=value` options are
/// accepted and ignored, matching the spec's leniency.
pub(super) fn parse_semantic_prompt(
    params: &[&[u8]],
) -> Option<crate::crosswords::grid::row::SemanticPrompt> {
    use crate::crosswords::grid::row::SemanticPrompt;

    let subcommand = *params.get(1)?.first()?;
    match subcommand {
        b'A' => Some(SemanticPrompt::Prompt),
        b'P' => {
            for option in &params[2..] {
                if let Some(kind) = option.strip_prefix(b"k=") {
                    if kind == b"c" || kind == b"s" {
                        return Some(SemanticPrompt::PromptContinuation);
                    }
                }
            }
            Some(SemanticPrompt::Prompt)
        }
        _ => None,
    }
}

/// Parse `OSC 1337 ; SetUserVar=name=<base64 value>`. The value is
/// base64 per iTerm2's spec; anything undecodable is dropped.
pub(super) fn parse_set_user_var(params: &[&[u8]]) -> Option<(String, String)> {
    let payload = params.get(1)?.strip_prefix(b"SetUserVar=")?;
    let mut parts = payload.splitn(2, |byte| *byte == b'=');
    let name = simd_utf8::from_utf8_fast(parts.next()?).ok()?;
    if name.is_empty() {
        return None;
    }
    let encoded = parts.next()?;
    let decoded = crate::simd_base64::decode(encoded)?;
    let value = String::from_utf8(decoded).ok()?;
    Some((name.to_string(), value))
}

/// Parse an `xterm`-style color value (`#rgb`, `#rrggbb`, `rgb:r/g/b`).
pub(super) fn xparse_color(color: &[u8]) -> Option<ColorRgb> {
    if !color.is_empty() && color[0] == b'#' {
        parse_legacy_color(&color[1..])
    } else if color.len() >= 4 && &color[..4] == b"rgb:" {
        parse_rgb_color(&color[4..])
    } else {
        None
    }
}

/// Parse colors in `rgb:r(rrr)/g(ggg)/b(bbb)` format.
fn parse_rgb_color(color: &[u8]) -> Option<ColorRgb> {
    let colors = simd_utf8::from_utf8_fast(color)
        .ok()?
        .split('/')
        .collect::<Vec<_>>();

    if colors.len() != 3 {
        return None;
    }

    // Scale values instead of filling with `0`s.
    let scale = |input: &str| {
        if input.len() > 4 {
            None
        } else {
            let max = u32::pow(16, input.len() as u32) - 1;
            let value = u32::from_str_radix(input, 16).ok()?;
            Some((255 * value / max) as u8)
        }
    };

    Some(ColorRgb {
        r: scale(colors[0])?,
        g: scale(colors[1])?,
        b: scale(colors[2])?,
    })
}

/// Parse colors in `#r(rrr)g(ggg)b(bbb)` format.
fn parse_legacy_color(color: &[u8]) -> Option<ColorRgb> {
    let item_len = color.len() / 3;

    // Truncate/Fill to two byte precision.
    let color_from_slice = |slice: &[u8]| {
        let col =
            usize::from_str_radix(simd_utf8::from_utf8_fast(slice).ok()?, 16).ok()? << 4;
        Some((col >> (4 * slice.len().saturating_sub(1))) as u8)
    };

    Some(ColorRgb {
        r: color_from_slice(&color[0..item_len])?,
        g: color_from_slice(&color[item_len..item_len * 2])?,
        b: color_from_slice(&color[item_len * 2..])?,
    })
}

pub(super) fn parse_number(input: &[u8]) -> Option<u8> {
    if input.is_empty() {
        return None;
    }
    let mut num: u8 = 0;
    for c in input {
        let c = *c as char;
        if let Some(digit) = c.to_digit(10) {
            num = num
                .checked_mul(10)
                .and_then(|v| v.checked_add(digit as u8))?
        } else {
            return None;
        }
    }
    Some(num)
}

/// OSC 0 / OSC 2: window title set as `;`-joined params.
pub(super) fn parse_title(params: &[&[u8]]) -> Option<String> {
    if params.len() < 2 {
        return None;
    }
    Some(
        params[1..]
            .iter()
            .flat_map(|x| simd_utf8::from_utf8_fast(x))
            .collect::<Vec<&str>>()
            .join(";")
            .trim()
            .to_owned(),
    )
}

/// OSC 4: a list of `(index, color | "?")` pairs in `params[1..]`.
pub(super) fn parse_palette_entries(params: &[&[u8]]) -> Option<Vec<PaletteEntry>> {
    if params.len() <= 1 || params.len().is_multiple_of(2) {
        return None;
    }

    let mut out = Vec::with_capacity(params.len() / 2);
    for chunk in params[1..].chunks(2) {
        let index = parse_number(chunk[0])?;
        let spec = if chunk[1] == b"?" {
            ColorSpec::Query
        } else {
            ColorSpec::Set(xparse_color(chunk[1])?)
        };
        out.push(PaletteEntry { index, spec });
    }
    Some(out)
}

/// The only URL scheme rio-vt ever parses.
const FILE_SCHEME: &str = "file://";

/// OSC 7: working directory as a `file://` URL.
///
/// The payload is `file://<host>/<path>`, where the host is informational and
/// the path is percent-encoded. Parsed by hand rather than with a URL crate:
/// this is the only URL the terminal core looks at, and a general parser costs
/// an IDNA/Unicode stack just to reach `.path()`.
pub(super) fn parse_current_directory(param: &[u8]) -> Option<String> {
    let s = simd_utf8::from_utf8_fast(param).ok()?;

    // Schemes are case-insensitive.
    let after_scheme = s
        .get(..FILE_SCHEME.len())
        .filter(|scheme| scheme.eq_ignore_ascii_case(FILE_SCHEME))
        .map(|_| &s[FILE_SCHEME.len()..])?;

    // Skip the host: the path begins at its trailing slash. A payload with no
    // path at all leaves nothing to report.
    let path = &after_scheme[after_scheme.find('/')?..];

    // A query or fragment is not part of the path.
    let path = &path[..path.find(['?', '#']).unwrap_or(path.len())];

    // Windows paths arrive as `/C:/...`; drop the leading slash.
    #[cfg(windows)]
    let path = path.strip_prefix('/').unwrap_or(path);

    percent_decode(path)
}

/// Percent-decode a URL path. Invalid escapes are passed through as written,
/// which is friendlier than dropping the directory over one stray `%`.
fn percent_decode(path: &str) -> Option<String> {
    if !path.contains('%') {
        return Some(path.to_owned());
    }

    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        // Only `%` followed by two hex digits is an escape. `from_str_radix`
        // would accept a leading `+` here, decoding `%+A` to a byte.
        let escape = (bytes[index] == b'%')
            .then(|| {
                let hex = bytes.get(index + 1..index + 3)?;
                let high = (hex[0] as char).to_digit(16)?;
                let low = (hex[1] as char).to_digit(16)?;
                Some((high * 16 + low) as u8)
            })
            .flatten();

        match escape {
            Some(byte) => {
                out.push(byte);
                index += 3;
            }
            None => {
                out.push(bytes[index]);
                index += 1;
            }
        }
    }

    String::from_utf8(out).ok()
}

/// OSC 8: extract `id=...` from `key=val:key=val` link params.
pub(super) fn parse_hyperlink_id(link_params: &[u8]) -> Option<&str> {
    link_params
        .split(|&b| b == b':')
        .find_map(|kv| kv.strip_prefix(b"id="))
        .and_then(|kv| simd_utf8::from_utf8_fast(kv).ok())
}

/// Construct a [`Hyperlink`] from the link params + URI bytes. Returns
/// `None` for an empty URI (caller should clear the active hyperlink).
pub(super) fn parse_hyperlink(link_params: &[u8], uri_param: &[u8]) -> Option<Hyperlink> {
    let uri = simd_utf8::from_utf8_fast(uri_param).unwrap_or_default();
    if uri.is_empty() {
        return None;
    }
    Some(Hyperlink::new(parse_hyperlink_id(link_params), uri))
}

/// OSC 9;4 — ConEmu/Windows-Terminal progress reporting.
/// Format: `9;4;<state>;<progress>` (progress optional).
pub(super) fn parse_progress_report(params: &[&[u8]]) -> Option<ProgressReport> {
    if params.len() < 3 || params[1] != b"4" {
        return None;
    }
    let state = match params[2] {
        b"0" => ProgressState::Remove,
        b"1" => ProgressState::Set,
        b"2" => ProgressState::Error,
        b"3" => ProgressState::Indeterminate,
        b"4" => ProgressState::Pause,
        _ => return None,
    };
    let progress = if params.len() >= 4 {
        parse_number(params[3]).map(|p| p.min(100))
    } else {
        None
    };
    Some(ProgressReport { state, progress })
}

/// OSC 10/11/12: dynamic color set/query, applied to consecutive named
/// colors starting at `dynamic_code - 10`.
pub(super) fn parse_dynamic_colors(params: &[&[u8]]) -> Option<Vec<DynamicColorEntry>> {
    if params.len() < 2 {
        return None;
    }
    let base_code = parse_number(params[0])? as u16;
    let mut out = Vec::with_capacity(params.len() - 1);
    for (dynamic_code, param) in (base_code..).zip(params[1..].iter()) {
        // 10 is the first dynamic color (foreground).
        let offset = (dynamic_code as usize).checked_sub(10)?;
        let index_usize = NamedColor::Foreground as usize + offset;
        if index_usize > NamedColor::Cursor as usize {
            return None;
        }
        let index = match offset {
            0 => NamedColor::Foreground,
            1 => NamedColor::Background,
            2 => NamedColor::Cursor,
            _ => return None,
        };
        let spec = if *param == b"?" {
            ColorSpec::Query
        } else if let Some(c) = xparse_color(param) {
            ColorSpec::Set(c)
        } else {
            return None;
        };
        out.push(DynamicColorEntry {
            index,
            dynamic_code,
            spec,
        });
    }
    Some(out)
}

/// OSC 22: mouse cursor icon name.
pub(super) fn parse_mouse_cursor_icon(param: &[u8]) -> Option<CursorIcon> {
    let shape = simd_utf8::from_utf8_lossy_fast(param);
    CursorIcon::from_str(&shape).ok()
}

/// OSC 50: `CursorShape=N` text cursor selector.
pub(super) fn parse_cursor_shape(params: &[&[u8]]) -> Option<CursorShape> {
    if params.len() < 2 || params[1].len() < 13 || params[1][0..12] != *b"CursorShape=" {
        return None;
    }
    match params[1][12] as char {
        '0' => Some(CursorShape::Block),
        '1' => Some(CursorShape::Beam),
        '2' => Some(CursorShape::Underline),
        _ => None,
    }
}

/// OSC 52: clipboard load (`?`) or store (base64 payload).
pub(super) fn parse_clipboard<'a>(params: &[&'a [u8]]) -> Option<ClipboardOp<'a>> {
    if params.len() < 3 {
        return None;
    }
    let kind = *params[1].first().unwrap_or(&b'c');
    Some(if params[2] == b"?" {
        ClipboardOp::Load { kind }
    } else {
        ClipboardOp::Store {
            kind,
            payload: params[2],
        }
    })
}

/// OSC 104: reset palette colors. Empty/omitted parameter list means "all".
pub(super) fn parse_palette_reset(params: &[&[u8]]) -> PaletteReset {
    if params.len() == 1 || params[1].is_empty() {
        return PaletteReset::All;
    }
    let indices = params[1..].iter().filter_map(|p| parse_number(p)).collect();
    PaletteReset::Indices(indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cwd(payload: &str) -> Option<String> {
        parse_current_directory(payload.as_bytes())
    }

    #[cfg(not(windows))]
    #[test]
    fn current_directory_from_file_url() {
        // Empty host is the common shape; a named host is informational.
        assert_eq!(cwd("file:///home/user"), Some("/home/user".into()));
        assert_eq!(cwd("file://localhost/home/user"), Some("/home/user".into()));
        assert_eq!(cwd("file:///"), Some("/".into()));

        // Schemes are case-insensitive.
        assert_eq!(cwd("FILE:///home/user"), Some("/home/user".into()));
    }

    #[cfg(windows)]
    #[test]
    fn current_directory_strips_windows_leading_slash() {
        assert_eq!(cwd("file:///C:/Users/user"), Some("C:/Users/user".into()));
    }

    #[test]
    fn current_directory_rejects_non_file_payloads() {
        // Another scheme is not a working directory, and a bare path has no
        // scheme to speak of.
        assert_eq!(cwd("http://example.com/home/user"), None);
        assert_eq!(cwd("/home/user"), None);
        assert_eq!(cwd(""), None);

        // No path component at all.
        assert_eq!(cwd("file://localhost"), None);
    }

    #[cfg(not(windows))]
    #[test]
    fn current_directory_percent_decodes() {
        assert_eq!(
            cwd("file:///home/My%20Files"),
            Some("/home/My Files".into())
        );
        // Multi-byte UTF-8 arrives as a run of escapes.
        assert_eq!(
            cwd("file:///home/%E1%BF%AC%CF%8C%CE%B4%CE%BF%CF%82"),
            Some("/home/Ῥόδος".into())
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn current_directory_keeps_malformed_escapes() {
        // A stray or truncated `%` is shown as sent rather than discarded.
        assert_eq!(cwd("file:///home/100%"), Some("/home/100%".into()));
        assert_eq!(cwd("file:///home/a%zz"), Some("/home/a%zz".into()));
        assert_eq!(cwd("file:///home/a%2"), Some("/home/a%2".into()));
    }

    #[cfg(not(windows))]
    #[test]
    fn current_directory_ignores_query_and_fragment() {
        assert_eq!(cwd("file:///home/user?x=1"), Some("/home/user".into()));
        assert_eq!(cwd("file:///home/user#frag"), Some("/home/user".into()));
    }

    #[cfg(not(windows))]
    #[test]
    fn current_directory_rejects_non_hex_escapes() {
        // `from_str_radix` accepts a leading sign, so `%+A` must not decode.
        assert_eq!(cwd("file:///home/a%+A"), Some("/home/a%+A".into()));
        assert_eq!(cwd("file:///home/a% 1"), Some("/home/a% 1".into()));
    }

    #[test]
    fn current_directory_rejects_invalid_utf8_escapes() {
        // Decoding must still yield valid UTF-8.
        assert_eq!(cwd("file:///home/%FF%FE"), None);
    }

    /// The unit tests above call the helper directly; this drives a real
    /// terminal through the parser so the OSC 7 wiring is covered too.
    #[cfg(not(windows))]
    #[test]
    fn osc7_sets_current_directory_end_to_end() {
        use crate::ansi::CursorShape;
        use crate::crosswords::{Crosswords, CrosswordsSize};
        use crate::event::{VoidListener, WindowId};
        use crate::performer::handler::Processor;

        let mut term = Crosswords::new(
            CrosswordsSize::new(20, 5),
            CursorShape::Block,
            VoidListener,
            WindowId::from(0),
            0,
            10,
        );
        let mut processor = Processor::default();

        processor.advance(&mut term, b"\x1b]7;file:///home/My%20Files\x1b\\");
        assert_eq!(
            term.current_directory.as_deref(),
            Some(std::path::Path::new("/home/My Files"))
        );
    }
}
