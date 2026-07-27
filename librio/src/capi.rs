#![allow(clippy::missing_safety_doc)]

use crate::{
    Action, Engine, Key, KeyEvent, Modifiers, RenderState, SelectionKind, Surface,
    SurfaceDelegate, SurfaceDesc, SurfaceId,
};
use rio_vt::config::colors::{AnsiColor, NamedColor};
use std::ffi::{c_char, c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

pub const RIO_ACTION_SET_TITLE: u32 = 0;
pub const RIO_ACTION_RING_BELL: u32 = 1;
pub const RIO_ACTION_CURSOR_BLINKING_CHANGE: u32 = 2;

pub const RIO_COLOR_NAMED: u8 = 0;
pub const RIO_COLOR_INDEXED: u8 = 1;
pub const RIO_COLOR_RGB: u8 = 2;

pub const RIO_KEY_CHAR: u32 = 0;
pub const RIO_KEY_ENTER: u32 = 1;
pub const RIO_KEY_TAB: u32 = 2;
pub const RIO_KEY_BACKSPACE: u32 = 3;
pub const RIO_KEY_ESCAPE: u32 = 4;
pub const RIO_KEY_UP: u32 = 5;
pub const RIO_KEY_DOWN: u32 = 6;
pub const RIO_KEY_LEFT: u32 = 7;
pub const RIO_KEY_RIGHT: u32 = 8;
pub const RIO_KEY_HOME: u32 = 9;
pub const RIO_KEY_END: u32 = 10;
pub const RIO_KEY_PAGE_UP: u32 = 11;
pub const RIO_KEY_PAGE_DOWN: u32 = 12;
pub const RIO_KEY_INSERT: u32 = 13;
pub const RIO_KEY_DELETE: u32 = 14;
pub const RIO_KEY_F: u32 = 15;

pub const RIO_SELECTION_SIMPLE: u8 = 0;
pub const RIO_SELECTION_WORD: u8 = 1;
pub const RIO_SELECTION_LINE: u8 = 2;
pub const RIO_SELECTION_BLOCK: u8 = 3;

#[repr(C)]
pub struct rio_action_s {
    pub tag: u32,
    pub title: *const c_char,
    pub subtitle: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct rio_runtime_config_s {
    pub userdata: *mut c_void,
    pub wakeup_cb: Option<extern "C" fn(*mut c_void, usize)>,
    pub action_cb: Option<extern "C" fn(*mut c_void, usize, rio_action_s)>,
    pub clipboard_write_cb: Option<extern "C" fn(*mut c_void, usize, u8, *const c_char)>,
    pub close_surface_cb: Option<extern "C" fn(*mut c_void, usize)>,
}

#[repr(C)]
pub struct rio_surface_config_s {
    pub shell: *const c_char,
    pub working_dir: *const c_char,
    pub cols: u16,
    pub rows: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
    pub scrollback: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct rio_color_s {
    pub kind: u8,
    pub value: u16,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[repr(C)]
pub struct rio_cell_s {
    pub codepoint: u32,
    pub fg: rio_color_s,
    pub bg: rio_color_s,
    pub style_flags: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct rio_selection_s {
    pub active: bool,
    pub start_line: u16,
    pub start_col: u16,
    pub end_line: u16,
    pub end_col: u16,
    pub is_block: bool,
}

#[repr(C)]
pub struct rio_cursor_s {
    pub line: u16,
    pub column: u16,
}

#[repr(C)]
pub struct rio_key_event_s {
    pub tag: u32,
    pub codepoint: u32,
    pub function_key: u8,
    pub mods: u8,
}

struct CDelegate {
    config: rio_runtime_config_s,
}

unsafe impl Send for CDelegate {}
unsafe impl Sync for CDelegate {}

impl SurfaceDelegate for CDelegate {
    fn wakeup(&self, surface: SurfaceId) {
        if let Some(cb) = self.config.wakeup_cb {
            cb(self.config.userdata, surface);
        }
    }

    fn action(&self, surface: SurfaceId, action: Action) {
        let Some(cb) = self.config.action_cb else {
            return;
        };
        match action {
            Action::SetTitle { title, subtitle } => {
                let title = CString::new(title).unwrap_or_default();
                let subtitle_c = subtitle.map(|s| CString::new(s).unwrap_or_default());
                cb(
                    self.config.userdata,
                    surface,
                    rio_action_s {
                        tag: RIO_ACTION_SET_TITLE,
                        title: title.as_ptr(),
                        subtitle: subtitle_c
                            .as_ref()
                            .map(|s| s.as_ptr())
                            .unwrap_or(std::ptr::null()),
                    },
                );
            }
            Action::RingBell => {
                cb(
                    self.config.userdata,
                    surface,
                    rio_action_s {
                        tag: RIO_ACTION_RING_BELL,
                        title: std::ptr::null(),
                        subtitle: std::ptr::null(),
                    },
                );
            }
            Action::CursorBlinkingChange => {
                cb(
                    self.config.userdata,
                    surface,
                    rio_action_s {
                        tag: RIO_ACTION_CURSOR_BLINKING_CHANGE,
                        title: std::ptr::null(),
                        subtitle: std::ptr::null(),
                    },
                );
            }
        }
    }

    fn clipboard_write(
        &self,
        surface: SurfaceId,
        kind: crate::ClipboardType,
        text: String,
    ) {
        if let Some(cb) = self.config.clipboard_write_cb {
            let text = CString::new(text).unwrap_or_default();
            cb(self.config.userdata, surface, kind as u8, text.as_ptr());
        }
    }

    fn close_surface(&self, surface: SurfaceId) {
        if let Some(cb) = self.config.close_surface_cb {
            cb(self.config.userdata, surface);
        }
    }
}

/// Standard xterm 16-color palette (the conventional defaults). Used to
/// resolve named / low-indexed colors to concrete RGB.
fn ansi16(i: u8) -> (u8, u8, u8) {
    const P: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (205, 0, 0),
        (0, 205, 0),
        (205, 205, 0),
        (0, 0, 238),
        (205, 0, 205),
        (0, 205, 205),
        (229, 229, 229),
        (127, 127, 127),
        (255, 0, 0),
        (0, 255, 0),
        (255, 255, 0),
        (92, 92, 255),
        (255, 0, 255),
        (0, 255, 255),
        (255, 255, 255),
    ];
    P[(i & 0x0f) as usize]
}

/// Resolve a 256-color index to RGB (16 ANSI, 6x6x6 cube, 24 grays).
fn indexed_rgb(i: u8) -> (u8, u8, u8) {
    match i {
        0..=15 => ansi16(i),
        16..=231 => {
            const STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];
            let i = i - 16;
            (
                STEPS[(i / 36) as usize],
                STEPS[((i / 6) % 6) as usize],
                STEPS[(i % 6) as usize],
            )
        }
        _ => {
            let v = 8u8.saturating_add((i - 232).saturating_mul(10));
            (v, v, v)
        }
    }
}

fn dim((r, g, b): (u8, u8, u8)) -> (u8, u8, u8) {
    (r * 2 / 3, g * 2 / 3, b * 2 / 3)
}

/// Resolve a named color to RGB. Default foreground/background match Rio's
/// built-in theme; the ANSI slots use the standard xterm palette.
fn named_rgb(n: NamedColor) -> (u8, u8, u8) {
    use NamedColor::*;
    match n {
        Black => ansi16(0),
        Red => ansi16(1),
        Green => ansi16(2),
        Yellow => ansi16(3),
        Blue => ansi16(4),
        Magenta => ansi16(5),
        Cyan => ansi16(6),
        White => ansi16(7),
        LightBlack => ansi16(8),
        LightRed => ansi16(9),
        LightGreen => ansi16(10),
        LightYellow => ansi16(11),
        LightBlue => ansi16(12),
        LightMagenta => ansi16(13),
        LightCyan => ansi16(14),
        LightWhite => ansi16(15),
        Foreground | LightForeground => (0xf8, 0xf8, 0xf2),
        DimForeground => (0x9a, 0x9a, 0x9a),
        Background => (0x0f, 0x0d, 0x0e),
        Cursor => (0xf8, 0xf8, 0xf2),
        DimBlack => dim(ansi16(0)),
        DimRed => dim(ansi16(1)),
        DimGreen => dim(ansi16(2)),
        DimYellow => dim(ansi16(3)),
        DimBlue => dim(ansi16(4)),
        DimMagenta => dim(ansi16(5)),
        DimCyan => dim(ansi16(6)),
        DimWhite => dim(ansi16(7)),
    }
}

/// Convert a terminal color to the C representation. `kind`/`value` keep
/// the original form (named / indexed / rgb) for callers that want it, but
/// `r`/`g`/`b` are ALWAYS the resolved RGB so a CPU renderer can read them
/// directly without owning a palette.
fn color_to_c(color: AnsiColor) -> rio_color_s {
    let (r, g, b) = match color {
        AnsiColor::Named(named) => named_rgb(named),
        AnsiColor::Indexed(index) => indexed_rgb(index),
        AnsiColor::Spec(rgb) => (rgb.r, rgb.g, rgb.b),
    };
    match color {
        AnsiColor::Named(named) => rio_color_s {
            kind: RIO_COLOR_NAMED,
            value: named as u16,
            r,
            g,
            b,
        },
        AnsiColor::Indexed(index) => rio_color_s {
            kind: RIO_COLOR_INDEXED,
            value: index as u16,
            r,
            g,
            b,
        },
        AnsiColor::Spec(_) => rio_color_s {
            kind: RIO_COLOR_RGB,
            value: 0,
            r,
            g,
            b,
        },
    }
}

unsafe fn cstr_opt(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
}

#[no_mangle]
pub unsafe extern "C" fn rio_engine_new(
    config: *const rio_runtime_config_s,
) -> *mut Engine {
    catch_unwind(AssertUnwindSafe(|| {
        if config.is_null() {
            return std::ptr::null_mut();
        }
        let delegate = Arc::new(CDelegate {
            config: unsafe { *config },
        });
        Box::into_raw(Box::new(Engine::new(delegate)))
    }))
    .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn rio_engine_free(engine: *mut Engine) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !engine.is_null() {
            drop(unsafe { Box::from_raw(engine) });
        }
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_surface_new(
    engine: *mut Engine,
    config: *const rio_surface_config_s,
) -> *mut Surface {
    catch_unwind(AssertUnwindSafe(|| {
        if engine.is_null() || config.is_null() {
            return std::ptr::null_mut();
        }
        let engine = unsafe { &*engine };
        let config = unsafe { &*config };
        let desc = SurfaceDesc {
            shell: unsafe { cstr_opt(config.shell) },
            args: Vec::new(),
            working_dir: unsafe { cstr_opt(config.working_dir) },
            cols: config.cols.max(2),
            rows: config.rows.max(2),
            pixel_width: config.pixel_width,
            pixel_height: config.pixel_height,
            scrollback: config.scrollback,
        };
        match engine.create_surface(&desc) {
            Ok(surface) => Box::into_raw(Box::new(surface)),
            Err(err) => {
                tracing::error!("rio_surface_new failed: {err}");
                std::ptr::null_mut()
            }
        }
    }))
    .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn rio_surface_free(surface: *mut Surface) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !surface.is_null() {
            drop(unsafe { Box::from_raw(surface) });
        }
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_surface_id(surface: *const Surface) -> usize {
    catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() {
            return 0;
        }
        unsafe { &*surface }.id()
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "C" fn rio_surface_text(
    surface: *mut Surface,
    bytes: *const c_char,
    len: usize,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() || bytes.is_null() {
            return;
        }
        let slice = unsafe { std::slice::from_raw_parts(bytes as *const u8, len) };
        unsafe { &*surface }.write(slice.to_vec());
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_surface_key(
    surface: *mut Surface,
    event: rio_key_event_s,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() {
            return false;
        }
        let key = match event.tag {
            RIO_KEY_CHAR => match char::from_u32(event.codepoint) {
                Some(c) => Key::Char(c),
                None => return false,
            },
            RIO_KEY_ENTER => Key::Enter,
            RIO_KEY_TAB => Key::Tab,
            RIO_KEY_BACKSPACE => Key::Backspace,
            RIO_KEY_ESCAPE => Key::Escape,
            RIO_KEY_UP => Key::Up,
            RIO_KEY_DOWN => Key::Down,
            RIO_KEY_LEFT => Key::Left,
            RIO_KEY_RIGHT => Key::Right,
            RIO_KEY_HOME => Key::Home,
            RIO_KEY_END => Key::End,
            RIO_KEY_PAGE_UP => Key::PageUp,
            RIO_KEY_PAGE_DOWN => Key::PageDown,
            RIO_KEY_INSERT => Key::Insert,
            RIO_KEY_DELETE => Key::Delete,
            RIO_KEY_F => Key::F(event.function_key),
            _ => return false,
        };
        let event = KeyEvent {
            key,
            mods: Modifiers::from_bits_truncate(event.mods),
        };
        unsafe { &*surface }.key(event)
    }))
    .unwrap_or(false)
}

#[no_mangle]
pub unsafe extern "C" fn rio_surface_resize(
    surface: *mut Surface,
    cols: u16,
    rows: u16,
    pixel_width: u16,
    pixel_height: u16,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() {
            return;
        }
        unsafe { &*surface }.resize(cols.max(2), rows.max(2), pixel_width, pixel_height);
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_surface_scroll(surface: *mut Surface, delta_lines: i32) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() {
            return;
        }
        unsafe { &*surface }.scroll(delta_lines);
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_surface_selection_begin(
    surface: *mut Surface,
    viewport_line: i32,
    col: u16,
    kind: u8,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() {
            return;
        }
        let kind = match kind {
            RIO_SELECTION_WORD => SelectionKind::Word,
            RIO_SELECTION_LINE => SelectionKind::Line,
            RIO_SELECTION_BLOCK => SelectionKind::Block,
            _ => SelectionKind::Simple,
        };
        unsafe { &*surface }.selection_begin(viewport_line, col as usize, kind);
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_surface_selection_update(
    surface: *mut Surface,
    viewport_line: i32,
    col: u16,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() {
            return;
        }
        unsafe { &*surface }.selection_update(viewport_line, col as usize);
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_surface_selection_clear(surface: *mut Surface) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() {
            return;
        }
        unsafe { &*surface }.selection_clear();
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_surface_selection_text(
    surface: *const Surface,
) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() {
            return std::ptr::null_mut();
        }
        match unsafe { &*surface }.selection_text() {
            Some(text) => CString::new(text).unwrap_or_default().into_raw(),
            None => std::ptr::null_mut(),
        }
    }))
    .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn rio_text_free(text: *mut c_char) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !text.is_null() {
            drop(unsafe { CString::from_raw(text) });
        }
    }));
}

