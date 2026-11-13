//! Typed parsing helpers for OSC (Operating System Command) sequences.
//!
//! The Williams parser hands the dispatcher a `&[&[u8]]` of separator-split
//! parameter slices. Each helper here takes those raw slices and returns a
//! typed result for the corresponding OSC command, leaving the dispatcher in
//! `handler.rs` as a thin glue layer.

use std::borrow::Cow;
use std::str::FromStr;

use cursor_icon::CursorIcon;

use crate::ansi::CursorShape;
use crate::config::colors::{ColorRgb, NamedColor};
use crate::crosswords::square::Hyperlink;
use crate::event::{
    BlockedKind, ProgramState, ProgramStatus, ProgressReport, ProgressState,
};
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
        {
            let digit = c.to_digit(10)?;
            num = num
                .checked_mul(10)
                .and_then(|v| v.checked_add(digit as u8))?
        }
    }
    Some(num)
}

/// Decodes a free-text payload, replacing invalid UTF-8.
pub(super) fn text_lossy(bytes: &[u8]) -> Cow<'_, str> {
    match simd_utf8::from_utf8_fast(bytes) {
        Ok(text) => Cow::Borrowed(text),
        Err(_) => String::from_utf8_lossy(bytes),
    }
}

/// Longest title (in bytes) a program can set. Longer titles are
/// ignored rather than truncated, so a cut never lands mid-word or
/// mid-character.
pub(super) const MAX_TITLE_BYTES: usize = 255;

/// OSC 0 / OSC 2: the window title, everything after `0;` or `2;`.
/// A title with invalid UTF-8 or longer than [`MAX_TITLE_BYTES`] is
/// rejected whole: a window title is too visible to show a guessed-at
/// version of what the program meant.
pub(super) fn parse_title(params: &[&[u8]]) -> Option<String> {
    let Some(title) = params.get(1).copied() else {
        tracing::warn!("ignoring OSC title without a payload");
        return None;
    };
    if title.len() > MAX_TITLE_BYTES {
        tracing::warn!(
            "ignoring OSC title of {} bytes (limit {MAX_TITLE_BYTES})",
            title.len()
        );
        return None;
    }
    match simd_utf8::from_utf8_fast(title) {
        Ok(title) => Some(title.to_owned()),
        Err(_) => {
            tracing::warn!("ignoring OSC title with invalid UTF-8");
            None
        }
    }
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

/// The URL schemes rio-vt ever parses, both OSC 7 working-directory
/// carriers: RFC 8089 file URLs and the `kitty-shell-cwd` scheme
/// (a raw, unencoded path) that kitty defined and several shell
/// integrations emit.
const FILE_SCHEME: &str = "file://";
const KITTY_SHELL_CWD_SCHEME: &str = "kitty-shell-cwd://";

/// Strip `scheme` from the front of `s`, case-insensitively.
fn strip_scheme<'a>(s: &'a str, scheme: &str) -> Option<&'a str> {
    s.get(..scheme.len())
        .filter(|prefix| prefix.eq_ignore_ascii_case(scheme))
        .map(|_| &s[scheme.len()..])
}

/// OSC 7: working directory as a `file://` URL (percent-encoded path)
/// or a `kitty-shell-cwd://` payload (raw path, no encoding). Parsed
/// by hand rather than with a URL crate: these are the only URLs the
/// terminal core looks at, and a general parser costs an IDNA/Unicode
/// stack just to reach `.path()`.
///
/// Anything on the other end of the PTY can send this, including a shell on the
/// far side of an ssh session, so a directory that belongs to another machine is
/// refused rather than reported as ours.
///
/// `Some(None)` is an empty payload: the program no longer knows its
/// directory, so the stored one is stale. `None` is a malformed
/// payload, which changes nothing.
pub(super) fn parse_current_directory(param: &[u8]) -> Option<Option<String>> {
    if param.is_empty() {
        return Some(None);
    }
    let dir = parse_directory_url(param);
    if dir.is_none() {
        // Debug, not warn: a broken shell integration repeats this at
        // every prompt. The non-local host case warns on its own.
        tracing::debug!("ignoring malformed OSC 7 payload {:?}", text_lossy(param));
    }
    dir.map(Some)
}

