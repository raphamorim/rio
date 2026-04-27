// Glyph Protocol wire parser.
//
// Protocol framing:
//   ESC _ 25a1 ; <verb> [ ; key=value ]* [ ; <payload> ] ESC \
//
// Verbs:
//   s — advertise supported payload formats; also serves as protocol
//       detection (any reply = protocol implemented)
//   q — query the state of a codepoint (status=0..3 bit field)
//   r — register a PUA codepoint with a glyph
//   c — clear one PUA codepoint or every registration in this session
//
// Payload formats (selected via `fmt=<name>` on the `r` verb):
//   glyf   — single monochrome OpenType simple-glyph outline.
//   colrv0 — up to 16 flat-color layers; each layer is an sRGBA colour
//            (or a "foreground" sentinel) plus a `glyf` outline; layers
//            composite in painter-order.
//   colrv1 — same layer model as colrv0 but each layer carries a paint:
//            solid, linear gradient, radial gradient, or foreground. No
//            affine transforms and no sweep gradients in v1.
//
// `cp` is always a single codepoint. For `r` and `c`, `cp` MUST be in
// one of the three Unicode Private Use Area ranges; otherwise the
// request is rejected with `reason=out_of_namespace`. `q` accepts any
// valid Unicode scalar value so applications can probe system-font
// coverage for codepoints they intend to register.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

/// Protocol identifier — the literal ASCII string `"25a1"` that
/// prefixes every Glyph Protocol APC body. Terminals MUST drop APC
/// messages whose body does not begin with this identifier.
pub const GLYPH_PROTOCOL_PREFIX: &[u8] = b"25a1";

/// Upper bound on a single registered payload, post-base64-decode.
/// Matches the 64 KiB limit in the spec; anything larger is rejected
/// with `payload_too_large`.
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;

/// Bitfield of payload formats this build supports, returned in the
/// reply to the `s` verb.
///   bit 0 = `glyf`   (OpenType simple glyphs)
///   bit 1 = `colrv0` (flat-color layered outlines)
///   bit 2 = `colrv1` (layered outlines with solid/linear/radial paints)
pub const SUPPORTED_FORMATS: u8 = 0b0000_0111;

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
    /// Advertise supported payload formats. Parameter-free; doubles as
    /// the protocol-detection ping.
    Support,
    /// Query state of a single codepoint.
    Query { cp: u32 },
    /// Register a glyph at a PUA codepoint chosen by the client. The
    /// `payload` carries format-specific data (monochrome `glyf`, or a
    /// `colrv0`/`colrv1` colour container wrapping OpenType tables).
    /// The `reply` level controls which replies (if any) the
    /// dispatcher emits — see [`ReplyMode`] for the three tiers.
    Register {
        cp: u32,
        payload: GlyphPayload,
        reply: ReplyMode,
    },
    /// Clear a single PUA codepoint (`Some`) or every slot (`None`).
    Clear { cp: Option<u32> },
}

/// Upper bound on the number of glyph outlines carried in a single
/// colour payload. Keeps the glossary's decode cost bounded and sits
/// well within the 16-bit GlyphId namespace used by COLR.
pub const MAX_COLR_GLYPHS: u16 = 1024;

/// Payload shipped with an `r` (register) request.
///
/// `Glyf` is a single OpenType simple-glyph record, rendered in the
/// current foreground colour.
///
/// `ColrV0` and `ColrV1` share a wire container ([`ColrContainer`]) —
/// a length-prefixed array of simple-glyph outlines plus raw OpenType
/// `COLR` and `CPAL` tables. The outer variant distinguishes the COLR
/// table version the terminal should expect (v0 is layer-only, v1 is
/// the full paint graph). Reusing the OpenType binary layout means
/// applications can slice existing fonts directly; the terminal uses
/// `ttf_parser::colr::Table` to walk the paint graph and our own
/// `glyf` decoder (same as `fmt=glyf`) for the leaf outlines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlyphPayload {
    Glyf { glyf: Vec<u8>, upm: u16 },
    ColrV0 { container: ColrContainer, upm: u16 },
    ColrV1 { container: ColrContainer, upm: u16 },
}