/// Inject bytes into the terminal's display (VT parser), NOT the PTY input.
/// Used to replay persisted scrollback on restore without the shell seeing
/// (and executing) it.
#[no_mangle]
pub unsafe extern "C" fn rio_surface_inject_output(
    surface: *mut Surface,
    bytes: *const c_char,
    len: usize,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() || bytes.is_null() {
            return;
        }
        let slice = unsafe { std::slice::from_raw_parts(bytes as *const u8, len) };
        unsafe { &*surface }.inject_output(slice);
    }));
}

/// The shell's current working directory (OSC 7), or NULL if unknown.
/// Caller owns the returned string; free it with `rio_text_free`.
#[no_mangle]
pub unsafe extern "C" fn rio_surface_working_dir(surface: *const Surface) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() {
            return std::ptr::null_mut();
        }
        match unsafe { &*surface }.working_dir() {
            Some(dir) => CString::new(dir).unwrap_or_default().into_raw(),
            None => std::ptr::null_mut(),
        }
    }))
    .unwrap_or(std::ptr::null_mut())
}

/// Dump the whole buffer (scrollback + screen) as UTF-8 text, for
/// persisting and replaying on restore. Caller owns the returned string;
/// free it with `rio_text_free`.
#[no_mangle]
pub unsafe extern "C" fn rio_surface_dump(surface: *const Surface) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() {
            return std::ptr::null_mut();
        }
        CString::new(unsafe { &*surface }.dump())
            .unwrap_or_default()
            .into_raw()
    }))
    .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn rio_render_state_selection(
    state: *const RenderState,
) -> rio_selection_s {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() {
            return rio_selection_s::default();
        }
        match unsafe { &*state }.selection() {
            Some(selection) => rio_selection_s {
                active: true,
                start_line: selection.start_line,
                start_col: selection.start_col,
                end_line: selection.end_line,
                end_col: selection.end_col,
                is_block: selection.is_block,
            },
            None => rio_selection_s::default(),
        }
    }))
    .unwrap_or_default()
}

