//! The forked VT500-series state machine for the canario terminal engine.
//!
//! Renderer-agnostic: it decodes a byte stream into parser events
//! (print runs, CSI/OSC/DCS/ESC dispatches) and assigns no terminal
//! semantics — that's the job of the [`Perform`] implementer.
//!
//! [`Parser`] implements [Paul Williams' ANSI parser state machine]. The state
//! machine doesn't assign meaning to the parsed data — that's the job of the
//! [`Perform`] implementer.
//!
//! Forked from Alacritty's VTE; previously the standalone `copa` crate. The
//! [`Perform`] trait keeps a single dispatch shape so the same state machine
//! drives both the production `Performer` and unit-test dispatchers.
//!
//! [Paul Williams' ANSI parser state machine]: https://vt100.net/emu/dec_ansi_parser

#![deny(clippy::all, clippy::if_not_else, clippy::enum_glob_use)]

use std::str;

mod params;
pub mod simd_utf8;

pub use params::{Params, ParamsIter};

const MAX_INTERMEDIATES: usize = 4;
const MAX_OSC_PARAMS: usize = 16;

/// Inline OSC byte capacity. Sized to absorb common OSCs (titles, color
/// queries, hyperlink URLs, kitty graphics control headers) without
/// allocation. Larger payloads (e.g. OSC 52 clipboard pastes) spill into
/// `OscBuffer::overflow`.
const OSC_FIXED_LEN: usize = 2048;

/// Parser for raw _VTE_ protocol which delegates actions to a [`Perform`].
#[derive(Default)]
pub struct Parser {
    state: State,
    intermediates: [u8; MAX_INTERMEDIATES],
    intermediate_idx: usize,
    params: Params,
    param: u16,
    osc_raw: OscBuffer,
    osc_params: [(usize, usize); MAX_OSC_PARAMS],
    osc_num_params: usize,
    ignoring: bool,
    partial_utf8: [u8; 4],
    partial_utf8_len: usize,
    /// Reused output buffer for [`simdutf::convert_utf8_to_utf32`]. Grows
    /// to fit the largest non-ASCII ground-state chunk seen so far; empty
    /// in steady state when sessions are pure ASCII.
    decode_buf: Vec<u32>,
}

/// OSC accumulator with a fixed-size inline buffer and a heap fallback.
///
/// The first `OSC_FIXED_LEN` bytes of any OSC sequence land in `fixed`
/// (zero allocation). On overflow, the populated prefix of `fixed` is copied
/// into `overflow` once and all subsequent writes go to the `Vec` only — so
/// at any moment a single backing slice holds the contiguous payload.
struct OscBuffer {
    fixed: [u8; OSC_FIXED_LEN],
    fixed_len: usize,
    overflow: Vec<u8>,
}

impl Default for OscBuffer {
    fn default() -> Self {
        Self {
            fixed: [0; OSC_FIXED_LEN],
            fixed_len: 0,
            overflow: Vec::new(),
        }
    }
}

impl OscBuffer {
    #[inline]
    fn len(&self) -> usize {
        if self.overflow.is_empty() {
            self.fixed_len
        } else {
            self.overflow.len()
        }
    }

    #[inline]
    fn push(&mut self, byte: u8) {
        if self.overflow.is_empty() {
            if self.fixed_len < OSC_FIXED_LEN {
                self.fixed[self.fixed_len] = byte;
                self.fixed_len += 1;
                return;
            }
            // Spill: promote the current contents to the heap once, then
            // append. After this point, `overflow.len() >= OSC_FIXED_LEN`,
            // so the `is_empty()` check above stays false until `clear`.
            self.overflow
                .extend_from_slice(&self.fixed[..self.fixed_len]);
        }
        self.overflow.push(byte);
    }

    #[inline]
    fn slice(&self, start: usize, end: usize) -> &[u8] {
        if self.overflow.is_empty() {
            &self.fixed[start..end]
        } else {
            &self.overflow[start..end]
        }
    }

    #[inline]
    fn clear(&mut self) {
        self.fixed_len = 0;
        // Keep `overflow`'s capacity so a session that hits one large paste
        // doesn't re-allocate on the next one.
        self.overflow.clear();
    }
}

impl Parser {
    #[inline]
    fn params(&self) -> &Params {
        &self.params
    }

    #[inline]
    fn intermediates(&self) -> &[u8] {
        &self.intermediates[..self.intermediate_idx]
    }

    /// Advance the parser state.
    ///
    /// Requires a [`Perform`] implementation to handle the triggered actions.
    #[inline]
    pub fn advance<P: Perform>(&mut self, performer: &mut P, bytes: &[u8]) {
        let mut i = 0;

        // Handle partial codepoints from previous calls to `advance`.
        if self.partial_utf8_len != 0 {
            i += self.advance_partial_utf8(performer, bytes);
        }

        while i != bytes.len() {
            match self.state {
                State::Ground => i += self.advance_ground(performer, &bytes[i..]),
                _ => {
                    // Inlining it results in worse codegen.
                    let byte = bytes[i];
                    self.change_state(performer, byte);
                    i += 1;
                }
            }
        }
    }

