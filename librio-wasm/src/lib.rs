//! librio's JS ABI: `librio` (without its `pty` feature) compiled to
//! wasm32-unknown-unknown and exposed through wasm-bindgen for the
//! `rioterm` npm package.
//!
//! There is no PTY in a browser, so the host owns the transport: child
//! output comes in through [`RioTerm::feed`], and bytes the terminal wants
//! delivered to the child (key encodings, mouse reports, DA responses)
//! come back out through the `output` callback: wire it to a WebSocket,
//! an ssh bridge, or an in-page demo shell.
//!
//! Delegate events are queued while librio holds the terminal lock and
//! drained into JS callbacks after each entry point returns, so a callback
//! can safely call back into this object.

// The crate is empty on native targets: the delegate is deliberately
// single-threaded (RefCell + JS callbacks), which the native
// SurfaceDelegate bound rightly rejects. Native embedders use librio.
#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::sync::Arc;

use librio::{
    Action, AnsiColor, ClipboardType, Engine, Key, KeyAction, KeyEvent, Modifiers,
    NamedColor, RenderState, SelectionKind, Side, Surface, SurfaceDelegate, SurfaceDesc,
    SurfaceId,
};
use wasm_bindgen::prelude::*;

// Key tags, matching librio's C ABI (`RIO_KEY_*`) so the two stay one
// vocabulary across Swift, C, and JS embedders.
pub const KEY_CHAR: u32 = 0;
pub const KEY_ENTER: u32 = 1;
pub const KEY_TAB: u32 = 2;
pub const KEY_BACKSPACE: u32 = 3;
pub const KEY_ESCAPE: u32 = 4;
pub const KEY_UP: u32 = 5;
pub const KEY_DOWN: u32 = 6;
pub const KEY_LEFT: u32 = 7;
pub const KEY_RIGHT: u32 = 8;
pub const KEY_HOME: u32 = 9;
pub const KEY_END: u32 = 10;
pub const KEY_PAGE_UP: u32 = 11;
pub const KEY_PAGE_DOWN: u32 = 12;
pub const KEY_INSERT: u32 = 13;
pub const KEY_DELETE: u32 = 14;
pub const KEY_F: u32 = 15;
pub const KEY_NONE: u32 = 16;
pub const KEY_CAPS_LOCK: u32 = 17;
pub const KEY_SHIFT_LEFT: u32 = 18;
pub const KEY_SHIFT_RIGHT: u32 = 19;
pub const KEY_CONTROL_LEFT: u32 = 20;
pub const KEY_CONTROL_RIGHT: u32 = 21;
pub const KEY_ALT_LEFT: u32 = 22;
pub const KEY_ALT_RIGHT: u32 = 23;
pub const KEY_SUPER_LEFT: u32 = 24;
pub const KEY_SUPER_RIGHT: u32 = 25;

pub const KEY_ACTION_PRESS: u32 = 0;
pub const KEY_ACTION_REPEAT: u32 = 1;
pub const KEY_ACTION_RELEASE: u32 = 2;

/// Color kinds in the packed cell words (`kind << 24 | payload`): named
/// (payload = rio-vt `NamedColor` discriminant), indexed (payload =
/// palette index), rgb (payload = `r << 16 | g << 8 | b`). The theme
/// lives in JS, so named/indexed resolve there.
pub const COLOR_NAMED: u32 = 0;
pub const COLOR_INDEXED: u32 = 1;
pub const COLOR_RGB: u32 = 2;

/// u32 words per cell in [`RioTerm::write_cells`]:
/// `[codepoint | wide << 21 | flags, fg, bg, style_flags]`.
pub const CELL_WORDS: usize = 4;

/// Word-0 flag: the cell carries attached cluster codepoints
/// (combining marks, or a mode-2027 grapheme cluster tail) beyond the
/// base codepoint in bits 0..21. Fetch the full text with
/// [`RioTerm::cluster_text`] and draw that instead of the base char;
/// a renderer that ignores the bit simply keeps drawing bases.
pub const CELL_HAS_CLUSTER: u32 = 1 << 23;

enum Event {
    Output(Vec<u8>),
    Wakeup,
    Title(String, Option<String>),
    Bell,
    CursorBlinkingChange,
    Progress(u8, u8),
    Clipboard(u8, String),
    Close,
}

#[derive(Default)]
struct JsDelegate {
    queue: RefCell<Vec<Event>>,
}

