//! Parser for implementing virtual terminal emulators
//!
//! [`Parser`] is implemented according to [Paul Williams' ANSI parser
//! state machine]. The state machine doesn't assign meaning to the parsed data
//! and is thus not itself sufficient for writing a terminal emulator. Instead,
//! it is expected that an implementation of [`Perform`] is provided which does
//! something useful with the parsed data. The [`Parser`] handles the book
//! keeping, and the [`Perform`] gets to simply handle actions.
//!
//! # Examples
//!
//! For an example of using the [`Parser`] please see the examples folder. The example included
//! there simply logs all the actions [`Perform`] does. One quick thing to see it in action is to
//! pipe `vim` into it
//!
//! ```sh
//! cargo build --release --example parselog
//! vim | target/release/examples/parselog
//! ```
//!
//! Just type `:q` to exit.
//!
//! # Differences from original state machine description
//!
//! * UTF-8 Support for Input
//! * OSC Strings can be terminated by 0x07
//! * Only supports 7-bit codes. Some 8-bit codes are still supported, but they no longer work in
//!   all states.
//!
//! [`Parser`]: struct.Parser.html
//! [`Perform`]: trait.Perform.html
//! [Paul Williams' ANSI parser state machine]: https://vt100.net/emu/dec_ansi_parser
#![deny(clippy::if_not_else, clippy::enum_glob_use)]
#![cfg_attr(feature = "no_std", no_std)]

use core::mem::MaybeUninit;

#[cfg(feature = "no_std")]
use arrayvec::ArrayVec;

mod definitions;
mod params;
mod table;
mod utf8;

pub use params::{Params, ParamsIter};

use definitions::{unpack, Action, State};

const MAX_INTERMEDIATES: usize = 2;
const MAX_OSC_PARAMS: usize = 16;
const MAX_OSC_RAW: usize = 1024;

struct VtUtf8Receiver<'a, P: Perform>(&'a mut P, &'a mut State);

impl<P: Perform> utf8::Receiver for VtUtf8Receiver<'_, P> {
    fn codepoint(&mut self, c: char) {
        self.0.print(c);
        *self.1 = State::Ground;
    }

    fn invalid_sequence(&mut self) {
        self.0.print('�');
        *self.1 = State::Ground;
    }
}

/// Parser for raw _VTE_ protocol which delegates actions to a [`Perform`]
///
/// [`Perform`]: trait.Perform.html
///
/// Generic over the value for the size of the raw Operating System Command
/// buffer. Only used when the `no_std` feature is enabled.
#[derive(Default)]
pub struct Parser<const OSC_RAW_BUF_SIZE: usize = MAX_OSC_RAW> {
    state: State,
    intermediates: [u8; MAX_INTERMEDIATES],
    intermediate_idx: usize,
    params: Params,
    param: u16,
    #[cfg(feature = "no_std")]
    osc_raw: ArrayVec<u8, OSC_RAW_BUF_SIZE>,
    #[cfg(not(feature = "no_std"))]
    osc_raw: Vec<u8>,
    osc_params: [(usize, usize); MAX_OSC_PARAMS],
    osc_num_params: usize,
    ignoring: bool,
    utf8_parser: utf8::Parser,
}

impl Parser {
    /// Create a new Parser
    pub fn new() -> Parser {
        Default::default()
    }
}

impl<const OSC_RAW_BUF_SIZE: usize> Parser<OSC_RAW_BUF_SIZE> {
    /// Create a new Parser with a custom size for the Operating System Command buffer.
    ///
    /// Call with a const-generic param on `Parser`, like:
    ///
    /// ```rust
    /// let mut p = copa::Parser::<64>::new_with_size();
    /// ```
    #[cfg(feature = "no_std")]
    pub fn new_with_size() -> Parser<OSC_RAW_BUF_SIZE> {
        Default::default()
    }

    #[inline]
    fn params(&self) -> &Params {
        &self.params
    }

    #[inline]
    fn intermediates(&self) -> &[u8] {
        &self.intermediates[..self.intermediate_idx]
    }