    #[inline(always)]
    fn change_state<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match self.state {
            State::CsiEntry => self.advance_csi_entry(performer, byte),
            State::CsiIgnore => self.advance_csi_ignore(performer, byte),
            State::CsiIntermediate => self.advance_csi_intermediate(performer, byte),
            State::CsiParam => self.advance_csi_param(performer, byte),
            State::DcsEntry => self.advance_dcs_entry(performer, byte),
            State::DcsIgnore => self.anywhere(performer, byte),
            State::DcsIntermediate => self.advance_dcs_intermediate(performer, byte),
            State::DcsParam => self.advance_dcs_param(performer, byte),
            State::DcsPassthrough => self.advance_dcs_passthrough(performer, byte),
            State::Escape => self.advance_esc(performer, byte),
            State::EscapeIntermediate => self.advance_esc_intermediate(performer, byte),
            State::OscString => self.advance_osc_string(performer, byte),
            State::SosString => self.advance_sos_string(performer, byte),
            State::ApcString => self.advance_apc_string(performer, byte),
            State::PmString => self.advance_pm_string(performer, byte),
            State::Ground => unreachable!(),
        }
    }

    #[inline(always)]
    fn advance_csi_entry<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => performer.execute(byte),
            0x20..=0x2F => {
                self.action_collect(byte);
                self.state = State::CsiIntermediate
            }
            0x30..=0x39 => {
                self.action_paramnext(byte);
                self.state = State::CsiParam
            }
            0x3A => {
                self.action_subparam();
                self.state = State::CsiParam
            }
            0x3B => {
                self.action_param();
                self.state = State::CsiParam
            }
            0x3C..=0x3F => {
                self.action_collect(byte);
                self.state = State::CsiParam
            }
            0x40..=0x7E => self.action_csi_dispatch(performer, byte),
            _ => self.anywhere(performer, byte),
        }
    }

    #[inline(always)]
    fn advance_csi_ignore<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => performer.execute(byte),
            0x20..=0x3F => (),
            0x40..=0x7E => self.state = State::Ground,
            0x7F => (),
            _ => self.anywhere(performer, byte),
        }
    }

    #[inline(always)]
    fn advance_csi_intermediate<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => performer.execute(byte),
            0x20..=0x2F => self.action_collect(byte),
            0x30..=0x3F => self.state = State::CsiIgnore,
            0x40..=0x7E => self.action_csi_dispatch(performer, byte),
            _ => self.anywhere(performer, byte),
        }
    }

    #[inline(always)]
    fn advance_csi_param<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => performer.execute(byte),
            0x20..=0x2F => {
                self.action_collect(byte);
                self.state = State::CsiIntermediate
            }
            0x30..=0x39 => self.action_paramnext(byte),
            0x3A => self.action_subparam(),
            0x3B => self.action_param(),
            0x3C..=0x3F => self.state = State::CsiIgnore,
            0x40..=0x7E => self.action_csi_dispatch(performer, byte),
            0x7F => (),
            _ => self.anywhere(performer, byte),
        }
    }

    #[inline(always)]
    fn advance_dcs_entry<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => (),
            0x20..=0x2F => {
                self.action_collect(byte);
                self.state = State::DcsIntermediate
            }
            0x30..=0x39 => {
                self.action_paramnext(byte);
                self.state = State::DcsParam
            }
            0x3A => {
                self.action_subparam();
                self.state = State::DcsParam
            }
            0x3B => {
                self.action_param();
                self.state = State::DcsParam
            }
            0x3C..=0x3F => {
                self.action_collect(byte);
                self.state = State::DcsParam
            }
            0x40..=0x7E => self.action_hook(performer, byte),
            0x7F => (),
            _ => self.anywhere(performer, byte),
        }
    }

    #[inline(always)]
    fn advance_dcs_intermediate<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => (),
            0x20..=0x2F => self.action_collect(byte),
            0x30..=0x3F => self.state = State::DcsIgnore,
            0x40..=0x7E => self.action_hook(performer, byte),
            0x7F => (),
            _ => self.anywhere(performer, byte),
        }
    }

    #[inline(always)]
    fn advance_dcs_param<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => (),
            0x20..=0x2F => {
                self.action_collect(byte);
                self.state = State::DcsIntermediate
            }
            0x30..=0x39 => self.action_paramnext(byte),
            0x3A => self.action_subparam(),
            0x3B => self.action_param(),
            0x3C..=0x3F => self.state = State::DcsIgnore,
            0x40..=0x7E => self.action_hook(performer, byte),
            0x7F => (),
            _ => self.anywhere(performer, byte),
        }
    }

    #[inline(always)]
    fn advance_dcs_passthrough<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x7E => performer.put(byte),
            0x18 | 0x1A => {
                performer.unhook();
                performer.execute(byte);
                self.state = State::Ground
            }
            0x1B => {
                performer.unhook();
                self.reset_params();
                self.state = State::Escape
            }
            0x7F => (),
            0x9C => {
                performer.unhook();
                self.state = State::Ground
            }
            _ => (),
        }
    }

    #[inline(always)]
    fn advance_esc<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => performer.execute(byte),
            0x20..=0x2F => {
                self.action_collect(byte);
                self.state = State::EscapeIntermediate
            }
            0x30..=0x4F => {
                performer.esc_dispatch(self.intermediates(), self.ignoring, byte);
                self.state = State::Ground
            }
            0x50 => {
                self.reset_params();
                self.state = State::DcsEntry
            }
            0x51..=0x57 => {
                performer.esc_dispatch(self.intermediates(), self.ignoring, byte);
                self.state = State::Ground
            }
            0x58 => {
                performer.sos_start();
                self.state = State::SosString
            }
            0x59..=0x5A => {
                performer.esc_dispatch(self.intermediates(), self.ignoring, byte);
                self.state = State::Ground
            }
            0x5B => {
                self.reset_params();
                self.state = State::CsiEntry
            }
            0x5C => {
                performer.esc_dispatch(self.intermediates(), self.ignoring, byte);
                self.state = State::Ground
            }
            0x5D => {
                // `osc_end` already clears state at the end of every OSC, so the
                // buffer is guaranteed empty when re-entering OSC state.
                self.state = State::OscString
            }
            0x5E => {
                performer.pm_start();
                self.state = State::PmString
            }
            0x5F => {
                performer.apc_start();
                self.state = State::ApcString
            }
            0x60..=0x7E => {
                performer.esc_dispatch(self.intermediates(), self.ignoring, byte);
                self.state = State::Ground
            }
            // Anywhere.
            0x18 | 0x1A => {
                performer.execute(byte);
                self.state = State::Ground
            }
            0x1B => (),
            _ => (),
        }
    }

    #[inline(always)]
    fn advance_esc_intermediate<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => performer.execute(byte),
            0x20..=0x2F => self.action_collect(byte),
            0x30..=0x7E => {
                performer.esc_dispatch(self.intermediates(), self.ignoring, byte);
                self.state = State::Ground
            }
            0x7F => (),
            _ => self.anywhere(performer, byte),
        }
    }

    #[inline(always)]
    fn advance_osc_string<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x06 | 0x08..=0x17 | 0x19 | 0x1C..=0x1F => (),
            0x07 => {
                self.osc_end(performer, byte);
                self.state = State::Ground
            }
            0x18 | 0x1A => {
                self.osc_end(performer, byte);
                performer.execute(byte);
                self.state = State::Ground
            }
            0x1B => {
                self.osc_end(performer, byte);
                self.reset_params();
                self.state = State::Escape
            }
            0x3B => self.action_osc_put_param(),
            _ => self.action_osc_put(byte),
        }
    }

    #[inline(always)]
    fn advance_apc_string<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        // Bytes stream straight through to `performer.apc_put`; the Performer
        // owns its own accumulation buffer (`apc_state.buffer`) and parses
        // kitty-style headers from there. The parser keeps no APC state.
        match byte {
            0x00..=0x06 | 0x08..=0x17 | 0x19 | 0x1C..=0x1F => (), // Ignore control bytes
            0x07 => {
                // Bell-terminated APC.
                performer.apc_end();
                self.state = State::Ground;
            }
            0x18 | 0x1A => {
                // C0 termination (CAN or SUB).
                performer.apc_put(byte);
                performer.apc_end();
                performer.execute(byte);
                self.state = State::Ground;
            }
            0x1B => {
                // Start of ST termination (`\x1b\`).
                performer.apc_end();
                self.state = State::Escape;
            }
            0x20..=0xFF => performer.apc_put(byte),
        }
    }

    #[inline(always)]
    fn advance_sos_string<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x07 => {
                performer.sos_end();
                self.state = State::Ground
            }
            0x18 | 0x1A => {
                performer.sos_end();
                performer.execute(byte);
                self.state = State::Ground
            }
            0x1B => {
                performer.sos_end();
                self.state = State::Escape
            }
            0x20..=0xFF => performer.sos_put(byte),
            // Ignore all other control bytes.
            _ => (),
        }
    }

    #[inline(always)]
    fn advance_pm_string<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x07 => {
                performer.pm_end();
                self.state = State::Ground
            }
            0x18 | 0x1A => {
                performer.pm_end();
                performer.execute(byte);
                self.state = State::Ground
            }
            0x1B => {
                performer.pm_end();
                self.state = State::Escape
            }
            0x20..=0xFF => performer.pm_put(byte),
            // Ignore all other control bytes.
            _ => (),
        }
    }

    #[inline(always)]
    fn anywhere<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x18 | 0x1A => {
                performer.execute(byte);
                self.state = State::Ground
            }
            0x1B => {
                self.reset_params();
                self.state = State::Escape
            }
            _ => (),
        }
    }

    #[inline]
    fn action_csi_dispatch<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        if self.params.is_full() {
            self.ignoring = true;
        } else {
            self.params.push(self.param);
        }
        performer.csi_dispatch(
            self.params(),
            self.intermediates(),
            self.ignoring,
            byte as char,
        );

        self.state = State::Ground
    }

    #[inline]
    fn action_hook<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        if self.params.is_full() {
            self.ignoring = true;
        } else {
            self.params.push(self.param);
        }
        performer.hook(
            self.params(),
            self.intermediates(),
            self.ignoring,
            byte as char,
        );
        self.state = State::DcsPassthrough;
    }

    #[inline]
    fn action_collect(&mut self, byte: u8) {
        if self.intermediate_idx == MAX_INTERMEDIATES {
            self.ignoring = true;
        } else {
            self.intermediates[self.intermediate_idx] = byte;
            self.intermediate_idx += 1;
        }
    }

    /// Advance to the next subparameter.
    #[inline]
    fn action_subparam(&mut self) {
        if self.params.is_full() {
            self.ignoring = true;
        } else {
            self.params.extend(self.param);
            self.param = 0;
        }
    }

    /// Advance to the next parameter.
    #[inline]
    fn action_param(&mut self) {
        if self.params.is_full() {
            self.ignoring = true;
        } else {
            self.params.push(self.param);
            self.param = 0;
        }
    }

    /// Advance inside the parameter without terminating it.
    #[inline]
    fn action_paramnext(&mut self, byte: u8) {
        if self.params.is_full() {
            self.ignoring = true;
        } else {
            // Continue collecting bytes into param.
            self.param = self.param.saturating_mul(10);
            self.param = self.param.saturating_add((byte - b'0') as u16);
        }
    }

    /// Add OSC param separator.
    #[inline]
    fn action_osc_put_param(&mut self) {
        let idx = self.osc_raw.len();

        let param_idx = self.osc_num_params;
        match param_idx {
            // First param is special - 0 to current byte index.
            0 => self.osc_params[param_idx] = (0, idx),

            // Only process up to MAX_OSC_PARAMS.
            MAX_OSC_PARAMS => return,

            // All other params depend on previous indexing.
            _ => {
                let prev = self.osc_params[param_idx - 1];
                let begin = prev.1;
                self.osc_params[param_idx] = (begin, idx);
            }
        }

        self.osc_num_params += 1;
    }

    #[inline(always)]
    fn action_osc_put(&mut self, byte: u8) {
        self.osc_raw.push(byte);
    }

    fn osc_end<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        self.action_osc_put_param();
        self.osc_dispatch(performer, byte);
        self.osc_raw.clear();
        self.osc_num_params = 0;
    }

    /// Reset escape sequence parameters and intermediates.
    #[inline]
    fn reset_params(&mut self) {
        self.intermediate_idx = 0;
        self.ignoring = false;
        self.param = 0;

        self.params.clear();
    }

    /// Separate method for osc_dispatch that borrows self as read-only.
    ///
    /// The aliasing is needed here for multiple slices into self.osc_raw.
    #[inline]
    fn osc_dispatch<P: Perform>(&self, performer: &mut P, byte: u8) {
        let mut slices: [&[u8]; MAX_OSC_PARAMS] = [&[]; MAX_OSC_PARAMS];
        for (slice, &(start, end)) in slices
            .iter_mut()
            .zip(self.osc_params.iter())
            .take(self.osc_num_params)
        {
            *slice = self.osc_raw.slice(start, end);
        }
        performer.osc_dispatch(&slices[..self.osc_num_params], byte == 0x07);
    }

    /// Advance the parser state from ground.
    ///
    /// Single-pass shape: `memchr` finds the next ESC, then
    /// [`Self::ground_dispatch`] walks the prefix, decoding non-ASCII runs
    /// through `simdutf` with inline Maximal-Subpart U+FFFD replacement for
    /// invalid UTF-8. The separate `validate` pass the previous shape ran
    /// is gone — validation is fused into the decode call.
    #[inline]
    fn advance_ground<P: Perform>(&mut self, performer: &mut P, bytes: &[u8]) -> usize {
        // Find the next escape character.
        let num_bytes = bytes.len();
        let plain_chars = memchr::memchr(0x1B, bytes).unwrap_or(num_bytes);

        // If the next character is ESC, just process it and short-circuit.
        if plain_chars == 0 {
            self.state = State::Escape;
            self.reset_params();
            return 1;
        }

        let has_trailing_esc = plain_chars < num_bytes;
        let prefix = &bytes[..plain_chars];

        // No ESC follows: peel any valid-so-far partial UTF-8 sequence off
        // the tail and stash it for the next call. With a trailing ESC, any
        // partial bytes before the ESC are pre-ESC garbage and get U+FFFD-
        // replaced by the MSP-aware decode loop instead.
        let process_len = if has_trailing_esc {
            plain_chars
        } else {
            trim_valid_partial_utf8(prefix)
        };

        self.ground_dispatch(performer, &prefix[..process_len]);

        if process_len < plain_chars {
            // Stash truncated bytes for the next call.
            let partial = &prefix[process_len..];
            let dst_start = self.partial_utf8_len;
            let dst_end = dst_start + partial.len();
            self.partial_utf8[dst_start..dst_end].copy_from_slice(partial);
            self.partial_utf8_len = dst_end;
            return num_bytes;
        }

        if has_trailing_esc {
            self.state = State::Escape;
            self.reset_params();
            plain_chars + 1
        } else {
            num_bytes
        }
    }

    /// Advance the parser while processing a partial utf8 codepoint.
    #[inline]
    fn advance_partial_utf8<P: Perform>(
        &mut self,
        performer: &mut P,
        bytes: &[u8],
    ) -> usize {
        // Try to copy up to 3 more characters, to ensure the codepoint is complete.
        let old_bytes = self.partial_utf8_len;
        let to_copy = bytes.len().min(self.partial_utf8.len() - old_bytes);
        self.partial_utf8[old_bytes..old_bytes + to_copy]
            .copy_from_slice(&bytes[..to_copy]);
        self.partial_utf8_len += to_copy;

        // Parse the unicode character.
        match crate::simd_utf8::validate(&self.partial_utf8[..self.partial_utf8_len]) {
            // If the entire buffer is valid, use the first character and continue parsing.
            Ok(parsed) => {
                // SAFETY: `partial_utf8_len >= 1` (caller guarantee) and `parsed`
                // is the validated UTF-8 view of those bytes, so it has at least
                // one character.
                let c = unsafe { parsed.chars().next().unwrap_unchecked() };
                performer.print(c);

                self.partial_utf8_len = 0;
                c.len_utf8() - old_bytes
            }
            Err(err) => {
                let valid_bytes = err.valid_up_to();

                // If we have any valid bytes, that means we partially copied another
                // utf8 character into `partial_utf8`. Since we only care about the
                // first character, we just ignore the rest.
                if valid_bytes > 0 {
                    // SAFETY: `valid_bytes > 0` and the slice up to `valid_bytes` was
                    // reported as valid UTF-8, so it contains at least one full
                    // character.
                    let c = unsafe {
                        let parsed =
                            str::from_utf8_unchecked(&self.partial_utf8[..valid_bytes]);
                        parsed.chars().next().unwrap_unchecked()
                    };

                    performer.print(c);

                    self.partial_utf8_len = 0;
                    return valid_bytes - old_bytes;
                }

                match err.error_len() {
                    // If the partial character was also invalid, emit the replacement
                    // character.
                    Some(invalid_len) => {
                        performer.print('�');

                        self.partial_utf8_len = 0;
                        invalid_len - old_bytes
                    }
                    // If the character still isn't complete, wait for more data.
                    None => to_copy,
                }
            }
        }
    }

    /// Handle ground dispatch of print/execute for a (possibly invalid)
    /// UTF-8 byte slice.
    ///
    /// Three batched paths:
    /// - ASCII printable runs (`0x20..=0x7E`) → one [`Perform::print_str`].
    /// - ASCII control bytes (`0x00..=0x1F`, `0x7F`) → [`Perform::execute`].
    /// - Multi-byte UTF-8 runs (any byte ≥ `0x80`) → SIMD-decoded via
    ///   `simdutf::convert_utf8_to_utf32_with_errors` into
    ///   [`Self::decode_buf`] with inline Maximal-Subpart U+FFFD
    ///   replacement, then dispatched as one [`Perform::print_codepoints`]
    ///   call (with C1 controls `U+0080..U+009F` split out as individual
    ///   `execute`).
    #[inline]
    fn ground_dispatch<P: Perform>(&mut self, performer: &mut P, bytes: &[u8]) {
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];

            if (0x20..=0x7E).contains(&b) {
                // LLVM auto-vectorizes this byte-range scan into a SIMD compare.
                let end = bytes[i..]
                    .iter()
                    .position(|&b| !(0x20..=0x7E).contains(&b))
                    .map(|p| i + p)
                    .unwrap_or(bytes.len());
                // SAFETY: every byte in 0x20..=0x7E is valid 1-byte UTF-8.
                let chunk = unsafe { std::str::from_utf8_unchecked(&bytes[i..end]) };
                performer.print_str(chunk);
                i = end;
            } else if b < 0x80 {
                performer.execute(b);
                i += 1;
            } else if (0x80..=0x9F).contains(&b) {
                // Bare C1 control byte: any properly-encoded UTF-8 sequence
                // starts with `C2..=F4`, so a leading byte in `80..=9F` is
                // by definition standalone. Some legacy protocols emit C1
                // controls as raw bytes — dispatch as `execute` rather
                // than U+FFFD-replacing them.
                performer.execute(b);
                i += 1;
            } else {
                // Multi-byte UTF-8 run. Find its end (next ASCII byte).
                let end = bytes[i..]
                    .iter()
                    .position(|&b| b < 0x80)
                    .map(|p| i + p)
                    .unwrap_or(bytes.len());
                self.decode_codepoints(&bytes[i..end]);
                Self::dispatch_codepoints(performer, &self.decode_buf);
                i = end;
            }
        }
    }

    /// SIMD-transcode a UTF-8 byte slice into [`Self::decode_buf`] as `u32`
    /// codepoints, replacing each invalid UTF-8 maximal subpart with one
    /// U+FFFD inline (W3C/Unicode "Substitution of Maximal Subparts").
    #[inline]
    fn decode_codepoints(&mut self, src: &[u8]) {
        self.decode_buf.clear();
        // Worst case: 1 codepoint per source byte. Reserve up-front so the
        // raw pointer writes simdutf does are always in-bounds.
        self.decode_buf.reserve(src.len());

        let mut consumed = 0;
        while consumed < src.len() {
            // SAFETY:
            //  - `src.as_ptr().add(consumed)` is a valid pointer into `src`
            //    (consumed < src.len()).
            //  - `decode_buf` has capacity `src.len()` and currently holds
            //    `decode_buf.len()` items; remaining slots ≥ remaining
            //    source bytes ≥ codepoints simdutf will produce.
            let result = unsafe {
                simdutf::convert_utf8_to_utf32_with_errors(
                    src.as_ptr().add(consumed),
                    src.len() - consumed,
                    self.decode_buf.as_mut_ptr().add(self.decode_buf.len()),
                )
            };
            if result.error == simdutf::ErrorCode::Success {
                // SAFETY: simdutf wrote `result.count` u32 values.
                unsafe {
                    self.decode_buf
                        .set_len(self.decode_buf.len() + result.count);
                }
                return;
            }

            // simdutf errored at offset `result.count` within the slice
            // starting at `consumed`. It already wrote codepoints for the
            // valid prefix; count them so the buffer length stays in sync.
            let valid_prefix = &src[consumed..consumed + result.count];
            let written = simdutf::count_utf8(valid_prefix);
            // SAFETY: simdutf wrote exactly `written` u32 values.
            unsafe {
                self.decode_buf.set_len(self.decode_buf.len() + written);
            }

            // Emit one U+FFFD for the maximal invalid subpart at the error
            // position and advance past it.
            let err_pos = consumed + result.count;
            let subpart = maximal_subpart(&src[err_pos..]);
            self.decode_buf.push(0xFFFD);
            consumed = err_pos + subpart;
        }
    }

    /// Split a decoded codepoint slice on C1 control codepoints
    /// (`U+0080..=U+009F`) and emit non-control runs as
    /// [`Perform::print_codepoints`], C1 codepoints as [`Perform::execute`].
    #[inline]
    fn dispatch_codepoints<P: Perform>(performer: &mut P, codepoints: &[u32]) {
        let mut start = 0;
        for (i, &cp) in codepoints.iter().enumerate() {
            if (0x80..=0x9F).contains(&cp) {
                if start < i {
                    performer.print_codepoints(&codepoints[start..i]);
                }
                performer.execute(cp as u8);
                start = i + 1;
            }
        }
        if start < codepoints.len() {
            performer.print_codepoints(&codepoints[start..]);
        }
    }
}