impl JsDelegate {
    fn push(&self, event: Event) {
        self.queue.borrow_mut().push(event);
    }
}

impl SurfaceDelegate for JsDelegate {
    fn wakeup(&self, _surface: SurfaceId) {
        // Coalesce: one wakeup per drain is all a rAF scheduler needs.
        let mut queue = self.queue.borrow_mut();
        if !matches!(queue.last(), Some(Event::Wakeup)) {
            queue.push(Event::Wakeup);
        }
    }

    fn action(&self, _surface: SurfaceId, action: Action) {
        self.push(match action {
            Action::SetTitle { title, subtitle } => Event::Title(title, subtitle),
            Action::RingBell => Event::Bell,
            Action::CursorBlinkingChange => Event::CursorBlinkingChange,
            Action::Progress { state, value } => Event::Progress(state, value),
        });
    }

    fn clipboard_write(&self, _surface: SurfaceId, kind: ClipboardType, text: String) {
        let kind = match kind {
            ClipboardType::Clipboard => 0,
            ClipboardType::Selection => 1,
        };
        self.push(Event::Clipboard(kind, text));
    }

    fn close_surface(&self, _surface: SurfaceId) {
        self.push(Event::Close);
    }

    fn output(&self, _surface: SurfaceId, bytes: &[u8]) {
        self.push(Event::Output(bytes.to_vec()));
    }
}

fn pack_color(color: AnsiColor) -> u32 {
    match color {
        AnsiColor::Named(named) => (COLOR_NAMED << 24) | named as u32,
        AnsiColor::Indexed(index) => (COLOR_INDEXED << 24) | index as u32,
        AnsiColor::Spec(rgb) => {
            (COLOR_RGB << 24)
                | ((rgb.r as u32) << 16)
                | ((rgb.g as u32) << 8)
                | rgb.b as u32
        }
    }
}

/// One terminal surface plus its pulled render state. The JS `Terminal`
/// class in the rioterm package owns exactly one of these.
#[wasm_bindgen]
pub struct RioTerm {
    surface: Surface,
    state: RenderState,
    delegate: Arc<JsDelegate>,
    on_output: Option<js_sys::Function>,
    on_wakeup: Option<js_sys::Function>,
    on_title: Option<js_sys::Function>,
    on_bell: Option<js_sys::Function>,
    on_cursor_blink: Option<js_sys::Function>,
    on_progress: Option<js_sys::Function>,
    on_clipboard: Option<js_sys::Function>,
    on_close: Option<js_sys::Function>,
}

#[wasm_bindgen]
impl RioTerm {
    #[wasm_bindgen(constructor)]
    pub fn new(
        cols: u16,
        rows: u16,
        pixel_width: u16,
        pixel_height: u16,
        scrollback: u32,
    ) -> Result<RioTerm, JsError> {
        // Single-threaded by design; Arc only because Engine's API is one
        // type on every target.
        #[allow(clippy::arc_with_non_send_sync)]
        let delegate = Arc::new(JsDelegate::default());
        let engine = Engine::new(delegate.clone());
        let desc = SurfaceDesc {
            cols: cols.max(2),
            rows: rows.max(2),
            pixel_width,
            pixel_height,
            scrollback: scrollback as usize,
            ..SurfaceDesc::default()
        };
        // The engine only mints surface ids; one surface per RioTerm, so
        // it has nothing else to hold onto.
        let surface = engine
            .create_surface(&desc)
            .map_err(|err| JsError::new(&err.to_string()))?;
        let state = RenderState::new(&surface);
        Ok(RioTerm {
            surface,
            state,
            delegate,
            on_output: None,
            on_wakeup: None,
            on_title: None,
            on_bell: None,
            on_cursor_blink: None,
            on_progress: None,
            on_clipboard: None,
            on_close: None,
        })
    }

    // ------------------------------------------------------------- events

    pub fn on_output(&mut self, callback: js_sys::Function) {
        self.on_output = Some(callback);
    }

    pub fn on_wakeup(&mut self, callback: js_sys::Function) {
        self.on_wakeup = Some(callback);
    }

    pub fn on_title(&mut self, callback: js_sys::Function) {
        self.on_title = Some(callback);
    }

    pub fn on_bell(&mut self, callback: js_sys::Function) {
        self.on_bell = Some(callback);
    }

    pub fn on_cursor_blink(&mut self, callback: js_sys::Function) {
        self.on_cursor_blink = Some(callback);
    }

    pub fn on_progress(&mut self, callback: js_sys::Function) {
        self.on_progress = Some(callback);
    }

