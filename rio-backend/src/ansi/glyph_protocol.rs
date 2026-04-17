// Glyph Protocol wire parser.
//
// Protocol framing:
//   ESC _ 1cc6D ; <verb> [ ; key=value ]* [ ; <payload> ] ESC \
//
// Verbs:
//   q — query the state of a codepoint (status=0..3 bit field)
//   r — register a PUA codepoint with a glyph
//   c — clear one PUA codepoint or every registration in this session
//
// `cp` is always a single codepoint. For `r` and `c`, `cp` MUST be in
// one of the three Unicode Private Use Area ranges; otherwise the
// request is rejected with `reason=out_of_namespace`. `q` accepts any
// valid Unicode scalar value so applications can probe system-font
// coverage for codepoints they intend to register.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

/// Protocol identifier — the literal ASCII string `"1cc6D"` that
/// prefixes every Glyph Protocol APC body. Terminals MUST drop APC
/// messages whose body does not begin with this identifier.
pub const GLYPH_PROTOCOL_PREFIX: &[u8] = b"1cc6D";

/// Upper bound on a single registered payload, post-base64-decode.
/// Matches the 64 KiB limit in the spec; anything larger is rejected
/// with `payload_too_large`.
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;

/// Check whether a codepoint is in any of the three Unicode Private
/// Use Areas.
#[inline]
pub fn is_pua(cp: u32) -> bool {
    (0xE000..=0xF8FF).contains(&cp)          // basic
        || (0xF_0000..=0xF_FFFD).contains(&cp)  // supplementary A
        || (0x10_0000..=0x10_FFFD).contains(&cp) // supplementary B
}

/// Parsed Glyph Protocol command, ready for dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlyphCommand {
    /// Query state of a single codepoint.
    Query { cp: u32 },
    /// Register a glyph at a PUA codepoint chosen by the client.
    Register { cp: u32, upm: u16, glyf: Vec<u8> },
    /// Clear a single PUA codepoint (`Some`) or every slot (`None`).
    Clear { cp: Option<u32> },
}

/// Query status — two-bit field per spec §5.2. Bit 0: system coverage.
/// Bit 1: glossary coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryStatus {
    Free = 0,
    System = 1,
    Glossary = 2,
    Both = 3,
}

impl QueryStatus {
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Defined register-error codes, per spec §6.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterError {
    OutOfNamespace,
    CompositeUnsupported,
    HintingUnsupported,
    MalformedPayload,
    PayloadTooLarge,
}

impl RegisterError {
    fn as_str(self) -> &'static str {
        match self {
            RegisterError::OutOfNamespace => "out_of_namespace",
            RegisterError::CompositeUnsupported => "composite_unsupported",
            RegisterError::HintingUnsupported => "hinting_unsupported",
            RegisterError::MalformedPayload => "malformed_payload",
            RegisterError::PayloadTooLarge => "payload_too_large",
        }
    }
}

/// Error returned when the APC body is not a valid Glyph Protocol
/// message, or when wire-level validation rejects the request before
/// it reaches the handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Body does not start with `1cc6D` — not our protocol; caller
    /// should fall through to other APC dispatchers.
    NotGlyphProtocol,
    /// Framing was recognised but malformed.
    Malformed(&'static str),
    /// Register rejected at parse time. Dispatcher formats this as
    /// `status=<nonzero>; reason=<code>` with the supplied `cp`.
    RegisterFailed { cp: u32, reason: RegisterError },
    /// `c;cp=<hex>` where the codepoint is not in any PUA range.
    ClearOutOfNamespace,
}

/// Parse a raw APC body (minus the `ESC _` introducer and `ESC \`
/// terminator) into a [`GlyphCommand`].
pub fn parse(body: &[u8]) -> Result<GlyphCommand, ParseError> {
    if !body.starts_with(GLYPH_PROTOCOL_PREFIX) {
        return Err(ParseError::NotGlyphProtocol);
    }
    let rest = &body[GLYPH_PROTOCOL_PREFIX.len()..];
    let rest = rest
        .strip_prefix(b";")
        .ok_or(ParseError::Malformed("missing verb separator"))?;

    let (verb, rest) = split_once(rest, b';');
    let verb = trim(verb);
    if verb.len() != 1 {
        return Err(ParseError::Malformed("verb must be a single byte"));
    }
    match verb[0] {
        b'q' => parse_query(rest),
        b'r' => parse_register(rest),
        b'c' => parse_clear(rest),
        _ => Err(ParseError::Malformed("unknown verb")),
    }
}