/// Length of the maximal valid subpart of a UTF-8 sequence starting at
/// `p[0]`, per Unicode Table 3-7 / W3C "U+FFFD Substitution of Maximal
/// Subparts". Each maximal subpart maps to exactly one U+FFFD when
/// reporting invalid input.
fn maximal_subpart(p: &[u8]) -> usize {
    if p.is_empty() {
        return 0;
    }
    let b0 = p[0];

    // Continuation bytes, overlong leads, or invalid leads: each is its
    // own 1-byte maximal subpart.
    if !(0xC2..=0xF4).contains(&b0) {
        return 1;
    }

    // Determine expected sequence length and per-byte continuation ranges
    // from the lead byte (Unicode Table 3-7).
    let (seq_len, lo, hi): (usize, [u8; 3], [u8; 3]) = match b0 {
        0xC2..=0xDF => (2, [0x80, 0, 0], [0xBF, 0, 0]),
        0xE0 => (3, [0xA0, 0x80, 0], [0xBF, 0xBF, 0]),
        0xE1..=0xEC => (3, [0x80, 0x80, 0], [0xBF, 0xBF, 0]),
        0xED => (3, [0x80, 0x80, 0], [0x9F, 0xBF, 0]),
        0xEE..=0xEF => (3, [0x80, 0x80, 0], [0xBF, 0xBF, 0]),
        0xF0 => (4, [0x90, 0x80, 0x80], [0xBF, 0xBF, 0xBF]),
        0xF1..=0xF3 => (4, [0x80, 0x80, 0x80], [0xBF, 0xBF, 0xBF]),
        0xF4 => (4, [0x80, 0x80, 0x80], [0x8F, 0xBF, 0xBF]),
        _ => unreachable!(),
    };

    let mut valid = 1;
    for i in 0..seq_len - 1 {
        if valid >= p.len() {
            break;
        }
        let cb = p[valid];
        if cb < lo[i] || cb > hi[i] {
            break;
        }
        valid += 1;
    }
    valid
}