    pub fn on_clipboard(&mut self, callback: js_sys::Function) {
        self.on_clipboard = Some(callback);
    }

    pub fn on_close(&mut self, callback: js_sys::Function) {
        self.on_close = Some(callback);
    }

    /// Drain queued delegate events into the JS callbacks. Runs after
    /// every entry point that can produce events; the queue exists so no
    /// JS runs while librio holds the terminal lock.
    fn flush(&self) {
        let events: Vec<Event> = self.delegate.queue.borrow_mut().drain(..).collect();
        let this = JsValue::NULL;
        for event in events {
            let result = match event {
                Event::Output(bytes) => self
                    .on_output
                    .as_ref()
                    .map(|f| f.call1(&this, &js_sys::Uint8Array::from(bytes.as_slice()))),
                Event::Wakeup => self.on_wakeup.as_ref().map(|f| f.call0(&this)),
                Event::Title(title, subtitle) => self.on_title.as_ref().map(|f| {
                    f.call2(
                        &this,
                        &JsValue::from_str(&title),
                        &subtitle
                            .map(|s| JsValue::from_str(&s))
                            .unwrap_or(JsValue::NULL),
                    )
                }),
                Event::Bell => self.on_bell.as_ref().map(|f| f.call0(&this)),
                Event::CursorBlinkingChange => {
                    self.on_cursor_blink.as_ref().map(|f| f.call0(&this))
                }
                Event::Progress(state, value) => self.on_progress.as_ref().map(|f| {
                    f.call2(&this, &JsValue::from(state), &JsValue::from(value))
                }),
                Event::Clipboard(kind, text) => self.on_clipboard.as_ref().map(|f| {
                    f.call2(&this, &JsValue::from(kind), &JsValue::from_str(&text))
                }),
                Event::Close => self.on_close.as_ref().map(|f| f.call0(&this)),
            };
            if let Some(Err(err)) = result {
                // A throwing callback must not wedge the drain.
                let _ = err;
            }
        }
    }

    // -------------------------------------------------------------- input

    /// Child output: bytes from the program the terminal is showing.
    /// (xterm.js calls this `write`.)
    pub fn feed(&self, bytes: &[u8]) {
        self.surface.inject_output(bytes);
        self.flush();
    }

    /// Send text to the child (a paste, or synthetic input). Reaches JS
    /// back through the `output` callback.
    pub fn send_text(&self, text: &str) {
        self.surface.text(text);
        self.flush();
    }

    /// Paste text: newlines normalized to CR and wrapped in
    /// bracketed-paste markers when the program enabled the mode.
    pub fn paste(&self, text: &str) {
        self.surface.paste(text);
        self.flush();
    }

    /// Terminal mode bits for embedder-side input decisions: bit 0 mouse
    /// reporting, bit 1 application cursor keys, bit 2 alternate screen,
    /// bit 3 bracketed paste.
    pub fn mode_bits(&self) -> u32 {
        self.surface.mode_bits()
    }

    /// A key event as the platform delivered it; librio decides what the
    /// terminal receives (kitty flags, app cursor mode, modifyOtherKeys).
    /// Tags/actions are the `KEY_*` constants. Returns true when the key
    /// produced bytes (delivered via the `output` callback).
    #[allow(clippy::too_many_arguments)]
    pub fn key(
        &self,
        action: u32,
        tag: u32,
        codepoint: u32,
        function_key: u8,
        mods: u8,
        consumed_mods: u8,
        composing: bool,
        text: Option<String>,
    ) -> bool {
        let key = match tag {
            KEY_CHAR => match char::from_u32(codepoint) {
                Some(c) => Some(Key::Char(c)),
                None => return false,
            },
            KEY_ENTER => Some(Key::Enter),
            KEY_TAB => Some(Key::Tab),
            KEY_BACKSPACE => Some(Key::Backspace),
            KEY_ESCAPE => Some(Key::Escape),
            KEY_UP => Some(Key::Up),
            KEY_DOWN => Some(Key::Down),
            KEY_LEFT => Some(Key::Left),
            KEY_RIGHT => Some(Key::Right),
            KEY_HOME => Some(Key::Home),
            KEY_END => Some(Key::End),
            KEY_PAGE_UP => Some(Key::PageUp),
            KEY_PAGE_DOWN => Some(Key::PageDown),
            KEY_INSERT => Some(Key::Insert),
            KEY_DELETE => Some(Key::Delete),
            KEY_F => Some(Key::F(function_key)),
            KEY_NONE => None,
            KEY_CAPS_LOCK => Some(Key::CapsLock),
            KEY_SHIFT_LEFT => Some(Key::ShiftLeft),
            KEY_SHIFT_RIGHT => Some(Key::ShiftRight),
            KEY_CONTROL_LEFT => Some(Key::ControlLeft),
            KEY_CONTROL_RIGHT => Some(Key::ControlRight),
            KEY_ALT_LEFT => Some(Key::AltLeft),
            KEY_ALT_RIGHT => Some(Key::AltRight),
            KEY_SUPER_LEFT => Some(Key::SuperLeft),
            KEY_SUPER_RIGHT => Some(Key::SuperRight),
            _ => return false,
        };
        let action = match action {
            KEY_ACTION_REPEAT => KeyAction::Repeat,
            KEY_ACTION_RELEASE => KeyAction::Release,
            _ => KeyAction::Press,
        };
        let event = KeyEvent {
            action,
            key,
            mods: Modifiers::from_bits_truncate(mods),
            consumed_mods: Modifiers::from_bits_truncate(consumed_mods),
            text,
            composing,
        };
        let handled = self.surface.key(&event);
        self.flush();
        handled
    }

