// Glyph Protocol wire parser.
//
// Protocol framing:
//   ESC _ 25a1 ; <verb> [ ; key=value ]* [ ; <payload> ] ESC \
//
// Verbs:
//   s — advertise supported payload formats; also serves as protocol
//       detection (any reply = protocol implemented)
//   q — query the state of a codepoint (`cp=`) or of one grapheme
//       cluster (`grapheme=`); status is a comma-separated list of
//       coverage names: `system`, `glossary`, both, or empty
//   r — register a glyph at a codepoint in the registration block
//   c — clear one block codepoint or every registration in this session
//
// Payload formats (selected via `fmt=<name>` on the `r` verb):
//   glyf   — single monochrome OpenType simple-glyph outline.
//   colrv0 — up to 16 flat-color layers; each layer is an sRGBA colour
//            (or a "foreground" sentinel) plus a `glyf` outline; layers
//            composite in painter-order.
//   colrv1 — same layer model as colrv0 but each layer carries a paint:
//            solid, linear, radial or sweep gradient, or foreground,
//            under affine transforms.
//   vg     — SVG's path data and paint model sent as plain ASCII, not
//            base64: layers of fills, strokes and clips, painted with
//            the foreground, a fixed colour or a gradient, in the
//            coordinate system of a `vb=x,y,w,h` key. Not an SVG
//            document. The recommended format (spec §8.9); see
//            `rio_graphics::glyph_vector`.
//
// `cp` is always a single codepoint. For `r` and `c`, `cp` MUST be in
// the registration block `U+100000`–`U+1003FF` (spec §4); otherwise the
// request is rejected with `reason=out_of_namespace`. `q` accepts any
// valid Unicode scalar value, so an application can check whether the
// user's font already has an icon (a Nerd Fonts codepoint, say) before
// registering its own.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rio_graphics::glyph_layout::{GlyphAlignH, GlyphAlignV, GlyphLayout, GlyphSize};

/// Protocol identifier — the literal ASCII string `"25a1"` that
/// prefixes every Glyph Protocol APC body, always followed by `;`.
/// Use [`is_glyph_protocol`] to recognise a message.
pub const GLYPH_PROTOCOL_PREFIX: &[u8] = b"25a1";

/// Whether an APC body (without `ESC _` and the terminator) is a
/// Glyph Protocol message: the identifier followed by `;` (§3.1).
/// A dispatcher that reads control strings the way DEC structures
/// them sees `APC 2` with a command string starting `5a1;`; anything
/// else after `25a1` is some other `APC 2` and not ours, so it falls
/// through to the other APC handlers instead of being rejected here.
pub fn is_glyph_protocol(body: &[u8]) -> bool {
    body.strip_prefix(GLYPH_PROTOCOL_PREFIX)
        .is_some_and(|rest| rest.first() == Some(&b';'))
}

/// Upper bound on a single registered payload, post-base64-decode.
/// Matches the 32 KiB limit in the spec (§6.1); anything larger is rejected
/// with `payload_too_large`.
pub const MAX_PAYLOAD_BYTES: usize = 32 * 1024;

/// A payload format (§8), named on the wire by [`GlyphFormat::name`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlyphFormat {
    /// A TrueType simple glyph (§8.1).
    Glyf,
    /// COLRv0 layers over `glyf` outlines (§8.6).
    ColrV0,
    /// COLRv1 paint graphs over `glyf` outlines (§8.7).
    ColrV1,
    /// SVG path data as text (§8.9).
    Vg,
}

impl GlyphFormat {
    /// Every format this build parses, in the order the `s` reply lists
    /// them. A frontend that draws registered glyphs itself may accept
    /// fewer (`Handler::glyph_formats`).
    pub const ALL: &'static [GlyphFormat] = &[
        GlyphFormat::Glyf,
        GlyphFormat::ColrV0,
        GlyphFormat::ColrV1,
        GlyphFormat::Vg,
    ];

    /// The name in `fmt=`.
    pub fn name(self) -> &'static str {
        match self {
            GlyphFormat::Glyf => "glyf",
            GlyphFormat::ColrV0 => "colrv0",
            GlyphFormat::ColrV1 => "colrv1",
            GlyphFormat::Vg => "vg",
        }
    }

    fn from_name(name: &[u8]) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|f| f.name().as_bytes() == name)
    }
}

pub use rio_graphics::glyph_namespace::in_registration_block;

/// Capability name for the grapheme query (`q;grapheme=`, spec §5.3),
/// listed in the `s` reply's `cap` key by hosts that answer it.
pub const CAP_GRAPHEME: &str = "grapheme";

/// Most codepoints one `q;grapheme=` may carry (spec §5.1).
pub const MAX_GRAPHEME_CODEPOINTS: usize = 32;

/// Parsed Glyph Protocol command, ready for dispatch.
#[derive(Debug, Clone, PartialEq)]
pub enum GlyphCommand {
    /// Advertise supported payload formats. Parameter-free; doubles as
    /// the protocol-detection ping.
    Support,
    /// Query state of a single codepoint.
    Query { cp: u32 },
    /// Query whether one grapheme cluster, segmented by the client,
    /// renders correctly as a whole (spec §5.3). Holds 1 to
    /// [`MAX_GRAPHEME_CODEPOINTS`] scalars in logical order.
    QueryGrapheme { cps: Vec<char> },
    /// Register a glyph at a block codepoint chosen by the client. The
    /// `payload` carries format-specific data (monochrome `glyf`, or a
    /// `colrv0`/`colrv1` colour container wrapping OpenType tables).
    /// The `reply` level controls which replies (if any) the
    /// dispatcher emits — see [`ReplyMode`] for the three tiers.
    /// `width` is the registration's declared render span: `1`
    /// (narrow, the default) or `2` (wide). It never changes the
    /// codepoint's logical cell width; the renderer paints the glyph
    /// across that many cells in pixels while layout, cursor and
    /// selection keep following system wcwidth. `layout` is the §8.5
    /// sizing and placement, with the protocol's defaults for absent keys.
    Register {
        cp: u32,
        payload: GlyphPayload,
        reply: ReplyMode,
        width: u8,
        layout: GlyphLayout,
    },
    /// Clear a single block codepoint (`Some`) or every slot (`None`).
    /// `reply` works as it does for `r` (§7.1).
    Clear { cp: Option<u32>, reply: ReplyMode },
}

/// Upper bound on the number of glyph outlines carried in a single
/// colour payload. Keeps the glossary's decode cost bounded and sits
/// well within the 16-bit GlyphId namespace used by COLR.
const MAX_COLR_GLYPHS: u16 = 1024;

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
///
/// `Vector` is already parsed and validated: a malformed or oversized
/// payload is rejected here, at registration, like every other format.
#[derive(Debug, Clone, PartialEq)]
pub enum GlyphPayload {
    Glyf {
        glyf: Vec<u8>,
        upm: u16,
    },
    ColrV0 {
        container: ColrContainer,
        upm: u16,
    },
    ColrV1 {
        container: ColrContainer,
        upm: u16,
    },
    Vector {
        glyph: rio_graphics::glyph_vector::VectorGlyph,
    },
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

/// Query status — the set of sources covering `cp`, per spec §5.2.
/// Encoded on the wire as a comma-separated list of coverage names
/// (`system`, `glossary`, both, or empty for no coverage).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryStatus {
    Free,
    System,
    Glossary,
    Both,
}

