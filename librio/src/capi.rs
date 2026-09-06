#![allow(clippy::missing_safety_doc)]

use crate::{
    Action, Engine, Key, KeyAction, KeyEvent, Modifiers, RenderState, SelectionKind,
    Side, Square, Style, StyleFlags, Surface, SurfaceDelegate, SurfaceDesc, SurfaceId,
};
use rio_vt::config::colors::{AnsiColor, ColorRgb, NamedColor};
use std::ffi::{c_char, c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

pub const RIO_ACTION_SET_TITLE: u32 = 0;
pub const RIO_ACTION_RING_BELL: u32 = 1;
pub const RIO_ACTION_CURSOR_BLINKING_CHANGE: u32 = 2;
/// OSC 9;4 progress: `data_a` is the ConEmu state (0 remove, 1 set,
/// 2 error, 3 indeterminate, 4 paused), `data_b` the 0-100 value.
pub const RIO_ACTION_PROGRESS: u32 = 3;

pub const RIO_COLOR_NAMED: u8 = 0;
pub const RIO_COLOR_INDEXED: u8 = 1;
pub const RIO_COLOR_RGB: u8 = 2;
/// An absent color (value and rgb are zero). Returned by
/// [`rio_render_state_cell_underline_color`] for cells without an
/// explicit underline color, and as the fg/bg of a missing cell.
pub const RIO_COLOR_NONE: u8 = 3;

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
/// No key, for an event that carries only text (an input method commit).
pub const RIO_KEY_NONE: u32 = 16;
// Modifier keys as keys; reported only in kitty report-all mode.
pub const RIO_KEY_CAPS_LOCK: u32 = 17;
pub const RIO_KEY_SHIFT_LEFT: u32 = 18;
pub const RIO_KEY_SHIFT_RIGHT: u32 = 19;
pub const RIO_KEY_CONTROL_LEFT: u32 = 20;
pub const RIO_KEY_CONTROL_RIGHT: u32 = 21;
pub const RIO_KEY_ALT_LEFT: u32 = 22;
pub const RIO_KEY_ALT_RIGHT: u32 = 23;
pub const RIO_KEY_SUPER_LEFT: u32 = 24;
pub const RIO_KEY_SUPER_RIGHT: u32 = 25;

pub const RIO_KEY_ACTION_PRESS: u32 = 0;
pub const RIO_KEY_ACTION_REPEAT: u32 = 1;
pub const RIO_KEY_ACTION_RELEASE: u32 = 2;

pub const RIO_SELECTION_SIMPLE: u8 = 0;
pub const RIO_SELECTION_WORD: u8 = 1;
pub const RIO_SELECTION_LINE: u8 = 2;
pub const RIO_SELECTION_BLOCK: u8 = 3;

pub const RIO_CURSOR_BLOCK: u8 = 0;
pub const RIO_CURSOR_UNDERLINE: u8 = 1;
pub const RIO_CURSOR_BEAM: u8 = 2;
pub const RIO_CURSOR_HIDDEN: u8 = 3;

#[repr(C)]
pub struct rio_action_s {
    pub tag: u32,
    pub title: *const c_char,
    pub subtitle: *const c_char,
    /// Numeric payload; meaning depends on `tag` (see RIO_ACTION_*).
    pub data_a: u32,
    pub data_b: u32,
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
    /// Arguments for the program, after argv[0]. NULL/0 means none. These
    /// reach the child through `execvp` as separate argv entries, so a host
    /// that has a command to run can spawn it instead of typing it: nothing
    /// here is ever read by a shell's line editor.
    pub args: *const *const c_char,
    pub args_len: usize,
}

/// A color in its original form (`RIO_COLOR_NAMED` / `_INDEXED` /
/// `_RGB`; `value` holds the named id or palette index), with `r`/`g`/`b`
/// resolved for every kind except [`RIO_COLOR_NONE`], which means no
/// color at all: `value` and `r`/`g`/`b` are zero.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct rio_color_s {
    pub kind: u8,
    pub value: u16,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// A renderable cell. `fg`/`bg` are exported pre-swap: when the flags
/// carry `StyleFlags::INVERSE` the host swaps them when drawing, as
/// rio's own renderer does. They have kind [`RIO_COLOR_NONE`] only on a
/// missing cell (NULL state or out-of-range query).
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

/// A key event as the platform delivered it.
///
/// The host reports what happened; librio decides what the terminal receives,
/// because that depends on state the host does not track (application cursor
/// mode, the kitty keyboard flags, `modifyOtherKeys`).
#[repr(C)]
pub struct rio_key_event_s {
    /// `RIO_KEY_ACTION_*`. Releases are dropped unless the program asked for
    /// event types.
    pub action: u32,
    /// `RIO_KEY_*`, naming the key without shift applied: shift+a is
    /// `RIO_KEY_CHAR` with codepoint `a`, and the `A` belongs in `text`.
    pub tag: u32,
    pub codepoint: u32,
    /// 1 to 12 when `tag` is `RIO_KEY_F`.
    pub function_key: u8,
    /// Every modifier held, as `RIO_MOD_*`.
    pub mods: u8,
    /// The modifiers the platform already spent producing `text`, so they are
    /// not encoded a second time. Zero if the platform cannot say.
    pub consumed_mods: u8,
    /// Whether an input method currently owns the key, in which case nothing
    /// is encoded and the text arrives when composition commits.
    pub composing: bool,
    /// What the platform produced for this key, UTF-8, after any dead key or
    /// input method composition. May be NULL.
    pub text: *const c_char,
    pub text_len: usize,
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
                        data_a: 0,
                        data_b: 0,
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
                        data_a: 0,
                        data_b: 0,
                    },
                );
            }
            Action::Progress { state, value } => {
                cb(
                    self.config.userdata,
                    surface,
                    rio_action_s {
                        tag: RIO_ACTION_PROGRESS,
                        title: std::ptr::null(),
                        subtitle: std::ptr::null(),
                        data_a: state as u32,
                        data_b: value as u32,
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
                        data_a: 0,
                        data_b: 0,
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

/// The active color scheme. Starts as Rio's default theme (shared with the
/// rio frontend via rio-vt so the two can never drift apart) and is replaced
/// wholesale by `rio_set_colors`. Everything downstream resolves through it,
/// so a swap re-themes every cell on the host's next draw: the snapshot
/// keeps original named/indexed colors and resolves at query time.
fn theme_lock() -> &'static std::sync::RwLock<rio_vt::config::Colors> {
    static THEME: std::sync::OnceLock<std::sync::RwLock<rio_vt::config::Colors>> =
        std::sync::OnceLock::new();
    THEME.get_or_init(|| std::sync::RwLock::new(rio_vt::config::Colors::default()))
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct rio_rgb_s {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// A color scheme, limited to what the engine itself resolves: the 16 ANSI
/// slots plus the named defaults. Selection/cursor *rendering* colors stay
/// host-side; `cursor` here covers cells that reference the cursor color.
#[repr(C)]
pub struct rio_colors_s {
    /// ANSI 0-7 normal, 8-15 bright.
    pub ansi: [rio_rgb_s; 16],
    pub foreground: rio_rgb_s,
    pub background: rio_rgb_s,
    pub cursor: rio_rgb_s,
}

/// Replace the palette used to resolve named/indexed cell colors. NULL
/// restores Rio's default theme. Dim variants derive from the new palette
/// (2/3 luminance, rio's fallback). The host owns redraw: call
/// `rio_render_state_update` + repaint after this.
///
/// # Safety
/// `colors`, when non-NULL, must point to a valid `rio_colors_s`.
#[no_mangle]
pub unsafe extern "C" fn rio_set_colors(colors: *const rio_colors_s) {
    let mut theme = rio_vt::config::Colors::default();
    if let Some(colors) = colors.as_ref() {
        let rgb = |c: rio_rgb_s| ColorRgb {
            r: c.r,
            g: c.g,
            b: c.b,
        };
        let arr = |c: rio_rgb_s| rgb(c).to_arr();
        theme.black = arr(colors.ansi[0]);
        theme.red = arr(colors.ansi[1]);
        theme.green = arr(colors.ansi[2]);
        theme.yellow = arr(colors.ansi[3]);
        theme.blue = arr(colors.ansi[4]);
        theme.magenta = arr(colors.ansi[5]);
        theme.cyan = arr(colors.ansi[6]);
        theme.white = arr(colors.ansi[7]);
        theme.light_black = arr(colors.ansi[8]);
        theme.light_red = arr(colors.ansi[9]);
        theme.light_green = arr(colors.ansi[10]);
        theme.light_yellow = arr(colors.ansi[11]);
        theme.light_blue = arr(colors.ansi[12]);
        theme.light_magenta = arr(colors.ansi[13]);
        theme.light_cyan = arr(colors.ansi[14]);
        theme.light_white = arr(colors.ansi[15]);
        theme.foreground = arr(colors.foreground);
        theme.background = rgb(colors.background).to_composition();
        theme.cursor = arr(colors.cursor);
    }
    *theme_lock().write().unwrap() = theme;
}

/// sRGB 0..1 component array (rio's `ColorArray`) to 8-bit RGB.
fn arr_rgb(c: [f32; 4]) -> (u8, u8, u8) {
    (
        (c[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (c[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (c[2].clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

/// The active 16-color palette. Used to resolve named / low-indexed
/// colors to concrete RGB.
fn ansi16(t: &rio_vt::config::Colors, i: u8) -> (u8, u8, u8) {
    let arr = match i & 0x0f {
        0 => t.black,
        1 => t.red,
        2 => t.green,
        3 => t.yellow,
        4 => t.blue,
        5 => t.magenta,
        6 => t.cyan,
        7 => t.white,
        8 => t.light_black,
        9 => t.light_red,
        10 => t.light_green,
        11 => t.light_yellow,
        12 => t.light_blue,
        13 => t.light_magenta,
        14 => t.light_cyan,
        _ => t.light_white,
    };
    arr_rgb(arr)
}

/// Resolve a 256-color index to RGB (16 ANSI, 6x6x6 cube, 24 grays).
fn indexed_rgb(t: &rio_vt::config::Colors, i: u8) -> (u8, u8, u8) {
    match i {
        0..=15 => ansi16(t, i),
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
    // Widen first: `r * 2` overflows u8 for components >= 128.
    (
        (u16::from(r) * 2 / 3) as u8,
        (u16::from(g) * 2 / 3) as u8,
        (u16::from(b) * 2 / 3) as u8,
    )
}

/// Resolve a named color to RGB using the active theme. Dim colors use
/// the theme's explicit dim entry when present, else 2/3 of the base color
/// (same fallback rio applies).
fn named_rgb(t: &rio_vt::config::Colors, n: NamedColor) -> (u8, u8, u8) {
    use NamedColor::*;
    let dim_or = |explicit: Option<[f32; 4]>, base: u8| {
        explicit
            .map(arr_rgb)
            .unwrap_or_else(|| dim(ansi16(t, base)))
    };
    match n {
        Black => ansi16(t, 0),
        Red => ansi16(t, 1),
        Green => ansi16(t, 2),
        Yellow => ansi16(t, 3),
        Blue => ansi16(t, 4),
        Magenta => ansi16(t, 5),
        Cyan => ansi16(t, 6),
        White => ansi16(t, 7),
        LightBlack => ansi16(t, 8),
        LightRed => ansi16(t, 9),
        LightGreen => ansi16(t, 10),
        LightYellow => ansi16(t, 11),
        LightBlue => ansi16(t, 12),
        LightMagenta => ansi16(t, 13),
        LightCyan => ansi16(t, 14),
        LightWhite => ansi16(t, 15),
        Foreground => arr_rgb(t.foreground),
        LightForeground => arr_rgb(t.light_foreground.unwrap_or(t.foreground)),
        DimForeground => t
            .dim_foreground
            .map(arr_rgb)
            .unwrap_or_else(|| dim(arr_rgb(t.foreground))),
        Background => arr_rgb(t.background.0),
        Cursor => arr_rgb(t.cursor),
        DimBlack => dim_or(t.dim_black, 0),
        DimRed => dim_or(t.dim_red, 1),
        DimGreen => dim_or(t.dim_green, 2),
        DimYellow => dim_or(t.dim_yellow, 3),
        DimBlue => dim_or(t.dim_blue, 4),
        DimMagenta => dim_or(t.dim_magenta, 5),
        DimCyan => dim_or(t.dim_cyan, 6),
        DimWhite => dim_or(t.dim_white, 7),
    }
}

/// Convert a terminal color to the C representation. `kind`/`value` keep
/// the original form (named / indexed / rgb, never `RIO_COLOR_NONE`) for
/// callers that want it, but `r`/`g`/`b` are always the resolved RGB so a
/// CPU renderer can read them directly without owning a palette.
fn color_to_c(color: AnsiColor) -> rio_color_s {
    let (r, g, b) = match color {
        AnsiColor::Named(named) => named_rgb(&theme_lock().read().unwrap(), named),
        AnsiColor::Indexed(index) => indexed_rgb(&theme_lock().read().unwrap(), index),
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

/// Resolve a cell color, honoring the terminal's dynamic OSC overrides
/// before falling back to the process-global theme. Those OSC-set colors
/// live per-terminal in the frame's `term_colors` snapshot: `OSC 10/11`
/// override the default foreground/background, `OSC 4` overrides an ANSI
/// palette slot. A named or indexed cell that has an override resolves to
/// it here, mirroring rioterm's and libsugarloaf's canonical
/// `term_colors[idx].unwrap_or(base)`, rather than resolving to the
/// host's own scheme. Direct-RGB cells carry their color inline and are
/// untouched. (Dim/bold intensity remapping isn't applied here, matching
/// `color_to_c`; it travels in the cell's style flags.)
fn resolve_cell_color(color: AnsiColor, term_colors: &crate::TermColors) -> rio_color_s {
    let (idx, value, kind) = match color {
        AnsiColor::Named(named) => (named as usize, named as u16, RIO_COLOR_NAMED),
        AnsiColor::Indexed(index) => (index as usize, index as u16, RIO_COLOR_INDEXED),
        AnsiColor::Spec(_) => return color_to_c(color),
    };
    if let Some(arr) = term_colors[idx] {
        let rgb = ColorRgb::from_color_arr(arr);
        return rio_color_s {
            kind,
            value,
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        };
    }
    color_to_c(color)
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
        let args = if config.args.is_null() {
            Vec::new()
        } else {
            (0..config.args_len)
                .filter_map(|i| unsafe { cstr_opt(*config.args.add(i)) })
                .collect()
        };
        let desc = SurfaceDesc {
            shell: unsafe { cstr_opt(config.shell) },
            args,
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

/// Pid of the spawned program (a session leader on unix, the conpty
/// child on Windows), 0 without a PTY or when the pid was unavailable.
#[no_mangle]
pub unsafe extern "C" fn rio_surface_child_pid(surface: *const Surface) -> u32 {
    catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() {
            return 0;
        }
        #[cfg(feature = "pty")]
        {
            unsafe { &*surface }.child_pid()
        }
        #[cfg(not(feature = "pty"))]
        {
            0
        }
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
pub unsafe extern "C" fn rio_surface_paste(
    surface: *mut Surface,
    bytes: *const c_char,
    len: usize,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() || bytes.is_null() {
            return;
        }
        let slice = unsafe { std::slice::from_raw_parts(bytes as *const u8, len) };
        unsafe { &*surface }.paste(&String::from_utf8_lossy(slice));
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_surface_key(
    surface: *mut Surface,
    event: *const rio_key_event_s,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() || event.is_null() {
            return false;
        }
        let event = unsafe { &*event };
        let key = match event.tag {
            RIO_KEY_CHAR => match char::from_u32(event.codepoint) {
                Some(c) => Some(Key::Char(c)),
                None => return false,
            },
            RIO_KEY_ENTER => Some(Key::Enter),
            RIO_KEY_TAB => Some(Key::Tab),
            RIO_KEY_BACKSPACE => Some(Key::Backspace),
            RIO_KEY_ESCAPE => Some(Key::Escape),
            RIO_KEY_UP => Some(Key::Up),
            RIO_KEY_DOWN => Some(Key::Down),
            RIO_KEY_LEFT => Some(Key::Left),
            RIO_KEY_RIGHT => Some(Key::Right),
            RIO_KEY_HOME => Some(Key::Home),
            RIO_KEY_END => Some(Key::End),
            RIO_KEY_PAGE_UP => Some(Key::PageUp),
            RIO_KEY_PAGE_DOWN => Some(Key::PageDown),
            RIO_KEY_INSERT => Some(Key::Insert),
            RIO_KEY_DELETE => Some(Key::Delete),
            RIO_KEY_F => Some(Key::F(event.function_key)),
            RIO_KEY_NONE => None,
            RIO_KEY_CAPS_LOCK => Some(Key::CapsLock),
            RIO_KEY_SHIFT_LEFT => Some(Key::ShiftLeft),
            RIO_KEY_SHIFT_RIGHT => Some(Key::ShiftRight),
            RIO_KEY_CONTROL_LEFT => Some(Key::ControlLeft),
            RIO_KEY_CONTROL_RIGHT => Some(Key::ControlRight),
            RIO_KEY_ALT_LEFT => Some(Key::AltLeft),
            RIO_KEY_ALT_RIGHT => Some(Key::AltRight),
            RIO_KEY_SUPER_LEFT => Some(Key::SuperLeft),
            RIO_KEY_SUPER_RIGHT => Some(Key::SuperRight),
            _ => return false,
        };

        let action = match event.action {
            RIO_KEY_ACTION_REPEAT => KeyAction::Repeat,
            RIO_KEY_ACTION_RELEASE => KeyAction::Release,
            _ => KeyAction::Press,
        };

        // Lossy: a platform that hands over ill-formed UTF-8 gets replacement
        // characters rather than a dropped keystroke.
        let text = if event.text.is_null() || event.text_len == 0 {
            None
        } else {
            let bytes = unsafe {
                std::slice::from_raw_parts(event.text as *const u8, event.text_len)
            };
            Some(String::from_utf8_lossy(bytes).into_owned())
        };

        let event = KeyEvent {
            action,
            key,
            mods: Modifiers::from_bits_truncate(event.mods),
            consumed_mods: Modifiers::from_bits_truncate(event.consumed_mods),
            text,
            composing: event.composing,
        };
        unsafe { &*surface }.key(&event)
    }))
    .unwrap_or(false)
}

#[no_mangle]
pub unsafe extern "C" fn rio_surface_mode_bits(surface: *const Surface) -> u32 {
    catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() {
            return 0;
        }
        unsafe { &*surface }.mode_bits()
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "C" fn rio_surface_set_alt_is_meta(
    surface: *mut Surface,
    enabled: bool,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() {
            return;
        }
        unsafe { &*surface }.set_alt_is_meta(enabled);
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_surface_set_grapheme_clustering(
    surface: *mut Surface,
    enabled: bool,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() {
            return;
        }
        unsafe { &*surface }.set_grapheme_clustering(enabled);
    }));
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
    side_right: bool,
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
        unsafe { &*surface }.selection_begin(
            viewport_line,
            col as usize,
            kind,
            if side_right { Side::Right } else { Side::Left },
        );
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_surface_selection_update(
    surface: *mut Surface,
    viewport_line: i32,
    col: u16,
    side_right: bool,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() {
            return;
        }
        unsafe { &*surface }.selection_update(
            viewport_line,
            col as usize,
            if side_right { Side::Right } else { Side::Left },
        );
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

/// The no-color value: kind [`RIO_COLOR_NONE`], everything else zero.
fn none_color() -> rio_color_s {
    rio_color_s {
        kind: RIO_COLOR_NONE,
        ..rio_color_s::default()
    }
}

/// The "no such cell" result: a blank whose fg/bg have kind
/// [`RIO_COLOR_NONE`], so a miss is detectable and never mistaken for a
/// real black cell.
fn empty_cell() -> rio_cell_s {
    rio_cell_s {
        codepoint: ' ' as u32,
        fg: none_color(),
        bg: none_color(),
        style_flags: 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn rio_render_state_cell(
    state: *const RenderState,
    line: u16,
    column: u16,
) -> rio_cell_s {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() {
            return empty_cell();
        }
        let state = unsafe { &*state };
        let Some((square, style)) = cell_style(state, line, column) else {
            return empty_cell();
        };
        let mut markers = 0;
        if state.cluster_of(square).is_some() {
            markers |= RIO_CELL_HAS_CLUSTER;
        }
        if style.underline_color.is_some() {
            markers |= RIO_CELL_HAS_UNDERLINE_COLOR;
        }
        let tc = state.term_colors();
        rio_cell_s {
            codepoint: square.c() as u32,
            fg: resolve_cell_color(style.fg, tc),
            bg: resolve_cell_color(style.bg, tc),
            style_flags: style.flags.bits() | markers,
        }
    }))
    .unwrap_or_else(|_| empty_cell())
}

/// Shared lookup for the per-cell accessors: the square at (line, column)
/// and its resolved style, or `None` out of bounds.
fn cell_style(state: &RenderState, line: u16, column: u16) -> Option<(&Square, Style)> {
    let square = state.square(line as usize, column as usize)?;
    let style = state.style_at(line as usize, column as usize, square);
    Some((square, style))
}

/// The cell's explicit SGR 58 underline color, or kind
/// [`RIO_COLOR_NONE`] when the cell has none (also for a NULL state, an
/// out-of-range cell, or an internal error); see
/// [`RIO_CELL_HAS_UNDERLINE_COLOR`].
#[no_mangle]
pub unsafe extern "C" fn rio_render_state_cell_underline_color(
    state: *const RenderState,
    line: u16,
    column: u16,
) -> rio_color_s {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() {
            return none_color();
        }
        let state = unsafe { &*state };
        let Some((_, style)) = cell_style(state, line, column) else {
            return none_color();
        };
        let Some(underline) = style.underline_color else {
            return none_color();
        };
        resolve_cell_color(underline, state.term_colors())
    }))
    .unwrap_or_else(|_| none_color())
}

/// Set in `rio_cell_s.style_flags` when the cell carries attached
/// cluster codepoints (combining marks, or a mode-2027 grapheme
/// cluster) beyond `codepoint`. Fetch the full text with
/// [`rio_render_state_cell_cluster`] and draw that instead of the
/// base char. StyleFlags proper occupies bits 0-12 (the
/// `RIO_STYLE_*` constants in librio.h); this is bit 15.
pub const RIO_CELL_HAS_CLUSTER: u16 = 1 << 15;

/// Set in `rio_cell_s.style_flags` when the cell carries an explicit
/// SGR 58 underline color; fetch it with
/// [`rio_render_state_cell_underline_color`]. Without it the underline
/// takes the glyph's color. This is bit 14.
pub const RIO_CELL_HAS_UNDERLINE_COLOR: u16 = 1 << 14;

// librio.h's RIO_STYLE_* and RIO_COLOR_* defines hand-mirror these
// values, and StyleFlags shares the u16 with the RIO_CELL_* marker bits:
// renumbering or colliding breaks the ABI silently, so pin every
// exported value.
const _: () = {
    assert!(RIO_COLOR_NAMED == 0);
    assert!(RIO_COLOR_INDEXED == 1);
    assert!(RIO_COLOR_RGB == 2);
    assert!(RIO_COLOR_NONE == 3);
    assert!(StyleFlags::INVERSE.bits() == 1 << 0);
    assert!(StyleFlags::BOLD.bits() == 1 << 1);
    assert!(StyleFlags::ITALIC.bits() == 1 << 2);
    assert!(StyleFlags::DIM.bits() == 1 << 3);
    assert!(StyleFlags::HIDDEN.bits() == 1 << 4);
    assert!(StyleFlags::STRIKEOUT.bits() == 1 << 5);
    assert!(StyleFlags::UNDERLINE.bits() == 1 << 6);
    assert!(StyleFlags::DOUBLE_UNDERLINE.bits() == 1 << 7);
    assert!(StyleFlags::UNDERCURL.bits() == 1 << 8);
    assert!(StyleFlags::DOTTED_UNDERLINE.bits() == 1 << 9);
    assert!(StyleFlags::DASHED_UNDERLINE.bits() == 1 << 10);
    assert!(StyleFlags::SLOW_BLINK.bits() == 1 << 11);
    assert!(StyleFlags::RAPID_BLINK.bits() == 1 << 12);
    assert!(
        StyleFlags::all().bits() & (RIO_CELL_HAS_CLUSTER | RIO_CELL_HAS_UNDERLINE_COLOR)
            == 0
    );
};

/// Write the full text of a cell (base codepoint plus attached
/// cluster codepoints) as UTF-32 into `out` (capacity `cap` code
/// units). Returns the total codepoint count, which may exceed `cap`
/// (call again with a larger buffer); 0 for plain cells, so callers
/// can treat 0 as "draw `codepoint` as usual".
#[no_mangle]
pub unsafe extern "C" fn rio_render_state_cell_cluster(
    state: *const RenderState,
    line: u16,
    column: u16,
    out: *mut u32,
    cap: usize,
) -> usize {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() {
            return 0;
        }
        let state = unsafe { &*state };
        let Some(square) = state.square(line as usize, column as usize) else {
            return 0;
        };
        let Some(cluster) = state.cluster_of(square) else {
            return 0;
        };
        let total = 1 + cluster.len();
        if !out.is_null() {
            let write = total.min(cap);
            let dst = unsafe { core::slice::from_raw_parts_mut(out, write) };
            let mut src = core::iter::once(square.c()).chain(cluster.iter().copied());
            for slot in dst.iter_mut() {
                *slot = src.next().unwrap_or('\0') as u32;
            }
        }
        total
    }))
    .unwrap_or(0)
}

/// Measure the first grapheme cluster in a UTF-32 buffer: returns the
/// number of codepoints the cluster spans and writes its terminal
/// cell width to `out_width` when non-null (2 for wide, 1 for narrow,
/// 0 for bare zero-width marks). Returns 0 for a null or empty
/// buffer. Segmentation and width follow the same rules
/// the terminal applies when printing under grapheme clustering, so
/// embedders can size text for cells without replaying input. The
/// buffer must contain a complete first cluster or the logical end of
/// the text; values that are not Unicode scalars measure as one
/// single-width codepoint when first and terminate the cluster when
/// later.
#[no_mangle]
pub unsafe extern "C" fn rio_cluster_width(
    codepoints: *const u32,
    len: usize,
    out_width: *mut u8,
) -> usize {
    catch_unwind(AssertUnwindSafe(|| {
        if codepoints.is_null() || len == 0 {
            if !out_width.is_null() {
                unsafe { *out_width = 0 };
            }
            return 0;
        }
        let cps = unsafe { core::slice::from_raw_parts(codepoints, len) };
        let (cluster_len, width) = crate::cluster_width(cps);
        if !out_width.is_null() {
            unsafe { *out_width = width };
        }
        cluster_len
    }))
    .unwrap_or(0)
}

/// Lines the view is scrolled up into history; 0 means the live screen.
/// Renderers use this to hide the cursor while scrolled.
#[no_mangle]
pub unsafe extern "C" fn rio_render_state_display_offset(
    state: *const RenderState,
) -> usize {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() {
            return 0;
        }
        unsafe { &*state }.display_offset()
    }))
    .unwrap_or(0)
}

/// Symbols-only Nerd Font, embedded in the library itself, so every
/// embedder can offer icon glyphs (Powerline, Font Awesome, Material, ...)
/// without shipping a font file or requiring one installed. Pair with
/// [`rio_nerd_constrain`] for patched-font-quality scaling.
static SYMBOLS_NERD_FONT: &[u8] = rio_fonts::SYMBOLS_NERD_FONT;

/// The embedded symbols-only Nerd Font as raw TTF bytes. The pointer is
/// static; never freed, valid for the process lifetime.
#[no_mangle]
pub unsafe extern "C" fn rio_symbols_nerd_font(len: *mut usize) -> *const u8 {
    if !len.is_null() {
        unsafe { *len = SYMBOLS_NERD_FONT.len() };
    }
    SYMBOLS_NERD_FONT.as_ptr()
}

/// A glyph's bounding box in pixels, y-up, origin at the pen/baseline.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct rio_glyph_box_s {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Apply the Nerd Fonts patcher's scaling/alignment rules to a glyph
/// (the same generated table sugarloaf uses). `glyph` is the
/// glyph's bounding box; `constraint_width` is how many cells are
/// horizontally free (1 or 2). Writes the adjusted box to `out` and
/// returns true when the codepoint has a rule; returns false (out
/// untouched) for codepoints the table doesn't cover.
///
/// `icon_height_single` follows the patcher heuristic
/// `(2 * cap_height + face_height) / 3`; pass the face's values so
/// single-cell icons sit on the visual x-height like patched fonts do.
#[no_mangle]
pub unsafe extern "C" fn rio_nerd_constrain(
    codepoint: u32,
    glyph: rio_glyph_box_s,
    cell_width: f64,
    cell_height: f64,
    icon_height_single: f64,
    constraint_width: u8,
    out: *mut rio_glyph_box_s,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        use rio_fonts::nerd_font::{self, GlyphSize, Metrics};
        if out.is_null() {
            return false;
        }
        let Some(constraint) = nerd_font::get_constraint(codepoint) else {
            return false;
        };
        let constrained = constraint.constrain(
            GlyphSize {
                width: glyph.width,
                height: glyph.height,
                x: glyph.x,
                y: glyph.y,
            },
            Metrics {
                face_width: cell_width,
                face_height: cell_height,
                face_y: 0.0,
                cell_width: cell_width.round() as u32,
                cell_height: cell_height.round() as u32,
                icon_height_single,
                icon_height: cell_height,
            },
            constraint_width.clamp(1, 2),
        );
        unsafe {
            *out = rio_glyph_box_s {
                x: constrained.x,
                y: constrained.y,
                width: constrained.width,
                height: constrained.height,
            };
        }
        true
    }))
    .unwrap_or(false)
}

/// A wheel scroll. librio decides what it means: a mouse report when the
/// program asked for mouse events, cursor keys on the alternate screen
/// with alternate-scroll on, otherwise the scrollback view. `lines` is
/// positive for scrolling up; `col`/`row` are the cell under the
/// pointer. Returns true when the program consumed it.
#[no_mangle]
pub unsafe extern "C" fn rio_surface_scroll_wheel(
    surface: *const Surface,
    lines: i32,
    col: u16,
    row: u16,
    mods: u8,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() {
            return false;
        }
        unsafe { &*surface }.scroll_wheel(
            lines,
            col,
            row,
            Modifiers::from_bits_truncate(mods),
        )
    }))
    .unwrap_or(false)
}