    pub fn set_alt_is_meta(&self, enabled: bool) {
        self.surface.set_alt_is_meta(enabled);
    }

    /// Grapheme cluster processing (DEC private mode 2027) as the
    /// default for cell layout. On by default; a renderer that does
    /// not read `CELL_HAS_CLUSTER` / `cluster_text()` yet can turn it
    /// off to keep legacy wcwidth layout.
    pub fn set_grapheme_clustering(&self, enabled: bool) {
        self.surface.set_grapheme_clustering(enabled);
    }

    /// Wheel scroll: the running program gets first claim (mouse reports,
    /// alternate scroll), else the scrollback view moves. `lines` positive
    /// scrolls towards history. Returns true when the program consumed it.
    pub fn scroll_wheel(&self, lines: i32, col: u16, row: u16, mods: u8) -> bool {
        let consumed = self.surface.scroll_wheel(
            lines,
            col,
            row,
            Modifiers::from_bits_truncate(mods),
        );
        self.flush();
        consumed
    }

    pub fn scroll(&self, delta_lines: i32) {
        self.surface.scroll(delta_lines);
        self.flush();
    }

    pub fn resize(&self, cols: u16, rows: u16, pixel_width: u16, pixel_height: u16) {
        self.surface
            .resize(cols.max(2), rows.max(2), pixel_width, pixel_height);
        self.flush();
    }

    // ---------------------------------------------------------- selection

    /// `kind`: 0 simple, 1 word, 2 line, 3 block (the C ABI's values).
    pub fn selection_begin(
        &self,
        viewport_line: i32,
        col: u32,
        kind: u8,
        side_right: bool,
    ) {
        let kind = match kind {
            1 => SelectionKind::Word,
            2 => SelectionKind::Line,
            3 => SelectionKind::Block,
            _ => SelectionKind::Simple,
        };
        let side = if side_right { Side::Right } else { Side::Left };
        self.surface
            .selection_begin(viewport_line, col as usize, kind, side);
        self.flush();
    }

    pub fn selection_update(&self, viewport_line: i32, col: u32, side_right: bool) {
        let side = if side_right { Side::Right } else { Side::Left };
        self.surface
            .selection_update(viewport_line, col as usize, side);
        self.flush();
    }

    pub fn selection_clear(&self) {
        self.surface.selection_clear();
        self.flush();
    }

    pub fn selection_text(&self) -> Option<String> {
        self.surface.selection_text()
    }

    // ------------------------------------------------------- render state

    /// Pull a fresh snapshot of the grid. Call once per animation frame
    /// (after a wakeup), then read cells/cursor/selection from it.
    pub fn update(&mut self) {
        self.state.update();
    }

    pub fn lines(&self) -> u32 {
        self.state.lines() as u32
    }

    pub fn columns(&self) -> u32 {
        self.state.columns() as u32
    }

    pub fn cursor_line(&self) -> u32 {
        self.state.cursor().0 as u32
    }

    pub fn cursor_col(&self) -> u32 {
        self.state.cursor().1 as u32
    }

    /// False when the program hid the cursor (`CSI ?25l`) or the view is
    /// scrolled into history; renderers skip painting it then.
    pub fn cursor_visible(&self) -> bool {
        self.state.cursor_visible()
    }