fn parse_directory_url(param: &[u8]) -> Option<String> {
    let s = simd_utf8::from_utf8_fast(param).ok()?;

    let (after_scheme, percent_encoded) = match strip_scheme(s, FILE_SCHEME) {
        Some(rest) => (rest, true),
        None => (strip_scheme(s, KITTY_SHELL_CWD_SCHEME)?, false),
    };

    // The path begins at the host's trailing slash. A payload with no path at
    // all leaves nothing to report.
    let path_start = after_scheme.find('/')?;
    let host = &after_scheme[..path_start];
    let path = &after_scheme[path_start..];

    if !host_is_local(host) {
        tracing::warn!("ignoring OSC 7 for non-local host {host:?}");
        return None;
    }

    // Windows paths arrive as `/C:/...` in both flavors; drop the
    // leading slash.
    #[cfg(windows)]
    let path = path.strip_prefix('/').unwrap_or(path);

    if !percent_encoded {
        // The kitty flavor is the path verbatim: `?`, `#` and `%` are
        // path bytes, not URL syntax.
        return Some(path.to_string());
    }

    // A query or fragment is not part of the path.
    let path = &path[..path.find(['?', '#']).unwrap_or(path.len())];

    percent_decode(path)
}

/// Whether an OSC 7 host names this machine.
///
/// An empty host is this machine by definition (RFC 8089), which is also the
/// form shells produce from a bare `file://$PWD`.
fn host_is_local(host: &str) -> bool {
    // Tolerate the FQDN root dot.
    let host = host.strip_suffix('.').unwrap_or(host);

    if host.is_empty() || host.eq_ignore_ascii_case("localhost") {
        return true;
    }

    match local_hostname() {
        Some(local) => host.eq_ignore_ascii_case(&local),
        // With no hostname to compare against, take the shell at its word
        // rather than dropping the directory outright.
        None => true,
    }
}

/// This machine's hostname, read per check: one syscall, where a
/// process-lifetime cache would keep rejecting a shell's cwd reports
/// after the hostname changes mid-session (macOS mDNS renames).
fn local_hostname() -> Option<String> {
    read_hostname()
}

#[cfg(unix)]
fn read_hostname() -> Option<String> {
    // `_POSIX_HOST_NAME_MAX` is 255; the extra byte is for the terminator,
    // which a truncating implementation is not required to write.
    let mut buffer = [0_u8; 256];
    // SAFETY: writing at most `buffer.len()` bytes into a buffer of that size.
    let result = unsafe {
        libc::gethostname(buffer.as_mut_ptr() as *mut libc::c_char, buffer.len())
    };
    if result != 0 {
        return None;
    }

    let end = buffer.iter().position(|byte| *byte == 0)?;
    std::str::from_utf8(&buffer[..end])
        .ok()
        .filter(|hostname| !hostname.is_empty())
        .map(str::to_owned)
}

#[cfg(not(unix))]
fn read_hostname() -> Option<String> {
    // No lookup here, so `host_is_local` falls back to accepting the payload,
    // which is what rio did on every platform before.
    None
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

/// OSC 8: the raw `id=...` value from `key=val:key=val` link params.
/// An empty value is the same as no id, and when the key repeats the
/// last non-empty value wins.
fn hyperlink_id_bytes(link_params: &[u8]) -> Option<&[u8]> {
    link_params
        .split(|&b| b == b':')
        .rev()
        .filter_map(|kv| kv.strip_prefix(b"id="))
        .find(|id| !id.is_empty())
}

/// OSC 8: the id to store, if the program sent one. The id is an
/// opaque token, so invalid UTF-8 is replaced rather than making the
/// id vanish.
pub(super) fn parse_hyperlink_id(link_params: &[u8]) -> Option<Cow<'_, str>> {
    hyperlink_id_bytes(link_params).map(text_lossy)
}

/// Longest OSC 8 payload accepted: everything after `8;`, that is the
/// link params, the `;` after them and the URI, as one budget shared
/// by both parts. The figure is a design cap, 2 KiB less a byte, and
/// is not tied to this parser's fixed OSC buffer, which also holds the
/// `8;` prefix. A URI is stored for as long as the cells carrying it
/// live, so the budget also keeps a link's raw size in the grid bounded.
pub(crate) const MAX_HYPERLINK_BYTES: usize = 2047;

