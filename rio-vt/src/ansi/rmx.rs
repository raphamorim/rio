//! rmx (Rio Multiplex Protocol) wire layer: APC frame parsing and reply
//! serialization. Spec: `specs/rio-multiplex-protocol.md` (v0.1 draft).
//!
//! This module owns only the grammar. Buffer semantics live with the
//! embedder: parsed commands surface through [`Handler::rmx_command`]
//! and terminal replies are formatted here with a fixed alphabet
//! (verb names, decimal integers, and validated buffer keys), never
//! reflecting application strings.
//!
//! [`Handler::rmx_command`]: crate::performer::handler::Handler::rmx_command

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

/// Every rmx APC body starts with this identifier (spec §3.1).
pub const RMX_PREFIX: &[u8] = b"rmx";

/// Highest protocol version this parser understands (spec §4).
pub const PROTOCOL_VERSION: u8 = 1;

/// Hard cap on one rmx frame, terminator excluded (spec §3.2).
pub const MAX_FRAME_BYTES: usize = 4096;

/// Hard cap on a raw burst declared by `t` (spec §7).
pub const MAX_BURST_BYTES: usize = 65536;

/// Reserved key naming the primary stream (spec §3.2).
pub const MAIN_KEY: &str = "main";

const MAX_KEY_LEN: usize = 16;

/// A validated buffer key: `[a-z0-9-]{1,16}` (spec §3.2). The
/// restricted alphabet is what makes keys safe to reflect in replies.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BufferKey(String);

impl BufferKey {
    pub fn parse(raw: &[u8]) -> Result<Self, ParseError> {
        if raw.is_empty() || raw.len() > MAX_KEY_LEN {
            return Err(ParseError::BadKey);
        }
        if !raw
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        {
            return Err(ParseError::BadKey);
        }
        // SAFETY of from_utf8: the alphabet above is pure ASCII.
        Ok(BufferKey(String::from_utf8(raw.to_vec()).unwrap()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_main(&self) -> bool {
        self.0 == MAIN_KEY
    }
}

/// Placement hint direction (spec §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Right,
    Down,
}

/// Close disposition (spec §10.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Disposition {
    #[default]
    Discard,
    Scrollback,
}

/// Reply mode, following the Glyph Protocol convention (spec §3.3):
/// `0` silent, `1` always, `2` errors only (default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReplyMode {
    Silent,
    Always,
    #[default]
    ErrorsOnly,
}

impl ReplyMode {
    pub fn emit_success(self) -> bool {
        matches!(self, ReplyMode::Always)
    }
    pub fn emit_error(self) -> bool {
        !matches!(self, ReplyMode::Silent)
    }
}

/// One parsed application→terminal rmx command (spec §3.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RmxCommand {
    /// `s`: support ping / negotiation (spec §4).
    Support,
    /// `o`: open or adopt a buffer (spec §5).
    Open {
        key: BufferKey,
        cols: Option<u16>,
        rows: Option<u16>,
        /// Decoded UTF-8 title, already length-bounded; sanitization
        /// against OSC-title rules is the embedder's job.
        title: Option<String>,
        at: Option<BufferKey>,
        dir: Option<Dir>,
        weight: Option<u8>,
        urgency: u8,
        focus: bool,
        reply: ReplyMode,
    },
    /// `w`: write decoded bytes to a buffer (spec §6).
    Write {
        key: BufferKey,
        payload: Vec<u8>,
        more: bool,
        reply: ReplyMode,
    },
    /// `t`: the next `len` raw stream bytes belong to `key` (spec §7).
    RawBurst { key: BufferKey, len: usize },
    /// `c`: close a buffer (spec §10.1).
    Close {
        key: BufferKey,
        disp: Disposition,
        reply: ReplyMode,
    },
    /// `q`: enumerate session state (spec §10.2).
    Enumerate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    NotRmx,
    FrameTooLong,
    UnknownVerb,
    MissingKey,
    BadKey,
    BadParam,
    BadPayload,
    BadLength,
}