/// Report a mouse button press/release to the program if it asked for mouse
/// events. `button` is 0=left, 1=middle, 2=right; `mods` uses the same bits
/// as rio_surface_scroll_wheel. Returns true when a report reached the
/// program (the host should then skip its own selection); false when nothing
/// grabs the mouse, or shift bypasses to a local selection.
#[no_mangle]
pub unsafe extern "C" fn rio_surface_mouse_button(
    surface: *const Surface,
    col: u16,
    row: u16,
    button: u8,
    pressed: bool,
    mods: u8,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() {
            return false;
        }
        unsafe { &*surface }.mouse_button(
            col,
            row,
            button,
            pressed,
            Modifiers::from_bits_truncate(mods),
        )
    }))
    .unwrap_or(false)
}

/// Report pointer motion for button-event (1002) / any-event (1003) modes.
/// `button` is 0/1/2 for the button held during a drag, or 3 for none.
/// Returns true when a report was emitted.
#[no_mangle]
pub unsafe extern "C" fn rio_surface_mouse_motion(
    surface: *const Surface,
    col: u16,
    row: u16,
    button: u8,
    mods: u8,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() {
            return false;
        }
        unsafe { &*surface }.mouse_motion(
            col,
            row,
            button,
            Modifiers::from_bits_truncate(mods),
        )
    }))
    .unwrap_or(false)
}

