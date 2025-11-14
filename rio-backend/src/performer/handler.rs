use crate::ansi::iterm2_image_protocol;
use crate::ansi::CursorShape;
use crate::ansi::{sixel, KeyboardModes, KeyboardModesApplyBehavior};
use crate::batched_parser::BatchedParser;
use crate::config::colors::{AnsiColor, ColorRgb, NamedColor};
use crate::crosswords::pos::{CharsetIndex, Column, Line, StandardCharset};
use crate::crosswords::square::Hyperlink;
use crate::simd_utf8;
use cursor_icon::CursorIcon;
use std::mem;
use std::str::FromStr;
use std::time::Duration;
use std::time::Instant;
use sugarloaf::GraphicData;
use tracing::{debug, warn};

use crate::crosswords::attr::Attr;

use crate::ansi::control::C0;
use crate::ansi::{
    mode::{Mode, NamedPrivateMode, PrivateMode},
    ClearMode, LineClearMode, TabulationClearMode,
};
use std::fmt::Write;

// https://vt100.net/emu/dec_ansi_parser
use copa::{Params, ParamsIter};

/// Maximum time before a synchronized update is aborted.
const SYNC_UPDATE_TIMEOUT: Duration = Duration::from_millis(150);

/// Maximum number of bytes read in one synchronized update (2MiB).
const SYNC_BUFFER_SIZE: usize = 0x20_0000;

/// Number of bytes in the BSU/ESU CSI sequences.
const SYNC_ESCAPE_LEN: usize = 8;

/// BSU CSI sequence for beginning or extending synchronized updates.
const BSU_CSI: [u8; SYNC_ESCAPE_LEN] = *b"\x1b[?2026h";

/// ESU CSI sequence for terminating synchronized updates.
const ESU_CSI: [u8; SYNC_ESCAPE_LEN] = *b"\x1b[?2026l";