/// Error names for `reason=` replies (fixed alphabet, spec §5.2/§6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    BadKey,
    Quota,
    Unsupported,
    NoSuchBuffer,
    NoCredit,
    Malformed,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::BadKey => "bad_key",
            Reason::Quota => "quota",
            Reason::Unsupported => "unsupported",
            Reason::NoSuchBuffer => "no_such_buffer",
            Reason::NoCredit => "no_credit",
            Reason::Malformed => "malformed",
        }
    }
}

/// What a terminal advertises in the `s` reply (spec §4). `core=false`
/// mirrors the Glyph Protocol's empty `fmt=`: the terminal recognizes
/// rmx but currently offers nothing; every `o` will be rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RmxCaps {
    pub core: bool,
    pub raw: bool,
    pub layout: bool,
    pub nest: bool,
    pub scrollback: bool,
    pub max_buffers: u16,
    pub initial_credit: u32,
}

fn split_params(body: &[u8]) -> impl Iterator<Item = &[u8]> {
    body.split(|b| *b == b';')
}

fn key_value(field: &[u8]) -> Option<(&[u8], &[u8])> {
    let eq = field.iter().position(|b| *b == b'=')?;
    Some((&field[..eq], &field[eq + 1..]))
}

fn parse_u32(v: &[u8]) -> Result<u32, ParseError> {
    if v.is_empty() || v.len() > 10 || !v.iter().all(u8::is_ascii_digit) {
        return Err(ParseError::BadParam);
    }
    std::str::from_utf8(v)
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or(ParseError::BadParam)
}

fn parse_reply(v: &[u8]) -> Result<ReplyMode, ParseError> {
    match v {
        b"0" => Ok(ReplyMode::Silent),
        b"1" => Ok(ReplyMode::Always),
        b"2" => Ok(ReplyMode::ErrorsOnly),
        _ => Err(ParseError::BadParam),
    }
}