/// Wire container for `fmt=colrv0` and `fmt=colrv1` payloads.
///
/// Layout after base64-decode:
/// ```text
///   u16 BE  n_glyphs
///   per glyph:
///     u16 BE  glyf_len
///     glyf_len bytes  (simple-glyph, same encoding as fmt=glyf)
///   u16 BE  colr_len
///   colr_len bytes   (OpenType COLR table, v0 or v1)
///   u16 BE  cpal_len
///   cpal_len bytes   (OpenType CPAL table; may be zero-length when
///                     the COLR references only foreground / direct
///                     sRGB values in the v1 paint graph)
/// ```
///
/// Glyph IDs in the COLR table resolve to indices into `glyphs`.
/// CPAL palette index `0xFFFF` means "current foreground colour", per
/// the OpenType spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColrContainer {
    pub glyphs: Vec<Vec<u8>>,
    pub colr: Vec<u8>,
    pub cpal: Vec<u8>,
}

/// Three-level reply control for the `r` verb, selected with the
/// `reply` parameter on a register request. The values mirror the
/// wire encoding (`reply=0` / `reply=1` / `reply=2`) so dispatchers
/// can skip a round of translation.
///
/// Fire-and-forget bulk registrations should use [`ReplyMode::None`]
/// so `status=0` ACKs don't queue in the PTY and spill to the shell
/// when the client exits. Bulk registrations that want failure
/// telemetry without the success noise should use
/// [`ReplyMode::ErrorsOnly`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReplyMode {
    /// `reply=0`: the dispatcher emits nothing for this registration.
    None,
    /// `reply=1` (default): the dispatcher emits both success (`status=0`)
    /// and failure (`status=<nonzero>`) replies. The default when
    /// `reply` is omitted or holds an unrecognised value.
    #[default]
    All,
    /// `reply=2`: the dispatcher emits only failure replies, dropping
    /// the `status=0` ACK on success. Handy for large bulk
    /// registrations that want errors surfaced without the noise of
    /// 256 ACKs on the happy path.
    ErrorsOnly,
}

impl ReplyMode {
    /// Whether a successful register should emit `status=0`.
    pub fn emit_success(self) -> bool {
        matches!(self, ReplyMode::All)
    }
    /// Whether a failed register should emit `status=<nonzero>;reason=…`.
    pub fn emit_error(self) -> bool {
        matches!(self, ReplyMode::All | ReplyMode::ErrorsOnly)
    }

    fn from_wire(raw: &[u8]) -> Self {
        match raw {
            b"0" => ReplyMode::None,
            b"2" => ReplyMode::ErrorsOnly,
            // `reply=1`, an unrecognised value, or an absent parameter
            // all land here. Per §11 unknown-params rule the default
            // behaviour (emit both) is the safe fallback.
            _ => ReplyMode::All,
        }
    }
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
    /// Body does not start with `25a1` — not our protocol; caller
    /// should fall through to other APC dispatchers.
    NotGlyphProtocol,
    /// Framing was recognised but malformed.
    Malformed(&'static str),
    /// Register rejected at parse time. Dispatcher formats this as
    /// `status=<nonzero>; reason=<code>` with the supplied `cp`,
    /// unless the original `r` request carried a `reply` level that
    /// disables error replies (see [`ReplyMode::emit_error`]).
    RegisterFailed {
        cp: u32,
        reason: RegisterError,
        reply: ReplyMode,
    },
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
        b's' => parse_support(rest),
        b'q' => parse_query(rest),
        b'r' => parse_register(rest),
        b'c' => parse_clear(rest),
        _ => Err(ParseError::Malformed("unknown verb")),
    }
}