fn xparse_color(color: &[u8]) -> Option<ColorRgb> {
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

fn parse_number(input: &[u8]) -> Option<u8> {
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

fn parse_sgr_color(params: &mut dyn Iterator<Item = u16>) -> Option<AnsiColor> {
    match params.next() {
        Some(2) => Some(AnsiColor::Spec(ColorRgb {
            r: u8::try_from(params.next()?).ok()?,
            g: u8::try_from(params.next()?).ok()?,
            b: u8::try_from(params.next()?).ok()?,
        })),
        Some(5) => Some(AnsiColor::Indexed(u8::try_from(params.next()?).ok()?)),
        _ => None,
    }
}

#[inline]
fn handle_colon_rgb(params: &[u16]) -> Option<AnsiColor> {
    let rgb_start = if params.len() > 4 { 2 } else { 1 };
    let rgb_iter = params[rgb_start..].iter().copied();
    let mut iter = std::iter::once(params[0]).chain(rgb_iter);

    parse_sgr_color(&mut iter)
}

pub trait Handler {
    /// OSC to set window title.
    fn set_title(&mut self, _: Option<String>) {}

    /// OSC to set current directory.
    fn set_current_directory(&mut self, _: std::path::PathBuf) {}

    /// Set the cursor style.
    fn set_cursor_style(&mut self, _style: Option<CursorShape>, _blinking: bool) {}

    /// Set the cursor shape.
    fn set_cursor_shape(&mut self, _shape: CursorShape) {}

    /// A character to be displayed.
    fn input(&mut self, _c: char) {}

    /// Set cursor to position.
    fn goto(&mut self, _: Line, _: Column) {}

    /// Set cursor to specific row.
    fn goto_line(&mut self, _: Line) {}

    /// Set cursor to specific column.
    fn goto_col(&mut self, _: Column) {}

    /// Insert blank characters in current line starting from cursor.
    fn insert_blank(&mut self, _: usize) {}

    /// Move cursor up `rows`.
    fn move_up(&mut self, _: usize) {}

    /// Move cursor down `rows`.
    fn move_down(&mut self, _: usize) {}

    /// Identify the terminal (should write back to the pty stream).
    fn identify_terminal(&mut self, _intermediate: Option<char>) {}

    /// Report device status.
    fn device_status(&mut self, _: usize) {}

    /// Move cursor forward `cols`.
    fn move_forward(&mut self, _: Column) {}

    /// Move cursor backward `cols`.
    fn move_backward(&mut self, _: Column) {}

    /// Move cursor down `rows` and set to column 1.
    fn move_down_and_cr(&mut self, _: usize) {}

    /// Move cursor up `rows` and set to column 1.
    fn move_up_and_cr(&mut self, _: usize) {}

    /// Put `count` tabs.
    fn put_tab(&mut self, _count: u16) {}

    /// Backspace `count` characters.
    fn backspace(&mut self) {}

    /// Carriage return.
    fn carriage_return(&mut self) {}

    /// Linefeed.
    fn linefeed(&mut self) {}

    /// Ring the bell.
    ///
    /// Hopefully this is never implemented.
    fn bell(&mut self) {}

    /// Substitute char under cursor.
    fn substitute(&mut self) {}

    /// Newline.
    fn newline(&mut self) {}

    /// Set current position as a tabstop.
    fn set_horizontal_tabstop(&mut self) {}

    /// Scroll up `rows` rows.
    fn scroll_up(&mut self, _: usize) {}

    /// Scroll down `rows` rows.
    fn scroll_down(&mut self, _: usize) {}

    /// Insert `count` blank lines.
    fn insert_blank_lines(&mut self, _: usize) {}

    /// Delete `count` lines.
    fn delete_lines(&mut self, _: usize) {}

    /// Erase `count` chars in current line following cursor.
    ///
    /// Erase means resetting to the default state (default colors, no content,
    /// no mode flags).
    fn erase_chars(&mut self, _: Column) {}

    /// Delete `count` chars.
    ///
    /// Deleting a character is like the delete key on the keyboard - everything
    /// to the right of the deleted things is shifted left.
    fn delete_chars(&mut self, _: usize) {}

    /// Move backward `count` tabs.
    fn move_backward_tabs(&mut self, _count: u16) {}

    /// Move forward `count` tabs.
    fn move_forward_tabs(&mut self, _count: u16) {}

    /// Save current cursor position.
    fn save_cursor_position(&mut self) {}

    /// Restore cursor position.
    fn restore_cursor_position(&mut self) {}

    /// Clear current line.
    fn clear_line(&mut self, _mode: LineClearMode) {}

    /// Clear screen.
    fn clear_screen(&mut self, _mode: ClearMode) {}

    /// Set tab stops at every `interval`.
    fn set_tabs(&mut self, _interval: u16) {}

    /// Clear tab stops.
    fn clear_tabs(&mut self, _mode: TabulationClearMode) {}

    /// Reset terminal state.
    fn reset_state(&mut self) {}

    /// Reverse Index.
    ///
    /// Move the active position to the same horizontal position on the
    /// preceding line. If the active position is at the top margin, a scroll
    /// down is performed.
    fn reverse_index(&mut self) {}

    /// Set a terminal attribute.
    fn terminal_attribute(&mut self, _attr: Attr) {}

    /// Set mode.
    fn set_mode(&mut self, _mode: Mode) {}

    /// Unset mode.
    fn unset_mode(&mut self, _mode: Mode) {}

    /// DECRPM - report mode.
    fn report_mode(&mut self, _mode: Mode) {}

    /// Set private mode.
    fn set_private_mode(&mut self, _mode: PrivateMode) {}

    /// Unset private mode.
    fn unset_private_mode(&mut self, _mode: PrivateMode) {}

    /// DECRPM - report private mode.
    fn report_private_mode(&mut self, _mode: PrivateMode) {}

    /// XTVERSION - Report terminal version.
    fn report_version(&mut self) {}

    /// DECSTBM - Set the terminal scrolling region.
    fn set_scrolling_region(&mut self, _top: usize, _bottom: Option<usize>) {}

    /// DECKPAM - Set keypad to applications mode (ESCape instead of digits).
    fn set_keypad_application_mode(&mut self) {}

    /// DECKPNM - Set keypad to numeric mode (digits instead of ESCape seq).
    fn unset_keypad_application_mode(&mut self) {}

    /// Set one of the graphic character sets, G0 to G3, as the active charset.
    ///
    /// 'Invoke' one of G0 to G3 in the GL area. Also referred to as shift in,
    /// shift out and locking shift depending on the set being activated.
    fn set_active_charset(&mut self, _: CharsetIndex) {}

    /// Assign a graphic character set to G0, G1, G2 or G3.
    ///
    /// 'Designate' a graphic character set as one of G0 to G3, so that it can
    /// later be 'invoked' by `set_active_charset`.
    fn configure_charset(&mut self, _: CharsetIndex, _: StandardCharset) {}

    /// Set an indexed color value.
    fn set_color(&mut self, _: usize, _: ColorRgb) {}

    /// Respond to a color query escape sequence.
    fn dynamic_color_sequence(&mut self, _: String, _: usize, _: &str) {}

    /// Reset an indexed color to original value.
    fn reset_color(&mut self, _: usize) {}

    /// Store data into clipboard.
    fn clipboard_store(&mut self, _: u8, _: &[u8]) {}

    /// Load data from clipboard.
    fn clipboard_load(&mut self, _: u8, _: &str) {}

    /// Run the decaln routine.
    fn decaln(&mut self) {}

    /// Push a title onto the stack.
    fn push_title(&mut self) {}

    /// Pop the last title from the stack.
    fn pop_title(&mut self) {}

    /// Report text area size in pixels.
    fn text_area_size_pixels(&mut self) {}

    /// Report cell size in pixels.
    fn cells_size_pixels(&mut self) {}

    /// Report text area size in characters.
    fn text_area_size_chars(&mut self) {}

    /// Report a graphics attribute.
    fn graphics_attribute(&mut self, _: u16, _: u16) {}

    /// Create a parser for Sixel data.
    fn sixel_graphic_start(&mut self, _params: &Params) {}
    fn is_sixel_graphic_active(&self) -> bool {
        false
    }
    fn sixel_graphic_put(&mut self, _byte: u8) -> Result<(), sixel::Error> {
        Ok(())
    }
    fn sixel_graphic_reset(&mut self) {}
    fn sixel_graphic_finish(&mut self) {}

    /// Insert a new graphic item.
    fn insert_graphic(&mut self, _data: GraphicData, _palette: Option<Vec<ColorRgb>>) {}

    /// Set hyperlink.
    fn set_hyperlink(&mut self, _: Option<Hyperlink>) {}

    /// Set mouse cursor icon.
    fn set_mouse_cursor_icon(&mut self, _: CursorIcon) {}

    /// Report current keyboard mode.
    fn report_keyboard_mode(&mut self) {}

    /// Push keyboard mode into the keyboard mode stack.
    fn push_keyboard_mode(&mut self, _mode: KeyboardModes) {}

    /// Pop the given amount of keyboard modes from the
    /// keyboard mode stack.
    fn pop_keyboard_modes(&mut self, _to_pop: u16) {}

    /// Set the [`keyboard mode`] using the given [`behavior`].
    ///
    /// [`keyboard mode`]: crate::ansi::KeyboardModes
    /// [`behavior`]: crate::ansi::KeyboardModesApplyBehavior
    fn set_keyboard_mode(
        &mut self,
        _mode: KeyboardModes,
        _behavior: KeyboardModesApplyBehavior,
    ) {
    }

    /// Handle XTGETTCAP response.
    fn xtgettcap_response(&mut self, _response: String) {}
}

pub trait Timeout: Default {
    /// Sets the timeout for the next synchronized update.
    ///
    /// The `duration` parameter specifies the duration of the timeout. Once the
    /// specified duration has elapsed, the synchronized update rotuine can be
    /// performed.
    fn set_timeout(&mut self, duration: Duration);
    /// Clear the current timeout.
    fn clear_timeout(&mut self);
    /// Returns whether a timeout is currently active and has not yet expired.
    fn pending_timeout(&self) -> bool;
}

#[derive(Debug, Default)]
struct ProcessorState<T: Timeout> {
    /// Last processed character for repetition.
    preceding_char: Option<char>,

    /// State for synchronized terminal updates.
    sync_state: SyncState<T>,

    /// State for XTGETTCAP requests.
    xtgettcap_state: XtgettcapState,
}

#[derive(Debug)]
struct SyncState<T: Timeout> {
    /// Expiration time of the synchronized update.
    timeout: T,

    /// Bytes read during the synchronized update.
    buffer: Vec<u8>,
}

#[derive(Debug, Default)]
struct XtgettcapState {
    /// Whether we're currently processing an XTGETTCAP request.
    active: bool,

    /// Buffer for collecting hex-encoded capability names.
    buffer: Vec<u8>,
}

impl<T: Timeout> Default for SyncState<T> {
    fn default() -> Self {
        Self {
            buffer: Vec::with_capacity(SYNC_BUFFER_SIZE),
            timeout: Default::default(),
        }
    }
}

#[derive(Default)]
pub struct StdSyncHandler {
    timeout: Option<Instant>,
}

impl StdSyncHandler {
    /// Synchronized update expiration time.
    #[inline]
    pub fn sync_timeout(&self) -> Option<Instant> {
        self.timeout
    }
}

impl Timeout for StdSyncHandler {
    #[inline]
    fn set_timeout(&mut self, duration: Duration) {
        self.timeout = Some(Instant::now() + duration);
    }

    #[inline]
    fn clear_timeout(&mut self) {
        self.timeout = None;
    }

    #[inline]
    fn pending_timeout(&self) -> bool {
        self.timeout.is_some()
    }
}

#[derive(Default)]
pub struct Processor<T: Timeout = StdSyncHandler> {
    state: ProcessorState<T>,
    parser: BatchedParser<1024>,
}

impl<T: Timeout> Processor<T> {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Synchronized update timeout.
    pub fn sync_timeout(&self) -> &T {
        &self.state.sync_state.timeout
    }

    /// Process a new byte from the PTY.
    #[inline]
    pub fn advance<H>(&mut self, handler: &mut H, bytes: &[u8])
    where
        H: Handler,
    {
        let mut processed = 0;
        while processed != bytes.len() {
            if self.state.sync_state.timeout.pending_timeout() {
                processed += self.advance_sync(handler, &bytes[processed..]);
            } else {
                let mut performer = Performer::new(&mut self.state, handler);
                processed += self
                    .parser
                    .advance_until_terminated(&mut performer, &bytes[processed..]);
            }
        }
    }

    /// Flush any pending batched input
    #[inline]
    pub fn flush<H>(&mut self, handler: &mut H)
    where
        H: Handler,
    {
        let mut performer = Performer::new(&mut self.state, handler);
        self.parser.flush(&mut performer);
    }

    /// End a synchronized update.
    pub fn stop_sync<H>(&mut self, handler: &mut H)
    where
        H: Handler,
    {
        self.stop_sync_internal(handler, None);
    }

    /// End a synchronized update.
    ///
    /// The `bsu_offset` parameter should be passed if the sync buffer contains
    /// a new BSU escape that is not part of the current synchronized
    /// update.
    fn stop_sync_internal<H>(&mut self, handler: &mut H, bsu_offset: Option<usize>)
    where
        H: Handler,
    {
        // Process all synchronized bytes.
        //
        // NOTE: We do not use `advance_until_terminated` here since BSU sequences are
        // processed automatically during the synchronized update.
        let buffer = mem::take(&mut self.state.sync_state.buffer);
        let offset = bsu_offset.unwrap_or(buffer.len());
        let mut performer = Performer::new(&mut self.state, handler);
        self.parser.advance(&mut performer, &buffer[..offset]);
        // Flush any pending batched input from synchronized processing
        self.parser.flush(&mut performer);
        self.state.sync_state.buffer = buffer;

        match bsu_offset {
            // Just clear processed bytes if there is a new BSU.
            //
            // NOTE: We do not need to re-process for a new ESU since the `advance_sync`
            // function checks for BSUs in reverse.
            Some(bsu_offset) => {
                let new_len = self.state.sync_state.buffer.len() - bsu_offset;
                self.state.sync_state.buffer.copy_within(bsu_offset.., 0);
                self.state.sync_state.buffer.truncate(new_len);
            }
            // Report mode and clear state if no new BSU is present.
            None => {
                handler.unset_private_mode(NamedPrivateMode::SyncUpdate.into());
                self.state.sync_state.timeout.clear_timeout();
                self.state.sync_state.buffer.clear();
            }
        }
    }

    /// Number of bytes in the synchronization buffer.
    #[inline]
    pub fn sync_bytes_count(&self) -> usize {
        self.state.sync_state.buffer.len()
    }

    /// Process a new byte during a synchronized update.
    ///
    /// Returns the number of bytes processed.
    #[cold]
    fn advance_sync<H>(&mut self, handler: &mut H, bytes: &[u8]) -> usize
    where
        H: Handler,
    {
        // Advance sync parser or stop sync if we'd exceed the maximum buffer size.
        if self.state.sync_state.buffer.len() + bytes.len() >= SYNC_BUFFER_SIZE - 1 {
            // Terminate the synchronized update.
            self.stop_sync_internal(handler, None);

            // Just parse the bytes normally.
            let mut performer = Performer::new(&mut self.state, handler);
            self.parser.advance_until_terminated(&mut performer, bytes)
        } else {
            self.state.sync_state.buffer.extend(bytes);
            self.advance_sync_csi(handler, bytes.len());
            bytes.len()
        }
    }

    /// Handle BSU/ESU CSI sequences during synchronized update.
    fn advance_sync_csi<H>(&mut self, handler: &mut H, new_bytes: usize)
    where
        H: Handler,
    {
        // Get constraints within which a new escape character might be relevant.
        let buffer_len = self.state.sync_state.buffer.len();
        let start_offset = (buffer_len - new_bytes).saturating_sub(SYNC_ESCAPE_LEN - 1);
        let end_offset = buffer_len.saturating_sub(SYNC_ESCAPE_LEN - 1);
        let search_buffer = &self.state.sync_state.buffer[start_offset..end_offset];

        // Search for termination/extension escapes in the added bytes.
        //
        // NOTE: It is technically legal to specify multiple private modes in the same
        // escape, but we only allow EXACTLY `\e[?2026h`/`\e[?2026l` to keep the parser
        // more simple.
        let mut bsu_offset = None;
        for index in memchr::memchr_iter(0x1B, search_buffer).rev() {
            let offset = start_offset + index;
            let escape = &self.state.sync_state.buffer[offset..offset + SYNC_ESCAPE_LEN];

            if escape == BSU_CSI {
                self.state
                    .sync_state
                    .timeout
                    .set_timeout(SYNC_UPDATE_TIMEOUT);
                bsu_offset = Some(offset);
            } else if escape == ESU_CSI {
                self.stop_sync_internal(handler, bsu_offset);
                break;
            }
        }
    }
}

struct Performer<'a, H: Handler, T: Timeout> {
    state: &'a mut ProcessorState<T>,
    handler: &'a mut H,
}

impl<'a, H: Handler + 'a, T: Timeout> Performer<'a, H, T> {
    /// Create a performer.
    #[inline]
    pub fn new<'b>(
        state: &'b mut ProcessorState<T>,
        handler: &'b mut H,
    ) -> Performer<'b, H, T> {
        Performer { state, handler }
    }
}