/// The foreground process's name, for host-side program detection
/// (agent state, tab icons). Free with rio_text_free.
#[no_mangle]
pub unsafe extern "C" fn rio_surface_foreground_process_name(
    surface: *const Surface,
) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        if surface.is_null() {
            return std::ptr::null_mut();
        }
        let name = unsafe { &*surface }.foreground_process_name();
        CString::new(name)
            .map(CString::into_raw)
            .unwrap_or(std::ptr::null_mut())
    }))
    .unwrap_or(std::ptr::null_mut())
}

/// Whether the alternate screen (full-screen TUIs) was active at the
/// last update.
#[no_mangle]
pub unsafe extern "C" fn rio_render_state_alt_screen(state: *const RenderState) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() {
            return false;
        }
        unsafe { &*state }.alt_screen()
    }))
    .unwrap_or(false)
}

/// A kitty graphics placement resolved to viewport pixels. `x`/`y` are
/// relative to the grid origin (add your padding); `src_*` is the
/// normalized source rectangle within the image.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct rio_kitty_placement_s {
    pub image_id: u32,
    pub z_index: i32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub src_x: f32,
    pub src_y: f32,
    pub src_w: f32,
    pub src_h: f32,
}

/// Number of kitty placements in the last snapshot (visible or not);
/// iterate them with [`rio_render_state_kitty_placement`], which filters
/// to the viewport. Ordered by z-index, lowest first.
#[no_mangle]
pub unsafe extern "C" fn rio_render_state_kitty_count(
    state: *const RenderState,
) -> usize {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() {
            return 0;
        }
        unsafe { &*state }.kitty_count()
    }))
    .unwrap_or(0)
}