/// Length of `input` excluding any trailing valid-so-far UTF-8 sequence.
/// Bytes that form an invalid lead (`<C2` or `>F4`) at the tail are NOT
/// trimmed — they're left in place so the MSP-aware decode loop can
/// replace them with U+FFFD. Mirrors ghostty's `TrimValidPartialUTF8`.
fn trim_valid_partial_utf8(input: &[u8]) -> usize {
    if input.is_empty() {
        return 0;
    }

    // A partial trailing sequence is at most 4 bytes (max UTF-8 length).
    let check_start = if input.len() > 4 { input.len() - 4 } else { 0 };
    let mut pos = input.len();
    while pos > check_start {
        let b = input[pos - 1];

        // Skip continuation bytes — they could belong to a partial sequence.
        if (b & 0xC0) == 0x80 {
            pos -= 1;
            continue;
        }

        // Found a non-continuation byte. Only a valid multi-byte lead
        // (C2-F4) can start a trim-worthy partial sequence.
        if !(0xC2..=0xF4).contains(&b) {
            return input.len();
        }

        let expected = if b <= 0xDF {
            2
        } else if b <= 0xEF {
            3
        } else {
            4
        };

        let seq_remaining = input.len() - (pos - 1);
        if seq_remaining >= expected {
            // Sequence is structurally complete — let the decoder handle it.
            return input.len();
        }

        let seq_start = pos - 1;
        let subpart = maximal_subpart(&input[seq_start..]);

        if subpart == seq_remaining {
            // All trailing bytes are part of a valid prefix → trim them.
            return seq_start;
        }

        // Sequence is ill-formed; let the MSP-aware decoder replace it.
        return input.len();
    }

    input.len()
}