impl<U: Handler, T: Timeout> copa::Perform for Performer<'_, U, T> {
    fn print(&mut self, c: char) {
        self.handler.input(c);
        self.state.preceding_char = Some(c);
    }

    fn execute(&mut self, byte: u8) {
        tracing::trace!("[execute] {byte:04x}");

        match byte {
            C0::HT => self.handler.put_tab(1),
            C0::BS => self.handler.backspace(),
            C0::CR => self.handler.carriage_return(),
            C0::LF | C0::VT | C0::FF => self.handler.linefeed(),
            C0::BEL => self.handler.bell(),
            C0::SUB => self.handler.substitute(),
            C0::SI => self.handler.set_active_charset(CharsetIndex::G0),
            C0::SO => self.handler.set_active_charset(CharsetIndex::G1),
            _ => warn!("[unhandled] execute byte={byte:02x}"),
        }
    }

    fn hook(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        match (action, intermediates) {
            ('q', []) => {
                self.handler.sixel_graphic_start(params);
            }
            ('q', [b'+']) => {
                // XTGETTCAP request: DCS + q <hex-encoded-names> ST
                self.state.xtgettcap_state.active = true;
                self.state.xtgettcap_state.buffer.clear();
            }
            _ => debug!(
                "[unhandled hook] params={:?}, ints: {:?}, ignore: {:?}, action: {:?}",
                params, intermediates, ignore, action
            ),
        }
    }

    fn put(&mut self, byte: u8) {
        if self.handler.is_sixel_graphic_active() {
            if let Err(err) = self.handler.sixel_graphic_put(byte) {
                tracing::warn!("Failed to parse Sixel data: {}", err);
                self.handler.sixel_graphic_reset();
            }
        } else if self.state.xtgettcap_state.active {
            // Collect hex-encoded capability names for XTGETTCAP
            self.state.xtgettcap_state.buffer.push(byte);
        } else {
            debug!("[unhandled put] byte={:?}", byte);
        }
    }

    #[inline]
    fn unhook(&mut self) {
        if self.handler.is_sixel_graphic_active() {
            self.handler.sixel_graphic_finish();
        } else if self.state.xtgettcap_state.active {
            // Process XTGETTCAP request
            let response = process_xtgettcap_request(&self.state.xtgettcap_state.buffer);
            self.handler.xtgettcap_response(response);

            // Reset state
            self.state.xtgettcap_state.active = false;
            self.state.xtgettcap_state.buffer.clear();
        } else {
            debug!("[unhandled dcs_unhook]");
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        debug!("[osc_dispatch] params={params:?} bell_terminated={bell_terminated}");

        let terminator = if bell_terminated { "\x07" } else { "\x1b\\" };

        fn unhandled(params: &[&[u8]]) {
            let mut buf = String::new();
            for items in params {
                buf.push('[');
                for item in *items {
                    let _ = write!(buf, "{:?}", *item as char);
                }
                buf.push_str("],");
            }
            warn!("[unhandled osc_dispatch]: [{}] at line {}", &buf, line!());
        }

        if params.is_empty() || params[0].is_empty() {
            return;
        }

        match params[0] {
            // Set window title.
            b"0" | b"2" => {
                if params.len() >= 2 {
                    let title = params[1..]
                        .iter()
                        .flat_map(|x| simd_utf8::from_utf8_fast(x))
                        .collect::<Vec<&str>>()
                        .join(";")
                        .trim()
                        .to_owned();
                    self.handler.set_title(Some(title));
                    return;
                }
                unhandled(params);
            }

            // Set color index.
            b"4" => {
                if params.len() <= 1 || params.len().is_multiple_of(2) {
                    unhandled(params);
                    return;
                }

                for chunk in params[1..].chunks(2) {
                    let index = match parse_number(chunk[0]) {
                        Some(index) => index,
                        None => {
                            unhandled(params);
                            continue;
                        }
                    };

                    if let Some(c) = xparse_color(chunk[1]) {
                        self.handler.set_color(index as usize, c);
                    } else if chunk[1] == b"?" {
                        let prefix = format!("4;{index}");
                        self.handler.dynamic_color_sequence(
                            prefix,
                            index as usize,
                            terminator,
                        );
                    } else {
                        unhandled(params);
                    }
                }
            }

            // Inform current directory.
            b"7" => {
                if let Ok(s) = simd_utf8::from_utf8_fast(params[1]) {
                    if let Ok(url) = url::Url::parse(s) {
                        let path = url.path();

                        // NB the path coming from Url has a leading slash; must slice that off
                        // in windows.
                        #[cfg(windows)]
                        let path = &path[1..];

                        self.handler.set_current_directory(path.into());
                    }
                }
            }

            // Hyperlink.
            b"8" if params.len() > 2 => {
                let link_params = params[1];
                let uri = simd_utf8::from_utf8_fast(params[2]).unwrap_or_default();

                // The OSC 8 escape sequence must be stopped when getting an empty `uri`.
                if uri.is_empty() {
                    self.handler.set_hyperlink(None);
                    return;
                }

                // Link parameters are in format of `key1=value1:key2=value2`. Currently only key
                // `id` is defined.
                let id = link_params
                    .split(|&b| b == b':')
                    .find_map(|kv| kv.strip_prefix(b"id="))
                    .and_then(|kv| simd_utf8::from_utf8_fast(kv).ok());

                self.handler.set_hyperlink(Some(Hyperlink::new(id, uri)));
            }

            b"10" | b"11" | b"12" => {
                if params.len() >= 2 {
                    if let Some(mut dynamic_code) = parse_number(params[0]) {
                        for param in &params[1..] {
                            // 10 is the first dynamic color, also the foreground.
                            let offset = dynamic_code as usize - 10;
                            let index = NamedColor::Foreground as usize + offset;

                            // End of setting dynamic colors.
                            if index > NamedColor::Cursor as usize {
                                unhandled(params);
                                break;
                            }

                            if let Some(color) = xparse_color(param) {
                                self.handler.set_color(index, color);
                            } else if param == b"?" {
                                self.handler.dynamic_color_sequence(
                                    dynamic_code.to_string(),
                                    index,
                                    terminator,
                                );
                            } else {
                                unhandled(params);
                            }
                            dynamic_code += 1;
                        }
                        return;
                    }
                }
                unhandled(params);
            }

            // Set mouse cursor shape.
            b"22" if params.len() == 2 => {
                let shape = simd_utf8::from_utf8_lossy_fast(params[1]);
                match CursorIcon::from_str(&shape) {
                    Ok(cursor_icon) => self.handler.set_mouse_cursor_icon(cursor_icon),
                    Err(_) => {
                        debug!("[osc 22] unrecognized cursor icon shape: {shape:?}")
                    }
                }
            }

            // Set cursor style.
            b"50" => {
                if params.len() >= 2
                    && params[1].len() >= 13
                    && params[1][0..12] == *b"CursorShape="
                {
                    let shape = match params[1][12] as char {
                        '0' => CursorShape::Block,
                        '1' => CursorShape::Beam,
                        '2' => CursorShape::Underline,
                        _ => return unhandled(params),
                    };
                    self.handler.set_cursor_shape(shape);
                    return;
                }
                unhandled(params);
            }

            // Set clipboard.
            b"52" => {
                if params.len() < 3 {
                    return unhandled(params);
                }

                let clipboard = params[1].first().unwrap_or(&b'c');
                match params[2] {
                    b"?" => self.handler.clipboard_load(*clipboard, terminator),
                    base64 => self.handler.clipboard_store(*clipboard, base64),
                }
            }

            b"104" => {
                // Reset all color indexes when no parameters are given.
                if params.len() == 1 || params[1].is_empty() {
                    for i in 0..256 {
                        self.handler.reset_color(i);
                    }
                    return;
                }

                // Reset color indexes given as parameters.
                for param in &params[1..] {
                    match parse_number(param) {
                        Some(index) => self.handler.reset_color(index as usize),
                        None => unhandled(params),
                    }
                }
            }

            // Reset foreground color.
            b"110" => self.handler.reset_color(NamedColor::Foreground as usize),

            // Reset background color.
            b"111" => self.handler.reset_color(NamedColor::Background as usize),

            // Reset text cursor color.
            b"112" => self.handler.reset_color(NamedColor::Cursor as usize),

            // OSC 1337 is not necessarily only used by iTerm2 protocol
            // OSC 1337 is equal to xterm OSC 50
            b"1337" => {
                if let Some(graphic) = iterm2_image_protocol::parse(params) {
                    self.handler.insert_graphic(graphic, None);
                }
            }

            _ => unhandled(params),
        }
    }

    // Control Sequence Introducer
    // CSI is the two-character sequence ESCape left-bracket or the 8-bit
    // C1 code of 233 octal, 9B hex. CSI introduces a Control Sequence, which
    // continues until an alphabetic character is received.
    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        should_ignore: bool,
        action: char,
    ) {
        debug!("[csi_dispatch] {params:?} {action:?}");
        macro_rules! csi_unhandled {
            () => {{
                warn!(
                    "[csi_dispatch] params={params:#?}, intermediates={intermediates:?}, should_ignore={should_ignore:?}, action={action:?}"
                );
            }};
        }

        if should_ignore || intermediates.len() > 2 {
            // We only handle up to two intermediate bytes. I haven't seen any sequences that use
            // more than that.
            csi_unhandled!();
            return;
        }

        let mut params_iter = params.iter();
        let handler = &mut self.handler;

        let mut next_param_or = |default: u16| match params_iter.next() {
            Some(&[param, ..]) if param != 0 => param,
            _ => default,
        };

        match (action, intermediates) {
            ('@', []) => handler.insert_blank(next_param_or(1) as usize),
            ('A', []) => handler.move_up(next_param_or(1) as usize),
            ('B', []) | ('e', []) => handler.move_down(next_param_or(1) as usize),
            ('b', []) => {
                if let Some(c) = self.state.preceding_char {
                    for _ in 0..next_param_or(1) {
                        handler.input(c);
                    }
                } else {
                    warn!("tried to repeat with no preceding char");
                }
            }
            ('C', []) | ('a', []) => {
                handler.move_forward(Column(next_param_or(1) as usize))
            }
            ('c', intermediates) if next_param_or(0) == 0 => {
                handler.identify_terminal(intermediates.first().map(|&i| i as char))
            }
            ('D', []) => handler.move_backward(Column(next_param_or(1) as usize)),
            ('d', []) => handler.goto_line(Line(next_param_or(1) as i32 - 1)),
            ('E', []) => handler.move_down_and_cr(next_param_or(1) as usize),
            ('F', []) => handler.move_up_and_cr(next_param_or(1) as usize),
            ('G', []) | ('`', []) => {
                handler.goto_col(Column(next_param_or(1) as usize - 1))
            }
            ('W', [b'?']) if next_param_or(0) == 5 => handler.set_tabs(8),
            ('g', []) => {
                let mode = match next_param_or(0) {
                    0 => TabulationClearMode::Current,
                    3 => TabulationClearMode::All,
                    _ => {
                        csi_unhandled!();
                        return;
                    }
                };

                handler.clear_tabs(mode);
            }
            ('H', []) | ('f', []) => {
                let y = next_param_or(1) as i32;
                let x = next_param_or(1) as usize;
                handler.goto(Line(y - 1), Column(x - 1));
            }
            ('h', []) => {
                for param in params_iter.map(|param| param[0]) {
                    handler.set_mode(Mode::new(param))
                }
            }
            ('h', [b'?']) => {
                for param in params_iter.map(|param| param[0]) {
                    // Handle sync updates opaquely.
                    if param == NamedPrivateMode::SyncUpdate as u16 {
                        self.state
                            .sync_state
                            .timeout
                            .set_timeout(SYNC_UPDATE_TIMEOUT);
                    }

                    handler.set_private_mode(PrivateMode::new(param))
                }
            }
            ('I', []) => handler.move_forward_tabs(next_param_or(1)),
            ('J', []) => {
                let mode = match next_param_or(0) {
                    0 => ClearMode::Below,
                    1 => ClearMode::Above,
                    2 => ClearMode::All,
                    3 => ClearMode::Saved,
                    _ => {
                        csi_unhandled!();
                        return;
                    }
                };

                handler.clear_screen(mode);
            }
            ('K', []) => {
                let mode = match next_param_or(0) {
                    0 => LineClearMode::Right,
                    1 => LineClearMode::Left,
                    2 => LineClearMode::All,
                    _ => {
                        csi_unhandled!();
                        return;
                    }
                };

                handler.clear_line(mode);
            }
            ('L', []) => handler.insert_blank_lines(next_param_or(1) as usize),
            ('l', []) => {
                for param in params_iter.map(|param| param[0]) {
                    handler.unset_mode(Mode::new(param))
                }
            }
            ('l', [b'?']) => {
                for param in params_iter.map(|param| param[0]) {
                    handler.unset_private_mode(PrivateMode::new(param))
                }
            }
            ('M', []) => handler.delete_lines(next_param_or(1) as usize),
            ('m', []) => {
                if params.is_empty() {
                    handler.terminal_attribute(Attr::Reset);
                } else {
                    for attr in attrs_from_sgr_parameters(&mut params_iter) {
                        match attr {
                            Some(attr) => handler.terminal_attribute(attr),
                            None => csi_unhandled!(),
                        }
                    }
                }
            }
            ('n', []) => handler.device_status(next_param_or(0) as usize),
            ('P', []) => handler.delete_chars(next_param_or(1) as usize),
            ('p', [b'$']) => {
                let mode = next_param_or(0);
                handler.report_mode(Mode::new(mode));
            }
            ('p', [b'?', b'$']) => {
                let mode = next_param_or(0);
                handler.report_private_mode(PrivateMode::new(mode));
            }
            ('q', [b'>']) => {
                // XTVERSION (CSI > q) -- Query Terminal Version.
                if next_param_or(0) != 0 {
                    csi_unhandled!();
                    return;
                }
                handler.report_version();
            }
            ('q', [b' ']) => {
                // DECSCUSR (CSI SP q) -- Set Cursor Style.
                let cursor_style_id = next_param_or(0);
                let shape = match cursor_style_id {
                    0 => None,
                    1 | 2 => Some(CursorShape::Block),
                    3 | 4 => Some(CursorShape::Underline),
                    5 | 6 => Some(CursorShape::Beam),
                    _ => {
                        csi_unhandled!();
                        return;
                    }
                };

                handler.set_cursor_style(shape, cursor_style_id % 2 == 1);
            }
            ('r', []) => {
                let top = next_param_or(1) as usize;
                let bottom = params_iter
                    .next()
                    .map(|param| param[0] as usize)
                    .filter(|&param| param != 0);

                handler.set_scrolling_region(top, bottom);
            }
            ('S', []) => handler.scroll_up(next_param_or(1) as usize),
            ('S', [b'?']) => {
                handler.graphics_attribute(next_param_or(0), next_param_or(0))
            }
            ('s', []) => handler.save_cursor_position(),
            ('T', []) => handler.scroll_down(next_param_or(1) as usize),
            ('t', []) => match next_param_or(1) as usize {
                14 => handler.text_area_size_pixels(),
                16 => handler.cells_size_pixels(),
                18 => handler.text_area_size_chars(),
                22 => handler.push_title(),
                23 => handler.pop_title(),
                _ => csi_unhandled!(),
            },
            ('u', [b'?']) => handler.report_keyboard_mode(),
            ('u', [b'=']) => {
                let mode = KeyboardModes::from_bits_truncate(next_param_or(0) as u8);
                let behavior = match next_param_or(1) {
                    3 => KeyboardModesApplyBehavior::Difference,
                    2 => KeyboardModesApplyBehavior::Union,
                    // Default is replace.
                    _ => KeyboardModesApplyBehavior::Replace,
                };
                handler.set_keyboard_mode(mode, behavior);
            }
            ('u', [b'>']) => {
                let mode = KeyboardModes::from_bits_truncate(next_param_or(0) as u8);
                handler.push_keyboard_mode(mode);
            }
            ('u', [b'<']) => {
                // The default is 1.
                handler.pop_keyboard_modes(next_param_or(1));
            }
            ('u', []) => handler.restore_cursor_position(),
            ('X', []) => handler.erase_chars(Column(next_param_or(1) as usize)),
            ('Z', []) => handler.move_backward_tabs(next_param_or(1)),
            _ => csi_unhandled!(),
        };
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        macro_rules! unhandled {
            () => {{
                warn!(
                    "[unhandled] esc_dispatch ints={:?}, byte={:?} ({:02x})",
                    intermediates, byte as char, byte
                );
            }};
        }

        macro_rules! configure_charset {
            ($charset:path, $intermediates:expr) => {{
                let index: CharsetIndex = match $intermediates {
                    [b'('] => CharsetIndex::G0,
                    [b')'] => CharsetIndex::G1,
                    [b'*'] => CharsetIndex::G2,
                    [b'+'] => CharsetIndex::G3,
                    _ => {
                        unhandled!();
                        return;
                    }
                };
                self.handler.configure_charset(index, $charset)
            }};
        }

        match (byte, intermediates) {
            (b'B', intermediates) => {
                configure_charset!(StandardCharset::Ascii, intermediates)
            }
            (b'D', []) => self.handler.linefeed(),
            (b'E', []) => {
                self.handler.linefeed();
                self.handler.carriage_return();
            }
            (b'H', []) => self.handler.set_horizontal_tabstop(),
            (b'M', []) => self.handler.reverse_index(),
            (b'Z', []) => self.handler.identify_terminal(None),
            (b'c', []) => self.handler.reset_state(),
            (b'0', intermediates) => {
                configure_charset!(
                    StandardCharset::SpecialCharacterAndLineDrawing,
                    intermediates
                )
            }
            (b'7', []) => self.handler.save_cursor_position(),
            (b'8', [b'#']) => self.handler.decaln(),
            (b'8', []) => self.handler.restore_cursor_position(),
            (b'=', []) => self.handler.set_keypad_application_mode(),
            (b'>', []) => self.handler.unset_keypad_application_mode(),
            // String terminator, do nothing (parser handles as string terminator).
            (b'\\', []) => (),
            _ => unhandled!(),
        }
    }
}