/// Resolve placement `index` against the current viewport with the
/// renderer's cell metrics. Returns false when it's scrolled out of view.
#[no_mangle]
pub unsafe extern "C" fn rio_render_state_kitty_placement(
    state: *const RenderState,
    index: usize,
    cell_width: f32,
    cell_height: f32,
    out: *mut rio_kitty_placement_s,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() || out.is_null() {
            return false;
        }
        let Some((image_id, z_index, geometry)) =
            unsafe { &*state }.kitty_geometry(index, cell_width, cell_height)
        else {
            return false;
        };
        unsafe {
            *out = rio_kitty_placement_s {
                image_id,
                z_index,
                x: geometry.x,
                y: geometry.y,
                width: geometry.width,
                height: geometry.height,
                // source_rect is [u0, v0, u1, v1]; the C side gets
                // origin + size, which is what image crops want.
                src_x: geometry.source_rect[0],
                src_y: geometry.source_rect[1],
                src_w: geometry.source_rect[2] - geometry.source_rect[0],
                src_h: geometry.source_rect[3] - geometry.source_rect[1],
            };
        }
        true
    }))
    .unwrap_or(false)
}

/// Pixel dimensions and a change stamp for a kitty image; the stamp
/// changes when the image is retransmitted, so decoded bitmaps can be
/// cached against it.
#[no_mangle]
pub unsafe extern "C" fn rio_render_state_kitty_image_info(
    state: *const RenderState,
    image_id: u32,
    out_width: *mut u32,
    out_height: *mut u32,
    out_stamp: *mut u64,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() {
            return false;
        }
        let Some((width, height, stamp)) = unsafe { &*state }.kitty_image_info(image_id)
        else {
            return false;
        };
        unsafe {
            if !out_width.is_null() {
                *out_width = width as u32;
            }
            if !out_height.is_null() {
                *out_height = height as u32;
            }
            if !out_stamp.is_null() {
                *out_stamp = stamp;
            }
        }
        true
    }))
    .unwrap_or(false)
}