/// Parse an rmx APC body (terminator already stripped by the APC
/// layer). Unknown keys are ignored per spec §3.2; unknown verbs are
/// an error so the embedder can log them.
pub fn parse(body: &[u8]) -> Result<RmxCommand, ParseError> {
    if !body.starts_with(RMX_PREFIX) {
        return Err(ParseError::NotRmx);
    }
    if body.len() > MAX_FRAME_BYTES {
        return Err(ParseError::FrameTooLong);
    }
    let rest = &body[RMX_PREFIX.len()..];
    let rest = rest.strip_prefix(b";").ok_or(ParseError::UnknownVerb)?;

    let mut fields = split_params(rest);
    let verb = fields.next().ok_or(ParseError::UnknownVerb)?;

    match verb {
        b"s" => Ok(RmxCommand::Support),
        b"q" => Ok(RmxCommand::Enumerate),
        b"o" => {
            let mut key = None;
            let mut cols = None;
            let mut rows = None;
            let mut title = None;
            let mut at = None;
            let mut dir = None;
            let mut weight = None;
            let mut urgency = 3u8;
            let mut focus = false;
            let mut reply = ReplyMode::default();
            for field in fields {
                let Some((k, v)) = key_value(field) else {
                    continue;
                };
                match k {
                    b"k" => key = Some(BufferKey::parse(v)?),
                    b"cols" => cols = Some(parse_u32(v)?.min(u16::MAX as u32) as u16),
                    b"rows" => rows = Some(parse_u32(v)?.min(u16::MAX as u32) as u16),
                    b"title" => {
                        let raw = BASE64.decode(v).map_err(|_| ParseError::BadPayload)?;
                        title = Some(
                            String::from_utf8(raw).map_err(|_| ParseError::BadPayload)?,
                        );
                    }
                    b"at" => at = Some(BufferKey::parse(v)?),
                    b"dir" => {
                        dir = Some(match v {
                            b"right" => Dir::Right,
                            b"down" => Dir::Down,
                            _ => return Err(ParseError::BadParam),
                        })
                    }
                    b"weight" => {
                        let w = parse_u32(v)?;
                        if !(1..=100).contains(&w) {
                            return Err(ParseError::BadParam);
                        }
                        weight = Some(w as u8);
                    }
                    b"u" => {
                        let u = parse_u32(v)?;
                        if u > 7 {
                            return Err(ParseError::BadParam);
                        }
                        urgency = u as u8;
                    }
                    b"focus" => focus = v == b"1",
                    b"reply" => reply = parse_reply(v)?,
                    _ => {}
                }
            }
            let key = key.ok_or(ParseError::MissingKey)?;
            if key.is_main() {
                return Err(ParseError::BadKey);
            }
            Ok(RmxCommand::Open {
                key,
                cols,
                rows,
                title,
                at,
                dir,
                weight,
                urgency,
                focus,
                reply,
            })
        }
        b"w" => {
            // The payload is always the final `;`-separated field
            // (spec §6.1); base64 padding makes it look like a
            // key=value pair, so it is taken positionally.
            let all: Vec<&[u8]> = fields.collect();
            let (payload_field, params) =
                all.split_last().ok_or(ParseError::BadPayload)?;
            let mut key = None;
            let mut more = false;
            let mut reply = ReplyMode::default();
            for field in params {
                let Some((k, v)) = key_value(field) else {
                    continue;
                };
                match k {
                    b"k" => key = Some(BufferKey::parse(v)?),
                    b"m" => more = v == b"1",
                    b"reply" => reply = parse_reply(v)?,
                    _ => {}
                }
            }
            let key = key.ok_or(ParseError::MissingKey)?;
            if key.is_main() {
                return Err(ParseError::BadKey);
            }
            let payload = BASE64
                .decode(payload_field)
                .map_err(|_| ParseError::BadPayload)?;
            Ok(RmxCommand::Write {
                key,
                payload,
                more,
                reply,
            })
        }
        b"t" => {
            let mut key = None;
            let mut len = None;
            for field in fields {
                let Some((k, v)) = key_value(field) else {
                    continue;
                };
                match k {
                    b"k" => key = Some(BufferKey::parse(v)?),
                    b"n" => len = Some(parse_u32(v)? as usize),
                    _ => {}
                }
            }
            let key = key.ok_or(ParseError::MissingKey)?;
            let len = len.ok_or(ParseError::BadLength)?;
            if key.is_main() {
                return Err(ParseError::BadKey);
            }
            if len == 0 || len > MAX_BURST_BYTES {
                return Err(ParseError::BadLength);
            }
            Ok(RmxCommand::RawBurst { key, len })
        }
        b"c" => {
            let mut key = None;
            let mut disp = Disposition::default();
            let mut reply = ReplyMode::default();
            for field in fields {
                let Some((k, v)) = key_value(field) else {
                    continue;
                };
                match k {
                    b"k" => key = Some(BufferKey::parse(v)?),
                    b"disp" => {
                        disp = match v {
                            b"discard" => Disposition::Discard,
                            b"scrollback" => Disposition::Scrollback,
                            _ => return Err(ParseError::BadParam),
                        }
                    }
                    b"reply" => reply = parse_reply(v)?,
                    _ => {}
                }
            }
            let key = key.ok_or(ParseError::MissingKey)?;
            if key.is_main() {
                return Err(ParseError::BadKey);
            }
            Ok(RmxCommand::Close { key, disp, reply })
        }
        _ => Err(ParseError::UnknownVerb),
    }
}

fn frame(body: &str) -> String {
    format!("\x1b_{body}\x1b\\")
}