#[inline]
fn attrs_from_sgr_parameters(params: &mut ParamsIter<'_>) -> Vec<Option<Attr>> {
    let mut attrs = Vec::with_capacity(params.size_hint().0);

    while let Some(param) = params.next() {
        let attr = match param {
            [0] => Some(Attr::Reset),
            [1] => Some(Attr::Bold),
            [2] => Some(Attr::Dim),
            [3] => Some(Attr::Italic),
            [4, 0] => Some(Attr::CancelUnderline),
            [4, 2] => Some(Attr::DoubleUnderline),
            [4, 3] => Some(Attr::Undercurl),
            [4, 4] => Some(Attr::DottedUnderline),
            [4, 5] => Some(Attr::DashedUnderline),
            [4, ..] => Some(Attr::Underline),
            [5] => Some(Attr::BlinkSlow),
            [6] => Some(Attr::BlinkFast),
            [7] => Some(Attr::Reverse),
            [8] => Some(Attr::Hidden),
            [9] => Some(Attr::Strike),
            [21] => Some(Attr::CancelBold),
            [22] => Some(Attr::CancelBoldDim),
            [23] => Some(Attr::CancelItalic),
            [24] => Some(Attr::CancelUnderline),
            [25] => Some(Attr::CancelBlink),
            [27] => Some(Attr::CancelReverse),
            [28] => Some(Attr::CancelHidden),
            [29] => Some(Attr::CancelStrike),
            [30] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::Black))),
            [31] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::Red))),
            [32] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::Green))),
            [33] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::Yellow))),
            [34] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::Blue))),
            [35] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::Magenta))),
            [36] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::Cyan))),
            [37] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::White))),
            [38] => {
                let mut iter = params.map(|param| param[0]);
                parse_sgr_color(&mut iter).map(Attr::Foreground)
            }
            [38, params @ ..] => handle_colon_rgb(params).map(Attr::Foreground),
            [39] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::Foreground))),
            [40] => Some(Attr::Background(AnsiColor::Named(NamedColor::Black))),
            [41] => Some(Attr::Background(AnsiColor::Named(NamedColor::Red))),
            [42] => Some(Attr::Background(AnsiColor::Named(NamedColor::Green))),
            [43] => Some(Attr::Background(AnsiColor::Named(NamedColor::Yellow))),
            [44] => Some(Attr::Background(AnsiColor::Named(NamedColor::Blue))),
            [45] => Some(Attr::Background(AnsiColor::Named(NamedColor::Magenta))),
            [46] => Some(Attr::Background(AnsiColor::Named(NamedColor::Cyan))),
            [47] => Some(Attr::Background(AnsiColor::Named(NamedColor::White))),
            [48] => {
                let mut iter = params.map(|param| param[0]);
                parse_sgr_color(&mut iter).map(Attr::Background)
            }
            [48, params @ ..] => handle_colon_rgb(params).map(Attr::Background),
            [49] => Some(Attr::Background(AnsiColor::Named(NamedColor::Background))),
            [58] => {
                let mut iter = params.map(|param| param[0]);
                parse_sgr_color(&mut iter).map(|color| Attr::UnderlineColor(Some(color)))
            }
            [58, params @ ..] => {
                handle_colon_rgb(params).map(|color| Attr::UnderlineColor(Some(color)))
            }
            [59] => Some(Attr::UnderlineColor(None)),
            [90] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::LightBlack))),
            [91] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::LightRed))),
            [92] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::LightGreen))),
            [93] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::LightYellow))),
            [94] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::LightBlue))),
            [95] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::LightMagenta))),
            [96] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::LightCyan))),
            [97] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::LightWhite))),
            [100] => Some(Attr::Background(AnsiColor::Named(NamedColor::LightBlack))),
            [101] => Some(Attr::Background(AnsiColor::Named(NamedColor::LightRed))),
            [102] => Some(Attr::Background(AnsiColor::Named(NamedColor::LightGreen))),
            [103] => Some(Attr::Background(AnsiColor::Named(NamedColor::LightYellow))),
            [104] => Some(Attr::Background(AnsiColor::Named(NamedColor::LightBlue))),
            [105] => Some(Attr::Background(AnsiColor::Named(NamedColor::LightMagenta))),
            [106] => Some(Attr::Background(AnsiColor::Named(NamedColor::LightCyan))),
            [107] => Some(Attr::Background(AnsiColor::Named(NamedColor::LightWhite))),
            _ => None,
        };
        attrs.push(attr);
    }

    attrs
}