fn parse_query(rest: &[u8]) -> Result<GlyphCommand, ParseError> {
    let params = parse_params(rest);
    let cp_raw = params
        .get("cp")
        .ok_or(ParseError::Malformed("query missing cp"))?;
    if cp_raw.contains(&b',') {
        return Err(ParseError::Malformed("cp must be a single codepoint"));
    }
    let cp = parse_hex_cp(cp_raw).ok_or(ParseError::Malformed("query cp invalid hex"))?;
    Ok(GlyphCommand::Query { cp })
}

fn parse_register(rest: &[u8]) -> Result<GlyphCommand, ParseError> {
    // Register splits control parameters from the base64 payload at
    // the LAST `;`. Base64 has no `;` so this is unambiguous.
    let (control, payload_b64) = split_last(rest, b';');
    let params = parse_params(control);

    let cp_raw = params
        .get("cp")
        .ok_or(ParseError::Malformed("register missing cp"))?;
    if cp_raw.contains(&b',') {
        return Err(ParseError::Malformed("cp must be a single codepoint"));
    }
    let cp =
        parse_hex_cp(cp_raw).ok_or(ParseError::Malformed("register cp invalid hex"))?;

    // PUA check is the protocol's security contract — reject early so
    // we don't bother decoding the payload.
    if !is_pua(cp) {
        return Err(ParseError::RegisterFailed {
            cp,
            reason: RegisterError::OutOfNamespace,
        });
    }

    if params.get("fmt").copied().unwrap_or(b"glyf") != b"glyf" {
        return Err(ParseError::Malformed("register fmt must be glyf"));
    }

    let upm = match params.get("upm") {
        Some(raw) => {
            parse_decimal_u16(raw).ok_or(ParseError::Malformed("register upm invalid"))?
        }
        None => 1000,
    };
    if upm == 0 {
        return Err(ParseError::Malformed("register upm must be non-zero"));
    }

    let payload_b64 = trim(payload_b64);
    let glyf = BASE64
        .decode(payload_b64)
        .map_err(|_| ParseError::RegisterFailed {
            cp,
            reason: RegisterError::MalformedPayload,
        })?;
    if glyf.len() > MAX_PAYLOAD_BYTES {
        return Err(ParseError::RegisterFailed {
            cp,
            reason: RegisterError::PayloadTooLarge,
        });
    }

    Ok(GlyphCommand::Register { cp, upm, glyf })
}

fn parse_clear(rest: &[u8]) -> Result<GlyphCommand, ParseError> {
    let params = parse_params(rest);
    match params.get("cp") {
        Some(cp_raw) => {
            if cp_raw.contains(&b',') {
                return Err(ParseError::Malformed("cp must be a single codepoint"));
            }
            let cp = parse_hex_cp(cp_raw)
                .ok_or(ParseError::Malformed("clear cp invalid hex"))?;
            if !is_pua(cp) {
                return Err(ParseError::ClearOutOfNamespace);
            }
            Ok(GlyphCommand::Clear { cp: Some(cp) })
        }
        None => Ok(GlyphCommand::Clear { cp: None }),
    }
}

/// Minimal parameter parser: semicolon-separated `key=value` pairs.
/// Keys are compared case-sensitively; unknown keys are silently kept
/// (callers ignore them, per spec §11 conformance rule).
fn parse_params(data: &[u8]) -> Params<'_> {
    let mut out = Params::default();
    for part in data.split(|&b| b == b';') {
        let part = trim(part);
        if part.is_empty() {
            continue;
        }
        if let Some(eq) = part.iter().position(|&b| b == b'=') {
            let k = trim(&part[..eq]);
            let v = trim(&part[eq + 1..]);
            out.insert(k, v);
        }
    }
    out
}