/// `s` reply (spec §4).
pub fn format_support_reply(caps: &RmxCaps) -> String {
    let mut names: Vec<&str> = Vec::new();
    if caps.core {
        names.push("core");
    }
    if caps.raw {
        names.push("raw");
    }
    if caps.layout {
        names.push("layout");
    }
    if caps.nest {
        names.push("nest");
    }
    if caps.scrollback {
        names.push("scrollback");
    }
    frame(&format!(
        "rmx;s;v={};cap={};max={};credit={}",
        PROTOCOL_VERSION,
        names.join(","),
        caps.max_buffers,
        caps.initial_credit
    ))
}

/// Successful `o` reply (spec §5.2).
pub fn format_open_ok(key: &BufferKey, cols: u16, rows: u16, credit: u32) -> String {
    frame(&format!(
        "rmx;o;k={};status=0;cols={cols};rows={rows};credit={credit}",
        key.as_str()
    ))
}

/// Failure reply for any keyed verb (spec §5.2/§6.2).
pub fn format_error(verb: char, key: Option<&BufferKey>, reason: Reason) -> String {
    match key {
        Some(k) => frame(&format!(
            "rmx;{verb};k={};status=1;reason={}",
            k.as_str(),
            reason.as_str()
        )),
        None => frame(&format!("rmx;{verb};status=1;reason={}", reason.as_str())),
    }
}

/// Credit grant (spec §9).
pub fn format_credit(key: &BufferKey, bytes: u32) -> String {
    frame(&format!("rmx;a;k={};n={bytes}", key.as_str()))
}

/// `q` enumeration row and terminator (spec §10.2).
pub fn format_enumerate_row(
    key: &BufferKey,
    cols: u16,
    rows: u16,
    urgency: u8,
) -> String {
    frame(&format!(
        "rmx;q;k={};cols={cols};rows={rows};u={urgency};more=1",
        key.as_str()
    ))
}

pub fn format_enumerate_end() -> String {
    frame("rmx;q;more=0")
}

/// Input/event frames (spec §8).
pub fn format_event_in(key: &BufferKey, bytes: &[u8]) -> String {
    frame(&format!(
        "rmx;i;k={};ev=in;{}",
        key.as_str(),
        BASE64.encode(bytes)
    ))
}

pub fn format_event_reply(key: &BufferKey, bytes: &[u8]) -> String {
    frame(&format!(
        "rmx;i;k={};ev=reply;{}",
        key.as_str(),
        BASE64.encode(bytes)
    ))
}

pub fn format_event_resize(key: &BufferKey, cols: u16, rows: u16) -> String {
    frame(&format!(
        "rmx;i;k={};ev=resize;cols={cols};rows={rows}",
        key.as_str()
    ))
}

pub fn format_event_focus(key: &BufferKey, focused: bool) -> String {
    frame(&format!(
        "rmx;i;k={};ev=focus;state={}",
        key.as_str(),
        if focused { "in" } else { "out" }
    ))
}