#[derive(PartialEq, Eq, Debug, Default, Copy, Clone)]
enum State {
    CsiEntry,
    CsiIgnore,
    CsiIntermediate,
    CsiParam,
    DcsEntry,
    DcsIgnore,
    DcsIntermediate,
    DcsParam,
    DcsPassthrough,
    Escape,
    EscapeIntermediate,
    OscString,
    SosString,
    ApcString,
    PmString,
    #[default]
    Ground,
}

/// Performs actions requested by the [`Parser`].
///
/// The single production implementer is the engine's `Performer`; tests in
/// this module supply their own recording dispatchers.
///
/// The methods correspond to actions described in
/// <http://vt100.net/emu/dec_ansi_parser>.
pub trait Perform {
    /// Draw a character to the screen and update states.
    fn print(&mut self, _c: char) {}

    /// Draw a contiguous run of characters in one batch. The default
    /// implementation iterates and calls [`Perform::print`]; implementers
    /// can specialize to skip per-char trait dispatch and per-char width
    /// lookup when the run is known to be all narrow / printable / non-
    /// combining (e.g. ASCII).
    fn print_str(&mut self, s: &str) {
        for c in s.chars() {
            self.print(c);
        }
    }

    /// Draw a contiguous run of pre-decoded Unicode codepoints in one batch.
    /// The default implementation iterates, converts each `u32` to `char`
    /// (replacing invalid encodings with U+FFFD), and calls
    /// [`Perform::print`]. Implementers can specialize to bulk-process
    /// codepoints — e.g. SIMD width lookup + bulk cell write.
    fn print_codepoints(&mut self, codepoints: &[u32]) {
        for &cp in codepoints {
            let c = char::from_u32(cp).unwrap_or('\u{FFFD}');
            self.print(c);
        }
    }

    /// Execute a C0 or C1 control function.
    fn execute(&mut self, _byte: u8) {}

    /// Invoked when a final character arrives in first part of device control
    /// string.
    ///
    /// The control function should be determined from the private marker, final
    /// character, and execute with a parameter list. A handler should be
    /// selected for remaining characters in the string; the handler
    /// function should subsequently be called by `put` for every character in
    /// the control string.
    ///
    /// The `ignore` flag indicates that more than two intermediates arrived and
    /// subsequent characters were ignored.
    fn hook(
        &mut self,
        _params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: char,
    ) {
    }

    /// Pass bytes as part of a device control string to the handle chosen in
    /// `hook`. C0 controls will also be passed to the handler.
    fn put(&mut self, _byte: u8) {}

    /// Called when a device control string is terminated.
    ///
    /// The previously selected handler should be notified that the DCS has
    /// terminated.
    fn unhook(&mut self) {}

