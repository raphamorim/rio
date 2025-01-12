use crate::ansi::iterm2_image_protocol;
use crate::ansi::CursorShape;
use crate::ansi::{sixel, KeyboardModes, KeyboardModesApplyBehavior};
use crate::config::colors::{AnsiColor, ColorRgb, NamedColor};
use crate::crosswords::pos::{CharsetIndex, Column, Line, StandardCharset};
use crate::crosswords::square::Hyperlink;
use cursor_icon::CursorIcon;
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
    let colors = std::str::from_utf8(color)
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
        let col = usize::from_str_radix(std::str::from_utf8(slice).ok()?, 16).ok()? << 4;
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
}

#[derive(Debug, Default)]
struct ProcessorState {
    /// Last processed character for repetition.
    preceding_char: Option<char>,

    /// State for synchronized terminal updates.
    sync_state: SyncState,
}

#[derive(Debug)]
struct SyncState {
    /// Expiration time of the synchronized update.
    timeout: Option<Instant>,

    /// Bytes read during the synchronized update.
    buffer: Vec<u8>,
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            buffer: Vec::with_capacity(SYNC_BUFFER_SIZE),
            timeout: None,
        }
    }
}

#[derive(Default)]
pub struct ParserProcessor {
    state: ProcessorState,
    parser: copa::Parser,
}

impl ParserProcessor {
    #[inline]
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a new byte from the PTY.
    #[inline]
    pub fn advance<H>(&mut self, handler: &mut H, byte: u8)
    where
        H: Handler,
    {
        if self.state.sync_state.timeout.is_none() {
            let mut performer = Performer::new(&mut self.state, handler);
            self.parser.advance(&mut performer, byte);
        } else {
            self.advance_sync(handler, byte);
        }
    }

    /// End a synchronized update.
    pub fn stop_sync<H>(&mut self, handler: &mut H)
    where
        H: Handler,
    {
        // Process all synchronized bytes.
        for i in 0..self.state.sync_state.buffer.len() {
            let byte = self.state.sync_state.buffer[i];
            let mut performer = Performer::new(&mut self.state, handler);
            self.parser.advance(&mut performer, byte);
        }

        // Report that update ended, since we could end due to timeout.
        handler.unset_private_mode(NamedPrivateMode::SyncUpdate.into());
        // Resetting state after processing makes sure we don't interpret buffered sync escapes.
        self.state.sync_state.buffer.clear();
        self.state.sync_state.timeout = None;
    }

    /// Synchronized update expiration time.
    #[inline]
    pub fn sync_timeout(&self) -> Option<&Instant> {
        self.state.sync_state.timeout.as_ref()
    }

    /// Number of bytes in the synchronization buffer.
    #[inline]
    pub fn sync_bytes_count(&self) -> usize {
        self.state.sync_state.buffer.len()
    }

    /// Process a new byte during a synchronized update.
    #[cold]
    fn advance_sync<H>(&mut self, handler: &mut H, byte: u8)
    where
        H: Handler,
    {
        self.state.sync_state.buffer.push(byte);

        // Handle sync CSI escape sequences.
        self.advance_sync_csi(handler);
    }

    /// Handle BSU/ESU CSI sequences during synchronized update.
    fn advance_sync_csi<H>(&mut self, handler: &mut H)
    where
        H: Handler,
    {
        // Get the last few bytes for comparison.
        let len = self.state.sync_state.buffer.len();
        let offset = len.saturating_sub(SYNC_ESCAPE_LEN);
        let end = &self.state.sync_state.buffer[offset..];

        // NOTE: It is technically legal to specify multiple private modes in the same
        // escape, but we only allow EXACTLY `\e[?2026h`/`\e[?2026l` to keep the parser
        // reasonable.
        //
        // Check for extension/termination of the synchronized update.
        if end == BSU_CSI {
            self.state.sync_state.timeout = Some(Instant::now() + SYNC_UPDATE_TIMEOUT);
        } else if end == ESU_CSI || len >= SYNC_BUFFER_SIZE - 1 {
            self.stop_sync(handler);
        }
    }
}

struct Performer<'a, H: Handler> {
    state: &'a mut ProcessorState,
    handler: &'a mut H,
}

impl<'a, H: Handler + 'a> Performer<'a, H> {
    /// Create a performer.
    #[inline]
    pub fn new<'b>(
        state: &'b mut ProcessorState,
        handler: &'b mut H,
    ) -> Performer<'b, H> {
        Performer { state, handler }
    }
}

impl<U: Handler> copa::Perform for Performer<'_, U> {
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
        } else {
            debug!("[unhandled put] byte={:?}", byte);
        }
    }

    #[inline]
    fn unhook(&mut self) {
        if self.handler.is_sixel_graphic_active() {
            self.handler.sixel_graphic_finish();
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
                        .flat_map(|x| std::str::from_utf8(x))
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
                if params.len() <= 1 || params.len() % 2 == 0 {
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
                if let Ok(s) = std::str::from_utf8(params[1]) {
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
                let uri = std::str::from_utf8(params[2]).unwrap_or_default();

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
                    .and_then(|kv| std::str::from_utf8(kv).ok());

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
                let shape = String::from_utf8_lossy(params[1]);
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

        if should_ignore || intermediates.len() > 1 {
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
                        self.state.sync_state.timeout =
                            Some(Instant::now() + SYNC_UPDATE_TIMEOUT);
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
            ('q', [b' ']) => {
                // DECSCUSR (CSI Ps SP q) -- Set Cursor Style.
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
