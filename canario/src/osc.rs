//! Typed parsing helpers for OSC (Operating System Command) sequences.
//!
//! The Williams parser hands the dispatcher a `&[&[u8]]` of separator-split
//! parameter slices. Each helper here takes those raw slices and returns a
//! typed result for the corresponding OSC command, leaving the dispatcher in
//! `handler.rs` as a thin glue layer.

use std::str::FromStr;

use cursor_icon::CursorIcon;

use crate::ansi::CursorShape;
use crate::host::{ProgressReport, ProgressState};
use crate::square::Hyperlink;
use rio_core::color::{ColorRgb, NamedColor};
use rio_parser::simd_utf8;

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

/// OSC 7: working directory as a `file://` URL.
pub(super) fn parse_current_directory(param: &[u8]) -> Option<String> {
    let s = simd_utf8::from_utf8_fast(param).ok()?;
    let url = url::Url::parse(s).ok()?;
    let path = url.path();

    // The URL crate prepends a leading slash on Windows paths; strip it.
    #[cfg(windows)]
    let path = path.strip_prefix('/').unwrap_or(path);

    Some(path.to_owned())
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