impl QueryStatus {
    /// The status of a codepoint, from whether the session's glossary
    /// and the terminal's fonts cover it.
    pub fn new(glossary: bool, system: bool) -> Self {
        match (glossary, system) {
            (true, true) => QueryStatus::Both,
            (true, false) => QueryStatus::Glossary,
            (false, true) => QueryStatus::System,
            (false, false) => QueryStatus::Free,
        }
    }

    /// Wire form of the `status=` value: a comma-separated list of
    /// coverage names. `Free` returns the empty string.
    pub fn as_str(self) -> &'static str {
        match self {
            QueryStatus::Free => "",
            QueryStatus::System => "system",
            QueryStatus::Glossary => "glossary",
            QueryStatus::Both => "system,glossary",
        }
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
    OutlineTooLarge,
    /// `fmt` names a format this terminal doesn't list in `s`.
    UnsupportedFormat,
    /// Too many replaced registrations are still shown by text (§9).
    NoSpace,
}

impl RegisterError {
    fn as_str(self) -> &'static str {
        match self {
            RegisterError::OutOfNamespace => "out_of_namespace",
            RegisterError::CompositeUnsupported => "composite_unsupported",
            RegisterError::HintingUnsupported => "hinting_unsupported",
            RegisterError::MalformedPayload => "malformed_payload",
            RegisterError::PayloadTooLarge => "payload_too_large",
            RegisterError::OutlineTooLarge => "outline_too_large",
            RegisterError::UnsupportedFormat => "unsupported_format",
            RegisterError::NoSpace => "no_space",
        }
    }
}

/// Error returned when the APC body is not a valid Glyph Protocol
/// message, or when wire-level validation rejects the request before
/// it reaches the handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Body does not start with `25a1;` — not our protocol; caller
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
    /// `c;cp=<hex>` where the codepoint is outside the registration
    /// block. The reply echoes `cp`, unless `reply` suppresses it.
    ClearOutOfNamespace { cp: u32, reply: ReplyMode },
    /// A `q` the terminal cannot read: neither or both of `cp` and
    /// `grapheme`, a bad hex value, a surrogate, or too many
    /// codepoints. Answered with `reason=malformed_query` (§5.4) so a
    /// client waiting on the reply does not hang until its timeout.
    QueryMalformed(&'static str),
}

/// Parse a raw APC body (minus the `ESC _` introducer and `ESC \`
/// terminator) into a [`GlyphCommand`].
pub fn parse(body: &[u8]) -> Result<GlyphCommand, ParseError> {
    parse_with(body, GlyphFormat::ALL)
}