/// OSC 8: `Some(None)` closes the link, `Some(Some(_))` opens one and
/// `None` ignores the sequence. Two sequences are ignored: an empty URI
/// with an `id`, and a payload over [`MAX_HYPERLINK_BYTES`], which
/// neither opens a link nor closes the current one.
pub(super) fn parse_hyperlink(
    link_params: &[u8],
    uri: &[u8],
) -> Option<Option<Hyperlink>> {
    let len = link_params.len() + 1 + uri.len();
    if len > MAX_HYPERLINK_BYTES {
        tracing::debug!("ignoring OSC 8 of {len} bytes (limit {MAX_HYPERLINK_BYTES})");
        return None;
    }
    if uri.is_empty() {
        let id_sent = hyperlink_id_bytes(link_params).is_some();
        if id_sent {
            tracing::debug!("ignoring OSC 8 with an id and no URI");
        }
        return (!id_sent).then_some(None);
    }
    let id = parse_hyperlink_id(link_params);
    Some(Some(Hyperlink::new(id.as_deref(), &*text_lossy(uri))))
}

/// OSC 9 notification: the body is everything after `9;`. The parser
/// keeps the payload verbatim only from the second `;` on (it has to see
/// the sub-command to tell 9;4 apart), so a body with a `;` of its own
/// arrives as two params and is joined back here.
pub(super) fn parse_notification_body(params: &[&[u8]]) -> String {
    params[1..]
        .iter()
        .map(|part| text_lossy(part))
        .collect::<Vec<_>>()
        .join(";")
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

/// An OSC 7501 sequence, after validation.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ProgramStatusRequest {
    /// `OSC 7501 ; ? ST`: feature detection, answered in kind.
    Query,
    Report(ProgramStatus),
}

// OSC 7501 limits (Program Status Protocol, rev 0.4). A report over
// any of them is discarded whole.
const PROGRAM_STATUS_MAX_SEQUENCE: usize = 4096;
const PROGRAM_STATUS_MAX_KEY: usize = 16;
const PROGRAM_STATUS_MAX_APP: usize = 32;
const PROGRAM_STATUS_MAX_MSG: (usize, usize) = (2732, 2048);
const PROGRAM_STATUS_MAX_TITLE: (usize, usize) = (256, 192);
const PROGRAM_STATUS_MAX_ID: usize = 128;
const PROGRAM_STATUS_MAX_SEGMENT: usize = 32;
const PROGRAM_STATUS_MAX_DEPTH: usize = 8;

/// OSC 7501 — program status (`state=working:app=cargo:progress=40`).
///
/// The body is `key=value` pairs separated by `:`. Malformed pairs and
/// unknown keys are skipped and a repeated key keeps its last value. The
/// report is dropped whole when the final `state` is missing or unknown,
/// the final `id` fails the grammar, any base64 is undecodable or decodes
/// to control characters, or any size limit is exceeded.
///
/// `terminator_len` is 1 for BEL and 2 for `ESC \`, so the 4096-byte cap
/// applies to the sequence exactly as it arrived.
pub(super) fn parse_program_status(
    params: &[&[u8]],
    terminator_len: usize,
) -> Option<ProgramStatusRequest> {
    // Values never contain `;`, but rejoin so a stray one fails the
    // alphabet check instead of silently truncating the body. The usual
    // single-param body is borrowed, not copied.
    let body: Cow<str> = match params {
        [_, body] => simd_utf8::from_utf8_fast(body).ok()?.into(),
        _ => params
            .get(1..)?
            .iter()
            .map(|param| simd_utf8::from_utf8_fast(param).ok())
            .collect::<Option<Vec<_>>>()?
            .join(";")
            .into(),
    };
    // `ESC ] 7501 ;` is 7 bytes.
    if body.len() + 7 + terminator_len > PROGRAM_STATUS_MAX_SEQUENCE {
        return None;
    }
    if body.trim() == "?" {
        return Some(ProgramStatusRequest::Query);
    }

    // `state` and `id` keep their last value, valid or not, and are
    // judged once the body is read: `state=bogus:state=idle` is idle.
    let mut state: Option<Option<ProgramState>> = None;
    let mut id: Option<Option<String>> = None;
    let mut kind = None;
    let mut app = None;
    let mut msg = None;
    let mut title = None;
    let mut progress = None;

    for pair in body.split(':') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        if key.len() > PROGRAM_STATUS_MAX_KEY {
            return None;
        }
        let well_formed =
            !value.is_empty() && value.bytes().all(is_program_status_value_byte);
        match key {
            // A malformed `id` must not fall back to the root record, so
            // unlike other keys it is recorded as invalid, not skipped.
            "id" => {
                if value.len() > PROGRAM_STATUS_MAX_ID {
                    return None;
                }
                id = Some(
                    (well_formed && is_valid_program_status_id(value))
                        .then(|| value.to_string()),
                );
            }
            _ if !well_formed => continue,
            "state" => {
                state = Some(match value {
                    "idle" => Some(ProgramState::Idle),
                    "working" => Some(ProgramState::Working),
                    "done" => Some(ProgramState::Done),
                    "blocked" => Some(ProgramState::Blocked),
                    "error" => Some(ProgramState::Error),
                    "clear" => Some(ProgramState::Clear),
                    _ => None,
                })
            }
            // Unrecognized kinds are treated as absent.
            "kind" => {
                kind = match value {
                    "permission" => Some(BlockedKind::Permission),
                    "question" => Some(BlockedKind::Question),
                    "auth" => Some(BlockedKind::Auth),
                    _ => None,
                }
            }
            "app" => {
                if value.len() > PROGRAM_STATUS_MAX_APP {
                    return None;
                }
                app = Some(value.to_string());
            }
            "msg" => {
                msg = Some(decode_program_status_text(value, PROGRAM_STATUS_MAX_MSG)?)
            }
            "title" => {
                title = Some(decode_program_status_text(value, PROGRAM_STATUS_MAX_TITLE)?)
            }
            // Not an integer in 0..=100: a malformed pair, skipped.
            "progress" => {
                if let Some(p) = value.parse::<u8>().ok().filter(|p| *p <= 100) {
                    progress = Some(p);
                }
            }
            _ => {}
        }
    }

    let state = state??;
    let id = match id {
        None => None,
        Some(valid) => Some(valid?),
    };
    Some(ProgramStatusRequest::Report(ProgramStatus {
        state,
        kind: kind.filter(|_| state == ProgramState::Blocked),
        app,
        msg,
        title,
        progress: progress
            .filter(|_| matches!(state, ProgramState::Working | ProgramState::Blocked)),
        id,
    }))
}