pub fn format_event_closed(key: &BufferKey, reason: &str) -> String {
    frame(&format!(
        "rmx;i;k={};ev=closed;reason={reason}",
        key.as_str()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(s: &str) -> BufferKey {
        BufferKey::parse(s.as_bytes()).unwrap()
    }

    #[test]
    fn support_and_enumerate_parse() {
        assert_eq!(parse(b"rmx;s"), Ok(RmxCommand::Support));
        assert_eq!(parse(b"rmx;q"), Ok(RmxCommand::Enumerate));
    }

    #[test]
    fn non_rmx_rejected() {
        assert_eq!(parse(b"25a1;s"), Err(ParseError::NotRmx));
        assert_eq!(parse(b""), Err(ParseError::NotRmx));
        assert_eq!(parse(b"rmxx;s"), Err(ParseError::UnknownVerb));
    }

    #[test]
    fn open_full() {
        let cmd = parse(
            b"rmx;o;k=build-log;cols=80;rows=24;title=aGk=;at=main;dir=down;weight=30;u=1;focus=1",
        )
        .unwrap();
        assert_eq!(
            cmd,
            RmxCommand::Open {
                key: key("build-log"),
                cols: Some(80),
                rows: Some(24),
                title: Some("hi".into()),
                at: Some(key("main")),
                dir: Some(Dir::Down),
                weight: Some(30),
                urgency: 1,
                focus: true,
                reply: ReplyMode::ErrorsOnly,
            }
        );
    }

    #[test]
    fn open_defaults_and_unknown_keys_ignored() {
        let cmd = parse(b"rmx;o;k=a;zzz=1;future=x").unwrap();
        match cmd {
            RmxCommand::Open {
                key: k,
                urgency,
                focus,
                reply,
                ..
            } => {
                assert_eq!(k.as_str(), "a");
                assert_eq!(urgency, 3);
                assert!(!focus);
                assert_eq!(reply, ReplyMode::ErrorsOnly);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn key_alphabet_enforced() {
        assert_eq!(parse(b"rmx;o;k=Bad"), Err(ParseError::BadKey));
        assert_eq!(parse(b"rmx;o;k=a_b"), Err(ParseError::BadKey));
        assert_eq!(parse(b"rmx;o;k=aaaaaaaaaaaaaaaaa"), Err(ParseError::BadKey));
        assert_eq!(parse(b"rmx;o"), Err(ParseError::MissingKey));
        assert_eq!(parse(b"rmx;o;k=main"), Err(ParseError::BadKey));
    }

    #[test]
    fn write_payload_decodes() {
        let cmd = parse(b"rmx;w;k=a;m=1;aGVsbG8=").unwrap();
        assert_eq!(
            cmd,
            RmxCommand::Write {
                key: key("a"),
                payload: b"hello".to_vec(),
                more: true,
                reply: ReplyMode::ErrorsOnly,
            }
        );
        assert_eq!(parse(b"rmx;w;k=a;!!!"), Err(ParseError::BadPayload));
        assert_eq!(parse(b"rmx;w;k=a"), Err(ParseError::MissingKey));
    }

    #[test]
    fn payload_is_positional_not_name_based() {
        // Base64 padding makes the payload look like `key=value`; it is
        // taken positionally, and nothing may follow it (spec §3.2).
        let cmd = parse(b"rmx;w;k=a;aGk=").unwrap();
        assert_eq!(
            cmd,
            RmxCommand::Write {
                key: key("a"),
                payload: b"hi".to_vec(),
                more: false,
                reply: ReplyMode::ErrorsOnly,
            }
        );
        // A field after the payload would be swallowed as payload, which
        // is why emitters must never place one there.
        assert!(matches!(
            parse(b"rmx;w;k=a;aGk=;serial=7"),
            Err(ParseError::BadPayload)
        ));
    }

    #[test]
    fn burst_bounds() {
        assert_eq!(
            parse(b"rmx;t;k=a;n=4096"),
            Ok(RmxCommand::RawBurst {
                key: key("a"),
                len: 4096
            })
        );
        assert_eq!(parse(b"rmx;t;k=a;n=0"), Err(ParseError::BadLength));
        assert_eq!(parse(b"rmx;t;k=a;n=65537"), Err(ParseError::BadLength));
        assert_eq!(parse(b"rmx;t;k=a"), Err(ParseError::BadLength));
    }

    #[test]
    fn close_dispositions() {
        assert_eq!(
            parse(b"rmx;c;k=a;disp=scrollback"),
            Ok(RmxCommand::Close {
                key: key("a"),
                disp: Disposition::Scrollback,
                reply: ReplyMode::ErrorsOnly,
            })
        );
        assert_eq!(parse(b"rmx;c;k=a;disp=zap"), Err(ParseError::BadParam));
    }

    #[test]
    fn frame_cap() {
        let mut big = b"rmx;w;k=a;".to_vec();
        big.extend(std::iter::repeat(b'A').take(MAX_FRAME_BYTES));
        assert_eq!(parse(&big), Err(ParseError::FrameTooLong));
    }

    #[test]
    fn replies_are_fixed_alphabet() {
        let caps = RmxCaps {
            core: true,
            raw: false,
            layout: true,
            nest: false,
            scrollback: false,
            max_buffers: 16,
            initial_credit: 65536,
        };
        assert_eq!(
            format_support_reply(&caps),
            "\x1b_rmx;s;v=1;cap=core,layout;max=16;credit=65536\x1b\\"
        );
        assert_eq!(
            format_support_reply(&RmxCaps::default()),
            "\x1b_rmx;s;v=1;cap=;max=0;credit=0\x1b\\"
        );
        assert_eq!(
            format_error('o', Some(&key("a")), Reason::Quota),
            "\x1b_rmx;o;k=a;status=1;reason=quota\x1b\\"
        );
        assert_eq!(
            format_event_resize(&key("a"), 100, 40),
            "\x1b_rmx;i;k=a;ev=resize;cols=100;rows=40\x1b\\"
        );
    }

    #[test]
    fn every_capability_is_named_in_order() {
        let caps = RmxCaps {
            core: true,
            raw: true,
            layout: true,
            nest: true,
            scrollback: true,
            max_buffers: 16,
            initial_credit: 262_144,
        };
        assert_eq!(
            format_support_reply(&caps),
            "\x1b_rmx;s;v=1;cap=core,raw,layout,nest,scrollback;max=16;credit=262144\x1b\\"
        );
    }

    #[test]
    fn focus_events_name_the_crossed_edge() {
        assert_eq!(
            format_event_focus(&key("build-log"), true),
            "\x1b_rmx;i;k=build-log;ev=focus;state=in\x1b\\"
        );
        assert_eq!(
            format_event_focus(&key("build-log"), false),
            "\x1b_rmx;i;k=build-log;ev=focus;state=out\x1b\\"
        );
    }

    #[test]
    fn placement_hints_are_bounded() {
        // `weight` is a percentage; anything outside 1..=100 is a
        // malformed frame, not a clamp (spec §5.1).
        assert_eq!(parse(b"rmx;o;k=a;weight=0"), Err(ParseError::BadParam));
        assert_eq!(parse(b"rmx;o;k=a;weight=101"), Err(ParseError::BadParam));
        assert!(matches!(
            parse(b"rmx;o;k=a;weight=100"),
            Ok(RmxCommand::Open {
                weight: Some(100),
                ..
            })
        ));
        // `at` obeys the key alphabet, and `main` is a legal anchor even
        // though it is never a legal target.
        assert_eq!(parse(b"rmx;o;k=a;at=BAD"), Err(ParseError::BadKey));
        assert!(matches!(
            parse(b"rmx;o;k=a;at=main;dir=right"),
            Ok(RmxCommand::Open {
                at: Some(_),
                dir: Some(Dir::Right),
                ..
            })
        ));
    }

    #[test]
    fn a_nested_frame_survives_the_reply_envelope() {
        // With `cap=nest` a sub-buffer's events reach the application
        // wrapped in the owning buffer's `ev=reply` payload, once per
        // level. The envelope must be transparent.
        let inner = format_event_focus(&key("sub"), true);
        let outer = format_event_reply(&key("outer"), inner.as_bytes());
        assert_eq!(
            outer,
            "\x1b_rmx;i;k=outer;ev=reply;G19ybXg7aTtrPXN1Yjtldj1mb2N1cztzdGF0ZT1pbhtc\x1b\\"
        );
        let payload = outer
            .strip_prefix("\x1b_rmx;i;k=outer;ev=reply;")
            .and_then(|rest| rest.strip_suffix("\x1b\\"))
            .unwrap();
        assert_eq!(BASE64.decode(payload).unwrap(), inner.as_bytes());
    }
}