    /// Dispatch an operating system command.
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}

    /// A final character has arrived for a CSI sequence
    ///
    /// The `ignore` flag indicates that either more than two intermediates
    /// arrived or the number of parameters exceeded the maximum supported
    /// length, and subsequent characters were ignored.
    fn csi_dispatch(
        &mut self,
        _params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: char,
    ) {
    }

    /// The final character of an escape sequence has arrived.
    ///
    /// The `ignore` flag indicates that more than two intermediates arrived and
    /// subsequent characters were ignored.
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}

    /// Invoked when the beginning of a new SOS (Start of String) sequence is
    /// encountered.
    fn sos_start(&mut self) {}

    /// Invoked for every valid byte (0x20-0xFF) in a SOS (Start of String)
    /// sequence.
    fn sos_put(&mut self, _byte: u8) {}

    /// Invoked when the end of an SOS (Start of String) sequence is
    /// encountered.
    fn sos_end(&mut self) {}

    /// Invoked when the beginning of a new PM (Privacy Message) sequence is
    /// encountered.
    fn pm_start(&mut self) {}

    /// Invoked for every valid byte (0x20-0xFF) in a PM (Privacy Message)
    /// sequence.
    fn pm_put(&mut self, _byte: u8) {}

    /// Invoked when the end of a PM (Privacy Message) sequence is encountered.
    fn pm_end(&mut self) {}

    /// Invoked when the beginning of a new APC (Application Program Command)
    /// sequence is encountered.
    fn apc_start(&mut self) {}

    /// Invoked for every valid byte (0x20-0xFF) in an APC (Application Program
    /// Command) sequence.
    fn apc_put(&mut self, _byte: u8) {}
    /// Invoked when the end of an APC (Application Program Command) sequence is
    /// encountered.
    fn apc_end(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    const OSC_BYTES: &[u8] = &[
        0x1B, 0x5D, // Begin OSC
        b'2', b';', b'j', b'w', b'i', b'l', b'm', b'@', b'j', b'w', b'i', b'l', b'm',
        b'-', b'd', b'e', b's', b'k', b':', b' ', b'~', b'/', b'c', b'o', b'd', b'e',
        b'/', b'a', b'l', b'a', b'c', b'r', b'i', b't', b't', b'y', 0x07, // End OSC
    ];

    const ST_ESC_SEQUENCE: &[Sequence] = &[Sequence::Esc(vec![], false, 0x5C)];

    #[derive(Default)]
    struct Dispatcher {
        dispatched: Vec<Sequence>,
    }

    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    enum OpaqueSequenceKind {
        Sos,
        Pm,
        Apc,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Sequence {
        Osc(Vec<Vec<u8>>, bool),
        Csi(Vec<Vec<u16>>, Vec<u8>, bool, char),
        Esc(Vec<u8>, bool, u8),
        DcsHook(Vec<Vec<u16>>, Vec<u8>, bool, char),
        DcsPut(u8),
        Print(char),
        Execute(u8),
        OpaqueStart(OpaqueSequenceKind),
        OpaquePut(OpaqueSequenceKind, u8),
        OpaqueEnd(OpaqueSequenceKind),
        DcsUnhook,
    }

    impl Perform for Dispatcher {
        fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
            let params = params.iter().map(|p| p.to_vec()).collect();
            self.dispatched.push(Sequence::Osc(params, bell_terminated));
        }

        fn csi_dispatch(
            &mut self,
            params: &Params,
            intermediates: &[u8],
            ignore: bool,
            c: char,
        ) {
            let params = params.iter().map(|subparam| subparam.to_vec()).collect();
            let intermediates = intermediates.to_vec();
            self.dispatched
                .push(Sequence::Csi(params, intermediates, ignore, c));
        }

        fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
            let intermediates = intermediates.to_vec();
            self.dispatched
                .push(Sequence::Esc(intermediates, ignore, byte));
        }

        fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, c: char) {
            let params = params.iter().map(|subparam| subparam.to_vec()).collect();
            let intermediates = intermediates.to_vec();
            self.dispatched
                .push(Sequence::DcsHook(params, intermediates, ignore, c));
        }

        fn put(&mut self, byte: u8) {
            self.dispatched.push(Sequence::DcsPut(byte));
        }

        fn unhook(&mut self) {
            self.dispatched.push(Sequence::DcsUnhook);
        }

        fn print(&mut self, c: char) {
            self.dispatched.push(Sequence::Print(c));
        }

        fn execute(&mut self, byte: u8) {
            self.dispatched.push(Sequence::Execute(byte));
        }

        fn sos_start(&mut self) {
            self.dispatched
                .push(Sequence::OpaqueStart(OpaqueSequenceKind::Sos));
        }

        fn sos_put(&mut self, byte: u8) {
            self.dispatched
                .push(Sequence::OpaquePut(OpaqueSequenceKind::Sos, byte));
        }

        fn sos_end(&mut self) {
            self.dispatched
                .push(Sequence::OpaqueEnd(OpaqueSequenceKind::Sos));
        }

        fn pm_start(&mut self) {
            self.dispatched
                .push(Sequence::OpaqueStart(OpaqueSequenceKind::Pm));
        }

        fn pm_put(&mut self, byte: u8) {
            self.dispatched
                .push(Sequence::OpaquePut(OpaqueSequenceKind::Pm, byte));
        }

        fn pm_end(&mut self) {
            self.dispatched
                .push(Sequence::OpaqueEnd(OpaqueSequenceKind::Pm));
        }

        fn apc_start(&mut self) {
            self.dispatched
                .push(Sequence::OpaqueStart(OpaqueSequenceKind::Apc));
        }

        fn apc_put(&mut self, byte: u8) {
            self.dispatched
                .push(Sequence::OpaquePut(OpaqueSequenceKind::Apc, byte));
        }

        fn apc_end(&mut self) {
            self.dispatched
                .push(Sequence::OpaqueEnd(OpaqueSequenceKind::Apc));
        }
    }

    #[test]
    fn parse_osc() {
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, OSC_BYTES);

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Osc(params, _) => {
                assert_eq!(params.len(), 2);
                assert_eq!(params[0], &OSC_BYTES[2..3]);
                assert_eq!(params[1], &OSC_BYTES[4..(OSC_BYTES.len() - 1)]);
            }
            _ => panic!("expected osc sequence"),
        }
    }

    #[test]
    fn parse_empty_osc() {
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, &[0x1B, 0x5D, 0x07]);

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Osc(..) => (),
            _ => panic!("expected osc sequence"),
        }
    }

    #[test]
    fn parse_osc_max_params() {
        let params = ";".repeat(params::MAX_PARAMS + 1);
        let input = format!("\x1b]{}\x1b", &params[..]).into_bytes();
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, &input);

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Osc(params, _) => {
                assert_eq!(params.len(), MAX_OSC_PARAMS);
                assert!(params.iter().all(Vec::is_empty));
            }
            _ => panic!("expected osc sequence"),
        }
    }

    #[test]
    fn osc_bell_terminated() {
        const INPUT: &[u8] = b"\x1b]11;ff/00/ff\x07";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT);

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Osc(_, true) => (),
            _ => panic!("expected osc with bell terminator"),
        }
    }

    #[test]
    fn osc_c0_st_terminated() {
        const INPUT: &[u8] = b"\x1b]11;ff/00/ff\x1b\\";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT);

        assert_eq!(dispatcher.dispatched.len(), 2);
        match &dispatcher.dispatched[0] {
            Sequence::Osc(_, false) => (),
            _ => panic!("expected osc with ST terminator"),
        }
    }

    #[test]
    fn parse_osc_with_utf8_arguments() {
        const INPUT: &[u8] = &[
            0x0D, 0x1B, 0x5D, 0x32, 0x3B, 0x65, 0x63, 0x68, 0x6F, 0x20, 0x27, 0xC2, 0xAF,
            0x5C, 0x5F, 0x28, 0xE3, 0x83, 0x84, 0x29, 0x5F, 0x2F, 0xC2, 0xAF, 0x27, 0x20,
            0x26, 0x26, 0x20, 0x73, 0x6C, 0x65, 0x65, 0x70, 0x20, 0x31, 0x07,
        ];
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT);

        assert_eq!(dispatcher.dispatched[0], Sequence::Execute(b'\r'));
        let osc_data = INPUT[5..(INPUT.len() - 1)].into();
        assert_eq!(
            dispatcher.dispatched[1],
            Sequence::Osc(vec![vec![b'2'], osc_data], true)
        );
        assert_eq!(dispatcher.dispatched.len(), 2);
    }

    #[test]
    fn osc_containing_string_terminator() {
        const INPUT: &[u8] = b"\x1b]2;\xe6\x9c\xab\x1b\\";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT);

        assert_eq!(dispatcher.dispatched.len(), 2);
        match &dispatcher.dispatched[0] {
            Sequence::Osc(params, _) => {
                assert_eq!(params[1], &INPUT[4..(INPUT.len() - 2)]);
            }
            _ => panic!("expected osc sequence"),
        }
    }

    #[test]
    fn osc_fits_in_inline_buffer() {
        // Stay below `OSC_FIXED_LEN`; the spill `Vec` should never grow.
        const NUM_BYTES: usize = OSC_FIXED_LEN - 32;
        const INPUT_START: &[u8] = b"\x1b]52;s";
        const INPUT_END: &[u8] = b"\x07";

        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT_START);
        parser.advance(&mut dispatcher, &[b'a'; NUM_BYTES]);
        parser.advance(&mut dispatcher, INPUT_END);

        assert!(parser.osc_raw.overflow.capacity() == 0);
        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Osc(params, _) => {
                assert_eq!(params.len(), 2);
                assert_eq!(params[0], b"52");
                assert_eq!(params[1].len(), NUM_BYTES + INPUT_END.len());
            }
            _ => panic!("expected osc sequence"),
        }
    }

    #[test]
    fn osc_spills_to_overflow() {
        // Push past `OSC_FIXED_LEN` to exercise the heap-fallback path.
        const NUM_BYTES: usize = OSC_FIXED_LEN + 512;
        const INPUT_START: &[u8] = b"\x1b]52;s";
        const INPUT_END: &[u8] = b"\x07";

        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT_START);
        parser.advance(&mut dispatcher, &[b'a'; NUM_BYTES]);
        parser.advance(&mut dispatcher, INPUT_END);

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Osc(params, _) => {
                assert_eq!(params.len(), 2);
                assert_eq!(params[0], b"52");
                assert_eq!(params[1].len(), NUM_BYTES + INPUT_END.len());
                assert_eq!(params[1][0], b's');
                assert!(params[1][1..].iter().all(|&b| b == b'a'));
            }
            _ => panic!("expected osc sequence"),
        }
    }

    #[test]
    fn parse_csi_max_params() {
        // This will build a list of repeating '1;'s
        // The length is MAX_PARAMS - 1 because the last semicolon is interpreted
        // as an implicit zero, making the total number of parameters MAX_PARAMS
        let params = "1;".repeat(params::MAX_PARAMS - 1);
        let input = format!("\x1b[{}p", &params[..]).into_bytes();

        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, &input);

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Csi(params, _, ignore, _) => {
                assert_eq!(params.len(), params::MAX_PARAMS);
                assert!(!ignore);
            }
            _ => panic!("expected csi sequence"),
        }
    }

    #[test]
    fn parse_csi_params_ignore_long_params() {
        // This will build a list of repeating '1;'s
        // The length is MAX_PARAMS because the last semicolon is interpreted
        // as an implicit zero, making the total number of parameters MAX_PARAMS + 1
        let params = "1;".repeat(params::MAX_PARAMS);
        let input = format!("\x1b[{}p", &params[..]).into_bytes();

        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, &input);

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Csi(params, _, ignore, _) => {
                assert_eq!(params.len(), params::MAX_PARAMS);
                assert!(ignore);
            }
            _ => panic!("expected csi sequence"),
        }
    }

    #[test]
    fn parse_csi_params_trailing_semicolon() {
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, b"\x1b[4;m");

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Csi(params, ..) => assert_eq!(params, &[[4], [0]]),
            _ => panic!("expected csi sequence"),
        }
    }

    #[test]
    fn parse_csi_params_leading_semicolon() {
        // Create dispatcher and check state
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, b"\x1b[;4m");

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Csi(params, ..) => assert_eq!(params, &[[0], [4]]),
            _ => panic!("expected csi sequence"),
        }
    }

    #[test]
    fn parse_long_csi_param() {
        // The important part is the parameter, which is (i64::MAX + 1)
        const INPUT: &[u8] = b"\x1b[9223372036854775808m";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT);

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Csi(params, ..) => assert_eq!(params, &[[u16::MAX]]),
            _ => panic!("expected csi sequence"),
        }
    }

    #[test]
    fn csi_reset() {
        const INPUT: &[u8] = b"\x1b[3;1\x1b[?1049h";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT);

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Csi(params, intermediates, ignore, _) => {
                assert_eq!(intermediates, b"?");
                assert_eq!(params, &[[1049]]);
                assert!(!ignore);
            }
            _ => panic!("expected csi sequence"),
        }
    }

    #[test]
    fn csi_subparameters() {
        const INPUT: &[u8] = b"\x1b[38:2:255:0:255;1m";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT);

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Csi(params, intermediates, ignore, _) => {
                assert_eq!(params, &[vec![38, 2, 255, 0, 255], vec![1]]);
                assert_eq!(intermediates, &[]);
                assert!(!ignore);
            }
            _ => panic!("expected csi sequence"),
        }
    }

    #[test]
    fn parse_dcs_max_params() {
        let params = "1;".repeat(params::MAX_PARAMS + 1);
        let input = format!("\x1bP{}p", &params[..]).into_bytes();
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, &input);

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::DcsHook(params, _, ignore, _) => {
                assert_eq!(params.len(), params::MAX_PARAMS);
                assert!(params.iter().all(|param| param == &[1]));
                assert!(ignore);
            }
            _ => panic!("expected dcs sequence"),
        }
    }

    #[test]
    fn dcs_reset() {
        const INPUT: &[u8] = b"\x1b[3;1\x1bP1$tx\x9c";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT);

        assert_eq!(dispatcher.dispatched.len(), 3);
        match &dispatcher.dispatched[0] {
            Sequence::DcsHook(params, intermediates, ignore, _) => {
                assert_eq!(intermediates, b"$");
                assert_eq!(params, &[[1]]);
                assert!(!ignore);
            }
            _ => panic!("expected dcs sequence"),
        }
        assert_eq!(dispatcher.dispatched[1], Sequence::DcsPut(b'x'));
        assert_eq!(dispatcher.dispatched[2], Sequence::DcsUnhook);
    }

    #[test]
    fn parse_dcs() {
        const INPUT: &[u8] = b"\x1bP0;1|17/ab\x9c";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT);

        assert_eq!(dispatcher.dispatched.len(), 7);
        match &dispatcher.dispatched[0] {
            Sequence::DcsHook(params, _, _, c) => {
                assert_eq!(params, &[[0], [1]]);
                assert_eq!(c, &'|');
            }
            _ => panic!("expected dcs sequence"),
        }
        for (i, byte) in b"17/ab".iter().enumerate() {
            assert_eq!(dispatcher.dispatched[1 + i], Sequence::DcsPut(*byte));
        }
        assert_eq!(dispatcher.dispatched[6], Sequence::DcsUnhook);
    }

    #[test]
    fn intermediate_reset_on_dcs_exit() {
        const INPUT: &[u8] = b"\x1bP=1sZZZ\x1b+\x5c";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT);

        assert_eq!(dispatcher.dispatched.len(), 6);
        match &dispatcher.dispatched[5] {
            Sequence::Esc(intermediates, ..) => assert_eq!(intermediates, b"+"),
            _ => panic!("expected esc sequence"),
        }
    }

    #[test]
    fn esc_reset() {
        const INPUT: &[u8] = b"\x1b[3;1\x1b(A";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT);

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Esc(intermediates, ignore, byte) => {
                assert_eq!(intermediates, b"(");
                assert_eq!(*byte, b'A');
                assert!(!ignore);
            }
            _ => panic!("expected esc sequence"),
        }
    }

    #[test]
    fn esc_reset_intermediates() {
        const INPUT: &[u8] = b"\x1b[?2004l\x1b#8";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT);

        assert_eq!(dispatcher.dispatched.len(), 2);
        assert_eq!(
            dispatcher.dispatched[0],
            Sequence::Csi(vec![vec![2004]], vec![63], false, 'l')
        );
        assert_eq!(dispatcher.dispatched[1], Sequence::Esc(vec![35], false, 56));
    }

    #[test]
    fn params_buffer_filled_with_subparam() {
        const INPUT: &[u8] = b"\x1b[::::::::::::::::::::::::::::::::x\x1b";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT);

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Csi(params, intermediates, ignore, c) => {
                assert_eq!(intermediates, &[]);
                assert_eq!(params, &[[0; 32]]);
                assert_eq!(c, &'x');
                assert!(ignore);
            }
            _ => panic!("expected csi sequence"),
        }
    }

    fn expect_opaque_sequence(
        input: &[u8],
        kind: OpaqueSequenceKind,
        expected_payload: &[u8],
        expected_trailer: &[Sequence],
    ) {
        let mut expected_dispatched: Vec<Sequence> = vec![Sequence::OpaqueStart(kind)];
        for byte in expected_payload {
            expected_dispatched.push(Sequence::OpaquePut(kind, *byte));
        }
        expected_dispatched.push(Sequence::OpaqueEnd(kind));
        for item in expected_trailer {
            expected_dispatched.push(item.clone());
        }

        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();
        parser.advance(&mut dispatcher, input);

        assert_eq!(dispatcher.dispatched, expected_dispatched);
    }

    #[test]
    fn sos_c0_st_terminated() {
        expect_opaque_sequence(
            b"\x1bXTest\x20\xFF;xyz\x1b\\",
            OpaqueSequenceKind::Sos,
            b"Test\x20\xFF;xyz",
            ST_ESC_SEQUENCE,
        );
    }

    #[test]
    fn sos_bell_terminated() {
        expect_opaque_sequence(
            b"\x1bXTest\x20\xFF;xyz\x07",
            OpaqueSequenceKind::Sos,
            b"Test\x20\xFF;xyz",
            &[],
        );
    }

    #[test]
    fn sos_empty() {
        expect_opaque_sequence(
            b"\x1bX\x1b\\",
            OpaqueSequenceKind::Sos,
            &[],
            ST_ESC_SEQUENCE,
        );
    }

    #[test]
    fn pm_c0_st_terminated() {
        expect_opaque_sequence(
            b"\x1b^Test\x20\xFF;xyz\x1b\\",
            OpaqueSequenceKind::Pm,
            b"Test\x20\xFF;xyz",
            ST_ESC_SEQUENCE,
        );
    }

    #[test]
    fn pm_bell_terminated() {
        expect_opaque_sequence(
            b"\x1b^Test\x20\xFF;xyz\x07",
            OpaqueSequenceKind::Pm,
            b"Test\x20\xFF;xyz",
            &[],
        );
    }

    #[test]
    fn pm_empty() {
        expect_opaque_sequence(
            b"\x1b^\x1b\\",
            OpaqueSequenceKind::Pm,
            &[],
            ST_ESC_SEQUENCE,
        );
    }

    #[test]
    fn parse_kitty_apc() {
        const INPUT: &[u8] = b"\x1b_Gf=24,s=10,v=20;Zm9v\x1b\\";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT);

        let expected = vec![
            Sequence::OpaqueStart(OpaqueSequenceKind::Apc),
            Sequence::OpaquePut(OpaqueSequenceKind::Apc, b'G'),
            Sequence::OpaquePut(OpaqueSequenceKind::Apc, b'f'),
            Sequence::OpaquePut(OpaqueSequenceKind::Apc, b'='),
            Sequence::OpaquePut(OpaqueSequenceKind::Apc, b'2'),
            Sequence::OpaquePut(OpaqueSequenceKind::Apc, b'4'),
            Sequence::OpaquePut(OpaqueSequenceKind::Apc, b','),
            Sequence::OpaquePut(OpaqueSequenceKind::Apc, b's'),
            Sequence::OpaquePut(OpaqueSequenceKind::Apc, b'='),
            Sequence::OpaquePut(OpaqueSequenceKind::Apc, b'1'),
            Sequence::OpaquePut(OpaqueSequenceKind::Apc, b'0'),
            Sequence::OpaquePut(OpaqueSequenceKind::Apc, b','),
            Sequence::OpaquePut(OpaqueSequenceKind::Apc, b'v'),
            Sequence::OpaquePut(OpaqueSequenceKind::Apc, b'='),
            Sequence::OpaquePut(OpaqueSequenceKind::Apc, b'2'),
            Sequence::OpaquePut(OpaqueSequenceKind::Apc, b'0'),
            Sequence::OpaquePut(OpaqueSequenceKind::Apc, b';'),
            Sequence::OpaquePut(OpaqueSequenceKind::Apc, b'Z'),
            Sequence::OpaquePut(OpaqueSequenceKind::Apc, b'm'),
            Sequence::OpaquePut(OpaqueSequenceKind::Apc, b'9'),
            Sequence::OpaquePut(OpaqueSequenceKind::Apc, b'v'),
            Sequence::OpaqueEnd(OpaqueSequenceKind::Apc),
            Sequence::Esc(vec![], false, b'\\'),
        ];

        assert_eq!(dispatcher.dispatched, expected)
    }

    #[test]
    fn parse_kitty_apc_dispatch_params() {
        // Test that commas in control data are NOT treated as param separators
        // Only semicolons should separate control data from payload
        const INPUT: &[u8] = b"\x1b_Gf=32,s=10,v=20;AQIDBA==\x1b\\";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT);

        // Verify we got an APC dispatch
        let apc_dispatch = dispatcher
            .dispatched
            .iter()
            .find(|s| matches!(s, Sequence::OpaqueEnd(OpaqueSequenceKind::Apc)));
        assert!(apc_dispatch.is_some(), "Should have APC dispatch");

        // The test in performer::handler verifies the actual param parsing
        // Here we just ensure the sequence completes correctly
    }

    #[test]
    fn apc_c0_st_terminated() {
        expect_opaque_sequence(
            b"\x1b_Test\x20\xFF;xyz\x1b\\",
            OpaqueSequenceKind::Apc,
            b"Test\x20\xFF;xyz",
            ST_ESC_SEQUENCE,
        );
    }

    #[test]
    fn apc_bell_terminated() {
        expect_opaque_sequence(
            b"\x1b_Test\x20\xFF;xyz\x07",
            OpaqueSequenceKind::Apc,
            b"Test\x20\xFF;xyz",
            &[],
        );
    }

    #[test]
    fn apc_empty() {
        expect_opaque_sequence(
            b"\x1b_\x1b\\",
            OpaqueSequenceKind::Apc,
            &[],
            ST_ESC_SEQUENCE,
        );
    }

    #[test]
    fn unicode() {
        const INPUT: &[u8] =
            b"\xF0\x9F\x8E\x89_\xF0\x9F\xA6\x80\xF0\x9F\xA6\x80_\xF0\x9F\x8E\x89";

        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT);

        assert_eq!(dispatcher.dispatched.len(), 6);
        assert_eq!(dispatcher.dispatched[0], Sequence::Print('🎉'));
        assert_eq!(dispatcher.dispatched[1], Sequence::Print('_'));
        assert_eq!(dispatcher.dispatched[2], Sequence::Print('🦀'));
        assert_eq!(dispatcher.dispatched[3], Sequence::Print('🦀'));
        assert_eq!(dispatcher.dispatched[4], Sequence::Print('_'));
        assert_eq!(dispatcher.dispatched[5], Sequence::Print('🎉'));
    }

    #[test]
    fn invalid_utf8() {
        const INPUT: &[u8] = b"a\xEF\xBCb";

        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT);

        assert_eq!(dispatcher.dispatched.len(), 3);
        assert_eq!(dispatcher.dispatched[0], Sequence::Print('a'));
        assert_eq!(dispatcher.dispatched[1], Sequence::Print('�'));
        assert_eq!(dispatcher.dispatched[2], Sequence::Print('b'));
    }

    #[test]
    fn partial_utf8() {
        const INPUT: &[u8] = b"\xF0\x9F\x9A\x80";

        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, &INPUT[..1]);
        parser.advance(&mut dispatcher, &INPUT[1..2]);
        parser.advance(&mut dispatcher, &INPUT[2..3]);
        parser.advance(&mut dispatcher, &INPUT[3..]);

        assert_eq!(dispatcher.dispatched.len(), 1);
        assert_eq!(dispatcher.dispatched[0], Sequence::Print('🚀'));
    }

    #[test]
    fn partial_utf8_separating_utf8() {
        // This is different from the `partial_utf8` test since it has a multi-byte UTF8
        // character after the partial UTF8 state, causing a partial byte to be present
        // in the `partial_utf8` buffer after the 2-byte codepoint.

        // "ĸ🎉"
        const INPUT: &[u8] = b"\xC4\xB8\xF0\x9F\x8E\x89";

        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, &INPUT[..1]);
        parser.advance(&mut dispatcher, &INPUT[1..]);

        assert_eq!(dispatcher.dispatched.len(), 2);
        assert_eq!(dispatcher.dispatched[0], Sequence::Print('ĸ'));
        assert_eq!(dispatcher.dispatched[1], Sequence::Print('🎉'));
    }

    #[test]
    fn partial_invalid_utf8() {
        const INPUT: &[u8] = b"a\xEF\xBCb";

        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, &INPUT[..1]);
        parser.advance(&mut dispatcher, &INPUT[1..2]);
        parser.advance(&mut dispatcher, &INPUT[2..3]);
        parser.advance(&mut dispatcher, &INPUT[3..]);

        assert_eq!(dispatcher.dispatched.len(), 3);
        assert_eq!(dispatcher.dispatched[0], Sequence::Print('a'));
        assert_eq!(dispatcher.dispatched[1], Sequence::Print('�'));
        assert_eq!(dispatcher.dispatched[2], Sequence::Print('b'));
    }

    #[test]
    fn partial_invalid_utf8_split() {
        const INPUT: &[u8] = b"\xE4\xBF\x99\xB5";

        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, &INPUT[..2]);
        parser.advance(&mut dispatcher, &INPUT[2..]);

        assert_eq!(dispatcher.dispatched[0], Sequence::Print('俙'));
        assert_eq!(dispatcher.dispatched[1], Sequence::Print('�'));
    }

    #[test]
    fn partial_utf8_into_esc() {
        const INPUT: &[u8] = b"\xD8\x1b012";

        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT);

        assert_eq!(dispatcher.dispatched.len(), 4);
        assert_eq!(dispatcher.dispatched[0], Sequence::Print('�'));
        assert_eq!(
            dispatcher.dispatched[1],
            Sequence::Esc(Vec::new(), false, b'0')
        );
        assert_eq!(dispatcher.dispatched[2], Sequence::Print('1'));
        assert_eq!(dispatcher.dispatched[3], Sequence::Print('2'));
    }

    #[test]
    fn c1s() {
        const INPUT: &[u8] = b"\x00\x1f\x80\x90\x98\x9b\x9c\x9d\x9e\x9fa";

        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT);

        assert_eq!(dispatcher.dispatched.len(), 11);
        assert_eq!(dispatcher.dispatched[0], Sequence::Execute(0));
        assert_eq!(dispatcher.dispatched[1], Sequence::Execute(31));
        assert_eq!(dispatcher.dispatched[2], Sequence::Execute(128));
        assert_eq!(dispatcher.dispatched[3], Sequence::Execute(144));
        assert_eq!(dispatcher.dispatched[4], Sequence::Execute(152));
        assert_eq!(dispatcher.dispatched[5], Sequence::Execute(155));
        assert_eq!(dispatcher.dispatched[6], Sequence::Execute(156));
        assert_eq!(dispatcher.dispatched[7], Sequence::Execute(157));
        assert_eq!(dispatcher.dispatched[8], Sequence::Execute(158));
        assert_eq!(dispatcher.dispatched[9], Sequence::Execute(159));
        assert_eq!(dispatcher.dispatched[10], Sequence::Print('a'));
    }

    #[test]
    fn execute_anywhere() {
        const INPUT: &[u8] = b"\x18\x1a";

        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::default();

        parser.advance(&mut dispatcher, INPUT);

        assert_eq!(dispatcher.dispatched.len(), 2);
        assert_eq!(dispatcher.dispatched[0], Sequence::Execute(0x18));
        assert_eq!(dispatcher.dispatched[1], Sequence::Execute(0x1A));
    }
}