    /// Advance the parser state
    ///
    /// Requires a [`Perform`] in case `byte` triggers an action
    ///
    /// [`Perform`]: trait.Perform.html
    #[inline]
    pub fn advance<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        // Utf8 characters are handled out-of-band.
        if let State::Utf8 = self.state {
            self.process_utf8(performer, byte);
            return;
        }

        // Handle state changes in the anywhere state before evaluating changes
        // for current state.
        let mut change = table::STATE_CHANGES[State::Anywhere as usize][byte as usize];

        if change == 0 {
            change = table::STATE_CHANGES[self.state as usize][byte as usize];
        }

        // Unpack into a state and action
        let (state, action) = unpack(change);

        self.perform_state_change(performer, state, action, byte);
    }

    #[inline]
    fn process_utf8<P>(&mut self, performer: &mut P, byte: u8)
    where
        P: Perform,
    {
        let mut receiver = VtUtf8Receiver(performer, &mut self.state);
        let utf8_parser = &mut self.utf8_parser;
        utf8_parser.advance(&mut receiver, byte);
    }

    #[inline]
    fn perform_state_change<P>(
        &mut self,
        performer: &mut P,
        state: State,
        action: Action,
        byte: u8,
    ) where
        P: Perform,
    {
        macro_rules! maybe_action {
            ($action:expr, $arg:expr) => {
                match $action {
                    Action::None => (),
                    action => {
                        self.perform_action(performer, action, $arg);
                    }
                }
            };
        }

        match state {
            State::Anywhere => {
                // Just run the action
                self.perform_action(performer, action, byte);
            }
            state => {
                match self.state {
                    State::DcsPassthrough => {
                        self.perform_action(performer, Action::Unhook, byte);
                    }
                    State::OscString => {
                        self.perform_action(performer, Action::OscEnd, byte);
                    }
                    _ => (),
                }

                maybe_action!(action, byte);

                match state {
                    State::CsiEntry | State::DcsEntry | State::Escape => {
                        self.perform_action(performer, Action::Clear, byte);
                    }
                    State::DcsPassthrough => {
                        self.perform_action(performer, Action::Hook, byte);
                    }
                    State::OscString => {
                        self.perform_action(performer, Action::OscStart, byte);
                    }
                    _ => (),
                }

                // Assume the new state
                self.state = state;
            }
        }
    }

    /// Separate method for osc_dispatch that borrows self as read-only
    ///
    /// The aliasing is needed here for multiple slices into self.osc_raw
    #[inline]
    fn osc_dispatch<P: Perform>(&self, performer: &mut P, byte: u8) {
        let mut slices: [MaybeUninit<&[u8]>; MAX_OSC_PARAMS] =
            unsafe { MaybeUninit::uninit().assume_init() };

        for (i, slice) in slices.iter_mut().enumerate().take(self.osc_num_params) {
            let indices = self.osc_params[i];
            *slice = MaybeUninit::new(&self.osc_raw[indices.0..indices.1]);
        }

        unsafe {
            let num_params = self.osc_num_params;
            let params = &slices[..num_params] as *const [MaybeUninit<&[u8]>]
                as *const [&[u8]];
            performer.osc_dispatch(&*params, byte == 0x07);
        }
    }

    #[inline]
    fn perform_action<P: Perform>(
        &mut self,
        performer: &mut P,
        action: Action,
        byte: u8,
    ) {
        match action {
            Action::Print => performer.print(byte as char),
            Action::Execute => performer.execute(byte),
            Action::Hook => {
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
            }
            Action::Put => performer.put(byte),
            Action::OscStart => {
                self.osc_raw.clear();
                self.osc_num_params = 0;
            }
            Action::OscPut => {
                #[cfg(feature = "no_std")]
                {
                    if self.osc_raw.is_full() {
                        return;
                    }
                }

                let idx = self.osc_raw.len();

                // Param separator
                if byte == b';' {
                    let param_idx = self.osc_num_params;
                    match param_idx {
                        // Only process up to MAX_OSC_PARAMS
                        MAX_OSC_PARAMS => return,

                        // First param is special - 0 to current byte index
                        0 => {
                            self.osc_params[param_idx] = (0, idx);
                        }

                        // All other params depend on previous indexing
                        _ => {
                            let prev = self.osc_params[param_idx - 1];
                            let begin = prev.1;
                            self.osc_params[param_idx] = (begin, idx);
                        }
                    }

                    self.osc_num_params += 1;
                } else {
                    self.osc_raw.push(byte);
                }
            }
            Action::OscEnd => {
                let param_idx = self.osc_num_params;
                let idx = self.osc_raw.len();

                match param_idx {
                    // Finish last parameter if not already maxed
                    MAX_OSC_PARAMS => (),

                    // First param is special - 0 to current byte index
                    0 => {
                        self.osc_params[param_idx] = (0, idx);
                        self.osc_num_params += 1;
                    }

                    // All other params depend on previous indexing
                    _ => {
                        let prev = self.osc_params[param_idx - 1];
                        let begin = prev.1;
                        self.osc_params[param_idx] = (begin, idx);
                        self.osc_num_params += 1;
                    }
                }
                self.osc_dispatch(performer, byte);
            }
            Action::Unhook => performer.unhook(),
            Action::CsiDispatch => {
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
            }
            Action::EscDispatch => {
                performer.esc_dispatch(self.intermediates(), self.ignoring, byte);
            }
            Action::Collect => {
                if self.intermediate_idx == MAX_INTERMEDIATES {
                    self.ignoring = true;
                } else {
                    self.intermediates[self.intermediate_idx] = byte;
                    self.intermediate_idx += 1;
                }
            }
            Action::Param => {
                if self.params.is_full() {
                    self.ignoring = true;
                    return;
                }

                if byte == b';' {
                    self.params.push(self.param);
                    self.param = 0;
                } else if byte == b':' {
                    self.params.extend(self.param);
                    self.param = 0;
                } else {
                    // Continue collecting bytes into param
                    self.param = self.param.saturating_mul(10);
                    self.param = self.param.saturating_add((byte - b'0') as u16);
                }
            }
            Action::Clear => {
                // Reset everything on ESC/CSI/DCS entry
                self.intermediate_idx = 0;
                self.ignoring = false;
                self.param = 0;

                self.params.clear();
            }
            Action::BeginUtf8 => self.process_utf8(performer, byte),
            Action::Ignore => (),
            Action::None => (),
        }
    }
}