#[no_mangle]
pub unsafe extern "C" fn rio_render_state_new(
    surface: *const Surface,
) -> *mut RenderState {
    catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() {
            return std::ptr::null_mut();
        }
        Box::into_raw(Box::new(RenderState::new(unsafe { &*surface })))
    }))
    .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn rio_render_state_free(state: *mut RenderState) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !state.is_null() {
            drop(unsafe { Box::from_raw(state) });
        }
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_render_state_update(state: *mut RenderState) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !state.is_null() {
            unsafe { &mut *state }.update();
        }
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_render_state_lines(state: *const RenderState) -> u16 {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() {
            return 0;
        }
        unsafe { &*state }.lines() as u16
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "C" fn rio_render_state_columns(state: *const RenderState) -> u16 {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() {
            return 0;
        }
        unsafe { &*state }.columns() as u16
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "C" fn rio_render_state_row_dirty(
    state: *const RenderState,
    line: u16,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() {
            return false;
        }
        unsafe { &*state }.row_dirty(line as usize)
    }))
    .unwrap_or(false)
}

#[no_mangle]
pub unsafe extern "C" fn rio_render_state_reset_dirty(state: *mut RenderState) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !state.is_null() {
            unsafe { &mut *state }.reset_dirty();
        }
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_render_state_cell(
    state: *const RenderState,
    line: u16,
    column: u16,
) -> rio_cell_s {
    catch_unwind(AssertUnwindSafe(|| {
        let empty = rio_cell_s {
            codepoint: ' ' as u32,
            fg: rio_color_s::default(),
            bg: rio_color_s::default(),
            style_flags: 0,
        };
        if state.is_null() {
            return empty;
        }
        let state = unsafe { &*state };
        let Some(square) = state.square(line as usize, column as usize) else {
            return empty;
        };
        let style = state.style_of(square);
        rio_cell_s {
            codepoint: square.c() as u32,
            fg: color_to_c(style.fg),
            bg: color_to_c(style.bg),
            style_flags: style.flags.bits(),
        }
    }))
    .unwrap_or(rio_cell_s {
        codepoint: ' ' as u32,
        fg: rio_color_s::default(),
        bg: rio_color_s::default(),
        style_flags: 0,
    })
}