/// [`parse`] for a terminal that lists only `formats` in its `s`
/// reply: a registration in any other is `unsupported_format`.
pub fn parse_with(
    body: &[u8],
    formats: &[GlyphFormat],
) -> Result<GlyphCommand, ParseError> {
    if !is_glyph_protocol(body) {
        return Err(ParseError::NotGlyphProtocol);
    }
    // The identifier and its `;`.
    let rest = &body[GLYPH_PROTOCOL_PREFIX.len() + 1..];

    let (verb, rest) = split_once(rest, b';');
    let verb = trim(verb);
    if verb.len() != 1 {
        return Err(ParseError::Malformed("verb must be a single byte"));
    }
    match verb[0] {
        b's' => parse_support(rest),
        b'q' => parse_query(rest),
        b'r' => parse_register(rest, formats),
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
    match (params.get("cp"), params.get("grapheme")) {
        (Some(cp_raw), None) => {
            if cp_raw.contains(&b',') {
                return Err(ParseError::QueryMalformed("cp must be a single codepoint"));
            }
            let cp = parse_hex_cp(cp_raw)
                .ok_or(ParseError::QueryMalformed("query cp invalid hex"))?;
            Ok(GlyphCommand::Query { cp })
        }
        (None, Some(list)) => {
            let mut cps = Vec::new();
            for raw in list.split(|&b| b == b',') {
                if cps.len() == MAX_GRAPHEME_CODEPOINTS {
                    return Err(ParseError::QueryMalformed(
                        "grapheme holds more than 32 codepoints",
                    ));
                }
                let ch = parse_hex_cp(raw)
                    .and_then(char::from_u32)
                    .ok_or(ParseError::QueryMalformed("grapheme invalid hex"))?;
                cps.push(ch);
            }
            Ok(GlyphCommand::QueryGrapheme { cps })
        }
        (None, None) => Err(ParseError::QueryMalformed("query missing cp")),
        (Some(_), Some(_)) => Err(ParseError::QueryMalformed(
            "query carries both cp and grapheme",
        )),
    }
}

fn parse_register(
    rest: &[u8],
    formats: &[GlyphFormat],
) -> Result<GlyphCommand, ParseError> {
    // Register splits control parameters from the payload at the LAST
    // `;`. Neither base64 nor `fmt=vg` text has a `;`, so this is
    // unambiguous.
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

    // The block check is the protocol's security contract: reject early
    // so we don't bother decoding the payload.
    if !in_registration_block(cp) {
        return Err(ParseError::RegisterFailed {
            cp,
            reason: RegisterError::OutOfNamespace,
            reply,
        });
    }

    // No `fmt` means `glyf`, so registrations from before formats were
    // negotiated keep working. A format this terminal doesn't list in
    // `s` gets its own reason, so a client can tell it from a bad
    // payload and fall back to a listed one.
    let fmt = params.get("fmt").copied().unwrap_or(b"glyf");
    let Some(fmt) = GlyphFormat::from_name(fmt).filter(|f| formats.contains(f)) else {
        return Err(ParseError::RegisterFailed {
            cp,
            reason: RegisterError::UnsupportedFormat,
            reply,
        });
    };

    let malformed = || ParseError::RegisterFailed {
        cp,
        reason: RegisterError::MalformedPayload,
        reply,
    };

    // Checks run in the order of spec §6.2: `cp`, `fmt`, then the
    // placement keys, then the payload's size, then the payload.

    // Declared Unicode width (spec §8.5): `1` or `2`, default `1`.
    let width = match params.get("width") {
        Some(raw) => match trim(raw) {
            b"1" => 1,
            b"2" => 2,
            _ => return Err(malformed()),
        },
        None => 1,
    };

    if fmt == GlyphFormat::Vg {
        let layout = parse_vector_layout(&params).ok_or_else(malformed)?;
        let view_box = params
            .get("vb")
            .and_then(|raw| rio_graphics::glyph_vector::parse_view_box(raw))
            .ok_or_else(malformed)?;
        let payload = parse_vector_payload(view_box, &layout, trim(payload_b64))
            .map_err(|reason| ParseError::RegisterFailed { cp, reason, reply })?;
        return Ok(GlyphCommand::Register {
            cp,
            payload,
            reply,
            width,
            layout,
        });
    }

    // `upm` only means something to the font formats; `vg` ignores it.
    let upm = match params.get("upm") {
        Some(raw) => parse_decimal_u16(raw).ok_or(malformed())?,
        None => 1000,
    };
    if upm == 0 {
        return Err(malformed());
    }
    let layout = parse_layout(&params).ok_or_else(malformed)?;
    let payload_b64 = trim(payload_b64);
    // Size before content: base64 longer than this decodes past the cap
    // even with padding, so it's too large whether or not it's valid.
    if payload_b64.len() > MAX_PAYLOAD_BYTES.div_ceil(3) * 4 {
        return Err(ParseError::RegisterFailed {
            cp,
            reason: RegisterError::PayloadTooLarge,
            reply,
        });
    }
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
    // Empty payload is never a valid registration: a `glyf` outline
    // needs at least the simple-glyph header bytes, and a colr
    // container needs the `n_glyphs` u16 plus per-glyph data. An
    // empty body usually means the trailing `;<base64>` segment was
    // omitted altogether (e.g. `r;cp=E000;` with no payload), which
    // would otherwise silently register an empty glyph for the slot.
    if raw.is_empty() {
        return Err(ParseError::RegisterFailed {
            cp,
            reason: RegisterError::MalformedPayload,
            reply,
        });
    }

    let payload = match fmt {
        GlyphFormat::Glyf => GlyphPayload::Glyf { glyf: raw, upm },
        GlyphFormat::ColrV0 => {
            let container = parse_colr_container(&raw)
                .map_err(|reason| ParseError::RegisterFailed { cp, reason, reply })?;
            GlyphPayload::ColrV0 { container, upm }
        }
        GlyphFormat::ColrV1 => {
            let container = parse_colr_container(&raw)
                .map_err(|reason| ParseError::RegisterFailed { cp, reason, reply })?;
            GlyphPayload::ColrV1 { container, upm }
        }
        GlyphFormat::Vg => unreachable!("vg returned above"),
    };

    Ok(GlyphCommand::Register {
        cp,
        payload,
        reply,
        width,
        layout,
    })
}

/// `fmt=vg`: the payload is the text itself, read against the `vb`
/// key. Placement is [`parse_vector_layout`]'s.
fn parse_vector_payload(
    view_box: rio_graphics::glyph_vector::ViewBox,
    layout: &GlyphLayout,
    text: &[u8],
) -> Result<GlyphPayload, RegisterError> {
    use rio_graphics::glyph_vector::{self, Fit, VectorError};
    if text.len() > MAX_PAYLOAD_BYTES {
        return Err(RegisterError::PayloadTooLarge);
    }
    // The limits are measured in the viewBox side `fit` maps onto the
    // cell (spec §8.9.5).
    let fit = match layout.size {
        GlyphSize::Cover => Fit::Cover,
        GlyphSize::Stretch => Fit::Stretch,
        _ => Fit::Contain,
    };
    let glyph = glyph_vector::parse(text, view_box, fit).map_err(|err| match err {
        VectorError::Malformed => RegisterError::MalformedPayload,
        VectorError::TooLarge => RegisterError::OutlineTooLarge,
    })?;
    Ok(GlyphPayload::Vector { glyph })
}

/// `fmt=vg` placement: the viewBox is the extent, so the keys are the
/// ones SVG's `preserveAspectRatio` has. `fit` is `contain` (default),
/// `cover` or `stretch`; `align` is `<h>,<v>` with `start`, `center`
/// or `end` on each axis, `start` being the left and the top. A viewBox is Y-down with no baseline, so
/// `baseline` is invalid here. `size`, `pad` and the font-metric keys
/// (`upm`, `aw`, `lh`, `ymin`) belong to the font formats and are
/// ignored, like any key a format doesn't use.
fn parse_vector_layout(params: &Params<'_>) -> Option<GlyphLayout> {
    let mut layout = GlyphLayout::default();
    if let Some(raw) = params.get("fit") {
        layout.size = match trim(raw) {
            b"contain" => GlyphSize::Contain,
            b"cover" => GlyphSize::Cover,
            b"stretch" => GlyphSize::Stretch,
            _ => return None,
        };
    }
    if let Some(raw) = params.get("align") {
        let mut parts = raw.split(|&b| b == b',').map(trim);
        let (Some(h), Some(v), None) = (parts.next(), parts.next(), parts.next()) else {
            return None;
        };
        let axis = |raw: &[u8]| match raw {
            b"start" => Some(0),
            b"center" => Some(1),
            b"end" => Some(2),
            _ => None,
        };
        layout.align_h =
            [GlyphAlignH::Start, GlyphAlignH::Center, GlyphAlignH::End][axis(h)?];
        // The viewBox is Y-down, as in SVG, so a vertical `start` is
        // the top (SVG's `yMin`). `GlyphAlignV` is Y-up like the font
        // formats (§8.5.4), where `start` is the bottom: flip it here.
        layout.align_v =
            [GlyphAlignV::End, GlyphAlignV::Center, GlyphAlignV::Start][axis(v)?];
    }
    Some(layout)
}

/// Read the §8.5 sizing keys; each absent key takes the protocol's
/// default (`size=contain`, `align=center,center`, no padding). `None`
/// when a value is invalid.
fn parse_layout(params: &Params<'_>) -> Option<GlyphLayout> {
    let extent = |key: &str| match params.get(key) {
        Some(raw) => parse_decimal_u16(raw).filter(|value| *value > 0).map(Some),
        None => Some(None),
    };
    let mut layout = GlyphLayout {
        aw: extent("aw")?,
        lh: extent("lh")?,
        ..Default::default()
    };

    if let Some(raw) = params.get("ymin") {
        layout.ymin = Some(parse_decimal_i16(raw)?);
    }

    if let Some(raw) = params.get("size") {
        layout.size = match trim(raw) {
            b"height" => GlyphSize::Height,
            b"advance" => GlyphSize::Advance,
            b"contain" => GlyphSize::Contain,
            b"cover" => GlyphSize::Cover,
            b"stretch" => GlyphSize::Stretch,
            _ => return None,
        };
    }

    if let Some(raw) = params.get("align") {
        let mut parts = raw.split(|&b| b == b',').map(trim);
        let (Some(h), Some(v), None) = (parts.next(), parts.next(), parts.next()) else {
            return None;
        };
        layout.align_h = match h {
            b"start" => GlyphAlignH::Start,
            b"center" => GlyphAlignH::Center,
            b"end" => GlyphAlignH::End,
            _ => return None,
        };
        layout.align_v = match v {
            b"start" => GlyphAlignV::Start,
            b"center" => GlyphAlignV::Center,
            b"end" => GlyphAlignV::End,
            b"baseline" => GlyphAlignV::Baseline,
            _ => return None,
        };
    }

    if let Some(raw) = params.get("pad") {
        let [top, right, bottom, left] = parse_pad(raw)?;
        // Spec §8.5.2: a degenerate span is treated as no padding.
        if left + right < 1.0 && top + bottom < 1.0 {
            layout.pad = [top, right, bottom, left];
        }
    }

    Some(layout)
}

/// Parse `<top>,<right>,<bottom>,<left>`, each a fraction in `0..=1`.
fn parse_pad(raw: &[u8]) -> Option<[f32; 4]> {
    let mut out = [0.0; 4];
    let mut parts = raw.split(|&b| b == b',').map(trim);
    for slot in &mut out {
        let value: f32 = std::str::from_utf8(parts.next()?).ok()?.parse().ok()?;
        if !(0.0..=1.0).contains(&value) {
            return None;
        }
        *slot = value;
    }
    parts.next().is_none().then_some(out)
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
    let reply = params
        .get("reply")
        .map(|v| ReplyMode::from_wire(v))
        .unwrap_or_default();
    match params.get("cp") {
        Some(cp_raw) => {
            if cp_raw.contains(&b',') {
                return Err(ParseError::Malformed("cp must be a single codepoint"));
            }
            let cp = parse_hex_cp(cp_raw)
                .ok_or(ParseError::Malformed("clear cp invalid hex"))?;
            if !in_registration_block(cp) {
                return Err(ParseError::ClearOutOfNamespace { cp, reply });
            }
            Ok(GlyphCommand::Clear {
                cp: Some(cp),
                reply,
            })
        }
        None => Ok(GlyphCommand::Clear { cp: None, reply }),
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

/// A decimal integer: digits with an optional leading `-`, and nothing
/// else `str::parse` would allow, such as a leading `+`.
fn parse_decimal_i16(raw: &[u8]) -> Option<i16> {
    let raw = trim(raw);
    let (negative, digits) = match raw.strip_prefix(b"-") {
        Some(digits) => (true, digits),
        None => (false, raw),
    };
    if !digits.first()?.is_ascii_digit() {
        return None;
    }
    let magnitude = i32::from(parse_decimal_u16(digits)?);
    i16::try_from(if negative { -magnitude } else { magnitude }).ok()
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

/// Format the reply to `s` (support). `fmts` are the accepted payload
/// formats, their names joined with `,` on the wire. A terminal that
/// accepts none doesn't answer `s` at all (`Handler::glyph_formats`).
/// `caps` lists optional capabilities (§3.3); the `cap` key is left
/// out when there are none, which the spec defines as equal to an
/// empty `cap=`.
pub(crate) fn format_support_response(fmts: &[GlyphFormat], caps: &[&str]) -> String {
    let fmts: Vec<&str> = fmts.iter().map(|f| f.name()).collect();
    if caps.is_empty() {
        format!("\x1b_25a1;s;fmt={}\x1b\\", fmts.join(","))
    } else {
        format!(
            "\x1b_25a1;s;fmt={};cap={}\x1b\\",
            fmts.join(","),
            caps.join(",")
        )
    }
}

/// Format the reply to `q;cp=<hex>`. Public because the frontend
/// (`rioterm::application`) is the one that has access to both
/// `FontLibrary` and the per-route registry needed to compute the
/// status; it formats the reply itself and writes it back to the PTY.
pub fn format_query_response(cp: u32, status: QueryStatus) -> String {
    format!("\x1b_25a1;q;cp={:x};status={}\x1b\\", cp, status.as_str())
}

/// The reply to `q;cp=<hex>` from a terminal that won't look it up
/// now (§5.4 `reason=unknown`), for one limiting how many queries it
/// answers. The client can ask again later.
pub fn format_query_unknown(cp: u32) -> String {
    format!("\x1b_25a1;q;cp={:x};reason=unknown\x1b\\", cp)
}

/// The `grapheme=` value as echoed in replies: lowercase hex joined
/// with `,`.
fn grapheme_hex(cps: &[char]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(cps.len() * 6);
    for (i, ch) in cps.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        // Writing to a String can't fail.
        let _ = write!(out, "{:x}", *ch as u32);
    }
    out
}

/// Format the reply to `q;grapheme=…` (§5.3). Public for the same
/// reason as [`format_query_response`]: the frontend computes it.
pub fn format_grapheme_query_response(cps: &[char], status: QueryStatus) -> String {
    format!(
        "\x1b_25a1;q;grapheme={};status={}\x1b\\",
        grapheme_hex(cps),
        status.as_str()
    )
}

/// `q;grapheme=…` sent to a host that does not answer grapheme
/// queries (§5.4).
pub(crate) fn format_grapheme_query_unsupported(cps: &[char]) -> String {
    format!(
        "\x1b_25a1;q;grapheme={};reason=unsupported\x1b\\",
        grapheme_hex(cps)
    )
}

/// A `q` the terminal could not read (§5.4). Nothing is echoed: the
/// request's target is exactly what failed to parse.
pub(crate) fn format_query_malformed() -> String {
    String::from("\x1b_25a1;q;reason=malformed_query\x1b\\")
}

/// Format a successful register reply.
pub(crate) fn format_register_ok(cp: u32) -> String {
    format!("\x1b_25a1;r;cp={:x};status=0\x1b\\", cp)
}

/// Format a register error reply.
pub(crate) fn format_register_error(cp: u32, reason: RegisterError) -> String {
    format!(
        "\x1b_25a1;r;cp={:x};status=1;reason={}\x1b\\",
        cp,
        reason.as_str()
    )
}

/// Format the reply to a clear request. `cp` is echoed back when the
/// request scoped to a single slot, omitted for "clear all".
pub(crate) fn format_clear_ok(cp: Option<u32>) -> String {
    match cp {
        Some(cp) => format!("\x1b_25a1;c;cp={:x};status=0\x1b\\", cp),
        None => String::from("\x1b_25a1;c;status=0\x1b\\"),
    }
}

/// Format a clear error reply (currently only `out_of_namespace`),
/// echoing the `cp` that was refused.
pub(crate) fn format_clear_error_out_of_namespace(cp: u32) -> String {
    format!("\x1b_25a1;c;cp={cp:x};status=1;reason=out_of_namespace\x1b\\")
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
    fn identifier_is_25a1_followed_by_a_semicolon() {
        // Read as DEC structures control strings, these are all `APC 2`.
        // Only a command string starting `5a1;` is Glyph Protocol; the
        // rest belong to someone else and fall through (§3.1).
        assert!(is_glyph_protocol(b"25a1;s"));
        assert_eq!(parse(b"25a1;s"), Ok(GlyphCommand::Support));
        for other in [&b"25a1"[..], b"25a1s", b"25a1 ;s", b"25a10;s", b"2", b"25a"] {
            assert!(!is_glyph_protocol(other), "{:?}", other);
            assert_eq!(
                parse(other),
                Err(ParseError::NotGlyphProtocol),
                "{:?}",
                other
            );
        }
    }

    #[test]
    fn parses_query_single_codepoint() {
        let got = parse(b"25a1;q;cp=E0A0").unwrap();
        assert_eq!(got, GlyphCommand::Query { cp: 0xE0A0 });
    }

    #[test]
    fn query_accepts_codepoints_outside_the_block() {
        // Query probes the world; it is not limited to the block.
        let got = parse(b"25a1;q;cp=61").unwrap();
        assert_eq!(got, GlyphCommand::Query { cp: 0x61 });
    }

    #[test]
    fn query_rejects_sequence() {
        assert!(matches!(
            parse(b"25a1;q;cp=2D,3E"),
            Err(ParseError::QueryMalformed(_))
        ));
    }

    #[test]
    fn query_rejects_surrogate() {
        assert!(matches!(
            parse(b"25a1;q;cp=D800"),
            Err(ParseError::QueryMalformed(_))
        ));
    }

    #[test]
    fn parses_register_at_block_codepoint() {
        let payload = b64(&[0x01, 0x02, 0x03]);
        let body = format!("25a1;r;cp=100000;upm=1000;{}", payload);
        let got = parse(body.as_bytes()).unwrap();
        assert_eq!(
            got,
            GlyphCommand::Register {
                cp: 0x10_0000,
                payload: GlyphPayload::Glyf {
                    glyf: vec![0x01, 0x02, 0x03],
                    upm: 1000,
                },
                reply: ReplyMode::All,
                width: 1,
                layout: GlyphLayout::default(),
            }
        );
    }

    fn parse_layout_of(params: &str) -> Result<GlyphLayout, ParseError> {
        let body = format!("25a1;r;cp=100000;{params};{}", b64(&[1, 2, 3]));
        parse(body.as_bytes()).map(|command| match command {
            GlyphCommand::Register { layout, .. } => layout,
            other => panic!("expected a registration, got {other:?}"),
        })
    }

    #[test]
    fn register_without_sizing_keys_takes_the_spec_defaults() {
        let layout = parse_layout_of("upm=1000;width=2;future=1").unwrap();
        assert_eq!(layout, GlyphLayout::default());
        assert_eq!(layout.size, GlyphSize::Contain);
        assert_eq!(layout.align_h, GlyphAlignH::Center);
        assert_eq!(layout.align_v, GlyphAlignV::Center);
    }

    #[test]
    fn register_parses_every_sizing_key() {
        assert_eq!(
            parse_layout_of(
                "upm=2048;aw=1024;lh=1536;ymin=-148;size=cover;align=end,baseline;pad=0.1,0.2,0.3,0.4"
            ),
            Ok(GlyphLayout {
                aw: Some(1024),
                lh: Some(1536),
                ymin: Some(-148),
                size: GlyphSize::Cover,
                align_h: GlyphAlignH::End,
                align_v: GlyphAlignV::Baseline,
                pad: [0.1, 0.2, 0.3, 0.4],
            })
        );
    }

    #[test]
    fn register_single_sizing_key_defaults_the_rest() {
        assert_eq!(
            parse_layout_of("size=stretch"),
            Ok(GlyphLayout {
                size: GlyphSize::Stretch,
                ..Default::default()
            })
        );
    }

    #[test]
    fn register_degenerate_pad_means_no_padding() {
        assert_eq!(
            parse_layout_of("pad=0.4,0.2,0.6,0.1"),
            Ok(GlyphLayout::default())
        );
    }

    #[test]
    fn register_invalid_sizing_value_honours_the_reply_level() {
        let body = format!("25a1;r;cp=100000;reply=2;aw=0;{}", b64(&[1, 2, 3]));
        assert!(matches!(
            parse(body.as_bytes()),
            Err(ParseError::RegisterFailed {
                reply: ReplyMode::ErrorsOnly,
                ..
            })
        ));
    }

    #[test]
    fn register_ymin_accepts_the_whole_signed_range() {
        for (text, expected) in [
            ("-148", -148),
            ("0", 0),
            ("-32768", i16::MIN),
            ("32767", i16::MAX),
        ] {
            let layout = parse_layout_of(&format!("ymin={text}")).unwrap();
            assert_eq!(layout.ymin, Some(expected), "ymin={text}");
        }
    }

    #[test]
    fn register_rejects_invalid_sizing_values() {
        for params in [
            "aw=0",
            "lh=x",
            "ymin=x",
            "ymin=99999",
            "ymin=+148",
            "ymin=--5",
            "ymin=- 5",
            "ymin=-",
            "ymin=",
            "ymin=32768",
            "ymin=-32769",
            "size=huge",
            "align=center",
            "align=middle,center",
            "align=center,center,center",
            "pad=0,0,0",
            "pad=0,0,0,0,0",
            "pad=0,1.2,0,0",
            "pad=nan,0,0,0",
        ] {
            assert!(
                matches!(
                    parse_layout_of(params),
                    Err(ParseError::RegisterFailed {
                        cp: 0x10_0000,
                        reason: RegisterError::MalformedPayload,
                        ..
                    })
                ),
                "{params} must be rejected"
            );
        }
    }

    #[test]
    fn register_width_defaults_to_narrow_and_accepts_wide() {
        let payload = b64(&[0x01]);

        // Absent → 1 (checked in parses_register_at_block_codepoint too).
        let body = format!("25a1;r;cp=100000;upm=1000;width=2;{}", payload);
        match parse(body.as_bytes()).unwrap() {
            GlyphCommand::Register { width, .. } => assert_eq!(width, 2),
            other => panic!("expected register, got {:?}", other),
        }

        let body = format!("25a1;r;cp=100000;upm=1000;width=1;{}", payload);
        match parse(body.as_bytes()).unwrap() {
            GlyphCommand::Register { width, .. } => assert_eq!(width, 1),
            other => panic!("expected register, got {:?}", other),
        }
    }

    #[test]
    fn register_rejects_invalid_width() {
        let payload = b64(&[0x01]);
        for bad in ["0", "3", "two", ""].iter() {
            let body = format!("25a1;r;cp=100000;upm=1000;width={};{}", bad, payload);
            assert!(
                matches!(
                    parse(body.as_bytes()),
                    Err(ParseError::RegisterFailed {
                        reason: RegisterError::MalformedPayload,
                        ..
                    })
                ),
                "width={:?} must be rejected",
                bad
            );
        }
    }

    #[test]
    fn parses_register_with_explicit_fmt() {
        let payload = b64(&[0xAA]);
        let body = format!("25a1;r;cp=100000;fmt=glyf;upm=1000;{}", payload);
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
        let body = format!("25a1;r;cp=100000;{}", payload);
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
    fn register_rejects_codepoint_outside_the_block() {
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
    fn register_accepts_only_the_block() {
        for cp in [0x10_0000u32, 0x10_03FF] {
            let body = format!("25a1;r;cp={:x};upm=1000;{}", cp, b64(b"x"));
            assert!(matches!(
                parse(body.as_bytes()).unwrap(),
                GlyphCommand::Register { .. }
            ));
        }
        // The rest of PUA holds the user's font icons (Nerd Fonts in the
        // basic PUA and PUA-A) and is out of reach, as is PUA-B past the
        // block.
        for cp in [
            0xE000u32, 0xE0A0, 0xF8FF, 0xF_0000, 0xF_FFFD, 0x10_0400, 0x10_FFFD,
        ] {
            let body = format!("25a1;r;cp={:x};upm=1000;{}", cp, b64(b"x"));
            assert_eq!(
                parse(body.as_bytes()),
                Err(ParseError::RegisterFailed {
                    cp,
                    reason: RegisterError::OutOfNamespace,
                    reply: ReplyMode::All,
                }),
                "U+{cp:X}"
            );
        }
    }

    #[test]
    fn register_rejects_unknown_fmt() {
        let payload = b64(b"x");
        // Not a format Rio lists in `s`: its own reason, not a parse
        // error, and the reply level still applies.
        for (fmt, reply) in [
            ("svg", ReplyMode::All),
            ("woff2", ReplyMode::All),
            ("GLYF", ReplyMode::All),
            ("", ReplyMode::None),
        ] {
            let level = if reply == ReplyMode::None {
                "reply=0;"
            } else {
                ""
            };
            let body = format!("25a1;r;cp=100000;fmt={fmt};{level}upm=1000;{payload}");
            assert_eq!(
                parse(body.as_bytes()),
                Err(ParseError::RegisterFailed {
                    cp: 0x10_0000,
                    reason: RegisterError::UnsupportedFormat,
                    reply,
                }),
                "fmt={fmt}"
            );
        }
        // The block check still comes first.
        let body = format!("25a1;r;cp=E000;fmt=svg;{payload}");
        assert!(matches!(
            parse(body.as_bytes()),
            Err(ParseError::RegisterFailed {
                reason: RegisterError::OutOfNamespace,
                ..
            })
        ));
        assert_eq!(
            format_register_error(0x10_0000, RegisterError::UnsupportedFormat),
            "\x1b_25a1;r;cp=100000;status=1;reason=unsupported_format\x1b\\"
        );
    }

    #[test]
    fn register_reports_problems_in_spec_order() {
        // Spec §6.2: `cp`, then `fmt`, then placement keys, then the
        // payload's size, then the payload itself.
        let reason = |body: String| match parse(body.as_bytes()) {
            Err(ParseError::RegisterFailed { reason, .. }) => reason,
            other => panic!("{body}: {other:?}"),
        };
        let huge_vg = format!("M0 0{}", "l0 0".repeat(MAX_PAYLOAD_BYTES / 4 + 1));
        let huge_b64 = "$".repeat(MAX_PAYLOAD_BYTES.div_ceil(3) * 4 + 4);
        use RegisterError::*;
        // Everything wrong: the codepoint wins.
        assert_eq!(
            reason(format!("25a1;r;cp=61;fmt=svg;vb=x;{huge_vg}")),
            OutOfNamespace
        );
        // Then the format.
        assert_eq!(
            reason(format!("25a1;r;cp=100000;fmt=svg;vb=x;{huge_vg}")),
            UnsupportedFormat
        );
        // Then the placement keys, before the size.
        assert_eq!(
            reason(format!("25a1;r;cp=100000;fmt=vg;vb=x;{huge_vg}")),
            MalformedPayload
        );
        assert_eq!(
            reason(format!(
                "25a1;r;cp=100000;fmt=vg;vb=0,0,24,24;width=3;{huge_vg}"
            )),
            MalformedPayload
        );
        assert_eq!(
            reason(format!("25a1;r;cp=100000;upm=0;{huge_b64}")),
            MalformedPayload
        );
        // Then the size, before the content: invalid and too large is
        // too large, for text and for base64.
        assert_eq!(
            reason(format!(
                "25a1;r;cp=100000;fmt=vg;vb=0,0,24,24;{}X",
                &huge_vg[..]
            )),
            PayloadTooLarge
        );
        assert_eq!(
            reason(format!("25a1;r;cp=100000;{huge_b64}")),
            PayloadTooLarge
        );
        // Small and invalid is malformed.
        assert_eq!(
            reason("25a1;r;cp=100000;$$$$".to_string()),
            MalformedPayload
        );
    }

    #[test]
    fn vg_ignores_the_font_keys() {
        // `upm` is a font key: invalid or zero, `vg` doesn't read it.
        for keys in ["upm=0;", "upm=x;", "aw=0;lh=x;ymin=y;", "size=bogus;pad=9;"] {
            let body = format!(
                "25a1;r;cp=100000;fmt=vg;vb=0,0,24,24;{keys}fill=nonzero,fg:M0 0h4v4z"
            );
            assert!(
                matches!(parse(body.as_bytes()), Ok(GlyphCommand::Register { .. })),
                "{keys}"
            );
        }
    }

    #[test]
    fn vg_limits_follow_fit() {
        // A thin viewBox under cover is measured by its smaller side.
        let reg = |fit: &str| {
            parse(
                format!("25a1;r;cp=100000;fmt=vg;vb=0,0,1000,1;{fit}fill=nonzero,fg:M0 0h1000")
                    .as_bytes(),
            )
        };
        assert!(matches!(reg(""), Ok(GlyphCommand::Register { .. })));
        for fit in ["fit=cover;", "fit=stretch;"] {
            assert!(
                matches!(
                    reg(fit),
                    Err(ParseError::RegisterFailed {
                        reason: RegisterError::OutlineTooLarge,
                        ..
                    })
                ),
                "{fit}"
            );
        }
        // And past 4096:1 a viewBox is malformed.
        assert!(matches!(
            parse(b"25a1;r;cp=100000;fmt=vg;vb=0 0 1 0.000001;fill=nonzero,fg:M0 0h1"),
            Err(ParseError::RegisterFailed {
                reason: RegisterError::MalformedPayload,
                ..
            })
        ));
    }

    #[test]
    fn every_advertised_format_is_accepted_by_name() {
        // `s` and `r` read the same list, so they can't drift apart.
        for fmt in GlyphFormat::ALL.iter().map(|f| f.name()) {
            let body = format!("25a1;r;cp=100000;fmt={fmt};x");
            assert!(
                !matches!(
                    parse(body.as_bytes()),
                    Err(ParseError::RegisterFailed {
                        reason: RegisterError::UnsupportedFormat,
                        ..
                    })
                ),
                "{fmt}"
            );
        }
    }

    #[test]
    fn parses_register_with_fmt_vg() {
        let body = b"25a1;r;cp=100001;fmt=vg;vb=0,0,24,24;fit=contain;\
stroke=2,fg:M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z";
        match parse(body) {
            Ok(GlyphCommand::Register {
                cp,
                payload: GlyphPayload::Vector { glyph },
                ..
            }) => {
                assert_eq!(cp, 0x10_0001);
                assert_eq!(glyph.view_box.w, 24.0);
                assert_eq!(glyph.layers.len(), 1);
            }
            other => panic!("expected a vg registration, got {other:?}"),
        }
    }

    #[test]
    fn fmt_vg_placement_is_fit_and_align() {
        let layout = |keys: &str| match parse(
            format!(
                "25a1;r;cp=100000;fmt=vg;vb=0,0,24,24;{keys}fill=nonzero,fg:M0 0h4v4z"
            )
            .as_bytes(),
        ) {
            Ok(GlyphCommand::Register { layout, .. }) => Ok(layout),
            Err(ParseError::RegisterFailed { reason, .. }) => Err(reason),
            other => panic!("{other:?}"),
        };
        assert_eq!(layout("").unwrap(), GlyphLayout::default());
        let l = layout("fit=cover;align=end,start;").unwrap();
        assert_eq!(l.size, GlyphSize::Cover);
        // Y-down: the top, which the Y-up enum calls `End`.
        assert_eq!((l.align_h, l.align_v), (GlyphAlignH::End, GlyphAlignV::End));
        assert_eq!(
            layout("align=start,end;").unwrap().align_v,
            GlyphAlignV::Start
        );
        assert_eq!(layout("fit=stretch;").unwrap().size, GlyphSize::Stretch);
        // The font formats' keys don't apply and are ignored.
        assert_eq!(
            layout("size=height;pad=0.2,0.2,0.2,0.2;upm=2048;aw=600;").unwrap(),
            GlyphLayout::default()
        );
        // A viewBox has no baseline, and fit has no height or advance.
        for keys in [
            "align=center,baseline;",
            "fit=height;",
            "fit=advance;",
            "align=center;",
        ] {
            assert_eq!(layout(keys), Err(RegisterError::MalformedPayload), "{keys}");
        }
    }

    #[test]
    fn fmt_vg_rejections() {
        let reason = |body: &str| match parse(body.as_bytes()) {
            Err(ParseError::RegisterFailed { reason, .. }) => Some(reason),
            _ => None,
        };
        // No viewBox, or a bad one.
        assert_eq!(
            reason("25a1;r;cp=100000;fmt=vg;fill=nonzero,fg:M0 0h4v4z"),
            Some(RegisterError::MalformedPayload)
        );
        assert_eq!(
            reason("25a1;r;cp=100000;fmt=vg;vb=0,0,0,24;fill=nonzero,fg:M0 0h4v4z"),
            Some(RegisterError::MalformedPayload)
        );
        // Bad path data rejects the whole registration.
        assert_eq!(
            reason("25a1;r;cp=100000;fmt=vg;vb=0,0,24,24;fill=nonzero,fg:M0 0L1"),
            Some(RegisterError::MalformedPayload)
        );
        // Geometry far outside the viewBox, and too many segments.
        assert_eq!(
            reason("25a1;r;cp=100000;fmt=vg;vb=0,0,24,24;fill=nonzero,fg:M0 0L1e6 0"),
            Some(RegisterError::OutlineTooLarge)
        );
        let many = format!(
            "25a1;r;cp=100000;fmt=vg;vb=0,0,24,24;fill=nonzero,fg:M0 0{}",
            "l1 0 -1 0 ".repeat(3000)
        );
        assert_eq!(reason(&many), Some(RegisterError::OutlineTooLarge));
        // Past 32 KiB of text.
        let huge = format!(
            "25a1;r;cp=100000;fmt=vg;vb=0,0,24,24;fill=nonzero,fg:M0 0{}",
            "l0 0".repeat(MAX_PAYLOAD_BYTES / 4 + 1)
        );
        assert_eq!(reason(&huge), Some(RegisterError::PayloadTooLarge));
        // The block check still comes first.
        assert_eq!(
            reason("25a1;r;cp=E000;fmt=vg;vb=0,0,24,24;fill=nonzero,fg:M0 0h4v4z"),
            Some(RegisterError::OutOfNamespace)
        );
    }

    #[test]
    fn register_rejects_bad_base64() {
        let body = b"25a1;r;cp=100000;upm=1000;$$$$not_base64";
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
        let body = format!("25a1;r;cp=100000;upm=1000;{}", payload);
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
        let body = format!("25a1;r;cp=100000;upm=0;{}", payload);
        assert!(matches!(
            parse(body.as_bytes()),
            Err(ParseError::RegisterFailed {
                cp: 0x10_0000,
                reason: RegisterError::MalformedPayload,
                ..
            })
        ));
    }

    #[test]
    fn register_invalid_upm_honours_the_reply_level() {
        let body = format!("25a1;r;cp=100000;reply=2;upm=abc;{}", b64(&[1, 2, 3]));
        assert!(matches!(
            parse(body.as_bytes()),
            Err(ParseError::RegisterFailed {
                reply: ReplyMode::ErrorsOnly,
                ..
            })
        ));
    }

    #[test]
    fn register_rejects_empty_payload() {
        // Trailing `;` followed by an empty base64 segment. Without the
        // post-decode empty check this would silently produce a
        // `Glyf{glyf: vec![]}` registration that the renderer would
        // later interpret as zero-byte garbage.
        let body = b"25a1;r;cp=100000;upm=1000;";
        assert!(matches!(
            parse(body),
            Err(ParseError::RegisterFailed {
                cp: 0x10_0000,
                reason: RegisterError::MalformedPayload,
                ..
            })
        ));
    }

    #[test]
    fn register_rejects_missing_payload_separator() {
        // No `;` between control params and the would-be payload: the
        // last-`;` split treats the whole tail as control, leaving the
        // payload section empty. Same MalformedPayload as the trailing-
        // semicolon case above.
        let body = b"25a1;r;cp=100000";
        assert!(matches!(
            parse(body),
            Err(ParseError::RegisterFailed {
                cp: 0x10_0000,
                reason: RegisterError::MalformedPayload,
                ..
            })
        ));
    }

    #[test]
    fn register_rejects_cp_above_unicode_max() {
        // 6-hex-digit cps that overflow the 0x10FFFF Unicode ceiling
        // must fail at hex parse time, not silently wrap. `parse_hex_cp`
        // surfaces this as a generic `Malformed` since we no longer
        // know which `cp` to attach to a `RegisterFailed`.
        let payload = b64(b"x");
        let body = format!("25a1;r;cp=110000;upm=1000;{}", payload);
        assert!(matches!(
            parse(body.as_bytes()),
            Err(ParseError::Malformed(_))
        ));

        let body = format!("25a1;r;cp=FFFFFF;upm=1000;{}", payload);
        assert!(matches!(
            parse(body.as_bytes()),
            Err(ParseError::Malformed(_))
        ));
    }

    #[test]
    fn register_rejects_empty_cp_value() {
        // `cp=` with no value: `parse_hex_cp` returns None on the empty
        // slice, surfacing as Malformed("register cp invalid hex").
        let payload = b64(b"x");
        let body = format!("25a1;r;cp=;upm=1000;{}", payload);
        assert!(matches!(
            parse(body.as_bytes()),
            Err(ParseError::Malformed(_))
        ));
    }

    #[test]
    fn query_rejects_cp_above_unicode_max() {
        let body = b"25a1;q;cp=110000";
        assert!(matches!(parse(body), Err(ParseError::QueryMalformed(_))));
    }

    #[test]
    fn clear_single_block_slot() {
        let got = parse(b"25a1;c;cp=100000").unwrap();
        assert_eq!(
            got,
            GlyphCommand::Clear {
                cp: Some(0x10_0000),
                reply: ReplyMode::All
            }
        );
    }

    #[test]
    fn clear_takes_a_reply_level() {
        assert_eq!(
            parse(b"25a1;c;cp=100000;reply=0").unwrap(),
            GlyphCommand::Clear {
                cp: Some(0x10_0000),
                reply: ReplyMode::None
            }
        );
        assert_eq!(
            parse(b"25a1;c;reply=2").unwrap(),
            GlyphCommand::Clear {
                cp: None,
                reply: ReplyMode::ErrorsOnly
            }
        );
        assert_eq!(
            parse(b"25a1;c;cp=61;reply=0"),
            Err(ParseError::ClearOutOfNamespace {
                cp: 0x61,
                reply: ReplyMode::None
            })
        );
    }

    #[test]
    fn clear_rejects_cp_outside_the_block() {
        for cp in [0x61, 0xE0A0, 0x10_0400, 0x1F600] {
            assert_eq!(
                parse(format!("25a1;c;cp={cp:X}").as_bytes()),
                Err(ParseError::ClearOutOfNamespace {
                    cp,
                    reply: ReplyMode::All
                })
            );
        }
    }

    #[test]
    fn clear_rejects_sequence_cp() {
        assert!(matches!(
            parse(b"25a1;c;cp=100000,100001"),
            Err(ParseError::Malformed(_))
        ));
    }

    #[test]
    fn clear_all() {
        let got = parse(b"25a1;c").unwrap();
        assert_eq!(
            got,
            GlyphCommand::Clear {
                cp: None,
                reply: ReplyMode::All
            }
        );
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
    fn support_response_advertises_every_format() {
        assert_eq!(
            format_support_response(GlyphFormat::ALL, &[]),
            "\x1b_25a1;s;fmt=glyf,colrv0,colrv1,vg\x1b\\"
        );
    }

    #[test]
    fn support_response_encodes_subset_and_empty() {
        assert_eq!(
            format_support_response(&[GlyphFormat::Glyf, GlyphFormat::ColrV0], &[]),
            "\x1b_25a1;s;fmt=glyf,colrv0\x1b\\"
        );
        assert_eq!(format_support_response(&[], &[]), "\x1b_25a1;s;fmt=\x1b\\");
    }

    #[test]
    fn only_the_listed_formats_register() {
        let body = |fmt: &str| format!("25a1;r;cp=100000;fmt={fmt};AAAA");
        let unsupported = |result| {
            matches!(
                result,
                Err(ParseError::RegisterFailed {
                    reason: RegisterError::UnsupportedFormat,
                    ..
                })
            )
        };
        let glyf_only = &[GlyphFormat::Glyf][..];
        assert!(unsupported(parse_with(
            body("colrv0").as_bytes(),
            glyf_only
        )));
        assert!(unsupported(parse_with(body("vg").as_bytes(), glyf_only)));
        assert!(unsupported(parse_with(body("svg").as_bytes(), glyf_only)));
        // No `fmt` is `glyf`, so it's refused where `glyf` isn't listed.
        let vg_only = &[GlyphFormat::Vg][..];
        assert!(unsupported(parse_with(b"25a1;r;cp=100000;AAAA", vg_only)));
        assert!(!unsupported(parse_with(body("glyf").as_bytes(), glyf_only)));
    }

    #[test]
    fn support_response_lists_capabilities() {
        assert_eq!(
            format_support_response(GlyphFormat::ALL, &[CAP_GRAPHEME]),
            "\x1b_25a1;s;fmt=glyf,colrv0,colrv1,vg;cap=grapheme\x1b\\"
        );
    }

    #[test]
    fn parses_grapheme_query() {
        let got = parse(b"25a1;q;grapheme=1F468,200d,1f469,200D,1f467").unwrap();
        assert_eq!(
            got,
            GlyphCommand::QueryGrapheme {
                cps: vec![
                    '\u{1F468}',
                    '\u{200D}',
                    '\u{1F469}',
                    '\u{200D}',
                    '\u{1F467}'
                ],
            }
        );
        assert_eq!(
            parse(b"25a1;q;grapheme=61").unwrap(),
            GlyphCommand::QueryGrapheme { cps: vec!['a'] }
        );
    }

    #[test]
    fn grapheme_query_takes_at_most_32_codepoints() {
        let list = |n: usize| vec!["301"; n].join(",");
        let ok = format!("25a1;q;grapheme=65,{}", list(31));
        assert!(matches!(
            parse(ok.as_bytes()),
            Ok(GlyphCommand::QueryGrapheme { cps }) if cps.len() == 32
        ));
        let over = format!("25a1;q;grapheme=65,{}", list(32));
        assert!(matches!(
            parse(over.as_bytes()),
            Err(ParseError::QueryMalformed(_))
        ));
    }

    #[test]
    fn grapheme_query_rejects_bad_values() {
        for body in [
            &b"25a1;q;grapheme="[..],
            b"25a1;q;grapheme=1f468,,1f469",
            b"25a1;q;grapheme=1f468,zz",
            b"25a1;q;grapheme=d800",
            b"25a1;q;grapheme=110000",
        ] {
            assert!(
                matches!(parse(body), Err(ParseError::QueryMalformed(_))),
                "{}",
                String::from_utf8_lossy(body)
            );
        }
    }

    #[test]
    fn query_needs_exactly_one_target() {
        assert!(matches!(
            parse(b"25a1;q"),
            Err(ParseError::QueryMalformed(_))
        ));
        assert!(matches!(
            parse(b"25a1;q;cp=61;grapheme=61"),
            Err(ParseError::QueryMalformed(_))
        ));
    }

    #[test]
    fn grapheme_query_replies() {
        let family = ['\u{1F468}', '\u{200D}', '\u{1F469}'];
        assert_eq!(
            format_grapheme_query_response(&family, QueryStatus::System),
            "\x1b_25a1;q;grapheme=1f468,200d,1f469;status=system\x1b\\"
        );
        assert_eq!(
            format_grapheme_query_response(&family, QueryStatus::Free),
            "\x1b_25a1;q;grapheme=1f468,200d,1f469;status=\x1b\\"
        );
        assert_eq!(
            format_grapheme_query_unsupported(&family),
            "\x1b_25a1;q;grapheme=1f468,200d,1f469;reason=unsupported\x1b\\"
        );
        assert_eq!(
            format_query_malformed(),
            "\x1b_25a1;q;reason=malformed_query\x1b\\"
        );
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
        let body = format!("25a1;r;cp=100000;fmt=colrv0;upm=1000;{}", b64(&container));
        let got = parse(body.as_bytes()).unwrap();
        match got {
            GlyphCommand::Register {
                cp: 0x10_0000,
                payload:
                    GlyphPayload::ColrV0 {
                        container: c,
                        upm: 1000,
                    },
                reply: ReplyMode::All,
                layout: _,
                width: 1,
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
                layout: _,
                width: 1,
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
        let body = format!("25a1;r;cp=100000;fmt=colrv0;upm=1000;{}", b64(&container));
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
        let body = format!("25a1;r;cp=100000;fmt=colrv0;upm=1000;{}", b64(&container));
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
        let body = format!("25a1;r;cp=100000;fmt=colrv1;upm=1000;{}", b64(&bad));
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
        let body = format!("25a1;r;cp=100000;fmt=colrv0;upm=1000;{}", b64(&container));
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
        let body = format!("25a1;r;cp=100000;fmt=colrv0;upm=1000;{}", b64(&bad));
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
        let body = format!("25a1;r;cp=100000;upm=1000;{}", payload);
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
            let body = format!("25a1;r;cp=100000;reply={};upm=1000;{}", raw, payload);
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
        // A cp outside the block fails validation; the reply level must propagate
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
            let body = format!("25a1;r;cp=100000;reply={};upm=1000;{}", bad, payload);
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
    fn colr_register_respects_block_check_before_fmt_parse() {
        // A cp outside the block is rejected for colour formats too, and the
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
    fn query_response_encodes_coverage_names() {
        assert_eq!(
            format_query_response(0x10_0000, QueryStatus::Free),
            "\x1b_25a1;q;cp=100000;status=\x1b\\"
        );
        assert_eq!(
            format_query_response(0x10_0000, QueryStatus::System),
            "\x1b_25a1;q;cp=100000;status=system\x1b\\"
        );
        assert_eq!(
            format_query_response(0x10_0000, QueryStatus::Glossary),
            "\x1b_25a1;q;cp=100000;status=glossary\x1b\\"
        );
        assert_eq!(
            format_query_response(0x10_0000, QueryStatus::Both),
            "\x1b_25a1;q;cp=100000;status=system,glossary\x1b\\"
        );
    }

    #[test]
    fn register_responses() {
        assert_eq!(
            format_register_ok(0x10_0000),
            "\x1b_25a1;r;cp=100000;status=0\x1b\\"
        );
        assert_eq!(
            format_register_error(0x61, RegisterError::OutOfNamespace),
            "\x1b_25a1;r;cp=61;status=1;reason=out_of_namespace\x1b\\"
        );
        assert_eq!(
            format_register_error(0x10_0000, RegisterError::CompositeUnsupported),
            "\x1b_25a1;r;cp=100000;status=1;reason=composite_unsupported\x1b\\"
        );
    }

    #[test]
    fn clear_responses() {
        assert_eq!(
            format_clear_ok(Some(0x10_0000)),
            "\x1b_25a1;c;cp=100000;status=0\x1b\\"
        );
        assert_eq!(format_clear_ok(None), "\x1b_25a1;c;status=0\x1b\\");
        assert_eq!(
            format_clear_error_out_of_namespace(0x61),
            "\x1b_25a1;c;cp=61;status=1;reason=out_of_namespace\x1b\\"
        );
    }

    #[test]
    fn unknown_params_are_ignored() {
        let got = parse(b"25a1;q;cp=E0A0;future=1").unwrap();
        assert_eq!(got, GlyphCommand::Query { cp: 0xE0A0 });
    }
}