/// Hex-parse a single codepoint (no leading `0x`, up to 6 digits).
fn parse_hex_cp(raw: &[u8]) -> Option<u32> {
    let raw = trim(raw);
    if raw.is_empty() || raw.len() > 6 {
        return None;
    }
    let mut out: u32 = 0;
    for &b in raw {
        let d = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => return None,
        } as u32;
        out = (out << 4) | d;
    }
    if out > 0x10FFFF || (0xD800..=0xDFFF).contains(&out) {
        return None;
    }
    Some(out)
}

fn parse_decimal_u16(raw: &[u8]) -> Option<u16> {
    let raw = trim(raw);
    if raw.is_empty() {
        return None;
    }
    let mut out: u32 = 0;
    for &b in raw {
        if !b.is_ascii_digit() {
            return None;
        }
        out = out.checked_mul(10)?.checked_add((b - b'0') as u32)?;
        if out > u16::MAX as u32 {
            return None;
        }
    }
    Some(out as u16)
}

fn split_once(data: &[u8], sep: u8) -> (&[u8], &[u8]) {
    if let Some(pos) = data.iter().position(|&b| b == sep) {
        (&data[..pos], &data[pos + 1..])
    } else {
        (data, &[])
    }
}

fn split_last(data: &[u8], sep: u8) -> (&[u8], &[u8]) {
    if let Some(pos) = data.iter().rposition(|&b| b == sep) {
        (&data[..pos], &data[pos + 1..])
    } else {
        (data, &[])
    }
}

fn trim(data: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = data.len();
    while start < end && matches!(data[start], b' ' | b'\t' | b'\r' | b'\n') {
        start += 1;
    }
    while end > start && matches!(data[end - 1], b' ' | b'\t' | b'\r' | b'\n') {
        end -= 1;
    }
    &data[start..end]
}

#[derive(Default)]
struct Params<'a> {
    entries: Vec<(&'a [u8], &'a [u8])>,
}

impl<'a> Params<'a> {
    fn insert(&mut self, k: &'a [u8], v: &'a [u8]) {
        for e in &mut self.entries {
            if e.0 == k {
                e.1 = v;
                return;
            }
        }
        self.entries.push((k, v));
    }

    fn get(&self, k: &str) -> Option<&&'a [u8]> {
        self.entries
            .iter()
            .find(|e| e.0 == k.as_bytes())
            .map(|e| &e.1)
    }
}

/// Format the reply to `q;cp=<hex>`.
pub fn format_query_response(cp: u32, status: QueryStatus) -> String {
    format!("\x1b_1cc6D;q;cp={:x};status={}\x1b\\", cp, status.as_u8())
}

/// Format a successful register reply.
pub fn format_register_ok(cp: u32) -> String {
    format!("\x1b_1cc6D;r;cp={:x};status=0\x1b\\", cp)
}

/// Format a register error reply.
pub fn format_register_error(cp: u32, reason: RegisterError) -> String {
    format!(
        "\x1b_1cc6D;r;cp={:x};status=1;reason={}\x1b\\",
        cp,
        reason.as_str()
    )
}

/// Format the reply to a clear request. `cp` is echoed back when the
/// request scoped to a single slot, omitted for "clear all".
pub fn format_clear_ok(cp: Option<u32>) -> String {
    match cp {
        Some(cp) => format!("\x1b_1cc6D;c;cp={:x};status=0\x1b\\", cp),
        None => String::from("\x1b_1cc6D;c;status=0\x1b\\"),
    }
}