/// Copy a kitty image into `buf` as tightly-packed RGBA8. Returns the
/// bytes written (width * height * 4), or 0 when the image is unknown or
/// `cap` is too small.
#[no_mangle]
pub unsafe extern "C" fn rio_render_state_kitty_image_rgba(
    state: *const RenderState,
    image_id: u32,
    buf: *mut u8,
    cap: usize,
) -> usize {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() || buf.is_null() {
            return 0;
        }
        let slice = unsafe { std::slice::from_raw_parts_mut(buf, cap) };
        unsafe { &*state }.kitty_image_rgba(image_id, slice)
    }))
    .unwrap_or(0)
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

/// The cursor's visual shape for this frame, as RIO_CURSOR_*: the program's
/// DECSCUSR choice (block / underline / beam), or RIO_CURSOR_HIDDEN when it
/// hid the cursor (DECTCEM) or the view is scrolled into history. Hosts that
/// paint the cursor themselves read this next to `rio_render_state_cursor`.
#[no_mangle]
pub unsafe extern "C" fn rio_render_state_cursor_shape(state: *const RenderState) -> u8 {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() {
            return RIO_CURSOR_HIDDEN;
        }
        let state = unsafe { &*state };
        if !state.cursor_visible() {
            return RIO_CURSOR_HIDDEN;
        }
        use rio_vt::ansi::CursorShape;
        match state.cursor_shape() {
            CursorShape::Block => RIO_CURSOR_BLOCK,
            CursorShape::Underline => RIO_CURSOR_UNDERLINE,
            CursorShape::Beam => RIO_CURSOR_BEAM,
            CursorShape::Hidden => RIO_CURSOR_HIDDEN,
        }
    }))
    .unwrap_or(RIO_CURSOR_HIDDEN)
}