/// Process XTGETTCAP request and return DCS response.
fn process_xtgettcap_request(buffer: &[u8]) -> String {
    // Decode hex-encoded capability name
    let capability_name = match decode_hex_string(buffer) {
        Ok(name) => name,
        Err(_) => {
            // Invalid hex encoding - return error response
            return "\x1bP0+r\x1b\\".to_string();
        }
    };

    if let Some(value) = get_termcap_capability(&capability_name) {
        // Encode both name and value in hex
        let hex_name = encode_hex_string(&capability_name);
        let hex_value = encode_hex_string(&value);
        format!("\x1bP1+r{hex_name}={hex_value}\x1b\\")
    } else {
        // Invalid capability name - return error response
        "\x1bP0+r\x1b\\".to_string()
    }
}

/// Decode hex-encoded string (2 hex digits per character).
fn decode_hex_string(hex_bytes: &[u8]) -> Result<String, &'static str> {
    if !hex_bytes.len().is_multiple_of(2) {
        return Err("Invalid hex string length");
    }

    let mut result = Vec::new();
    for chunk in hex_bytes.chunks(2) {
        let hex_str = std::str::from_utf8(chunk).map_err(|_| "Invalid UTF-8")?;
        let byte = u8::from_str_radix(hex_str, 16).map_err(|_| "Invalid hex digit")?;
        result.push(byte);
    }

    String::from_utf8(result).map_err(|_| "Invalid UTF-8 in decoded string")
}