fn parse_support(_rest: &[u8]) -> Result<GlyphCommand, ParseError> {
    // `s` takes no parameters. Per spec §11 conformance rule, unknown
    // params are silently ignored rather than erroring out, so future
    // clients that send extra hints (e.g. a client-advertised format
    // preference) still get a valid reply from this implementation.
    Ok(GlyphCommand::Support)
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

    // Extract `reply` before any can-fail validation so every error
    // path below can honour the level. Unrecognised values fall back
    // to the default (emit both success and failure replies).
    let reply = params
        .get("reply")
        .map(|v| ReplyMode::from_wire(v))
        .unwrap_or_default();

    // PUA check is the protocol's security contract — reject early so
    // we don't bother decoding the payload.
    if !is_pua(cp) {
        return Err(ParseError::RegisterFailed {
            cp,
            reason: RegisterError::OutOfNamespace,
            reply,
        });
    }

    let fmt = params.get("fmt").copied().unwrap_or(b"glyf");
    if fmt != b"glyf" && fmt != b"colrv0" && fmt != b"colrv1" {
        return Err(ParseError::Malformed("register fmt unknown"));
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
    let raw = BASE64
        .decode(payload_b64)
        .map_err(|_| ParseError::RegisterFailed {
            cp,
            reason: RegisterError::MalformedPayload,
            reply,
        })?;
    if raw.len() > MAX_PAYLOAD_BYTES {
        return Err(ParseError::RegisterFailed {
            cp,
            reason: RegisterError::PayloadTooLarge,
            reply,
        });
    }

    let payload = match fmt {
        b"glyf" => GlyphPayload::Glyf { glyf: raw, upm },
        b"colrv0" => {
            let container = parse_colr_container(&raw)
                .map_err(|reason| ParseError::RegisterFailed { cp, reason, reply })?;
            GlyphPayload::ColrV0 { container, upm }
        }
        b"colrv1" => {
            let container = parse_colr_container(&raw)
                .map_err(|reason| ParseError::RegisterFailed { cp, reason, reply })?;
            GlyphPayload::ColrV1 { container, upm }
        }
        _ => unreachable!("fmt validated above"),
    };

    Ok(GlyphCommand::Register { cp, payload, reply })
}

/// Decode a `colrv0`/`colrv1` container (see [`ColrContainer`] doc for
/// the wire layout). Validation is structural only: the OpenType COLR
/// and CPAL tables are handed off to the renderer, which parses them
/// with `ttf_parser::colr::Table` when the glyph is rasterised — that
/// way any COLR-version-specific validation lives next to the code
/// that actually interprets it.
fn parse_colr_container(data: &[u8]) -> Result<ColrContainer, RegisterError> {
    let mut cur = Cursor::new(data);

    let n_glyphs = cur.u16_be().ok_or(RegisterError::MalformedPayload)?;
    if n_glyphs == 0 || n_glyphs > MAX_COLR_GLYPHS {
        return Err(RegisterError::MalformedPayload);
    }

    let mut glyphs: Vec<Vec<u8>> = Vec::with_capacity(n_glyphs as usize);
    for _ in 0..n_glyphs {
        let glyf_len = cur.u16_be().ok_or(RegisterError::MalformedPayload)? as usize;
        let glyf = cur
            .slice(glyf_len)
            .ok_or(RegisterError::MalformedPayload)?
            .to_vec();
        glyphs.push(glyf);
    }

    let colr_len = cur.u16_be().ok_or(RegisterError::MalformedPayload)? as usize;
    if colr_len == 0 {
        return Err(RegisterError::MalformedPayload);
    }
    let colr = cur
        .slice(colr_len)
        .ok_or(RegisterError::MalformedPayload)?
        .to_vec();

    let cpal_len = cur.u16_be().ok_or(RegisterError::MalformedPayload)? as usize;
    let cpal = cur
        .slice(cpal_len)
        .ok_or(RegisterError::MalformedPayload)?
        .to_vec();

    if cur.remaining() != 0 {
        return Err(RegisterError::MalformedPayload);
    }

    Ok(ColrContainer { glyphs, colr, cpal })
}

/// Minimal big-endian byte cursor. Used by the `colrv0`/`colrv1`
/// container parser — the OpenType tables nested inside are parsed by
/// `ttf-parser` downstream, so we only need enough here to carve out
/// their byte ranges.
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }
    fn u16_be(&mut self) -> Option<u16> {
        if self.pos + 2 > self.data.len() {
            return None;
        }
        let hi = self.data[self.pos] as u16;
        let lo = self.data[self.pos + 1] as u16;
        self.pos += 2;
        Some((hi << 8) | lo)
    }
    fn slice(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.pos + n > self.data.len() {
            return None;
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Some(s)
    }
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

/// Format the reply to `s` (support). `fmt_bits` is the bitfield of
/// supported payload formats; bit 0 = `glyf`. A reply of `fmt=0` means
/// the protocol is implemented but no payload format is accepted — a
/// degenerate state reserved for future negotiation, never produced by
/// this build.
pub fn format_support_response(fmt_bits: u8) -> String {
    format!("\x1b_25a1;s;fmt={}\x1b\\", fmt_bits)
}

/// Format the reply to `q;cp=<hex>`.
pub fn format_query_response(cp: u32, status: QueryStatus) -> String {
    format!("\x1b_25a1;q;cp={:x};status={}\x1b\\", cp, status.as_u8())
}

/// Format a successful register reply.
pub fn format_register_ok(cp: u32) -> String {
    format!("\x1b_25a1;r;cp={:x};status=0\x1b\\", cp)
}

/// Format a register error reply.
pub fn format_register_error(cp: u32, reason: RegisterError) -> String {
    format!(
        "\x1b_25a1;r;cp={:x};status=1;reason={}\x1b\\",
        cp,
        reason.as_str()
    )
}

/// Format the reply to a clear request. `cp` is echoed back when the
/// request scoped to a single slot, omitted for "clear all".
pub fn format_clear_ok(cp: Option<u32>) -> String {
    match cp {
        Some(cp) => format!("\x1b_25a1;c;cp={:x};status=0\x1b\\", cp),
        None => String::from("\x1b_25a1;c;status=0\x1b\\"),
    }
}

/// Format a clear error reply (currently only `out_of_namespace`).
pub fn format_clear_error_out_of_namespace() -> String {
    String::from("\x1b_25a1;c;status=1;reason=out_of_namespace\x1b\\")
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
        let got = parse(b"25a1;q;cp=E0A0").unwrap();
        assert_eq!(got, GlyphCommand::Query { cp: 0xE0A0 });
    }

    #[test]
    fn query_accepts_non_pua_codepoints() {
        // Query probes the world; it does not care about PUA.
        let got = parse(b"25a1;q;cp=61").unwrap();
        assert_eq!(got, GlyphCommand::Query { cp: 0x61 });
    }

    #[test]
    fn query_rejects_sequence() {
        assert!(matches!(
            parse(b"25a1;q;cp=2D,3E"),
            Err(ParseError::Malformed(_))
        ));
    }

    #[test]
    fn query_rejects_surrogate() {
        assert!(matches!(
            parse(b"25a1;q;cp=D800"),
            Err(ParseError::Malformed(_))
        ));
    }

    #[test]
    fn parses_register_at_pua_codepoint() {
        let payload = b64(&[0x01, 0x02, 0x03]);
        let body = format!("25a1;r;cp=E0A0;upm=1000;{}", payload);
        let got = parse(body.as_bytes()).unwrap();
        assert_eq!(
            got,
            GlyphCommand::Register {
                cp: 0xE0A0,
                payload: GlyphPayload::Glyf {
                    glyf: vec![0x01, 0x02, 0x03],
                    upm: 1000,
                },
                reply: ReplyMode::All,
            }
        );
    }

    #[test]
    fn parses_register_with_explicit_fmt() {
        let payload = b64(&[0xAA]);
        let body = format!("25a1;r;cp=E0A0;fmt=glyf;upm=1000;{}", payload);
        assert!(matches!(
            parse(body.as_bytes()).unwrap(),
            GlyphCommand::Register {
                payload: GlyphPayload::Glyf { .. },
                ..
            }
        ));
    }

    #[test]
    fn register_defaults_upm_to_1000() {
        let payload = b64(&[0x01]);
        let body = format!("25a1;r;cp=E0A0;{}", payload);
        let got = parse(body.as_bytes()).unwrap();
        if let GlyphCommand::Register {
            payload: GlyphPayload::Glyf { upm, .. },
            ..
        } = got
        {
            assert_eq!(upm, 1000);
        } else {
            panic!("expected glyf register");
        }
    }

    #[test]
    fn register_rejects_non_pua_codepoint() {
        let payload = b64(&[0x01]);
        let body = format!("25a1;r;cp=61;upm=1000;{}", payload);
        assert_eq!(
            parse(body.as_bytes()),
            Err(ParseError::RegisterFailed {
                cp: 0x61,
                reason: RegisterError::OutOfNamespace,
                reply: ReplyMode::All,
            })
        );
    }

    #[test]
    fn register_requires_cp() {
        let payload = b64(&[0x01]);
        let body = format!("25a1;r;upm=1000;{}", payload);
        assert!(matches!(
            parse(body.as_bytes()),
            Err(ParseError::Malformed(_))
        ));
    }

    #[test]
    fn register_accepts_each_pua_range() {
        for &cp_hex in &[0xE0A0u32, 0xF_0000, 0x10_0000] {
            let payload = b64(b"x");
            let body = format!("25a1;r;cp={:x};upm=1000;{}", cp_hex, payload);
            assert!(matches!(
                parse(body.as_bytes()).unwrap(),
                GlyphCommand::Register { .. }
            ));
        }
    }

    #[test]
    fn register_rejects_unknown_fmt() {
        let payload = b64(b"x");
        let body = format!("25a1;r;cp=E0A0;fmt=svg;upm=1000;{}", payload);
        assert!(matches!(
            parse(body.as_bytes()),
            Err(ParseError::Malformed(_))
        ));
    }

    #[test]
    fn register_rejects_bad_base64() {
        let body = b"25a1;r;cp=E0A0;upm=1000;$$$$not_base64";
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
        let body = format!("25a1;r;cp=E0A0;upm=1000;{}", payload);
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
        let body = format!("25a1;r;cp=E0A0;upm=0;{}", payload);
        assert!(matches!(
            parse(body.as_bytes()),
            Err(ParseError::Malformed(_))
        ));
    }

    #[test]
    fn clear_single_pua_slot() {
        let got = parse(b"25a1;c;cp=E0A0").unwrap();
        assert_eq!(got, GlyphCommand::Clear { cp: Some(0xE0A0) });
    }

    #[test]
    fn clear_rejects_non_pua_cp() {
        assert_eq!(parse(b"25a1;c;cp=61"), Err(ParseError::ClearOutOfNamespace));
        assert_eq!(
            parse(b"25a1;c;cp=1F600"),
            Err(ParseError::ClearOutOfNamespace)
        );
    }

    #[test]
    fn clear_rejects_sequence_cp() {
        assert!(matches!(
            parse(b"25a1;c;cp=E0A0,E0A1"),
            Err(ParseError::Malformed(_))
        ));
    }

    #[test]
    fn clear_all() {
        let got = parse(b"25a1;c").unwrap();
        assert_eq!(got, GlyphCommand::Clear { cp: None });
    }

    #[test]
    fn parses_support_with_no_params() {
        assert_eq!(parse(b"25a1;s").unwrap(), GlyphCommand::Support);
    }

    #[test]
    fn support_ignores_unknown_params() {
        // §11: unknown params are silently ignored. The verb is
        // parameter-free, but a forward-compatible client may send
        // hints; we still produce a valid reply.
        assert_eq!(
            parse(b"25a1;s;future=1;anything=else").unwrap(),
            GlyphCommand::Support
        );
    }

    #[test]
    fn support_response_advertises_glyf_colrv0_colrv1() {
        // bit 0 = glyf, bit 1 = colrv0, bit 2 = colrv1.
        assert_eq!(
            format_support_response(SUPPORTED_FORMATS),
            "\x1b_25a1;s;fmt=7\x1b\\"
        );
    }

    #[test]
    fn support_response_encodes_arbitrary_bitfield() {
        assert_eq!(
            format_support_response(0b0000_0011),
            "\x1b_25a1;s;fmt=3\x1b\\"
        );
        assert_eq!(format_support_response(0), "\x1b_25a1;s;fmt=0\x1b\\");
    }

    #[test]
    fn unknown_verb_is_malformed() {
        assert!(matches!(
            parse(b"25a1;z;cp=0061"),
            Err(ParseError::Malformed(_))
        ));
    }


    /// Build a colour-payload container from component byte slices.
    /// Lays out exactly as documented on [`ColrContainer`].
    fn build_container(glyphs: &[&[u8]], colr: &[u8], cpal: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(glyphs.len() as u16).to_be_bytes());
        for g in glyphs {
            out.extend_from_slice(&(g.len() as u16).to_be_bytes());
            out.extend_from_slice(g);
        }
        out.extend_from_slice(&(colr.len() as u16).to_be_bytes());
        out.extend_from_slice(colr);
        out.extend_from_slice(&(cpal.len() as u16).to_be_bytes());
        out.extend_from_slice(cpal);
        out
    }

    #[test]
    fn parses_colrv0_single_glyph() {
        let container = build_container(&[&[0xAA, 0xBB]], &[0x01; 14], &[0x02; 12]);
        let body = format!("25a1;r;cp=E0A0;fmt=colrv0;upm=1000;{}", b64(&container));
        let got = parse(body.as_bytes()).unwrap();
        match got {
            GlyphCommand::Register {
                cp: 0xE0A0,
                payload:
                    GlyphPayload::ColrV0 {
                        container: c,
                        upm: 1000,
                    },
                reply: ReplyMode::All,
            } => {
                assert_eq!(c.glyphs.len(), 1);
                assert_eq!(c.glyphs[0], vec![0xAA, 0xBB]);
                assert_eq!(c.colr.len(), 14);
                assert_eq!(c.cpal.len(), 12);
            }
            other => panic!("expected colrv0 register, got {:?}", other),
        }
    }

    #[test]
    fn parses_colrv1_multi_glyph_with_empty_cpal() {
        // CPAL can legitimately be zero-length when the COLR uses only
        // foreground or direct-sRGB paints (v1 doesn't require CPAL at
        // all if no palette index is referenced).
        let container = build_container(
            &[&[0x01], &[0x02, 0x03], &[0x04, 0x05, 0x06]],
            &[0xF0; 32],
            &[],
        );
        let body = format!("25a1;r;cp=100000;fmt=colrv1;upm=2048;{}", b64(&container));
        let got = parse(body.as_bytes()).unwrap();
        match got {
            GlyphCommand::Register {
                cp: 0x100000,
                payload:
                    GlyphPayload::ColrV1 {
                        container: c,
                        upm: 2048,
                    },
                reply: ReplyMode::All,
            } => {
                assert_eq!(c.glyphs.len(), 3);
                assert_eq!(c.glyphs[2], vec![0x04, 0x05, 0x06]);
                assert_eq!(c.colr.len(), 32);
                assert!(c.cpal.is_empty());
            }
            other => panic!("expected colrv1 register, got {:?}", other),
        }
    }

    #[test]
    fn colr_rejects_zero_glyphs() {
        // Every colour glyph needs at least one outline; `0 glyphs` is
        // meaningless and likely indicates a corrupt payload.
        let container = build_container(&[], &[0x00; 4], &[]);
        let body = format!("25a1;r;cp=E0A0;fmt=colrv0;upm=1000;{}", b64(&container));
        assert!(matches!(
            parse(body.as_bytes()),
            Err(ParseError::RegisterFailed {
                reason: RegisterError::MalformedPayload,
                ..
            })
        ));
    }

    #[test]
    fn colr_rejects_empty_colr_table() {
        let container = build_container(&[&[0x01]], &[], &[]);
        let body = format!("25a1;r;cp=E0A0;fmt=colrv0;upm=1000;{}", b64(&container));
        assert!(matches!(
            parse(body.as_bytes()),
            Err(ParseError::RegisterFailed {
                reason: RegisterError::MalformedPayload,
                ..
            })
        ));
    }

    #[test]
    fn colr_rejects_truncated_payload() {
        // Claim 2 glyphs but only ship one — the cursor runs out of
        // bytes inside the loop.
        let mut bad = Vec::new();
        bad.extend_from_slice(&2u16.to_be_bytes());
        bad.extend_from_slice(&1u16.to_be_bytes());
        bad.push(0xAA);
        // …no second glyph, no COLR, no CPAL.
        let body = format!("25a1;r;cp=E0A0;fmt=colrv1;upm=1000;{}", b64(&bad));
        assert!(matches!(
            parse(body.as_bytes()),
            Err(ParseError::RegisterFailed {
                reason: RegisterError::MalformedPayload,
                ..
            })
        ));
    }

    #[test]
    fn colr_rejects_trailing_garbage() {
        // Extra bytes after the CPAL slice means the sender's layout
        // doesn't match ours; reject rather than silently ignoring.
        let mut container = build_container(&[&[0x01]], &[0x00; 4], &[]);
        container.push(0xFF);
        let body = format!("25a1;r;cp=E0A0;fmt=colrv0;upm=1000;{}", b64(&container));
        assert!(matches!(
            parse(body.as_bytes()),
            Err(ParseError::RegisterFailed {
                reason: RegisterError::MalformedPayload,
                ..
            })
        ));
    }

    #[test]
    fn colr_rejects_excessive_glyph_count() {
        // n_glyphs = MAX_COLR_GLYPHS + 1 blows the bound.
        let mut bad = Vec::new();
        bad.extend_from_slice(&(MAX_COLR_GLYPHS + 1).to_be_bytes());
        // … no actual glyph bytes; parse should reject at the count.
        let body = format!("25a1;r;cp=E0A0;fmt=colrv0;upm=1000;{}", b64(&bad));
        assert!(matches!(
            parse(body.as_bytes()),
            Err(ParseError::RegisterFailed {
                reason: RegisterError::MalformedPayload,
                ..
            })
        ));
    }

    #[test]
    fn register_defaults_reply_to_all() {
        let payload = b64(&[0x01]);
        let body = format!("25a1;r;cp=E0A0;upm=1000;{}", payload);
        match parse(body.as_bytes()).unwrap() {
            GlyphCommand::Register { reply, .. } => {
                assert_eq!(reply, ReplyMode::All);
            }
            other => panic!("expected register, got {:?}", other),
        }
    }

    #[test]
    fn register_accepts_every_reply_level() {
        // reply=0 → None, reply=1 → All, reply=2 → ErrorsOnly.
        let payload = b64(&[0x01]);
        for (raw, expected) in [
            ("0", ReplyMode::None),
            ("1", ReplyMode::All),
            ("2", ReplyMode::ErrorsOnly),
        ] {
            let body = format!("25a1;r;cp=E0A0;reply={};upm=1000;{}", raw, payload);
            match parse(body.as_bytes()).unwrap() {
                GlyphCommand::Register { reply, .. } => {
                    assert_eq!(
                        reply, expected,
                        "reply={} should map to {:?}",
                        raw, expected
                    );
                }
                other => panic!("expected register, got {:?}", other),
            }
        }
    }

    #[test]
    fn register_reply_propagates_on_parse_failure() {
        // Non-PUA cp fails validation; the reply level must propagate
        // into the error so the dispatcher can honour it consistently.
        let payload = b64(&[0x01]);
        let body = format!("25a1;r;cp=61;reply=0;upm=1000;{}", payload);
        assert_eq!(
            parse(body.as_bytes()),
            Err(ParseError::RegisterFailed {
                cp: 0x61,
                reason: RegisterError::OutOfNamespace,
                reply: ReplyMode::None,
            })
        );
    }

    #[test]
    fn register_reply_unknown_values_fall_back_to_all() {
        // Per §11 unknown-params rule, garbage values don't break the
        // register — they just revert to the default reply behaviour.
        let payload = b64(&[0x01]);
        for bad in ["3", "true", "yes", "01", ""].iter() {
            let body = format!("25a1;r;cp=E0A0;reply={};upm=1000;{}", bad, payload);
            match parse(body.as_bytes()).unwrap() {
                GlyphCommand::Register { reply, .. } => {
                    assert_eq!(
                        reply,
                        ReplyMode::All,
                        "reply={:?} should fall back to All",
                        bad
                    );
                }
                other => panic!("expected register, got {:?}", other),
            }
        }
    }

    #[test]
    fn reply_mode_emit_matrix() {
        // Sanity-check the two helpers the dispatcher relies on.
        assert!(ReplyMode::All.emit_success());
        assert!(ReplyMode::All.emit_error());
        assert!(!ReplyMode::ErrorsOnly.emit_success());
        assert!(ReplyMode::ErrorsOnly.emit_error());
        assert!(!ReplyMode::None.emit_success());
        assert!(!ReplyMode::None.emit_error());
    }

    #[test]
    fn colr_register_respects_pua_check_before_fmt_parse() {
        // Non-PUA should still be rejected for colour formats, and the
        // error should be `out_of_namespace` (not a payload error) so
        // the client sees the same contract as fmt=glyf.
        let container = build_container(&[&[0x01]], &[0x00; 4], &[]);
        let body = format!("25a1;r;cp=61;fmt=colrv0;upm=1000;{}", b64(&container));
        assert_eq!(
            parse(body.as_bytes()),
            Err(ParseError::RegisterFailed {
                cp: 0x61,
                reason: RegisterError::OutOfNamespace,
                reply: ReplyMode::All,
            })
        );
    }

    #[test]
    fn query_response_encodes_numeric_status() {
        assert_eq!(
            format_query_response(0xE0A0, QueryStatus::Free),
            "\x1b_25a1;q;cp=e0a0;status=0\x1b\\"
        );
        assert_eq!(
            format_query_response(0xE0A0, QueryStatus::System),
            "\x1b_25a1;q;cp=e0a0;status=1\x1b\\"
        );
        assert_eq!(
            format_query_response(0xE0A0, QueryStatus::Glossary),
            "\x1b_25a1;q;cp=e0a0;status=2\x1b\\"
        );
        assert_eq!(
            format_query_response(0xE0A0, QueryStatus::Both),
            "\x1b_25a1;q;cp=e0a0;status=3\x1b\\"
        );
    }

    #[test]
    fn register_responses() {
        assert_eq!(
            format_register_ok(0xE0A0),
            "\x1b_25a1;r;cp=e0a0;status=0\x1b\\"
        );
        assert_eq!(
            format_register_error(0x61, RegisterError::OutOfNamespace),
            "\x1b_25a1;r;cp=61;status=1;reason=out_of_namespace\x1b\\"
        );
        assert_eq!(
            format_register_error(0xE0A0, RegisterError::CompositeUnsupported),
            "\x1b_25a1;r;cp=e0a0;status=1;reason=composite_unsupported\x1b\\"
        );
    }

    #[test]
    fn clear_responses() {
        assert_eq!(
            format_clear_ok(Some(0xE0A0)),
            "\x1b_25a1;c;cp=e0a0;status=0\x1b\\"
        );
        assert_eq!(format_clear_ok(None), "\x1b_25a1;c;status=0\x1b\\");
        assert_eq!(
            format_clear_error_out_of_namespace(),
            "\x1b_25a1;c;status=1;reason=out_of_namespace\x1b\\"
        );
    }

    #[test]
    fn unknown_params_are_ignored() {
        let got = parse(b"25a1;q;cp=E0A0;future=1").unwrap();
        assert_eq!(got, GlyphCommand::Query { cp: 0xE0A0 });
    }
}
