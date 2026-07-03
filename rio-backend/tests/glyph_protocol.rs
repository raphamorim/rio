//! Glyph Protocol integration tests.
//!
//! These tests assert the engine's Glyph Protocol register/clear behaviour
//! through a real host that owns a `sugarloaf` `GlyphRegistry`. They live in
//! `rio-backend` (not the `canario` engine crate) because they depend on
//! `sugarloaf` for the host-side `glyf_decode` validation and registry —
//! `canario` has no `sugarloaf` dependency. The fixture below (`GlyphTestHost`)
//! reproduces exactly what the engine used to do inline before the Glyph
//! Protocol render-side glossary was severed out into the host, so the
//! behaviour contract these tests pin is identical to the pre-severance engine.

use canario::host::TerminalHost;
use rio_backend::ansi::CursorShape;
use rio_backend::crosswords::{Crosswords, CrosswordsSize};
use rio_backend::performer::handler::Handler;

/// Glyph severance moved the render-side glossary out of the engine and
/// into the host (the embedder owns the `sugarloaf` font types). This
/// test host reproduces exactly what the engine used to do inline:
/// validate the `glyf` payload with `glyf_decode`, then store it in a
/// real [`GlyphRegistry`]. The shared `registry` handle lets the tests
/// assert on registry state, preserving the original behaviour contract.
#[derive(Clone, Default)]
struct GlyphTestHost {
    registry: sugarloaf::font::glyph_registry::GlyphRegistry,
    /// Mirrors the engine's old lazy-init: `false` until the first
    /// successful register, so `registry_is_initialised` stays
    /// observably the same as the old `glyph_registry.is_some()`.
    initialised: std::rc::Rc<std::cell::Cell<bool>>,
}

impl TerminalHost for GlyphTestHost {
    type WindowId = rio_backend::event::WindowId;

    fn glyph_register(
        &mut self,
        _id: Self::WindowId,
        cp: u32,
        payload: rio_backend::ansi::glyph_protocol::GlyphPayload,
    ) -> Result<(), rio_backend::ansi::glyph_protocol::RegisterError> {
        use rio_backend::ansi::glyph_protocol::{GlyphPayload, RegisterError};
        use sugarloaf::font::glyf_decode;
        use sugarloaf::font::glyph_registry::{RegisterRejection, StoredPayload};

        fn translate(err: glyf_decode::DecodeError) -> RegisterError {
            match err {
                glyf_decode::DecodeError::Composite => {
                    RegisterError::CompositeUnsupported
                }
                glyf_decode::DecodeError::Hinted => {
                    RegisterError::HintingUnsupported
                }
                glyf_decode::DecodeError::Malformed => {
                    RegisterError::MalformedPayload
                }
            }
        }

        let (stored, upm) = match payload {
            GlyphPayload::Glyf { glyf, upm } => {
                glyf_decode::decode(&glyf).map_err(translate)?;
                (StoredPayload::Glyf { glyf }, upm)
            }
            GlyphPayload::ColrV0 { container, upm } => (
                StoredPayload::ColrV0 {
                    glyphs: container.glyphs,
                    colr: container.colr,
                    cpal: container.cpal,
                },
                upm,
            ),
            GlyphPayload::ColrV1 { container, upm } => (
                StoredPayload::ColrV1 {
                    glyphs: container.glyphs,
                    colr: container.colr,
                    cpal: container.cpal,
                },
                upm,
            ),
        };

        match self.registry.register(cp, stored, upm) {
            Ok(_evicted) => {
                self.initialised.set(true);
                Ok(())
            }
            Err(RegisterRejection::OutOfNamespace) => {
                Err(RegisterError::OutOfNamespace)
            }
        }
    }

    fn glyph_clear(&mut self, _id: Self::WindowId, cp: Option<u32>) {
        match cp {
            None => self.registry.clear_all(),
            Some(cp) => self.registry.clear_one(cp),
        }
    }
}

fn make_crosswords() -> Crosswords<GlyphTestHost> {
    let size = CrosswordsSize::new(4, 4);
    let window_id = rio_backend::event::WindowId::from(0);
    Crosswords::new(
        size,
        CursorShape::Block,
        GlyphTestHost::default(),
        window_id,
        0,
        10,
    )
}

// Minimum-valid simple glyph: one contour, one on-curve point.
fn minimal_glyf_bytes() -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&1i16.to_be_bytes()); // numberOfContours
    v.extend_from_slice(&[0u8; 8]); // bounding box
    v.extend_from_slice(&0u16.to_be_bytes()); // endPtsOfContours[0]
    v.extend_from_slice(&0u16.to_be_bytes()); // instructionLength
    v.push(0x01); // flags: on-curve, no shorts
    v.extend_from_slice(&0i16.to_be_bytes()); // x delta
    v.extend_from_slice(&0i16.to_be_bytes()); // y delta
    v
}