/// Encode string as hex (2 hex digits per character).
fn encode_hex_string(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02X}")).collect()
}

/// Get termcap/terminfo capability value for Rio terminal.
/// Based on misc/rio.termcap and misc/rio.terminfo files.
fn get_termcap_capability(name: &str) -> Option<String> {
    match name {
        // Terminal name
        "TN" | "name" => Some("rio".to_string()),

        // Colors capability - from terminfo: colors#0x100 (256), pairs#0x7FFF
        "Co" | "colors" => Some("256".to_string()),
        "pa" | "pairs" => Some("32767".to_string()),

        // RGB/direct color support - from terminfo: ccc capability
        "RGB" => Some("8/8/8".to_string()),
        "ccc" => Some("".to_string()),

        // Terminal dimensions - from termcap: co#80:li#24
        "co" | "cols" => Some("80".to_string()),
        "li" | "lines" => Some("24".to_string()),
        "it" => Some("8".to_string()), // Tab stops

        // Boolean capabilities from terminfo
        "OTbs" | "bs" => Some("".to_string()),  // Backspace overstrike
        "am" => Some("".to_string()),    // Automatic margins
        "bce" => Some("".to_string()),   // Background color erase
        "km" => Some("".to_string()),    // Has meta key
        "mir" => Some("".to_string()),   // Safe to move in insert mode
        "msgr" => Some("".to_string()),  // Safe to move in standout mode
        "xenl" | "xn" => Some("".to_string()), // Newline ignored after 80 cols
        "AX" => Some("".to_string()),    // Default color pair is white on black
        "XT" => Some("".to_string()),    // Supports title setting
        "XF" => Some("".to_string()),    // Xterm focus events
        "hs" => Some("".to_string()),    // Has status line
        "ms" => Some("".to_string()),    // Safe to move in standout mode
        "mi" => Some("".to_string()),    // Safe to move in insert mode
        "mc5i" => Some("".to_string()),  // Printer won't echo on screen
        "npc" => Some("".to_string()),   // No pad character

        // Image protocol support
        "sixel" => Some("".to_string()), // Sixel graphics support
        "iterm2" => Some("".to_string()), // iTerm2 image protocol support

        // Cursor movement - from terminfo
        "cup" | "cm" => Some("\\E[%i%p1%d;%p2%dH".to_string()),
        "cuu1" | "up" => Some("\\E[A".to_string()),
        "cud1" | "do" => Some("\\n".to_string()),
        "cuf1" | "nd" => Some("\\E[C".to_string()),
        "cub1" | "le" => Some("^H".to_string()),
        "home" | "ho" => Some("\\E[H".to_string()),
        "cuu" | "UP" => Some("\\E[%p1%dA".to_string()),
        "cud" | "DO" => Some("\\E[%p1%dB".to_string()),
        "cuf" | "RI" => Some("\\E[%p1%dC".to_string()),
        "cub" | "LE" => Some("\\E[%p1%dD".to_string()),
        "hpa" => Some("\\E[%i%p1%dG".to_string()),
        "vpa" => Some("\\E[%i%p1%dd".to_string()),

        // Clear operations
        "clear" | "cl" => Some("\\E[H\\E[2J".to_string()),
        "el" | "ce" => Some("\\E[K".to_string()),
        "ed" | "cd" => Some("\\E[J".to_string()),
        "el1" => Some("\\E[1K".to_string()),
        "E3" => Some("\\E[3J".to_string()),

        // Colors and attributes
        "setaf" => Some("\\E[%?%p1%{8}%<%t3%p1%d%e%p1%{16}%<%t9%p1%{8}%-%d%e38;5;%p1%d%;m".to_string()),
        "setab" => Some("\\E[%?%p1%{8}%<%t4%p1%d%e%p1%{16}%<%t10%p1%{8}%-%d%e48;5;%p1%d%;m".to_string()),
        "setf" => Some("\\E[3%?%p1%{1}%=%t4%e%p1%{3}%=%t6%e%p1%{4}%=%t1%e%p1%{6}%=%t3%e%p1%d%;m".to_string()),
        "setb" => Some("\\E[4%?%p1%{1}%=%t4%e%p1%{3}%=%t6%e%p1%{4}%=%t1%e%p1%{6}%=%t3%e%p1%d%;m".to_string()),
        "op" => Some("\\E[39;49m".to_string()),
        "oc" => Some("\\E]104\\007".to_string()),
        "initc" => Some("\\E]4;%p1%d;rgb\\:%p2%{255}%*%{1000}%/%2.2X/%p3%{255}%*%{1000}%/%2.2X/%p4%{255}%*%{1000}%/%2.2X\\E\\\\".to_string()),

        // Text attributes
        "bold" | "md" => Some("\\E[1m".to_string()),
        "dim" | "mh" => Some("\\E[2m".to_string()),
        "smul" | "us" => Some("\\E[4m".to_string()),
        "rmul" | "ue" => Some("\\E[24m".to_string()),
        "rev" | "mr" => Some("\\E[7m".to_string()),
        "smso" | "so" => Some("\\E[7m".to_string()),
        "rmso" | "se" => Some("\\E[27m".to_string()),
        "invis" => Some("\\E[8m".to_string()),
        "blink" | "mb" => Some("\\E[5m".to_string()),
        "sitm" => Some("\\E[3m".to_string()),
        "ritm" => Some("\\E[23m".to_string()),
        "smxx" => Some("\\E[9m".to_string()),
        "rmxx" => Some("\\E[29m".to_string()),
        "sgr0" | "me" => Some("\\E(B\\E[m".to_string()),
        "sgr" => Some("%?%p9%t\\E(0%e\\E(B%;\\E[0%?%p6%t;1%;%?%p5%t;2%;%?%p2%t;4%;%?%p1%p3%|%t;7%;%?%p4%t;5%;%?%p7%t;8%;m".to_string()),

        // Character sets
        "smacs" | "as" => Some("\\E(0".to_string()),
        "rmacs" | "ae" => Some("\\E(B".to_string()),
        "acsc" => Some("``aaffggiijjkkllmmnnooppqqrrssttuuvvwwxxyyzz{{||}}~~".to_string()),

        // Insert/delete operations
        "ich" | "IC" => Some("\\E[%p1%d@".to_string()),
        "dch1" | "dc" => Some("\\E[P".to_string()),
        "dch" | "DC" => Some("\\E[%p1%dP".to_string()),
        "il1" | "al" => Some("\\E[L".to_string()),
        "il" | "AL" => Some("\\E[%p1%dL".to_string()),
        "dl1" => Some("\\E[M".to_string()),
        "dl" | "DL" => Some("\\E[%p1%dM".to_string()),
        "ech" | "ec" => Some("\\E[%p1%dX".to_string()),

        // Scrolling
        "csr" | "cs" => Some("\\E[%i%p1%d;%p2%dr".to_string()),
        "ri" | "sr" => Some("\\EM".to_string()),
        "ind" | "sf" => Some("\\n".to_string()),
        "indn" | "SF" => Some("\\E[%p1%dS".to_string()),
        "rin" | "SR" => Some("\\E[%p1%dT".to_string()),

        // Cursor visibility
        "civis" | "vi" => Some("\\E[?25l".to_string()),
        "cnorm" | "ve" => Some("\\E[?12l\\E[?25h".to_string()),
        "cvvis" | "vs" => Some("\\E[?12;25h".to_string()),

        // Cursor styles
        "Ss" => Some("\\E[%p1%d q".to_string()),
        "Se" => Some("\\E[0 q".to_string()),
        "Cs" => Some("\\E]12;%p1%s\\007".to_string()),
        "Cr" => Some("\\E]112\\007".to_string()),

        // Keypad and modes
        "smkx" | "ks" => Some("\\E[?1h\\E=".to_string()),
        "rmkx" | "ke" => Some("\\E[?1l\\E>".to_string()),
        "smir" | "im" => Some("\\E[4h".to_string()),
        "rmir" | "ei" => Some("\\E[4l".to_string()),
        "smam" => Some("\\E[?7h".to_string()),
        "rmam" => Some("\\E[?7l".to_string()),
        "smm" => Some("\\E[?1034h".to_string()),
        "rmm" => Some("\\E[?1034l".to_string()),

        // Alternate screen
        "smcup" | "ti" => Some("\\E[?1049h\\E[22;0;0t".to_string()),
        "rmcup" | "te" => Some("\\E[?1049l\\E[23;0;0t".to_string()),

        // Save/restore cursor
        "sc" => Some("\\E7".to_string()),
        "rc" => Some("\\E8".to_string()),

        // Tabs
        "ht" | "ta" => Some("^I".to_string()),
        "hts" | "st" => Some("\\EH".to_string()),
        "tbc" | "ct" => Some("\\E[3g".to_string()),
        "cbt" | "bt" => Some("\\E[Z".to_string()),

        // Bell and flash
        "bel" | "bl" => Some("^G".to_string()),
        "flash" | "vb" => Some("\\E[?5h$<100/>\\E[?5l".to_string()),

        // Status line
        "tsl" | "ts" => Some("\\E]2;".to_string()),
        "fsl" | "fs" => Some("^G".to_string()),
        "dsl" | "ds" => Some("\\E]2;\\007".to_string()),

        // Function keys
        "kf1" | "k1" => Some("\\EOP".to_string()),
        "kf2" | "k2" => Some("\\EOQ".to_string()),
        "kf3" | "k3" => Some("\\EOR".to_string()),
        "kf4" | "k4" => Some("\\EOS".to_string()),
        "kf5" | "k5" => Some("\\E[15~".to_string()),
        "kf6" | "k6" => Some("\\E[17~".to_string()),
        "kf7" | "k7" => Some("\\E[18~".to_string()),
        "kf8" | "k8" => Some("\\E[19~".to_string()),
        "kf9" | "k9" => Some("\\E[20~".to_string()),
        "kf10" => Some("\\E[21~".to_string()),
        "kf11" => Some("\\E[23~".to_string()),
        "kf12" => Some("\\E[24~".to_string()),

        // Extended function keys with modifiers
        "kf13" => Some("\\E[1;2P".to_string()),
        "kf14" => Some("\\E[1;2Q".to_string()),
        "kf15" => Some("\\E[1;2R".to_string()),
        "kf16" => Some("\\E[1;2S".to_string()),
        "kf17" => Some("\\E[15;2~".to_string()),
        "kf18" => Some("\\E[17;2~".to_string()),
        "kf19" => Some("\\E[18;2~".to_string()),
        "kf20" => Some("\\E[19;2~".to_string()),
        "kf21" => Some("\\E[20;2~".to_string()),
        "kf22" => Some("\\E[21;2~".to_string()),
        "kf23" => Some("\\E[23;2~".to_string()),
        "kf24" => Some("\\E[24;2~".to_string()),
        "kf25" => Some("\\E[1;5P".to_string()),
        "kf26" => Some("\\E[1;5Q".to_string()),
        "kf27" => Some("\\E[1;5R".to_string()),
        "kf28" => Some("\\E[1;5S".to_string()),
        "kf29" => Some("\\E[15;5~".to_string()),
        "kf30" => Some("\\E[17;5~".to_string()),
        "kf31" => Some("\\E[18;5~".to_string()),
        "kf32" => Some("\\E[19;5~".to_string()),
        "kf33" => Some("\\E[20;5~".to_string()),
        "kf34" => Some("\\E[21;5~".to_string()),
        "kf35" => Some("\\E[23;5~".to_string()),
        "kf36" => Some("\\E[24;5~".to_string()),
        "kf37" => Some("\\E[1;6P".to_string()),
        "kf38" => Some("\\E[1;6Q".to_string()),
        "kf39" => Some("\\E[1;6R".to_string()),
        "kf40" => Some("\\E[1;6S".to_string()),
        "kf41" => Some("\\E[15;6~".to_string()),
        "kf42" => Some("\\E[17;6~".to_string()),
        "kf43" => Some("\\E[18;6~".to_string()),
        "kf44" => Some("\\E[19;6~".to_string()),
        "kf45" => Some("\\E[20;6~".to_string()),
        "kf46" => Some("\\E[21;6~".to_string()),
        "kf47" => Some("\\E[23;6~".to_string()),
        "kf48" => Some("\\E[24;6~".to_string()),
        "kf49" => Some("\\E[1;3P".to_string()),
        "kf50" => Some("\\E[1;3Q".to_string()),
        "kf51" => Some("\\E[1;3R".to_string()),
        "kf52" => Some("\\E[1;3S".to_string()),
        "kf53" => Some("\\E[15;3~".to_string()),
        "kf54" => Some("\\E[17;3~".to_string()),
        "kf55" => Some("\\E[18;3~".to_string()),
        "kf56" => Some("\\E[19;3~".to_string()),
        "kf57" => Some("\\E[20;3~".to_string()),
        "kf58" => Some("\\E[21;3~".to_string()),
        "kf59" => Some("\\E[23;3~".to_string()),
        "kf60" => Some("\\E[24;3~".to_string()),
        "kf61" => Some("\\E[1;4P".to_string()),
        "kf62" => Some("\\E[1;4Q".to_string()),
        "kf63" => Some("\\E[1;4R".to_string()),

        // Arrow keys
        "kcuu1" | "ku" => Some("\\EOA".to_string()),
        "kcud1" | "kd" => Some("\\EOB".to_string()),
        "kcuf1" | "kr" => Some("\\EOC".to_string()),
        "kcub1" | "kl" => Some("\\EOD".to_string()),

        // Navigation keys
        "khome" | "kh" => Some("\\EOH".to_string()),
        "kend" => Some("\\EOF".to_string()),
        "kbs" | "kb" => Some("\x7f".to_string()),
        "kdch1" | "kD" => Some("\\E[3~".to_string()),
        "kich1" | "kI" => Some("\\E[2~".to_string()),
        "knp" | "kN" => Some("\\E[6~".to_string()),
        "kpp" | "kP" => Some("\\E[5~".to_string()),
        "kb2" => Some("\\EOE".to_string()),
        "kcbt" => Some("\\E[Z".to_string()),
        "kent" => Some("\\EOM".to_string()),

        // Modified arrow keys
        "kLFT" => Some("\\E[1;2D".to_string()),
        "kRIT" => Some("\\E[1;2C".to_string()),
        "kind" => Some("\\E[1;2B".to_string()),
        "kri" => Some("\\E[1;2A".to_string()),
        "kDN" => Some("\\E[1;2B".to_string()),
        "kUP" => Some("\\E[1;2A".to_string()),

        // Arrow keys with Alt modifier
        "kDN3" => Some("\\E[1;3B".to_string()),
        "kLFT3" => Some("\\E[1;3D".to_string()),
        "kRIT3" => Some("\\E[1;3C".to_string()),
        "kUP3" => Some("\\E[1;3A".to_string()),

        // Arrow keys with Shift+Alt modifier
        "kDN4" => Some("\\E[1;4B".to_string()),
        "kLFT4" => Some("\\E[1;4D".to_string()),
        "kRIT4" => Some("\\E[1;4C".to_string()),
        "kUP4" => Some("\\E[1;4A".to_string()),

        // Arrow keys with Ctrl modifier
        "kDN5" => Some("\\E[1;5B".to_string()),
        "kLFT5" => Some("\\E[1;5D".to_string()),
        "kRIT5" => Some("\\E[1;5C".to_string()),
        "kUP5" => Some("\\E[1;5A".to_string()),

        // Arrow keys with Ctrl+Shift modifier
        "kDN6" => Some("\\E[1;6B".to_string()),
        "kLFT6" => Some("\\E[1;6D".to_string()),
        "kRIT6" => Some("\\E[1;6C".to_string()),
        "kUP6" => Some("\\E[1;6A".to_string()),

        // Arrow keys with Ctrl+Alt modifier
        "kDN7" => Some("\\E[1;7B".to_string()),
        "kLFT7" => Some("\\E[1;7D".to_string()),
        "kRIT7" => Some("\\E[1;7C".to_string()),
        "kUP7" => Some("\\E[1;7A".to_string()),

        // Modified navigation keys
        "kDC" => Some("\\E[3;2~".to_string()),
        "kEND" => Some("\\E[1;2F".to_string()),
        "kHOM" => Some("\\E[1;2H".to_string()),
        "kIC" => Some("\\E[2;2~".to_string()),
        "kNXT" => Some("\\E[6;2~".to_string()),
        "kPRV" => Some("\\E[5;2~".to_string()),

        // Navigation keys with Alt modifier
        "kDC3" => Some("\\E[3;3~".to_string()),
        "kEND3" => Some("\\E[1;3F".to_string()),
        "kHOM3" => Some("\\E[1;3H".to_string()),
        "kIC3" => Some("\\E[2;3~".to_string()),
        "kNXT3" => Some("\\E[6;3~".to_string()),
        "kPRV3" => Some("\\E[5;3~".to_string()),

        // Navigation keys with Shift+Alt modifier
        "kDC4" => Some("\\E[3;4~".to_string()),
        "kEND4" => Some("\\E[1;4F".to_string()),
        "kHOM4" => Some("\\E[1;4H".to_string()),
        "kIC4" => Some("\\E[2;4~".to_string()),
        "kNXT4" => Some("\\E[6;4~".to_string()),
        "kPRV4" => Some("\\E[5;4~".to_string()),

        // Navigation keys with Ctrl modifier
        "kDC5" => Some("\\E[3;5~".to_string()),
        "kEND5" => Some("\\E[1;5F".to_string()),
        "kHOM5" => Some("\\E[1;5H".to_string()),
        "kIC5" => Some("\\E[2;5~".to_string()),
        "kNXT5" => Some("\\E[6;5~".to_string()),
        "kPRV5" => Some("\\E[5;5~".to_string()),

        // Navigation keys with Ctrl+Shift modifier
        "kDC6" => Some("\\E[3;6~".to_string()),
        "kEND6" => Some("\\E[1;6F".to_string()),
        "kHOM6" => Some("\\E[1;6H".to_string()),
        "kIC6" => Some("\\E[2;6~".to_string()),
        "kNXT6" => Some("\\E[6;6~".to_string()),
        "kPRV6" => Some("\\E[5;6~".to_string()),

        // Navigation keys with Ctrl+Alt modifier
        "kDC7" => Some("\\E[3;7~".to_string()),
        "kEND7" => Some("\\E[1;7F".to_string()),
        "kHOM7" => Some("\\E[1;7H".to_string()),
        "kIC7" => Some("\\E[2;7~".to_string()),
        "kNXT7" => Some("\\E[6;7~".to_string()),
        "kPRV7" => Some("\\E[5;7~".to_string()),

        // Mouse
        "kmous" => Some("\\E[M".to_string()),

        // Memory operations
        "meml" => Some("\\El".to_string()),
        "memu" => Some("\\Em".to_string()),

        // Printing
        "mc0" => Some("\\E[i".to_string()),
        "mc4" => Some("\\E[4i".to_string()),
        "mc5" => Some("\\E[5i".to_string()),

        // Reset sequences
        "rs1" => Some("\\Ec\\E]104\\007".to_string()),
        "rs2" => Some("\\E[!p\\E[?3;4l\\E[4l\\E>".to_string()),
        "is2" => Some("\\E[!p\\E[?3;4l\\E[4l\\E>".to_string()),

        // Device control
        "u6" => Some("\\E[%i%d;%dR".to_string()),
        "u7" => Some("\\E[6n".to_string()),
        "u8" => Some("\\E[?%[;0123456789]c".to_string()),
        "u9" => Some("\\E[c".to_string()),

        // Repeat character
        "rep" => Some("%p1%c\\E[%p2%{1}%-%db".to_string()),

        // Extended underline
        "Smulx" => Some("\\E[4\\:%p1%dm".to_string()),

        // Synchronized output
        "Sync" => Some("\\EP=%p1%ds\\E\\\\".to_string()),

        // Focus events
        "kxIN" => Some("\\E[I".to_string()),
        "kxOUT" => Some("\\E[O".to_string()),

        // Bracketed paste
        "BE" => Some("\\E[?2004h".to_string()),
        "BD" => Some("\\E[?2004l".to_string()),
        "PS" => Some("\\E[200~".to_string()),
        "PE" => Some("\\E[201~".to_string()),

        // Clipboard
        "Ms" => Some("\\E]52;%p1%s;%p2%s\\007".to_string()),

        // Carriage return
        "cr" => Some("\\r".to_string()),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_encoding() {
        assert_eq!(encode_hex_string("TN"), "544E");
        assert_eq!(encode_hex_string("Co"), "436F");
        assert_eq!(encode_hex_string("RGB"), "524742");
    }

    #[test]
    fn test_hex_decoding() {
        assert_eq!(decode_hex_string(b"544E").unwrap(), "TN");
        assert_eq!(decode_hex_string(b"436F").unwrap(), "Co");
        assert_eq!(decode_hex_string(b"524742").unwrap(), "RGB");
    }

    #[test]
    fn test_xtgettcap_processing() {
        // Test terminal name query
        let response = process_xtgettcap_request(b"544E");
        assert!(response.starts_with("\x1bP1+r"));
        assert!(response.contains("544E="));
        assert!(response.ends_with("\x1b\\"));

        // Test colors query
        let response = process_xtgettcap_request(b"436F");
        assert!(response.starts_with("\x1bP1+r"));
        assert!(response.contains("436F="));

        // Test invalid capability
        let response = process_xtgettcap_request(b"5858"); // "XX"
        assert_eq!(response, "\x1bP0+r\x1b\\");

        // Test invalid hex
        let response = process_xtgettcap_request(b"ZZ");
        assert_eq!(response, "\x1bP0+r\x1b\\");
    }

    #[test]
    fn test_single_capability_requests() {
        // Test terminal name
        let response = process_xtgettcap_request(b"544E"); // "TN"
        assert_eq!(response, "\x1bP1+r544E=72696F\x1b\\");

        // Test colors capability
        let response = process_xtgettcap_request(b"436F"); // "Co"
        assert_eq!(response, "\x1bP1+r436F=323536\x1b\\");

        // Test RGB capability
        let response = process_xtgettcap_request(b"524742"); // "RGB"
        assert_eq!(response, "\x1bP1+r524742=382F382F38\x1b\\");

        // Test invalid capability
        let response = process_xtgettcap_request(b"5858"); // "XX"
        assert_eq!(response, "\x1bP0+r\x1b\\");
    }

    #[test]
    fn test_capability_lookup() {
        assert_eq!(get_termcap_capability("TN"), Some("rio".to_string()));
        assert_eq!(get_termcap_capability("Co"), Some("256".to_string()));
        assert_eq!(get_termcap_capability("RGB"), Some("8/8/8".to_string()));
        assert_eq!(get_termcap_capability("invalid"), None);
    }

    #[test]
    fn test_extended_capabilities() {
        // Test extended function keys
        assert_eq!(get_termcap_capability("kf13"), Some("\\E[1;2P".to_string()));
        assert_eq!(get_termcap_capability("kf25"), Some("\\E[1;5P".to_string()));

        // Test modified arrow keys
        assert_eq!(get_termcap_capability("kLFT"), Some("\\E[1;2D".to_string()));
        assert_eq!(get_termcap_capability("kUP3"), Some("\\E[1;3A".to_string()));

        // Test modified navigation keys
        assert_eq!(get_termcap_capability("kDC5"), Some("\\E[3;5~".to_string()));
        assert_eq!(
            get_termcap_capability("kHOM7"),
            Some("\\E[1;7H".to_string())
        );

        // Test updated reset sequence
        assert_eq!(
            get_termcap_capability("rs1"),
            Some("\\Ec\\E]104\\007".to_string())
        );

        // Test image protocol capabilities
        assert_eq!(get_termcap_capability("sixel"), Some("".to_string()));
        assert_eq!(get_termcap_capability("iterm2"), Some("".to_string()));
    }
}