/// This frame's dynamic (OSC 10/11/12) default colors. `which`:
/// 0 = foreground, 1 = background, 2 = cursor. Writes the resolved RGB
/// into `out` and returns true when the program has set that color via
/// OSC; returns false and leaves `out` untouched when it is unset, so the
/// host falls back to its own scheme (this also covers OSC 110/111/112
/// reset, which clears the override). Cell colors already follow the
/// override via `rio_render_state_cell` (and the GPU grid palette); the
/// host reads this only for the parts that aren't cells: the solid pane
/// ground a default-bg cell shows through, and the cursor color.
#[no_mangle]
pub unsafe extern "C" fn rio_render_state_dynamic_color(
    state: *const RenderState,
    which: u8,
    out: *mut rio_rgb_s,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() || out.is_null() {
            return false;
        }
        let named = match which {
            0 => NamedColor::Foreground,
            1 => NamedColor::Background,
            2 => NamedColor::Cursor,
            _ => return false,
        };
        match unsafe { &*state }.term_colors()[named] {
            Some(arr) => {
                let rgb = ColorRgb::from_color_arr(arr);
                unsafe {
                    *out = rio_rgb_s {
                        r: rgb.r,
                        g: rgb.g,
                        b: rgb.b,
                    };
                }
                true
            }
            None => false,
        }
    }))
    .unwrap_or(false)
}