/// Performs actions requested by the Parser
///
/// Actions in this case mean, for example, handling a CSI escape sequence describing cursor
/// movement, or simply printing characters to the screen.
///
/// The methods on this type correspond to actions described in
/// http://vt100.net/emu/dec_ansi_parser. I've done my best to describe them in
/// a useful way in my own words for completeness, but the site should be
/// referenced if something isn't clear. If the site disappears at some point in
/// the future, consider checking archive.org.
pub trait Perform {
    /// Draw a character to the screen and update states.
    fn print(&mut self, _c: char) {}

    /// Execute a C0 or C1 control function.
    fn execute(&mut self, _byte: u8) {}

    /// Invoked when a final character arrives in first part of device control string.
    ///
    /// The control function should be determined from the private marker, final character, and
    /// execute with a parameter list. A handler should be selected for remaining characters in the
    /// string; the handler function should subsequently be called by `put` for every character in
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

    /// Pass bytes as part of a device control string to the handle chosen in `hook`. C0 controls
    /// will also be passed to the handler.
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
    /// The `ignore` flag indicates that either more than two intermediates arrived
    /// or the number of parameters exceeded the maximum supported length,
    /// and subsequent characters were ignored.
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
}

#[cfg(all(test, feature = "no_std"))]
#[macro_use]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;

    use std::vec::Vec;

    static OSC_BYTES: &[u8] = &[
        0x1b, 0x5d, // Begin OSC
        b'2', b';', b'j', b'w', b'i', b'l', b'm', b'@', b'j', b'w', b'i', b'l', b'm',
        b'-', b'd', b'e', b's', b'k', b':', b' ', b'~', b'/', b'c', b'o', b'd', b'e',
        b'/', b'a', b'l', b'a', b'c', b'r', b'i', b't', b't', b'y', 0x07, // End OSC
    ];

    #[derive(Default)]
    struct Dispatcher {
        dispatched: Vec<Sequence>,
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Sequence {
        Osc(Vec<Vec<u8>>, bool),
        Csi(Vec<Vec<u16>>, Vec<u8>, bool, char),
        Esc(Vec<u8>, bool, u8),
        DcsHook(Vec<Vec<u16>>, Vec<u8>, bool, char),
        DcsPut(u8),
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
    }

    #[test]
    fn parse_osc() {
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::new();

        for byte in OSC_BYTES {
            parser.advance(&mut dispatcher, *byte);
        }

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
        let mut parser = Parser::new();

        for byte in &[0x1b, 0x5d, 0x07] {
            parser.advance(&mut dispatcher, *byte);
        }

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
        let mut parser = Parser::new();

        for byte in input {
            parser.advance(&mut dispatcher, byte);
        }

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
        static INPUT: &[u8] = b"\x1b]11;ff/00/ff\x07";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Osc(_, true) => (),
            _ => panic!("expected osc with bell terminator"),
        }
    }

    #[test]
    fn osc_c0_st_terminated() {
        static INPUT: &[u8] = b"\x1b]11;ff/00/ff\x1b\\";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        assert_eq!(dispatcher.dispatched.len(), 2);
        match &dispatcher.dispatched[0] {
            Sequence::Osc(_, false) => (),
            _ => panic!("expected osc with ST terminator"),
        }
    }

    #[test]
    fn parse_osc_with_utf8_arguments() {
        static INPUT: &[u8] = &[
            0x0d, 0x1b, 0x5d, 0x32, 0x3b, 0x65, 0x63, 0x68, 0x6f, 0x20, 0x27, 0xc2, 0xaf,
            0x5c, 0x5f, 0x28, 0xe3, 0x83, 0x84, 0x29, 0x5f, 0x2f, 0xc2, 0xaf, 0x27, 0x20,
            0x26, 0x26, 0x20, 0x73, 0x6c, 0x65, 0x65, 0x70, 0x20, 0x31, 0x07,
        ];
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Osc(params, _) => {
                assert_eq!(params[0], b"2");
                assert_eq!(params[1], &INPUT[5..(INPUT.len() - 1)]);
            }
            _ => panic!("expected osc sequence"),
        }
    }

    #[test]
    fn osc_containing_string_terminator() {
        static INPUT: &[u8] = b"\x1b]2;\xe6\x9c\xab\x1b\\";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        assert_eq!(dispatcher.dispatched.len(), 2);
        match &dispatcher.dispatched[0] {
            Sequence::Osc(params, _) => {
                assert_eq!(params[1], &INPUT[4..(INPUT.len() - 2)]);
            }
            _ => panic!("expected osc sequence"),
        }
    }

    // #[test]
    // fn issue_191() {
    //     use crate::std::string::{String, ToString};
    //     use unicode_normalization::{is_nfc_quick, IsNormalized, UnicodeNormalization};

    //     // https://github.com/raphamorim/rio/issues/191
    //     // 한 isn't rendered correctly via printf '\xe1\x84\x92\xe1\x85\xa1\xe1\x86\xab\n'
    //     let mut input =
    //         std::str::from_utf8(b"\xe1\x84\x92\xe1\x85\xa1\xe1\x86\xab").unwrap();
    //     let mut dispatcher = Dispatcher::default();
    //     let mut parser = Parser::new();

    //     let p = std::mem::take(&mut input).to_string();
    //     let normalized: String;
    //     let text = if is_nfc_quick(p.chars()) == IsNormalized::Yes {
    //         p.as_str()
    //     } else {
    //         normalized = p.as_str().nfc().collect();
    //         normalized.as_str()
    //     };

    //     let bytes: &[u8] = &text.bytes().collect::<Vec<u8>>();

    //     for byte in bytes {
    //         parser.advance(&mut dispatcher, *byte);
    //     }

    //     assert_eq!(text, "한");
    // }

    #[test]
    fn exceed_max_buffer_size() {
        static NUM_BYTES: usize = MAX_OSC_RAW + 100;
        static INPUT_START: &[u8] = &[0x1b, b']', b'5', b'2', b';', b's'];
        static INPUT_END: &[u8] = b"\x07";

        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::new();

        // Create valid OSC escape
        for byte in INPUT_START {
            parser.advance(&mut dispatcher, *byte);
        }

        // Exceed max buffer size
        for _ in 0..NUM_BYTES {
            parser.advance(&mut dispatcher, b'a');
        }

        // Terminate escape for dispatch
        for byte in INPUT_END {
            parser.advance(&mut dispatcher, *byte);
        }

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Osc(params, _) => {
                assert_eq!(params.len(), 2);
                assert_eq!(params[0], b"52");

                #[cfg(not(feature = "no_std"))]
                assert_eq!(params[1].len(), NUM_BYTES + INPUT_END.len());

                #[cfg(feature = "no_std")]
                assert_eq!(params[1].len(), MAX_OSC_RAW - params[0].len());
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
        let mut parser = Parser::new();

        for byte in input {
            parser.advance(&mut dispatcher, byte);
        }

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
        let mut parser = Parser::new();

        for byte in input {
            parser.advance(&mut dispatcher, byte);
        }

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
        let mut parser = Parser::new();

        for byte in b"\x1b[4;m" {
            parser.advance(&mut dispatcher, *byte);
        }

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
        let mut parser = Parser::new();

        for byte in b"\x1b[;4m" {
            parser.advance(&mut dispatcher, *byte);
        }

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Csi(params, ..) => assert_eq!(params, &[[0], [4]]),
            _ => panic!("expected csi sequence"),
        }
    }

    #[test]
    fn parse_long_csi_param() {
        // The important part is the parameter, which is (i64::MAX + 1)
        static INPUT: &[u8] = b"\x1b[9223372036854775808m";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Csi(params, ..) => assert_eq!(params, &[[u16::MAX]]),
            _ => panic!("expected csi sequence"),
        }
    }

    #[test]
    fn csi_reset() {
        static INPUT: &[u8] = b"\x1b[3;1\x1b[?1049h";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

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
        static INPUT: &[u8] = b"\x1b[38:2:255:0:255;1m";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

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
        let mut parser = Parser::new();

        for byte in input {
            parser.advance(&mut dispatcher, byte);
        }

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
        static INPUT: &[u8] = b"\x1b[3;1\x1bP1$tx\x9c";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

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
        static INPUT: &[u8] = b"\x1bP0;1|17/ab\x9c";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

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
        static INPUT: &[u8] = b"\x1bP=1sZZZ\x1b+\x5c";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        assert_eq!(dispatcher.dispatched.len(), 6);
        match &dispatcher.dispatched[5] {
            Sequence::Esc(intermediates, ..) => assert_eq!(intermediates, b"+"),
            _ => panic!("expected esc sequence"),
        }
    }

    #[test]
    fn esc_reset() {
        static INPUT: &[u8] = b"\x1b[3;1\x1b(A";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

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
    fn params_buffer_filled_with_subparam() {
        static INPUT: &[u8] = b"\x1b[::::::::::::::::::::::::::::::::x\x1b";
        let mut dispatcher = Dispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

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

    #[cfg(feature = "no_std")]
    #[test]
    fn build_with_fixed_size() {
        static INPUT: &[u8] = b"\x1b[3;1\x1b[?1049h";
        let mut dispatcher = Dispatcher::default();
        let mut parser: Parser<30> = Parser::new_with_size();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

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

    #[cfg(feature = "no_std")]
    #[test]
    fn exceed_fixed_osc_buffer_size() {
        const OSC_BUFFER_SIZE: usize = 32;
        static NUM_BYTES: usize = OSC_BUFFER_SIZE + 100;
        static INPUT_START: &[u8] = b"\x1b]52;";
        static INPUT_END: &[u8] = b"\x07";

        let mut dispatcher = Dispatcher::default();
        let mut parser: Parser<OSC_BUFFER_SIZE> = Parser::new_with_size();

        // Create valid OSC escape
        for byte in INPUT_START {
            parser.advance(&mut dispatcher, *byte);
        }

        // Exceed max buffer size
        for _ in 0..NUM_BYTES {
            parser.advance(&mut dispatcher, b'a');
        }

        // Terminate escape for dispatch
        for byte in INPUT_END {
            parser.advance(&mut dispatcher, *byte);
        }

        assert_eq!(dispatcher.dispatched.len(), 1);
        match &dispatcher.dispatched[0] {
            Sequence::Osc(params, _) => {
                assert_eq!(params.len(), 2);
                assert_eq!(params[0], b"52");
                assert_eq!(params[1].len(), OSC_BUFFER_SIZE - params[0].len());
                for item in params[1].iter() {
                    assert_eq!(*item, b'a');
                }
            }
            _ => panic!("expected osc sequence"),
        }
    }

    #[cfg(feature = "no_std")]
    #[test]
    fn fixed_size_osc_containing_string_terminator() {
        static INPUT_START: &[u8] = b"\x1b]2;";
        static INPUT_MIDDLE: &[u8] = b"s\xe6\x9c\xab";
        static INPUT_END: &[u8] = b"\x1b\\";

        let mut dispatcher = Dispatcher::default();
        let mut parser: Parser<5> = Parser::new_with_size();

        for byte in INPUT_START {
            parser.advance(&mut dispatcher, *byte);
        }
        for byte in INPUT_MIDDLE {
            parser.advance(&mut dispatcher, *byte);
        }
        for byte in INPUT_END {
            parser.advance(&mut dispatcher, *byte);
        }

        assert_eq!(dispatcher.dispatched.len(), 2);
        match &dispatcher.dispatched[0] {
            Sequence::Osc(params, false) => {
                assert_eq!(params[0], b"2");
                assert_eq!(params[1], INPUT_MIDDLE);
            }
            _ => panic!("expected osc sequence"),
        }
    }
}

// #[cfg(all(feature = "nightly", test))]
// mod bench {
//     extern crate std;
//     extern crate test;

//     use super::*;

//     use test::{black_box, Bencher};

//     static VTE_DEMO: &[u8] = include_bytes!("../tests/demo.vte");

//     struct BenchDispatcher;
//     impl Perform for BenchDispatcher {
//         fn print(&mut self, c: char) {
//             black_box(c);
//         }

//         fn execute(&mut self, byte: u8) {
//             black_box(byte);
//         }

//         fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, c: char) {
//             black_box((params, intermediates, ignore, c));
//         }

//         fn put(&mut self, byte: u8) {
//             black_box(byte);
//         }

//         fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
//             black_box((params, bell_terminated));
//         }

//         fn csi_dispatch(
//             &mut self,
//             params: &Params,
//             intermediates: &[u8],
//             ignore: bool,
//             c: char,
//         ) {
//             black_box((params, intermediates, ignore, c));
//         }

//         fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
//             black_box((intermediates, ignore, byte));
//         }
//     }

//     fn testfile(b: &mut Bencher) {
//         b.iter(|| {
//             let mut dispatcher = BenchDispatcher;
//             let mut parser = Parser::new();

//             for byte in VTE_DEMO {
//                 parser.advance(&mut dispatcher, *byte);
//             }
//         });
//     }

//     fn state_changes(b: &mut Bencher) {
//         let input = b"\x1b]2;X\x1b\\ \x1b[0m \x1bP0@\x1b\\";
//         b.iter(|| {
//             let mut dispatcher = BenchDispatcher;
//             let mut parser = Parser::new();

//             for _ in 0..1_000 {
//                 for byte in input {
//                     parser.advance(&mut dispatcher, *byte);
//                 }
//             }
//         });
//     }
// }