#[no_mangle]
pub unsafe extern "C" fn rio_render_state_cursor(
    state: *const RenderState,
) -> rio_cursor_s {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() {
            return rio_cursor_s { line: 0, column: 0 };
        }
        let (line, column) = unsafe { &*state }.cursor();
        rio_cursor_s {
            line: line as u16,
            column: column as u16,
        }
    }))
    .unwrap_or(rio_cursor_s { line: 0, column: 0 })
}

#[cfg(test)]
mod color_tests {
    use super::*;

    #[test]
    fn resolves_indexed_palette() {
        assert_eq!(indexed_rgb(0), (0, 0, 0)); // ansi black
        assert_eq!(indexed_rgb(15), (255, 255, 255)); // ansi bright white
        assert_eq!(indexed_rgb(16), (0, 0, 0)); // cube origin
        assert_eq!(indexed_rgb(231), (255, 255, 255)); // cube max
        assert_eq!(indexed_rgb(232), (8, 8, 8)); // first gray
        assert_eq!(indexed_rgb(255), (238, 238, 238)); // last gray
    }

    #[test]
    fn named_fills_rgb_and_keeps_kind() {
        let c = color_to_c(AnsiColor::Named(NamedColor::Red));
        assert_eq!(c.kind, RIO_COLOR_NAMED);
        assert_eq!((c.r, c.g, c.b), (205, 0, 0));

        let bg = color_to_c(AnsiColor::Named(NamedColor::Background));
        assert_eq!((bg.r, bg.g, bg.b), (0x0f, 0x0d, 0x0e));
    }