/// Format a clear error reply (currently only `out_of_namespace`).
pub fn format_clear_error_out_of_namespace() -> String {
    String::from("\x1b_1cc6D;c;status=1;reason=out_of_namespace\x1b\\")
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn b64(data: &[u8]) -> String {
        BASE64.encode(data)
    }

    #[test]
    fn rejects_non_glyph_protocol_bodies() {
        assert_eq!(parse(b"G,a=T;payload"), Err(ParseError::NotGlyphProtocol));
        assert_eq!(parse(b""), Err(ParseError::NotGlyphProtocol));
    }

    #[test]
    fn is_pua_covers_all_three_ranges() {
        assert!(is_pua(0xE000));
        assert!(is_pua(0xE0A0)); // Powerline branch
        assert!(is_pua(0xF8FF)); // end of basic PUA
        assert!(is_pua(0xF_0000)); // start of supp-A
        assert!(is_pua(0xF_FFFD));
        assert!(is_pua(0x10_0000));
        assert!(is_pua(0x10_FFFD));
    }

    #[test]
    fn is_pua_excludes_real_text_and_emoji() {
        assert!(!is_pua(0x0061)); // 'a'
        assert!(!is_pua(0x002D)); // '-'
        assert!(!is_pua(0x1F600)); // grinning face — supplementary but NOT PUA
        assert!(!is_pua(0xFFFE)); // noncharacter just before PUA-A
        assert!(!is_pua(0xF_FFFE)); // noncharacter just after PUA-A
        assert!(!is_pua(0x10_FFFF)); // noncharacter just after PUA-B
    }

    #[test]
    fn parses_query_single_codepoint() {
        let got = parse(b"1cc6D;q;cp=E0A0").unwrap();
        assert_eq!(got, GlyphCommand::Query { cp: 0xE0A0 });
    }

    #[test]
    fn query_accepts_non_pua_codepoints() {
        // Query probes the world; it does not care about PUA.
        let got = parse(b"1cc6D;q;cp=61").unwrap();
        assert_eq!(got, GlyphCommand::Query { cp: 0x61 });
    }

    #[test]
    fn query_rejects_sequence() {
        assert!(matches!(
            parse(b"1cc6D;q;cp=2D,3E"),
            Err(ParseError::Malformed(_))
        ));
    }

    #[test]
    fn query_rejects_surrogate() {
        assert!(matches!(
            parse(b"1cc6D;q;cp=D800"),
            Err(ParseError::Malformed(_))
        ));
    }

    #[test]
    fn parses_register_at_pua_codepoint() {
        let payload = b64(&[0x01, 0x02, 0x03]);
        let body = format!("1cc6D;r;cp=E0A0;upm=1000;{}", payload);
        let got = parse(body.as_bytes()).unwrap();
        assert_eq!(
            got,
            GlyphCommand::Register {
                cp: 0xE0A0,
                upm: 1000,
                glyf: vec![0x01, 0x02, 0x03],
            }
        );
    }

    #[test]
    fn parses_register_with_explicit_fmt() {
        let payload = b64(&[0xAA]);
        let body = format!("1cc6D;r;cp=E0A0;fmt=glyf;upm=1000;{}", payload);
        assert!(matches!(
            parse(body.as_bytes()).unwrap(),
            GlyphCommand::Register { .. }
        ));
    }

    #[test]
    fn register_defaults_upm_to_1000() {
        let payload = b64(&[0x01]);
        let body = format!("1cc6D;r;cp=E0A0;{}", payload);
        let got = parse(body.as_bytes()).unwrap();
        if let GlyphCommand::Register { upm, .. } = got {
            assert_eq!(upm, 1000);
        } else {
            panic!("expected register");
        }
    }

    #[test]
    fn register_rejects_non_pua_codepoint() {
        let payload = b64(&[0x01]);
        let body = format!("1cc6D;r;cp=61;upm=1000;{}", payload);
        assert_eq!(
            parse(body.as_bytes()),
            Err(ParseError::RegisterFailed {
                cp: 0x61,
                reason: RegisterError::OutOfNamespace,
            })
        );
    }

    #[test]
    fn register_requires_cp() {
        let payload = b64(&[0x01]);
        let body = format!("1cc6D;r;upm=1000;{}", payload);
        assert!(matches!(
            parse(body.as_bytes()),
            Err(ParseError::Malformed(_))
        ));
    }

    #[test]
    fn register_accepts_each_pua_range() {
        for &cp_hex in &[0xE0A0u32, 0xF_0000, 0x10_0000] {
            let payload = b64(b"x");
            let body = format!("1cc6D;r;cp={:x};upm=1000;{}", cp_hex, payload);
            assert!(matches!(
                parse(body.as_bytes()).unwrap(),
                GlyphCommand::Register { .. }
            ));
        }
    }

    #[test]
    fn register_rejects_unknown_fmt() {
        let payload = b64(b"x");
        let body = format!("1cc6D;r;cp=E0A0;fmt=svg;upm=1000;{}", payload);
        assert!(matches!(
            parse(body.as_bytes()),
            Err(ParseError::Malformed(_))
        ));
    }

    #[test]
    fn register_rejects_bad_base64() {
        let body = b"1cc6D;r;cp=E0A0;upm=1000;$$$$not_base64";
        assert!(matches!(
            parse(body),
            Err(ParseError::RegisterFailed {
                reason: RegisterError::MalformedPayload,
                ..
            })
        ));
    }

    #[test]
    fn register_rejects_oversized_payload() {
        let payload = b64(&vec![0u8; MAX_PAYLOAD_BYTES + 1]);
        let body = format!("1cc6D;r;cp=E0A0;upm=1000;{}", payload);
        assert!(matches!(
            parse(body.as_bytes()),
            Err(ParseError::RegisterFailed {
                reason: RegisterError::PayloadTooLarge,
                ..
            })
        ));
    }

    #[test]
    fn register_rejects_zero_upm() {
        let payload = b64(b"x");
        let body = format!("1cc6D;r;cp=E0A0;upm=0;{}", payload);
        assert!(matches!(
            parse(body.as_bytes()),
            Err(ParseError::Malformed(_))
        ));
    }

    #[test]
    fn clear_single_pua_slot() {
        let got = parse(b"1cc6D;c;cp=E0A0").unwrap();
        assert_eq!(got, GlyphCommand::Clear { cp: Some(0xE0A0) });
    }

    #[test]
    fn clear_rejects_non_pua_cp() {
        assert_eq!(
            parse(b"1cc6D;c;cp=61"),
            Err(ParseError::ClearOutOfNamespace)
        );
        assert_eq!(
            parse(b"1cc6D;c;cp=1F600"),
            Err(ParseError::ClearOutOfNamespace)
        );
    }

    #[test]
    fn clear_rejects_sequence_cp() {
        assert!(matches!(
            parse(b"1cc6D;c;cp=E0A0,E0A1"),
            Err(ParseError::Malformed(_))
        ));
    }

    #[test]
    fn clear_all() {
        let got = parse(b"1cc6D;c").unwrap();
        assert_eq!(got, GlyphCommand::Clear { cp: None });
    }

    #[test]
    fn unknown_verb_is_malformed() {
        assert!(matches!(
            parse(b"1cc6D;z;cp=0061"),
            Err(ParseError::Malformed(_))
        ));
    }

    #[test]
    fn query_response_encodes_numeric_status() {
        assert_eq!(
            format_query_response(0xE0A0, QueryStatus::Free),
            "\x1b_1cc6D;q;cp=e0a0;status=0\x1b\\"
        );
        assert_eq!(
            format_query_response(0xE0A0, QueryStatus::System),
            "\x1b_1cc6D;q;cp=e0a0;status=1\x1b\\"
        );
        assert_eq!(
            format_query_response(0xE0A0, QueryStatus::Glossary),
            "\x1b_1cc6D;q;cp=e0a0;status=2\x1b\\"
        );
        assert_eq!(
            format_query_response(0xE0A0, QueryStatus::Both),
            "\x1b_1cc6D;q;cp=e0a0;status=3\x1b\\"
        );
    }

    #[test]
    fn register_responses() {
        assert_eq!(
            format_register_ok(0xE0A0),
            "\x1b_1cc6D;r;cp=e0a0;status=0\x1b\\"
        );
        assert_eq!(
            format_register_error(0x61, RegisterError::OutOfNamespace),
            "\x1b_1cc6D;r;cp=61;status=1;reason=out_of_namespace\x1b\\"
        );
        assert_eq!(
            format_register_error(0xE0A0, RegisterError::CompositeUnsupported),
            "\x1b_1cc6D;r;cp=e0a0;status=1;reason=composite_unsupported\x1b\\"
        );
    }

    #[test]
    fn clear_responses() {
        assert_eq!(
            format_clear_ok(Some(0xE0A0)),
            "\x1b_1cc6D;c;cp=e0a0;status=0\x1b\\"
        );
        assert_eq!(format_clear_ok(None), "\x1b_1cc6D;c;status=0\x1b\\");
        assert_eq!(
            format_clear_error_out_of_namespace(),
            "\x1b_1cc6D;c;status=1;reason=out_of_namespace\x1b\\"
        );
    }

    #[test]
    fn unknown_params_are_ignored() {
        let got = parse(b"1cc6D;q;cp=E0A0;future=1").unwrap();
        assert_eq!(got, GlyphCommand::Query { cp: 0xE0A0 });
    }
}