/// The value alphabet: letters, digits and `_ . , + / = -`.
fn is_program_status_value_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"_.,+/=-".contains(&byte)
}

/// Decode a base64 `msg`/`title`, enforcing its (encoded, decoded) caps
/// and refusing control characters, which would let a status line carry
/// its own escape sequences into the embedder's UI.
fn decode_program_status_text(
    value: &str,
    (encoded, decoded): (usize, usize),
) -> Option<String> {
    if value.len() > encoded {
        return None;
    }
    let bytes = crate::simd_base64::decode(value.as_bytes())?;
    if bytes.len() > decoded {
        return None;
    }
    let text = String::from_utf8(bytes).ok()?;
    if text.chars().any(char::is_control) {
        return None;
    }
    Some(text)
}

/// `seg(/seg)*`, each segment 1-32 bytes of `[A-Za-z0-9_.+-]`, at most
/// 8 deep (the 128-byte total is checked by the caller).
fn is_valid_program_status_id(id: &str) -> bool {
    id.split('/').count() <= PROGRAM_STATUS_MAX_DEPTH
        && id.split('/').all(|segment| {
            (1..=PROGRAM_STATUS_MAX_SEGMENT).contains(&segment.len())
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.+-".contains(&b))
        })
}

/// OSC 9;9: ConEmu/Windows-Terminal working directory report, the
/// Windows counterpart of OSC 7. Format: `9;9;<path>`, with the path
/// optionally wrapped in double quotes and travelling verbatim (no URL
/// encoding): the parser keeps everything after `9;9;` as one param.
///
/// The sequence carries no host field, so unlike OSC 7 a remote shell
/// cannot be told apart from a local one (ConEmu and Windows Terminal
/// accept the same). Requiring an absolute path at least drops
/// free-text payloads and relative garbage.
///
/// As with OSC 7, an empty path (quoted or not) is `Some(None)`: the
/// program no longer knows its directory.
pub(super) fn parse_conemu_working_directory(params: &[&[u8]]) -> Option<Option<String>> {
    if params.len() < 3 || params[1] != b"9" {
        return None;
    }
    let path = simd_utf8::from_utf8_fast(params[2]).ok()?;
    let path = path
        .strip_prefix('"')
        .and_then(|p| p.strip_suffix('"'))
        .unwrap_or(path);
    if path.is_empty() {
        return Some(None);
    }
    if !is_absolute_path(path) {
        tracing::debug!("ignoring OSC 9;9 with non-absolute path {path:?}");
        return None;
    }
    Some(Some(path.to_string()))
}