#[cfg(test)]
mod color_tests {
    use super::*;

    #[test]
    fn resolves_indexed_palette() {
        // One guard for the whole test: the active theme is process-global
        // and another test may swap it between two separate reads.
        let t = theme_lock().read().unwrap();
        // ANSI 0-15 come from the active theme...
        assert_eq!(indexed_rgb(&t, 0), arr_rgb(t.black));
        assert_eq!(indexed_rgb(&t, 15), arr_rgb(t.light_white));
        // ...while the cube and grays stay the standard xterm ramp.
        assert_eq!(indexed_rgb(&t, 16), (0, 0, 0)); // cube origin
        assert_eq!(indexed_rgb(&t, 231), (255, 255, 255)); // cube max
        assert_eq!(indexed_rgb(&t, 232), (8, 8, 8)); // first gray
        assert_eq!(indexed_rgb(&t, 255), (238, 238, 238)); // last gray
    }

    // Default assertions, the rio_set_colors swap, and the NULL reset live
    // in ONE test: the theme is process-global, so a separate swap test
    // would race the default assertions under the parallel test runner.
    #[test]
    fn named_fills_rgb_and_follows_set_colors() {
        // Rio's default red (#FF1261), not xterm's (205, 0, 0).
        let c = color_to_c(AnsiColor::Named(NamedColor::Red));
        assert_eq!(c.kind, RIO_COLOR_NAMED);
        assert_eq!((c.r, c.g, c.b), (0xff, 0x12, 0x61));

        let bg = color_to_c(AnsiColor::Named(NamedColor::Background));
        assert_eq!((bg.r, bg.g, bg.b), (0x0f, 0x0d, 0x0e));

        // Rio's signature pink cursor.
        let cur = color_to_c(AnsiColor::Named(NamedColor::Cursor));
        assert_eq!((cur.r, cur.g, cur.b), (0xf7, 0x12, 0xff));

        // Swap in a scheme and every resolution path follows it.
        let mut scheme = rio_colors_s {
            ansi: [rio_rgb_s::default(); 16],
            foreground: rio_rgb_s {
                r: 0xf8,
                g: 0xf8,
                b: 0xf2,
            },
            background: rio_rgb_s {
                r: 0x28,
                g: 0x2a,
                b: 0x36,
            },
            cursor: rio_rgb_s {
                r: 0xff,
                g: 0xff,
                b: 0xff,
            },
        };
        scheme.ansi[1] = rio_rgb_s {
            r: 0xff,
            g: 0x55,
            b: 0x55,
        };
        unsafe { rio_set_colors(&scheme) };

        let c = color_to_c(AnsiColor::Named(NamedColor::Red));
        assert_eq!((c.r, c.g, c.b), (0xff, 0x55, 0x55));
        let c = color_to_c(AnsiColor::Indexed(1));
        assert_eq!((c.r, c.g, c.b), (0xff, 0x55, 0x55));
        // Dim falls back to 2/3 of the new base, not the old theme's.
        let c = color_to_c(AnsiColor::Named(NamedColor::DimRed));
        assert_eq!((c.r, c.g, c.b), (dim((0xff, 0x55, 0x55))));
        let bg = color_to_c(AnsiColor::Named(NamedColor::Background));
        assert_eq!((bg.r, bg.g, bg.b), (0x28, 0x2a, 0x36));

        // NULL restores the default theme.
        unsafe { rio_set_colors(std::ptr::null()) };
        let c = color_to_c(AnsiColor::Named(NamedColor::Red));
        assert_eq!((c.r, c.g, c.b), (0xff, 0x12, 0x61));
    }