    pub fn display_offset(&self) -> u32 {
        self.state.display_offset() as u32
    }

    pub fn alt_screen(&self) -> bool {
        self.state.alt_screen()
    }

    pub fn row_dirty(&self, line: u32) -> bool {
        self.state.row_dirty(line as usize)
    }

    pub fn reset_dirty(&mut self) {
        self.state.reset_dirty();
    }

    /// Fill `out` with the whole viewport as [`CELL_WORDS`] u32 words per
    /// cell, row-major. Returns the number of words written, 0 if `out`
    /// is too small (needs `lines * columns * CELL_WORDS`).
    pub fn write_cells(&self, out: &mut [u32]) -> usize {
        let lines = self.state.lines();
        let cols = self.state.columns();
        let needed = lines * cols * CELL_WORDS;
        if out.len() < needed {
            return 0;
        }
        for line in 0..lines {
            self.fill_row(line, cols, &mut out[line * cols * CELL_WORDS..]);
        }
        needed
    }

    /// Fill `out` with one row (`columns * CELL_WORDS` words). Lets a
    /// renderer repaint only dirty rows. Returns words written, 0 if
    /// `out` is too small.
    pub fn write_row(&self, line: u32, out: &mut [u32]) -> usize {
        let cols = self.state.columns();
        let needed = cols * CELL_WORDS;
        if out.len() < needed {
            return 0;
        }
        self.fill_row(line as usize, cols, out);
        needed
    }

    /// Plain text of one viewport row, fills trimmed (for tests and
    /// accessibility trees, not rendering).
    pub fn text_row(&self, line: u32) -> String {
        self.state.text_row(line as usize)
    }

    /// The whole buffer (scrollback + screen) as plain text.
    pub fn dump(&self) -> String {
        self.surface.dump()
    }

    /// The whole buffer as a VT byte stream that reproduces content,
    /// SGR styling, and OSC 8 hyperlinks when written into a fresh
    /// same-width terminal.
    pub fn serialize(&self) -> String {
        self.surface.serialize()
    }

    /// Lines currently held in scrollback. Search coordinates are
    /// relative to the top of this ring.
    pub fn history_lines(&self) -> u32 {
        self.surface.history_size() as u32
    }

    /// Regex matches across scrollback + screen, top to bottom, as flat
    /// quads `[start_line, start_col, end_line, end_col, ...]` with lines
    /// relative to the top of the scrollback ring. Empty on no match or
    /// an invalid pattern.
    pub fn search(&self, pattern: &str, max: u32) -> Vec<u32> {
        let Some(matches) = self.surface.search(pattern, max as usize) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(matches.len() * 4);
        for (sl, sc, el, ec) in matches {
            out.extend_from_slice(&[sl, sc as u32, el, ec as u32]);
        }
        out
    }

    /// Selection in viewport coordinates, `[start_line, start_col,
    /// end_line, end_col, is_block]`; empty when there is none.
    pub fn viewport_selection(&self) -> Vec<u32> {
        match self.state.selection() {
            Some(sel) => vec![
                sel.start_line as u32,
                sel.start_col as u32,
                sel.end_line as u32,
                sel.end_col as u32,
                sel.is_block as u32,
            ],
            None => Vec::new(),
        }
    }

    /// The OSC 8 hyperlink URI under a viewport cell, if any. Hit-test
    /// shaped on purpose: links only matter on pointer events, so nothing
    /// is paid per frame.
    pub fn link_at(&self, line: u32, col: u32) -> Option<String> {
        self.state
            .link_at(line as usize, col as usize)
            .map(str::to_owned)
    }

    /// `[start_col, end_col]` of the hyperlink run under a cell (what a
    /// renderer underlines on hover); empty when there is no link.
    pub fn link_run(&self, line: u32, col: u32) -> Vec<u32> {
        match self.state.link_run(line as usize, col as usize) {
            Some((start, end)) => vec![start as u32, end as u32],
            None => Vec::new(),
        }
    }

    /// Plain-text URL under a viewport cell (regex detection over the
    /// logical line, so wrapped URLs resolve whole). Hit-test shaped.
    pub fn url_at(&self, line: u32, col: u32) -> Option<String> {
        self.surface
            .url_at(line as u16, col as u16)
            .map(|(uri, _, _)| uri)
    }