fn glyf_payload(
    bytes: Vec<u8>,
    upm: u16,
) -> rio_backend::ansi::glyph_protocol::GlyphPayload {
    rio_backend::ansi::glyph_protocol::GlyphPayload::Glyf { glyf: bytes, upm }
}

fn registry_initialised(cw: &Crosswords<GlyphTestHost>) -> bool {
    cw.host().initialised.get()
}

fn registry_contains(cw: &Crosswords<GlyphTestHost>, cp: u32) -> bool {
    cw.host().registry.contains(cp)
}

fn registry_len(cw: &Crosswords<GlyphTestHost>) -> usize {
    cw.host().registry.len()
}

#[test]
fn glyph_registry_is_none_until_first_register() {
    let cw = make_crosswords();
    assert!(!registry_initialised(&cw));
}

#[test]
fn glyph_protocol_register_populates_registry() {
    let mut cw = make_crosswords();

    let glyf = minimal_glyf_bytes();
    // E0A0 is the Powerline branch codepoint — in basic PUA.
    let res = Handler::glyph_register(&mut cw, 0xE0A0, glyf_payload(glyf, 1000));
    assert!(res.is_ok());
    assert!(registry_initialised(&cw));
    assert!(registry_contains(&cw, 0xE0A0));
}

#[test]
fn glyph_protocol_register_rejects_non_pua() {
    use rio_backend::ansi::glyph_protocol::RegisterError;
    let mut cw = make_crosswords();

    // 0x61 is 'a' — not in PUA. Registry must refuse.
    let res = Handler::glyph_register(
        &mut cw,
        0x61,
        glyf_payload(minimal_glyf_bytes(), 1000),
    );
    assert_eq!(res, Err(RegisterError::OutOfNamespace));
    assert!(!registry_initialised(&cw));
}

#[test]
fn glyph_protocol_register_rejects_hinted_payload() {
    use rio_backend::ansi::glyph_protocol::RegisterError;
    let mut cw = make_crosswords();

    let mut v = Vec::new();
    v.extend_from_slice(&1i16.to_be_bytes());
    v.extend_from_slice(&[0u8; 8]);
    v.extend_from_slice(&0u16.to_be_bytes()); // endPts[0]
    v.extend_from_slice(&1u16.to_be_bytes()); // instructionLength = 1
    v.push(0x00); // the instruction
    v.push(0x01); // on-curve flag
    v.extend_from_slice(&0i16.to_be_bytes());
    v.extend_from_slice(&0i16.to_be_bytes());

    let res = Handler::glyph_register(&mut cw, 0xE0A0, glyf_payload(v, 1000));
    assert_eq!(res, Err(RegisterError::HintingUnsupported));
    // Decode failed before the registry was touched, so it stays
    // uninitialised.
    assert!(!registry_initialised(&cw));
}

#[test]
fn glyph_protocol_clear_before_any_register_is_noop() {
    let mut cw = make_crosswords();
    Handler::glyph_clear(&mut cw, None);
    // No panic, registry still absent.
    assert!(!registry_initialised(&cw));
}

#[test]
fn glyph_protocol_clear_all_wipes_registry() {
    let mut cw = make_crosswords();
    Handler::glyph_register(
        &mut cw,
        0xE0A0,
        glyf_payload(minimal_glyf_bytes(), 1000),
    )
    .unwrap();
    Handler::glyph_register(
        &mut cw,
        0xE0A1,
        glyf_payload(minimal_glyf_bytes(), 1000),
    )
    .unwrap();
    assert_eq!(registry_len(&cw), 2);

    Handler::glyph_clear(&mut cw, None);
    // Clear-all empties the registry but leaves the Arc in place,
    // since a program that cleared once will likely register again.
    assert_eq!(registry_len(&cw), 0);
    assert!(registry_initialised(&cw));
}

#[test]
fn glyph_protocol_clear_one_leaves_others_intact() {
    let mut cw = make_crosswords();
    Handler::glyph_register(
        &mut cw,
        0xE0A0,
        glyf_payload(minimal_glyf_bytes(), 1000),
    )
    .unwrap();
    Handler::glyph_register(
        &mut cw,
        0xE0A1,
        glyf_payload(minimal_glyf_bytes(), 1000),
    )
    .unwrap();

    Handler::glyph_clear(&mut cw, Some(0xE0A0));
    assert!(!registry_contains(&cw, 0xE0A0));
    assert!(registry_contains(&cw, 0xE0A1));
}