    #[test]
    fn spec_passes_through() {
        let c = color_to_c(AnsiColor::Spec(rio_vt::config::colors::ColorRgb {
            r: 1,
            g: 2,
            b: 3,
        }));
        assert_eq!(c.kind, RIO_COLOR_RGB);
        assert_eq!((c.r, c.g, c.b), (1, 2, 3));
    }
}

#[cfg(test)]
mod persistence_tests {
    use crate::{SurfaceDelegate, SurfaceId};
    use std::sync::Arc;

    struct NopDelegate;
    impl SurfaceDelegate for NopDelegate {
        fn wakeup(&self, _: SurfaceId) {}
        fn action(&self, _: SurfaceId, _: crate::Action) {}
        fn clipboard_write(&self, _: SurfaceId, _: crate::ClipboardType, _: String) {}
        fn close_surface(&self, _: SurfaceId) {}
    }

    #[test]
    fn dump_captures_written_text() {
        let engine = crate::Engine::new(Arc::new(NopDelegate));
        let surface = engine
            .create_surface(&crate::SurfaceDesc {
                shell: Some("/bin/sh".into()),
                args: vec![],
                working_dir: None,
                cols: 80,
                rows: 24,
                pixel_width: 640,
                pixel_height: 384,
                scrollback: 1000,
            })
            .expect("surface");
        // Drive bytes straight into the terminal (no PTY round-trip).
        surface.write(b"hello persistence".to_vec());
        std::thread::sleep(std::time::Duration::from_millis(50));
        let dump = surface.dump();
        assert!(dump.contains("hello persistence"), "dump was: {dump:?}");
    }
}