    /// `[start_col, end_col]` of the detected URL's run on the hovered
    /// row; empty when there is none.
    pub fn url_run(&self, line: u32, col: u32) -> Vec<u32> {
        match self.surface.url_at(line as u16, col as u16) {
            Some((_, start, end)) => vec![start as u32, end as u32],
            None => Vec::new(),
        }
    }

    // ------------------------------------------------------ kitty images

    pub fn kitty_count(&self) -> u32 {
        self.state.kitty_count() as u32
    }

    /// Geometry for the placement at `index`, resolved against the
    /// viewport: `[image_id, z_index, x, y, width, height, src_x, src_y,
    /// src_w, src_h]` (f64 so image_id and z survive the trip). Empty
    /// when scrolled out of view.
    pub fn kitty_geometry(
        &self,
        index: u32,
        cell_width: f32,
        cell_height: f32,
    ) -> Vec<f64> {
        match self
            .state
            .kitty_geometry(index as usize, cell_width, cell_height)
        {
            Some((image_id, z_index, geometry)) => vec![
                image_id as f64,
                z_index as f64,
                geometry.x as f64,
                geometry.y as f64,
                geometry.width as f64,
                geometry.height as f64,
                geometry.source_rect[0] as f64,
                geometry.source_rect[1] as f64,
                geometry.source_rect[2] as f64,
                geometry.source_rect[3] as f64,
            ],
            None => Vec::new(),
        }
    }

    /// `[width, height, stamp]` for a stored image, empty if unknown.
    /// The stamp changes when the pixels do, so renderers can cache
    /// uploads/bitmaps by (id, stamp).
    pub fn kitty_image_info(&self, image_id: u32) -> Vec<f64> {
        match self.state.kitty_image_info(image_id) {
            Some((width, height, stamp)) => {
                vec![width as f64, height as f64, stamp as f64]
            }
            None => Vec::new(),
        }
    }

    /// Copy an image's RGBA pixels into `out` (needs `width * height * 4`
    /// bytes). Returns bytes written, 0 when unknown or `out` too small.
    pub fn kitty_image_rgba(&self, image_id: u32, out: &mut [u8]) -> usize {
        self.state.kitty_image_rgba(image_id, out)
    }

    /// The full text of a cell flagged [`CELL_HAS_CLUSTER`]: the base
    /// codepoint followed by its attached cluster codepoints
    /// (combining marks, or a mode-2027 grapheme cluster). Draw this
    /// string in place of the base char so a ZWJ emoji or a
    /// decomposed accent renders as the glyph the sequence means.
    /// `undefined` for cells without attachments.
    pub fn cluster_text(&self, line: usize, column: usize) -> Option<String> {
        self.state.cell_cluster_text(line, column)
    }
}

/// Measure the first grapheme cluster in a UTF-32 buffer: returns
/// `[len, width]`, the number of codepoints the cluster spans and its
/// terminal cell width (2 wide, 1 narrow, 0 for bare zero-width
/// marks); `[0, 0]` for an empty buffer. Same
/// segmentation and width rules as printing under grapheme clustering
/// (DEC mode 2027), so renderers can size text for cells without
/// replaying input. The buffer must contain a complete first cluster
/// or the logical end of the text; values that are not Unicode
/// scalars measure as one single-width codepoint when first and
/// terminate the cluster when later.
#[wasm_bindgen]
pub fn cluster_width(codepoints: &[u32]) -> Vec<u32> {
    let (len, width) = librio::cluster_width(codepoints);
    vec![len as u32, width as u32]
}

impl RioTerm {
    fn fill_row(&self, line: usize, cols: usize, out: &mut [u32]) {
        for col in 0..cols {
            let base = col * CELL_WORDS;
            match self.state.square(line, col) {
                Some(square) => {
                    let style = self.state.style_of(square);
                    let cluster = if self.state.cluster_of(square).is_some() {
                        CELL_HAS_CLUSTER
                    } else {
                        0
                    };
                    out[base] = (square.c() as u32 & 0x1F_FFFF)
                        | ((square.wide() as u32) << 21)
                        | cluster;
                    out[base + 1] = pack_color(style.fg);
                    out[base + 2] = pack_color(style.bg);
                    out[base + 3] = style.flags.bits() as u32;
                }
                None => {
                    out[base] = ' ' as u32;
                    out[base + 1] = pack_color(AnsiColor::Named(NamedColor::Foreground));
                    out[base + 2] = pack_color(AnsiColor::Named(NamedColor::Background));
                    out[base + 3] = 0;
                }
            }
        }
    }
}