/// Whether an OSC 9 sub-command (the text before the second `;`) is one
/// of the ConEmu extensions: 1 sleep, 2 message box, 3 tab title, 4
/// progress, 5-7 GuiMacro, 8 environment variable, 9 working directory,
/// 10-12 xterm emulation. Anything else is an iTerm2 notification body.
pub(super) fn is_conemu_subcommand(sub: &[u8]) -> bool {
    matches!(
        sub,
        b"1" | b"2"
            | b"3"
            | b"4"
            | b"5"
            | b"6"
            | b"7"
            | b"8"
            | b"9"
            | b"10"
            | b"11"
            | b"12"
    )
}

/// Whether `path` is absolute in either flavor a cwd report can carry:
/// a POSIX `/` root or a Windows drive-letter root. UNC (`\\server`)
/// is refused: it names another machine, which the host-checked OSC 7
/// flavors refuse too.
fn is_absolute_path(path: &str) -> bool {
    if path.starts_with('/') {
        return true;
    }
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
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
        } else {
            let c = xparse_color(param)?;
            ColorSpec::Set(c)
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

    fn status(body: &str) -> Option<ProgramStatusRequest> {
        let params: Vec<&[u8]> = vec![b"7501", body.as_bytes()];
        parse_program_status(&params, 2)
    }

    fn report(body: &str) -> ProgramStatus {
        match status(body) {
            Some(ProgramStatusRequest::Report(report)) => report,
            other => panic!("expected a report for {body:?}, got {other:?}"),
        }
    }

    #[test]
    fn program_status_full_report() {
        // msg = "Apply plan?", title = "tf"
        let r = report("state=blocked:kind=permission:app=terraform:msg=QXBwbHkgcGxhbj8=:title=dGY=:id=plan/apply:progress=40");
        assert_eq!(r.state, ProgramState::Blocked);
        assert_eq!(r.kind, Some(BlockedKind::Permission));
        assert_eq!(r.app.as_deref(), Some("terraform"));
        assert_eq!(r.msg.as_deref(), Some("Apply plan?"));
        assert_eq!(r.title.as_deref(), Some("tf"));
        assert_eq!(r.id.as_deref(), Some("plan/apply"));
        assert_eq!(r.progress, Some(40));
    }

    #[test]
    fn program_status_minimal_and_query() {
        let r = report("state=idle");
        assert_eq!(r.state, ProgramState::Idle);
        assert_eq!(
            (r.kind, r.app, r.msg, r.id, r.progress),
            (None, None, None, None, None)
        );
        assert_eq!(status("?"), Some(ProgramStatusRequest::Query));
        assert_eq!(report("state=clear").state, ProgramState::Clear);
    }

    #[test]
    fn program_status_lenient_parts() {
        // Whitespace stripped, malformed pairs and unknown keys skipped,
        // last repeat wins, unknown kind is absent.
        let r = report(" state = working : junk : future=1 : app=a : app=b ");
        assert_eq!(r.state, ProgramState::Working);
        assert_eq!(r.app.as_deref(), Some("b"));
        assert_eq!(report("state=blocked:kind=telepathy").kind, None);
        // kind only applies to blocked, progress to working/blocked.
        assert_eq!(report("state=idle:kind=question").kind, None);
        assert_eq!(report("state=done:progress=50").progress, None);
        assert_eq!(report("state=working:progress=101").progress, None);
    }

    #[test]
    fn program_status_discards_whole_report() {
        let long_app = format!("state=idle:app={}", "a".repeat(33));
        let long_body = format!("state=idle:pad={}", "a".repeat(4096));
        for body in [
            "",
            "app=cargo",                         // no state
            "state=sleeping",                    // unknown state
            "state=idle:msg=A",                  // undecodable base64
            "state=idle:msg=G1sybQ==",           // decodes to ESC [ 2 m
            "state=idle:id=a//b",                // empty segment
            "state=idle:id=a/b c",               // byte outside the id grammar
            "state=idle:id=1/2/3/4/5/6/7/8/9",   // nine levels deep
            "state=idle:averyveryverylongkey=1", // key over 16 bytes
            &long_app,
            &long_body,
        ] {
            assert_eq!(status(body), None, "{body}");
        }
    }

    #[test]
    fn program_status_last_repeat_wins() {
        assert_eq!(report("state=bogus:state=idle").state, ProgramState::Idle);
        // An empty or out-of-alphabet value is a malformed pair: skipped.
        assert_eq!(report("state=idle:state=").state, ProgramState::Idle);
        assert_eq!(report("state=idle:state=!").state, ProgramState::Idle);
        assert_eq!(status("state=idle:state=bogus"), None);
        assert_eq!(report("state=idle:id=a//b:id=ok").id.as_deref(), Some("ok"));
        assert_eq!(status("state=idle:id=ok:id=a//b"), None);
        // A malformed progress is skipped, not a reset to indeterminate.
        assert_eq!(
            report("state=working:progress=40:progress=abc").progress,
            Some(40)
        );
        assert_eq!(
            report("state=working:progress=40:progress=101").progress,
            Some(40)
        );
    }

    #[test]
    fn program_status_sequence_cap_counts_the_terminator() {
        // 7 bytes of `ESC ] 7501 ;` + body + terminator = 4096 exactly.
        let fill = |len: usize| format!("state=idle:p={}", "a".repeat(len - 13));
        let bel_max = fill(4096 - 7 - 1);
        let params: Vec<&[u8]> = vec![b"7501", bel_max.as_bytes()];
        assert!(parse_program_status(&params, 1).is_some());
        assert!(parse_program_status(&params, 2).is_none());
    }

    #[test]
    fn program_status_skips_malformed_pairs() {
        // A value outside the alphabet makes only that pair malformed.
        let r = report("state=working:msg=!!!:app=cargo");
        assert_eq!((r.msg, r.app.as_deref()), (None, Some("cargo")));
    }

    fn cwd(payload: &str) -> Option<String> {
        parse_current_directory(payload.as_bytes()).flatten()
    }

    #[test]
    fn current_directory_empty_payload_clears() {
        // Empty means "unknown": the caller clears the stored directory.
        assert_eq!(parse_current_directory(b""), Some(None));
        // Malformed means "ignore".
        assert_eq!(parse_current_directory(b"not-a-url"), None);
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

    #[cfg(unix)]
    #[test]
    fn current_directory_accepts_local_hosts() {
        // Empty host means this machine (RFC 8089), as does localhost.
        assert_eq!(cwd("file:///tmp"), Some("/tmp".into()));
        assert_eq!(cwd("file://localhost/tmp"), Some("/tmp".into()));
        assert_eq!(cwd("file://LOCALHOST/tmp"), Some("/tmp".into()));

        // Our own hostname, as sent by a local shell, in either case and with
        // the FQDN root dot.
        let local = local_hostname().expect("hostname");
        assert_eq!(cwd(&format!("file://{local}/tmp")), Some("/tmp".into()));
        assert_eq!(
            cwd(&format!("file://{}/tmp", local.to_uppercase())),
            Some("/tmp".into())
        );
        assert_eq!(cwd(&format!("file://{local}./tmp")), Some("/tmp".into()));
    }

    #[cfg(unix)]
    #[test]
    fn current_directory_rejects_remote_hosts() {
        // A shell on the far side of an ssh session must not move our cwd.
        assert_eq!(cwd("file://not-this-machine.invalid/tmp"), None);
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
        assert_eq!(cwd("kitty-shell-cwd://localhost"), None);
    }

    #[test]
    fn conemu_working_directory() {
        let parse = parse_conemu_working_directory;
        assert_eq!(
            parse(&[b"9", b"9", b"C:\\Users\\rapha"]),
            Some(Some("C:\\Users\\rapha".into()))
        );
        // Windows Terminal profiles commonly quote the path.
        assert_eq!(
            parse(&[b"9", b"9", b"\"C:\\Program Files\""]),
            Some(Some("C:\\Program Files".into()))
        );
        // The parser keeps a `;` in the path inside the last param.
        assert_eq!(
            parse(&[b"9", b"9", b"C:\\a;b"]),
            Some(Some("C:\\a;b".into()))
        );

        // Progress reports and notifications are not directories.
        assert_eq!(parse(&[b"9", b"4", b"1", b"50"]), None);
        assert_eq!(parse(&[b"9", b"hello"]), None);
        assert_eq!(parse(&[b"9", b"9"]), None);

        // An empty path, quoted or not, means the directory is unknown.
        assert_eq!(parse(&[b"9", b"9", b""]), Some(None));
        assert_eq!(parse(&[b"9", b"9", b"\"\""]), Some(None));

        // Only absolute paths: free text and relative paths are not a
        // cwd, and UNC names another machine like a remote OSC 7 host.
        assert_eq!(parse(&[b"9", b"9", b"Meeting at 5"]), None);
        assert_eq!(parse(&[b"9", b"9", b"..\\up"]), None);
        assert_eq!(parse(&[b"9", b"9", b"\\\\server\\share"]), None);
        assert_eq!(
            parse(&[b"9", b"9", b"/home/user"]),
            Some(Some("/home/user".into()))
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn current_directory_accepts_kitty_shell_cwd() {
        assert_eq!(
            cwd("kitty-shell-cwd:///home/user"),
            Some("/home/user".into())
        );
        assert_eq!(
            cwd("kitty-shell-cwd://localhost/home/user"),
            Some("/home/user".into())
        );
        // Scheme is case-insensitive, like file://.
        assert_eq!(
            cwd("KITTY-SHELL-CWD:///home/user"),
            Some("/home/user".into())
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn kitty_shell_cwd_keeps_semicolons() {
        assert_eq!(cwd("kitty-shell-cwd:///tmp/a;b"), Some("/tmp/a;b".into()));
    }

    #[cfg(not(windows))]
    #[test]
    fn kitty_shell_cwd_is_verbatim() {
        // Raw spaces pass through, and percent sequences are path
        // bytes rather than encodings.
        assert_eq!(cwd("kitty-shell-cwd:///tmp/a b"), Some("/tmp/a b".into()));
        assert_eq!(
            cwd("kitty-shell-cwd:///tmp/50%25 off?really"),
            Some("/tmp/50%25 off?really".into())
        );
    }

    #[cfg(windows)]
    #[test]
    fn kitty_shell_cwd_strips_windows_leading_slash() {
        // The PowerShell integration emits `/C:\...`; the drive letter
        // must come back without the URL-shaped leading slash, and the
        // path stays verbatim (spaces, `#`, `%` untouched).
        assert_eq!(
            cwd("kitty-shell-cwd:///C:\\Users\\a b"),
            Some("C:\\Users\\a b".into())
        );
        assert_eq!(
            cwd("kitty-shell-cwd://localhost/C:\\projects\\c#"),
            Some("C:\\projects\\c#".into())
        );
    }

    #[cfg(unix)]
    #[test]
    fn kitty_shell_cwd_rejects_remote_hosts() {
        assert_eq!(
            cwd("kitty-shell-cwd://other-machine.example/home/user"),
            None
        );
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

        // A directory on someone else's machine leaves ours in place.
        processor.advance(
            &mut term,
            b"\x1b]7;file://not-this-machine.invalid/tmp\x1b\\",
        );
        assert_eq!(
            term.current_directory.as_deref(),
            Some(std::path::Path::new("/home/My Files"))
        );

        // A malformed payload is ignored, not treated as "unknown".
        processor.advance(&mut term, b"\x1b]7;not-a-url\x1b\\");
        assert_eq!(
            term.current_directory.as_deref(),
            Some(std::path::Path::new("/home/My Files"))
        );

        // An empty payload means the program no longer knows its
        // directory, so the stale one is forgotten.
        processor.advance(&mut term, b"\x1b]7;\x1b\\");
        assert_eq!(term.current_directory, None);

        // Same for the ConEmu flavor.
        processor.advance(&mut term, b"\x1b]9;9;/tmp/x\x07");
        assert_eq!(
            term.current_directory.as_deref(),
            Some(std::path::Path::new("/tmp/x"))
        );
        processor.advance(&mut term, b"\x1b]9;9;\x07");
        assert_eq!(term.current_directory, None);

        // A quoted empty path clears too.
        processor.advance(&mut term, b"\x1b]9;9;/tmp/y\x07");
        processor.advance(&mut term, b"\x1b]9;9;\"\"\x07");
        assert_eq!(term.current_directory, None);
    }

    #[test]
    fn hyperlink_follows_empty_uri_rules() {
        let open = parse_hyperlink(b"id=a", b"https://x.dev/a;b")
            .unwrap()
            .unwrap();
        assert_eq!(open.uri(), "https://x.dev/a;b");
        assert_eq!(parse_hyperlink(b"", b""), Some(None));
        assert_eq!(parse_hyperlink(b"id=", b""), Some(None));
        assert_eq!(parse_hyperlink(b"id=a", b""), None);

        // A repeated key: the last non-empty value wins, and an empty
        // value does not unset an earlier one.
        let open = parse_hyperlink(b"id=a:id=b", b"https://x.dev")
            .unwrap()
            .unwrap();
        assert_eq!(open.id(), "b");
        let open = parse_hyperlink(b"id=a:id=", b"https://x.dev")
            .unwrap()
            .unwrap();
        assert_eq!(open.id(), "a");
        assert_eq!(parse_hyperlink(b"id=:id=a", b""), None);
        assert_eq!(parse_hyperlink(b"id=:other=x", b""), Some(None));

        // An id with invalid UTF-8 is still an id: it must not turn an
        // open into a close, and the program's grouping is kept.
        assert_eq!(parse_hyperlink(b"id=caf\xe9", b""), None);
        let open = parse_hyperlink(b"id=caf\xe9", b"https://x.dev")
            .unwrap()
            .unwrap();
        assert_eq!(open.id(), "caf\u{FFFD}");
    }

    #[test]
    fn hyperlink_payload_is_bounded() {
        const URI: &str = "https://x.dev";
        // The budget covers the params, the `;` and the URI together.
        let room = MAX_HYPERLINK_BYTES - 1;
        let uri = format!("{URI}/{}", "a".repeat(room - URI.len() - 1));
        assert_eq!(1 + uri.len(), MAX_HYPERLINK_BYTES);
        let open = parse_hyperlink(b"", uri.as_bytes()).unwrap().unwrap();
        assert_eq!(open.uri(), uri);
        // One byte over, in either part: invalid and ignored, so the
        // sequence neither opens a link nor closes the current one.
        assert_eq!(parse_hyperlink(b"", format!("{uri}a").as_bytes()), None);
        assert_eq!(parse_hyperlink(b"a", uri.as_bytes()), None);

        // Params spend the same budget as the URI, and raw bytes count,
        // not the lossy decoding (each invalid byte would otherwise
        // count as three).
        let id_len = room - URI.len() - "id=".len();
        let id = format!("id={}", "i".repeat(id_len));
        assert_eq!(id.len() + 1 + URI.len(), MAX_HYPERLINK_BYTES);
        let open = parse_hyperlink(id.as_bytes(), URI.as_bytes())
            .unwrap()
            .unwrap();
        assert_eq!(open.id().len(), id_len);
        assert_eq!(
            parse_hyperlink(format!("{id}i").as_bytes(), URI.as_bytes()),
            None
        );
        let mut raw = b"id=".to_vec();
        raw.extend(std::iter::repeat_n(0xFF, id_len));
        let open = parse_hyperlink(&raw, URI.as_bytes()).unwrap().unwrap();
        assert_eq!(open.id().chars().count(), id_len);
        raw.push(0xFF);
        assert_eq!(parse_hyperlink(&raw, URI.as_bytes()), None);

        // An oversized close is ignored as well, not applied.
        let params = "a".repeat(MAX_HYPERLINK_BYTES);
        assert_eq!(parse_hyperlink(params.as_bytes(), b""), None);
        assert_eq!(parse_hyperlink(&params.as_bytes()[1..], b""), Some(None));
    }

    #[test]
    fn conemu_subcommands() {
        for sub in [b"1" as &[u8], b"4", b"9", b"10", b"12"] {
            assert!(is_conemu_subcommand(sub), "{sub:?}");
        }
        for sub in [b"" as &[u8], b"0", b"13", b"01", b"hello", b"4x"] {
            assert!(!is_conemu_subcommand(sub), "{sub:?}");
        }
    }

    #[test]
    fn notification_body_is_everything_after_nine() {
        assert_eq!(
            parse_notification_body(&[b"9", b"done", b" 3 warnings"]),
            "done; 3 warnings"
        );
        assert_eq!(parse_notification_body(&[b"9", b"caf\xe9"]), "caf\u{FFFD}");
    }

    #[test]
    fn title_is_verbatim() {
        assert_eq!(parse_title(&[b"2", b" a;b "]), Some(" a;b ".into()));
        assert_eq!(parse_title(&[b"2", b""]), Some(String::new()));
        assert_eq!(parse_title(&[b"2"]), None);
    }

    #[test]
    fn title_rejects_invalid_utf8_and_oversize() {
        // Invalid UTF-8 drops the whole title instead of replacing bytes.
        assert_eq!(parse_title(&[b"0", b"caf\xe9"]), None);

        let at_limit = "a".repeat(MAX_TITLE_BYTES);
        assert_eq!(
            parse_title(&[b"0", at_limit.as_bytes()]),
            Some(at_limit.clone())
        );
        let over = "a".repeat(MAX_TITLE_BYTES + 1);
        assert_eq!(parse_title(&[b"0", over.as_bytes()]), None);

        // The limit is in bytes: a multi-byte title near the limit
        // is accepted or rejected whole, never cut mid-character.
        let wide = "é".repeat(MAX_TITLE_BYTES / 2); // 254 bytes
        assert_eq!(parse_title(&[b"0", wide.as_bytes()]), Some(wide));
        let wide_over = "é".repeat(MAX_TITLE_BYTES / 2 + 1); // 256 bytes
        assert_eq!(parse_title(&[b"0", wide_over.as_bytes()]), None);
    }
}