    #[test]
    fn nerd_constrain_scales_icons_and_skips_plain_text() {
        // U+E0B0 (Powerline right triangle) is in the table: stretch rules.
        let glyph = rio_glyph_box_s {
            x: 0.0,
            y: -2.0,
            width: 20.0,
            height: 30.0,
        };
        let mut out = rio_glyph_box_s::default();
        let hit =
            unsafe { rio_nerd_constrain(0xE0B0, glyph, 10.0, 22.0, 18.0, 1, &mut out) };
        assert!(hit, "powerline glyphs must be constrained");
        assert!(out.width <= 10.0 + f64::EPSILON, "fits the cell width");

        // 'a' has no entry: untouched, reported as such.
        let miss =
            unsafe { rio_nerd_constrain(0x61, glyph, 10.0, 22.0, 18.0, 1, &mut out) };
        assert!(!miss);
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

#[cfg(all(test, not(target_os = "windows")))]
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

    /// Spawning with argv is what lets a host run a command without typing
    /// it, so the bytes never reach a shell's line editor. A control
    /// character in the argument must survive as data: were it interpreted,
    /// `printf` would not print it back.
    #[test]
    fn args_reach_the_child_as_argv() {
        let engine = crate::Engine::new(Arc::new(NopDelegate));
        let surface = engine
            .create_surface(&crate::SurfaceDesc {
                shell: Some("/bin/sh".into()),
                args: vec![
                    "-c".into(),
                    // A literal ^U (0x15) between two markers.
                    "printf '%s' 'AR\u{15}GV_OK'".into(),
                ],
                working_dir: None,
                cols: 80,
                rows: 24,
                pixel_width: 640,
                pixel_height: 384,
                scrollback: 1000,
            })
            .expect("surface");
        // The child writes and exits; poll rather than sleep a fixed span.
        let mut dump = String::new();
        for _ in 0..100 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            dump = surface.dump();
            if dump.contains("GV_OK") {
                break;
            }
        }
        assert!(dump.contains("AR"), "dump was: {dump:?}");
        assert!(dump.contains("GV_OK"), "dump was: {dump:?}");
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
