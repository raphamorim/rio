#[cfg(feature = "pty")]
pub mod capi;

pub mod key;
mod render_state;

pub use key::{
    encode as encode_key, EncodeContext, Key, KeyAction, KeyEvent, KittyFlags, Modifiers,
};
pub use render_state::{RenderState, ViewportSelection};
pub use rio_vt::clipboard::ClipboardType;
pub use rio_vt::config::colors::{AnsiColor, ColorRgb, NamedColor};
pub use rio_vt::crosswords::pos::Column;
pub use rio_vt::crosswords::square::Square;
pub use rio_vt::crosswords::style::{Style, StyleFlags};
pub use rio_vt::selection::SelectionRange;

use rio_vt::ansi::CursorShape;
pub use rio_vt::crosswords::pos::Side;
use rio_vt::crosswords::pos::{Column as PosColumn, Line, Pos};
use rio_vt::crosswords::{Crosswords, Mode};
use rio_vt::event::sync::FairMutex;
#[cfg(feature = "pty")]
use rio_vt::event::Msg;
#[cfg(feature = "pty")]
use rio_vt::event::WindowSize;
use rio_vt::event::{EventListener, RioEvent, WindowId};
#[cfg(feature = "pty")]
use rio_vt::performer::Machine;
use rio_vt::selection::{Selection, SelectionType};
use std::borrow::Cow;
use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
#[cfg(feature = "pty")]
use std::sync::Mutex;
#[cfg(all(feature = "pty", target_os = "windows"))]
use teletypewriter::create_pty;
#[cfg(all(feature = "pty", not(target_os = "windows")))]
use teletypewriter::create_pty_with_spawn;

pub type SurfaceId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionKind {
    Simple,
    Word,
    Line,
    Block,
}

impl SelectionKind {
    fn to_type(self) -> SelectionType {
        match self {
            SelectionKind::Simple => SelectionType::Simple,
            SelectionKind::Word => SelectionType::Semantic,
            SelectionKind::Line => SelectionType::Lines,
            SelectionKind::Block => SelectionType::Block,
        }
    }
}

struct GridSize {
    rows: usize,
    cols: usize,
    cell_width: f32,
    cell_height: f32,
}

impl GridSize {
    /// Cell metrics come from the host's pixel size; graphics protocols
    /// (kitty image placements) need them to map pixels onto cells, so a
    /// zero pixel size would silently drop every placement.
    fn new(cols: usize, rows: usize, pixel_width: u16, pixel_height: u16) -> Self {
        let cell = |pixels: u16, cells: usize| {
            if pixels == 0 || cells == 0 {
                0.
            } else {
                pixels as f32 / cells as f32
            }
        };
        Self {
            rows,
            cols,
            cell_width: cell(pixel_width, cols),
            cell_height: cell(pixel_height, rows),
        }
    }
}

impl rio_vt::crosswords::grid::Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }

    fn square_width(&self) -> f32 {
        self.cell_width
    }

    fn square_height(&self) -> f32 {
        self.cell_height
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    SetTitle {
        title: String,
        subtitle: Option<String>,
    },
    RingBell,
    CursorBlinkingChange,
    /// OSC 9;4 progress (ConEmu numbering): 0 remove, 1 set, 2 error,
    /// 3 indeterminate, 4 paused. `value` is 0-100 where the state
    /// carries one.
    Progress {
        state: u8,
        value: u8,
    },
}

/// `Send + Sync` everywhere threads exist. On wasm there is one thread and
/// delegates hold JS callbacks (which are `!Send`), so the bound relaxes to
/// nothing rather than forcing unsafe impls on the embedder.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSendSync: Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync> MaybeSendSync for T {}
#[cfg(target_arch = "wasm32")]
pub trait MaybeSendSync {}
#[cfg(target_arch = "wasm32")]
impl<T> MaybeSendSync for T {}

pub trait SurfaceDelegate: MaybeSendSync + 'static {
    fn wakeup(&self, surface: SurfaceId);
    fn action(&self, _surface: SurfaceId, _action: Action) {}
    fn clipboard_write(&self, _surface: SurfaceId, _kind: ClipboardType, _text: String) {}
    fn close_surface(&self, _surface: SurfaceId) {}
    /// Bytes the terminal wants delivered to the child process. Only called
    /// on non-`pty` builds, where the host owns the transport (a WebSocket
    /// to a real shell, an in-page demo interpreter, ...); with a PTY the
    /// bytes go straight to it and this never fires.
    fn output(&self, _surface: SurfaceId, _bytes: &[u8]) {}
}

#[derive(Clone)]
pub(crate) struct Listener {
    surface_id: SurfaceId,
    delegate: Arc<dyn SurfaceDelegate>,
    #[cfg(feature = "pty")]
    pty_writer: Arc<Mutex<Option<corcovado::channel::Sender<Msg>>>>,
}

impl Listener {
    fn dispatch(&self, event: RioEvent) {
        match event {
            RioEvent::TerminalDamaged(_)
            | RioEvent::Render
            | RioEvent::RenderRoute(_) => {
                self.delegate.wakeup(self.surface_id);
            }
            RioEvent::Title(title) => {
                self.delegate.action(
                    self.surface_id,
                    Action::SetTitle {
                        title,
                        subtitle: None,
                    },
                );
            }
            RioEvent::TitleWithSubtitle(title, subtitle) => {
                self.delegate.action(
                    self.surface_id,
                    Action::SetTitle {
                        title,
                        subtitle: Some(subtitle),
                    },
                );
            }
            RioEvent::Bell => {
                self.delegate.action(self.surface_id, Action::RingBell);
            }
            RioEvent::CursorBlinkingChange | RioEvent::CursorBlinkingChangeOnRoute(_) => {
                self.delegate
                    .action(self.surface_id, Action::CursorBlinkingChange);
            }
            RioEvent::ClipboardStore(kind, text) => {
                self.delegate.clipboard_write(self.surface_id, kind, text);
            }
            RioEvent::PtyWrite(_, text) => {
                #[cfg(feature = "pty")]
                if let Some(channel) = self.pty_writer.lock().unwrap().as_ref() {
                    let _ = channel.send(Msg::Input(Cow::Owned(text.into_bytes())));
                }
                #[cfg(not(feature = "pty"))]
                self.delegate.output(self.surface_id, text.as_bytes());
            }
            RioEvent::CloseTerminal(_) | RioEvent::Exit => {
                self.delegate.close_surface(self.surface_id);
            }
            RioEvent::ProgressReport(report) => {
                use rio_vt::event::ProgressState;
                let state = match report.state {
                    ProgressState::Remove => 0,
                    ProgressState::Set => 1,
                    ProgressState::Error => 2,
                    ProgressState::Indeterminate => 3,
                    ProgressState::Pause => 4,
                };
                self.delegate.action(
                    self.surface_id,
                    Action::Progress {
                        state,
                        value: report.progress.unwrap_or(0),
                    },
                );
            }
            _ => {}
        }
    }
}

impl EventListener for Listener {
    fn send_event(&self, event: RioEvent, _id: WindowId) {
        self.dispatch(event);
    }

    fn send_event_with_high_priority(&self, event: RioEvent, _id: WindowId) {
        self.dispatch(event);
    }
}

#[derive(Debug, Clone)]
pub struct SurfaceDesc {
    pub shell: Option<String>,
    pub args: Vec<String>,
    pub working_dir: Option<String>,
    pub cols: u16,
    pub rows: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
    pub scrollback: usize,
}

impl Default for SurfaceDesc {
    fn default() -> Self {
        Self {
            shell: None,
            args: Vec::new(),
            working_dir: None,
            cols: 80,
            rows: 24,
            pixel_width: 720,
            pixel_height: 432,
            scrollback: 10_000,
        }
    }
}

pub struct Engine {
    delegate: Arc<dyn SurfaceDelegate>,
    next_surface_id: AtomicUsize,
}

impl Engine {
    pub fn new(delegate: Arc<dyn SurfaceDelegate>) -> Self {
        Self {
            delegate,
            next_surface_id: AtomicUsize::new(1),
        }
    }

    pub fn create_surface(
        &self,
        desc: &SurfaceDesc,
    ) -> Result<Surface, Box<dyn Error + Send + Sync>> {
        Surface::new(self, desc)
    }
}

/// Translate the terminal's kitty keyboard flags into the encoder's own set.
/// Only the flags that change what a key produces are carried across; see the
/// note in [`key`] about the two that are not implemented.
fn kitty_flags(modes: rio_vt::ansi::KeyboardModes) -> key::KittyFlags {
    use rio_vt::ansi::KeyboardModes;
    let mut flags = key::KittyFlags::empty();
    if modes.contains(KeyboardModes::DISAMBIGUATE_ESC_CODES) {
        flags |= key::KittyFlags::DISAMBIGUATE;
    }
    if modes.contains(KeyboardModes::REPORT_EVENT_TYPES) {
        flags |= key::KittyFlags::REPORT_EVENT_TYPES;
    }
    if modes.contains(KeyboardModes::REPORT_ALL_KEYS_AS_ESC) {
        flags |= key::KittyFlags::REPORT_ALL_AS_ESC;
    }
    flags
}

pub struct Surface {
    id: SurfaceId,
    alt_is_meta: std::sync::atomic::AtomicBool,
    terminal: Arc<FairMutex<Crosswords<Listener>>>,
    /// Non-pty transport: `write` hands the bytes to the delegate instead
    /// of a PTY channel.
    #[cfg(not(feature = "pty"))]
    delegate: Arc<dyn SurfaceDelegate>,
    #[cfg(feature = "pty")]
    channel: corcovado::channel::Sender<Msg>,
    #[cfg(all(feature = "pty", not(target_os = "windows")))]
    shell_pid: u32,
    #[cfg(all(feature = "pty", not(target_os = "windows")))]
    main_fd: std::os::fd::RawFd,
    #[cfg(feature = "pty")]
    _io_thread: std::thread::JoinHandle<(
        Machine<teletypewriter::Pty, Listener>,
        rio_vt::performer::State,
    )>,
}

/// Encode one mouse report. SGR (`CSI < b ; x ; y M`) when the program
/// asked for it, else the original X10 form, whose coordinates are
/// offset by 32 and cannot exceed 223 without the UTF-8 extension.
fn mouse_report(button: u8, col: u16, row: u16, sgr: bool, utf8: bool) -> Vec<u8> {
    let x = col.saturating_add(1);
    let y = row.saturating_add(1);
    if sgr {
        return format!("\x1b[<{button};{x};{y}M").into_bytes();
    }
    let mut out = vec![0x1b, b'[', b'M', 32u8.saturating_add(button)];
    for value in [x, y] {
        if utf8 && value >= 95 {
            // Two-byte UTF-8 for the extended range.
            let encoded = char::from_u32(32 + value as u32).unwrap_or('\u{20}');
            let mut buffer = [0u8; 4];
            out.extend_from_slice(encoded.encode_utf8(&mut buffer).as_bytes());
        } else {
            out.push(32u8.saturating_add(value.min(223) as u8));
        }
    }
    out
}

impl Surface {
    fn new(
        engine: &Engine,
        desc: &SurfaceDesc,
    ) -> Result<Surface, Box<dyn Error + Send + Sync>> {
        let id = engine.next_surface_id.fetch_add(1, Ordering::SeqCst);
        #[cfg(feature = "pty")]
        let pty_writer = Arc::new(Mutex::new(None));
        let listener = Listener {
            surface_id: id,
            delegate: engine.delegate.clone(),
            #[cfg(feature = "pty")]
            pty_writer: pty_writer.clone(),
        };

        let terminal = Crosswords::new(
            GridSize::new(
                desc.cols as usize,
                desc.rows as usize,
                desc.pixel_width,
                desc.pixel_height,
            ),
            CursorShape::Block,
            listener.clone(),
            WindowId::from(id as u64),
            id,
            desc.scrollback,
        );
        // On wasm the delegate (and so the whole graph) is single-threaded
        // by design; Arc stays because the pty build shares it with the IO
        // thread and the API is one type on every target.
        #[allow(clippy::arc_with_non_send_sync)]
        let terminal = Arc::new(FairMutex::new(terminal));

        #[cfg(not(feature = "pty"))]
        {
            Ok(Surface {
                id,
                // Terminals default alt to meta; the host may override it.
                alt_is_meta: std::sync::atomic::AtomicBool::new(true),
                terminal,
                delegate: engine.delegate.clone(),
            })
        }

        #[cfg(feature = "pty")]
        {
            // No shell in the descriptor means "whatever the user's default is",
            // which teletypewriter resolves (and starts as a login shell).
            let shell = desc.shell.as_deref();

            // The child inherits the host process's environment, which for GUI
            // hosts has no TERM at all (or a stale one). Resolve it the way rio
            // does: prefer rio's terminfo when it's installed, else fall back to
            // the universally known xterm-256color so local prompts and remote
            // ssh sessions both keep working.
            #[cfg(not(target_os = "windows"))]
            let env = {
                let terminfo = match (
                    teletypewriter::terminfo_exists("xterm-rio"),
                    teletypewriter::terminfo_exists("rio"),
                ) {
                    (true, _) => "xterm-rio",
                    (false, true) => "rio",
                    (false, false) => "xterm-256color",
                };
                Some(vec![
                    ("TERM".to_string(), terminfo.to_string()),
                    ("COLORTERM".to_string(), "truecolor".to_string()),
                ])
            };

            #[cfg(not(target_os = "windows"))]
            let pty = create_pty_with_spawn(
                shell,
                desc.args.clone(),
                &desc.working_dir,
                env,
                desc.cols,
                desc.rows,
                desc.pixel_width,
                desc.pixel_height,
            )
            .map_err(|err| Box::new(err) as Box<dyn Error + Send + Sync>)?;

            #[cfg(target_os = "windows")]
            let pty = create_pty(
                shell,
                desc.args.clone(),
                &desc.working_dir,
                None,
                desc.cols,
                desc.rows,
            )
            .map_err(|err| Box::new(err) as Box<dyn Error + Send + Sync>)?;

            #[cfg(not(target_os = "windows"))]
            let shell_pid = *pty.child.pid.clone() as u32;
            #[cfg(not(target_os = "windows"))]
            let main_fd = *pty.child.id;

            let machine = Machine::new(
                Arc::clone(&terminal),
                pty,
                listener,
                WindowId::from(id as u64),
                id,
            )
            .map_err(|err| std::io::Error::other(err.to_string()))?;
            let channel = machine.channel();
            *pty_writer.lock().unwrap() = Some(channel.clone());
            let io_thread = machine.spawn();

            Ok(Surface {
                id,
                // Terminals default alt to meta; the host may override it.
                alt_is_meta: std::sync::atomic::AtomicBool::new(true),
                terminal,
                channel,
                #[cfg(not(target_os = "windows"))]
                shell_pid,
                #[cfg(not(target_os = "windows"))]
                main_fd,
                _io_thread: io_thread,
            })
        }
    }

    pub fn id(&self) -> SurfaceId {
        self.id
    }

    pub fn write<B: Into<Cow<'static, [u8]>>>(&self, bytes: B) {
        // Input snaps the view back to the live screen and drops any
        // selection, matching ghostty's scroll-to-bottom / clear-on-typing
        // behavior (only reached when a key actually produced PTY bytes).
        {
            use rio_vt::crosswords::grid::Scroll;
            let mut term = self.terminal.lock();
            if term.display_offset() != 0 {
                term.scroll_display(Scroll::Bottom);
            }
            if term.selection.is_some() {
                term.selection = None;
            }
        }
        #[cfg(feature = "pty")]
        let _ = self.channel.send(Msg::Input(bytes.into()));
        #[cfg(not(feature = "pty"))]
        self.delegate.output(self.id, &bytes.into());
    }

    pub fn text(&self, text: &str) {
        self.write(text.as_bytes().to_vec());
    }

    /// Paste text the way terminals do: newlines normalized to CR, and the
    /// whole run wrapped in bracketed-paste markers when the program asked
    /// for them (so shells and editors can treat it as one atomic paste).
    pub fn paste(&self, text: &str) {
        let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
        let bracketed = self.terminal.lock().mode().contains(Mode::BRACKETED_PASTE);
        if bracketed {
            self.write(format!("\x1b[200~{normalized}\x1b[201~").into_bytes());
        } else {
            self.write(normalized.into_bytes());
        }
    }

    /// A stable, C-friendly view of the terminal modes an embedder needs
    /// for input decisions it makes on its own (touch scrolling, key bars).
    /// Bit 0 mouse reporting, bit 1 application cursor keys, bit 2
    /// alternate screen, bit 3 bracketed paste.
    pub fn mode_bits(&self) -> u32 {
        let mode = self.terminal.lock().mode();
        let mut bits = 0;
        if mode.intersects(Mode::MOUSE_MODE) {
            bits |= 1;
        }
        if mode.contains(Mode::APP_CURSOR) {
            bits |= 1 << 1;
        }
        if mode.contains(Mode::ALT_SCREEN) {
            bits |= 1 << 2;
        }
        if mode.contains(Mode::BRACKETED_PASTE) {
            bits |= 1 << 3;
        }
        bits
    }

    /// Whether alt acts as meta, prefixing with ESC, instead of letting the
    /// platform's text through. See [`key::EncodeContext::alt_is_meta`].
    pub fn set_alt_is_meta(&self, enabled: bool) {
        self.alt_is_meta.store(enabled, Ordering::Relaxed);
    }

    pub fn alt_is_meta(&self) -> bool {
        self.alt_is_meta.load(Ordering::Relaxed)
    }

    pub fn key(&self, event: &KeyEvent) -> bool {
        // The encoding depends on terminal state the embedder does not track,
        // which is the reason this lives here and not in the host.
        let ctx = {
            let terminal = self.terminal.lock();
            key::EncodeContext {
                app_cursor: terminal.mode().contains(Mode::APP_CURSOR),
                kitty: kitty_flags(terminal.keyboard_mode()),
                modify_other_keys: terminal.modify_other_keys(),
                alt_is_meta: self.alt_is_meta.load(Ordering::Relaxed),
            }
        };
        match key::encode(event, &ctx) {
            Some(bytes) => {
                self.write(bytes);
                true
            }
            None => false,
        }
    }

    pub fn resize(&self, cols: u16, rows: u16, pixel_width: u16, pixel_height: u16) {
        self.terminal.lock().resize(GridSize::new(
            cols as usize,
            rows as usize,
            pixel_width,
            pixel_height,
        ));
        #[cfg(feature = "pty")]
        let _ = self.channel.send(Msg::Resize(WindowSize {
            rows,
            cols,
            width: pixel_width,
            height: pixel_height,
        }));
    }

    /// A wheel scroll, dispatched the way terminals do it: the program
    /// running in the terminal gets first claim.
    ///
    /// Three cases, in order (the same order rio and ghostty use):
    /// mouse reporting on, so the wheel is a mouse event; the alternate
    /// screen with alternate-scroll on, where there is no scrollback to
    /// move so the wheel becomes cursor keys and pagers scroll; and
    /// otherwise the host's scrollback view. Holding shift always means
    /// "give me the scrollback", overriding the first two.
    ///
    /// `lines` is positive for scrolling up (towards history). `col` and
    /// `row` are the cell under the pointer, needed by mouse reports.
    /// Returns true when the program consumed it, false when the
    /// scrollback moved instead.
    pub fn scroll_wheel(&self, lines: i32, col: u16, row: u16, mods: Modifiers) -> bool {
        if lines == 0 {
            return false;
        }
        let (mouse_mode, alt_screen, alt_scroll, app_cursor, sgr, utf8) = {
            let terminal = self.terminal.lock();
            let mode = terminal.mode();
            (
                mode.intersects(Mode::MOUSE_MODE),
                mode.contains(Mode::ALT_SCREEN),
                mode.contains(Mode::ALTERNATE_SCROLL),
                mode.contains(Mode::APP_CURSOR),
                mode.contains(Mode::SGR_MOUSE),
                mode.contains(Mode::UTF8_MOUSE),
            )
        };
        let shift = mods.contains(Modifiers::SHIFT);

        if mouse_mode && !shift {
            // Wheel buttons are 64 (up) and 65 (down), with the modifier
            // bits every mouse report carries.
            let mut button = if lines > 0 { 64 } else { 65 };
            if mods.contains(Modifiers::SHIFT) {
                button += 4;
            }
            if mods.contains(Modifiers::ALT) {
                button += 8;
            }
            if mods.contains(Modifiers::CTRL) {
                button += 16;
            }
            let mut out = Vec::new();
            for _ in 0..lines.abs() {
                out.extend_from_slice(&mouse_report(button, col, row, sgr, utf8));
            }
            self.write(out);
            return true;
        }

        if alt_screen && alt_scroll && !shift {
            let up = lines > 0;
            let seq: &[u8] = match (app_cursor, up) {
                (true, true) => b"\x1bOA",
                (true, false) => b"\x1bOB",
                (false, true) => b"\x1b[A",
                (false, false) => b"\x1b[B",
            };
            let mut out = Vec::with_capacity(seq.len() * lines.unsigned_abs() as usize);
            for _ in 0..lines.abs() {
                out.extend_from_slice(seq);
            }
            self.write(out);
            return true;
        }

        self.scroll(lines);
        false
    }

    pub fn scroll(&self, delta_lines: i32) {
        use rio_vt::crosswords::grid::Scroll;
        self.terminal
            .lock()
            .scroll_display(Scroll::Delta(delta_lines));
    }

    /// Begin a selection. `side` says which half of the cell the pointer
    /// is on, which is what decides whether that cell is inside the
    /// selection; assuming a side makes cells at the ends of a drag
    /// unreachable in one direction.
    pub fn selection_begin(
        &self,
        viewport_line: i32,
        col: usize,
        kind: SelectionKind,
        side: Side,
    ) {
        let mut term = self.terminal.lock();
        let offset = term.display_offset() as i32;
        let pos = Pos::new(Line(viewport_line - offset), PosColumn(col));
        term.selection = Some(Selection::new(kind.to_type(), pos, side));
        term.mark_fully_damaged();
    }

    pub fn selection_update(&self, viewport_line: i32, col: usize, side: Side) {
        let mut term = self.terminal.lock();
        let offset = term.display_offset() as i32;
        let pos = Pos::new(Line(viewport_line - offset), PosColumn(col));
        if let Some(selection) = &mut term.selection {
            selection.update(pos, side);
            term.mark_fully_damaged();
        }
    }

    pub fn selection_clear(&self) {
        let mut term = self.terminal.lock();
        if term.selection.take().is_some() {
            term.mark_fully_damaged();
        }
    }

    pub fn selection_text(&self) -> Option<String> {
        self.terminal.lock().selection_to_string()
    }

    /// The shell's current working directory: OSC 7 when the shell reports
    /// it, otherwise the OS's view of the foreground process's cwd (so it
    /// works without any shell integration). Used by session persistence to
    /// restore each surface in the directory it was left in.
    pub fn working_dir(&self) -> Option<String> {
        let reported = self
            .terminal
            .lock()
            .current_directory
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned());
        if reported.is_some() {
            return reported;
        }
        #[cfg(all(feature = "pty", not(target_os = "windows")))]
        {
            teletypewriter::foreground_process_path(self.main_fd, self.shell_pid)
                .ok()
                .map(|path| path.to_string_lossy().into_owned())
        }
        #[cfg(any(not(feature = "pty"), target_os = "windows"))]
        None
    }

    /// Dump the whole buffer (scrollback + screen) to plain text, so a
    /// frontend can persist it and replay it as inert scrollback on
    /// restore. Trailing blank rows are trimmed by `bounds_to_string`.
    /// The foreground process's name (the program the user is running
    /// right now: `claude`, `vim`, or the shell itself), from the kernel.
    /// Hosts use it to tell what a pane is running without any shell
    /// integration.
    #[cfg(feature = "pty")]
    pub fn foreground_process_name(&self) -> String {
        teletypewriter::foreground_process_name(self.main_fd, self.shell_pid)
    }

    /// Inject bytes into the terminal's DISPLAY (the VT parser), as if they
    /// came from the child process — NOT into the PTY input. Used to replay
    /// saved scrollback on restore; the shell never sees these bytes, so it
    /// can't execute them. Bytes are plain output (convert `\n` to `\r\n`
    /// upstream if you want proper line starts).
    pub fn inject_output(&self, bytes: &[u8]) {
        use rio_vt::performer::handler::Processor;
        let mut term = self.terminal.lock();
        let mut processor = Processor::default();
        processor.advance(&mut *term, bytes);
    }

    pub fn dump(&self) -> String {
        let term = self.terminal.lock();
        let rows = term.screen_lines() as i32;
        let cols = term.columns();
        if rows == 0 || cols == 0 {
            return String::new();
        }
        let history = term.history_size() as i32;
        // Scrollback lives in negative line coordinates above the screen.
        let start = Pos::new(Line(-history), PosColumn(0));
        let end = Pos::new(Line(rows - 1), PosColumn(cols - 1));
        term.bounds_to_string(start, end)
    }

    pub(crate) fn terminal(&self) -> Arc<FairMutex<Crosswords<Listener>>> {
        self.terminal.clone()
    }
}

#[cfg(feature = "pty")]
impl Drop for Surface {
    fn drop(&mut self) {
        let _ = self.channel.send(Msg::Shutdown);
        #[cfg(not(target_os = "windows"))]
        teletypewriter::kill_pid(self.shell_pid as i32);
    }
}

#[cfg(all(test, feature = "pty", not(target_os = "windows")))]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::{Duration, Instant};

    struct CountingDelegate {
        wakeups: AtomicUsize,
    }

    impl SurfaceDelegate for CountingDelegate {
        fn wakeup(&self, _surface: SurfaceId) {
            self.wakeups.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct ActionRecorder {
        actions: Mutex<Vec<Action>>,
    }

    impl SurfaceDelegate for ActionRecorder {
        fn wakeup(&self, _surface: SurfaceId) {}
        fn action(&self, _surface: SurfaceId, action: Action) {
            self.actions.lock().unwrap().push(action);
        }
    }

    // The wheel means different things to different programs. A pager on
    // the alternate screen wants cursor keys (there is no scrollback to
    // move), a mouse-aware program wants a mouse report, and a plain
    // shell wants the scrollback view. Shift always means the last one.
    #[test]
    fn wheel_becomes_cursor_keys_on_the_alternate_screen() {
        let engine = Engine::new(Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        }));
        let surface = engine
            .create_surface(&SurfaceDesc::default())
            .expect("spawn shell");

        // Alternate screen + alternate scroll, as pagers and TUIs set it.
        surface.inject_output(b"\x1b[?1049h\x1b[?1007h");
        assert!(surface.scroll_wheel(3, 0, 0, Modifiers::empty()));

        // Application cursor mode swaps CSI for SS3.
        surface.inject_output(b"\x1b[?1h");
        assert!(surface.scroll_wheel(-1, 0, 0, Modifiers::empty()));

        // Shift is the user asking for the scrollback regardless.
        assert!(!surface.scroll_wheel(3, 0, 0, Modifiers::SHIFT));
    }

    #[test]
    fn wheel_becomes_a_mouse_report_when_the_program_asks() {
        let engine = Engine::new(Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        }));
        let surface = engine
            .create_surface(&SurfaceDesc::default())
            .expect("spawn shell");

        surface.inject_output(b"\x1b[?1000h\x1b[?1006h");
        assert!(surface.scroll_wheel(1, 4, 2, Modifiers::empty()));
        assert!(!surface.scroll_wheel(1, 4, 2, Modifiers::SHIFT));
    }

    #[test]
    fn wheel_scrolls_the_view_in_a_plain_shell() {
        let engine = Engine::new(Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        }));
        let surface = engine
            .create_surface(&SurfaceDesc::default())
            .expect("spawn shell");
        let mut state = RenderState::new(&surface);

        let mut text = String::new();
        for i in 0..80 {
            text.push_str(&format!("line {i}\r\n"));
        }
        surface.inject_output(text.as_bytes());

        assert!(!surface.scroll_wheel(5, 0, 0, Modifiers::empty()));
        state.update();
        assert!(state.display_offset() > 0, "the view should have moved");
    }

    // SGR is the modern form; the X10 fallback offsets by 32.
    #[test]
    fn mouse_reports_encode_both_forms() {
        assert_eq!(
            mouse_report(64, 4, 2, true, false),
            b"\x1b[<64;5;3M".to_vec()
        );
        assert_eq!(
            mouse_report(65, 0, 0, false, false),
            vec![0x1b, b'[', b'M', 32 + 65, 33, 33]
        );
    }

    // Dragging right to left has to be able to reach the first column.
    // The side says which half of the cell the pointer is on; with it
    // hardcoded to the right, column 0 could never be included because
    // the drag would have to pass a point left of the screen.
    #[test]
    fn a_backwards_drag_reaches_the_first_column() {
        let delegate = Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        });
        let engine = Engine::new(delegate);
        let surface = engine
            .create_surface(&SurfaceDesc::default())
            .expect("spawn shell");

        surface.inject_output(b"\x1b[H\x1b[2JABCDEF");

        // Press inside the right half of column 5, drag left to column 0.
        surface.selection_begin(0, 5, SelectionKind::Simple, Side::Right);
        surface.selection_update(0, 0, Side::Left);
        let text = surface.selection_text().unwrap_or_default();
        assert!(
            text.starts_with('A'),
            "backwards drag should include column 0, got {text:?}"
        );
    }

    // Paste wraps in bracketed-paste markers exactly when the program
    // turned the mode on, and newlines never reach the shell as LF.
    #[test]
    fn paste_brackets_when_the_program_asks() {
        let delegate = Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        });
        let engine = Engine::new(delegate);
        let surface = engine
            .create_surface(&SurfaceDesc::default())
            .expect("spawn shell");

        assert_eq!(surface.mode_bits() & (1 << 3), 0);
        surface.inject_output(b"\x1b[?2004h");
        assert_eq!(surface.mode_bits() & (1 << 3), 1 << 3);
        surface.inject_output(b"\x1b[?1h\x1b[?1049h\x1b[?1000h");
        assert_eq!(surface.mode_bits(), 0b1111);
    }

    // OSC 9;4 (ConEmu progress) must reach the embedder as an action:
    // set with a value, then remove.
    #[test]
    fn progress_reports_reach_the_delegate() {
        let delegate = Arc::new(ActionRecorder {
            actions: Mutex::new(Vec::new()),
        });
        let engine = Engine::new(delegate.clone());
        let surface = engine
            .create_surface(&SurfaceDesc::default())
            .expect("spawn shell");

        surface.inject_output(b"\x1b]9;4;1;42\x07");
        surface.inject_output(b"\x1b]9;4;0\x07");

        let actions = delegate.actions.lock().unwrap();
        let progress: Vec<&Action> = actions
            .iter()
            .filter(|a| matches!(a, Action::Progress { .. }))
            .collect();
        assert_eq!(
            progress,
            vec![
                &Action::Progress {
                    state: 1,
                    value: 42
                },
                &Action::Progress { state: 0, value: 0 },
            ]
        );
    }

    #[test]
    fn drives_a_real_shell_and_reads_cells() {
        let delegate = Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        });
        let engine = Engine::new(delegate.clone());
        let desc = SurfaceDesc::default();
        let surface = engine.create_surface(&desc).expect("spawn shell");
        let mut state = RenderState::new(&surface);

        std::thread::sleep(Duration::from_millis(400));
        surface.text("printf '%s%s\\n' li brio-gate\r");

        let deadline = Instant::now() + Duration::from_secs(8);
        let mut found = false;
        while Instant::now() < deadline {
            state.update();
            let lines = state.lines();
            if (0..lines).any(|i| state.text_row(i).contains("librio-gate")) {
                found = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }

        if !found {
            let rows: Vec<String> = (0..6).map(|i| state.text_row(i)).collect();
            panic!(
                "expected shell output in grid; wakeups={} rows={:?}",
                delegate.wakeups.load(Ordering::SeqCst),
                rows
            );
        }
        assert!(delegate.wakeups.load(Ordering::SeqCst) > 0);

        surface.selection_begin(0, 0, SelectionKind::Simple, Side::Left);
        surface.selection_update(0, 9, Side::Right);
        let text = surface.selection_text().expect("selection text");
        assert!(!text.is_empty());
        state.update();
        assert!(state.selection().is_some());
        surface.selection_clear();
        assert!(surface.selection_text().is_none());
    }

    // Typing while scrolled into history must snap the view back to the
    // live screen (and hide-cursor logic keys off the same offset).
    #[test]
    fn input_scrolls_back_to_live_screen() {
        let delegate = Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        });
        let engine = Engine::new(delegate);
        let surface = engine
            .create_surface(&SurfaceDesc::default())
            .expect("spawn shell");
        let mut state = RenderState::new(&surface);

        // Three screens of output builds scrollback to scroll into.
        let mut text = String::new();
        for i in 0..72 {
            text.push_str(&format!("line {i}\r\n"));
        }
        surface.inject_output(text.as_bytes());
        surface.scroll(10);
        state.update();
        assert!(
            state.display_offset() > 0,
            "scroll(10) should enter history"
        );

        surface.write(b"x".to_vec());
        state.update();
        assert_eq!(state.display_offset(), 0, "input should snap to bottom");
    }

    // Erase fills (EL/ED with a colored bg, htop's header bar, `clear`)
    // produce bg-only cells that encode the color inline instead of a
    // style id; the snapshot accessors must decode them, not read the
    // color bits as a style-table index.
    #[test]
    fn erase_fills_resolve_inline_bg() {
        use rio_vt::config::colors::{AnsiColor, ColorRgb};

        let delegate = Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        });
        let engine = Engine::new(delegate);
        let surface = engine
            .create_surface(&SurfaceDesc::default())
            .expect("spawn shell");
        let mut state = RenderState::new(&surface);

        // Rows 5-6 (below any shell prompt): green bg + erase-to-EOL,
        // then a truecolor bg + erase-whole-line.
        surface.inject_output(
            b"\x1b[6;1H\x1b[42mA\x1b[K\r\n\x1b[48;2;9;8;7m\x1b[2KB\x1b[0m",
        );
        state.update();

        let last = state.columns() - 1;
        let el_fill = state.style_of(state.square(5, last).unwrap());
        assert_eq!(el_fill.bg, AnsiColor::Indexed(2));

        let el2_fill = state.style_of(state.square(6, last).unwrap());
        assert_eq!(el2_fill.bg, AnsiColor::Spec(ColorRgb { r: 9, g: 8, b: 7 }));

        // Snapshot text renders the fills as trimmable spaces, not NULs.
        assert_eq!(state.text_row(5), "A");
        assert_eq!(state.text_row(6), "B");
    }

    // A kitty graphics transmit-and-display (a=T) must surface through
    // the render-state snapshot: placement geometry resolved against the
    // viewport, image dimensions, and an RGBA copy for the renderer.
    #[test]
    fn kitty_image_reaches_render_state() {
        let delegate = Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        });
        let engine = Engine::new(delegate);
        let surface = engine
            .create_surface(&SurfaceDesc::default())
            .expect("spawn shell");
        let mut state = RenderState::new(&surface);

        // 2x2 RGBA (red, green, blue, white) placed at row 6, col 5.
        let pixels: [u8; 16] = [
            0xFF, 0x00, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF, //
            0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        ];
        surface.inject_output(
            b"\x1b[6;5H\x1b_Gf=32,s=2,v=2,i=7,a=T;/wAA/wD/AP8AAP///////w==\x1b\\",
        );
        state.update();

        assert_eq!(state.kitty_count(), 1);
        let (image_id, z_index, geometry) = state
            .kitty_geometry(0, 8.0, 16.0)
            .expect("placement in view");
        assert_eq!(image_id, 7);
        assert_eq!(z_index, 0);
        assert_eq!(geometry.x, 4.0 * 8.0);
        assert_eq!(geometry.y, 5.0 * 16.0);
        assert_eq!(geometry.width, 2.0);
        assert_eq!(geometry.height, 2.0);
        assert_eq!(geometry.source_rect, [0.0, 0.0, 1.0, 1.0]);

        let (width, height, _stamp) = state.kitty_image_info(7).expect("stored image");
        assert_eq!((width, height), (2, 2));

        let mut buf = [0u8; 16];
        assert_eq!(state.kitty_image_rgba(7, &mut buf), 16);
        assert_eq!(buf, pixels);

        // Too-small buffers are refused rather than partially filled.
        let mut small = [0u8; 4];
        assert_eq!(state.kitty_image_rgba(7, &mut small), 0);
    }

    // Same sequence the live repro used: deep scrollback, clear, then a
    // kitty transmit+display. The placement must resolve on screen.
    #[test]
    fn kitty_geometry_survives_scrollback() {
        let delegate = Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        });
        let engine = Engine::new(delegate);
        // A tiny scrollback forces ring eviction: kitty dest_rows count
        // evicted lines too, so the viewport math must include them or
        // every placement in a long-lived session drifts off-screen.
        let surface = engine
            .create_surface(&SurfaceDesc {
                scrollback: 16,
                ..SurfaceDesc::default()
            })
            .expect("spawn shell");
        let mut state = RenderState::new(&surface);

        let mut text = String::new();
        for i in 0..200 {
            text.push_str(&format!("line {i}\r\n"));
        }
        surface.inject_output(text.as_bytes());
        surface.inject_output(b"\x1b[2J\x1b[H");
        surface
            .inject_output(b"\x1b_Gf=32,s=2,v=2,i=7,a=T;/wAA/wD/AP8AAP///////w==\x1b\\");
        state.update();

        assert_eq!(state.kitty_count(), 1);
        let (image_id, _z, geometry) = state
            .kitty_geometry(0, 8.0, 16.0)
            .expect("placement visible after scrollback");
        assert_eq!(image_id, 7);
        assert_eq!(geometry.y, 0.0);
    }

    // Virtual placements (`U=1`): the image is registered but only drawn
    // where the application prints U+10EEEE placeholder cells whose fg
    // color + combining diacritics say which image/row/column each cell
    // shows. This is what `kitten icat --unicode-placeholder` (and yazi
    // under a multiplexer) emits.
    #[test]
    fn kitty_virtual_placeholders_resolve_runs() {
        use rio_vt::ansi::kitty_virtual::encode_placeholder;

        let delegate = Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        });
        let engine = Engine::new(delegate);
        let surface = engine
            .create_surface(&SurfaceDesc::default())
            .expect("spawn shell");
        let mut state = RenderState::new(&surface);

        // 2x2 RGBA transmitted as a virtual placement spanning 2 cols x 1
        // row, then a run of two placeholder cells (image row 0, cols 0-1)
        // with fg palette index 7 = image id 7.
        let mut text = String::from(
            "\x1b[2J\x1b[H\x1b_Gf=32,s=2,v=2,i=7,a=T,U=1,c=2,r=1;/wAA/wD/AP8AAP///////w==\x1b\\",
        );
        text.push_str("\x1b[4;3H\x1b[38;5;7m");
        text.push_str(&encode_placeholder(0, 0, None));
        text.push_str(&encode_placeholder(0, 1, None));
        text.push_str("\x1b[39m");
        surface.inject_output(text.as_bytes());
        state.update();

        assert_eq!(state.kitty_count(), 1);
        let (image_id, z_index, geometry) = state
            .kitty_geometry(0, 8.0, 16.0)
            .expect("run resolves to geometry");
        assert_eq!(image_id, 7);
        assert_eq!(z_index, -1, "virtual placements draw under text");
        // Placement box: 2 cols x 1 row of 8x16 cells = 16x16 px; the 2x2
        // image aspect-fits to exactly 16x16, and the run starts at cell
        // (row 3, col 2), i.e. pixel (16, 48).
        assert_eq!(geometry.x, 16.0);
        assert_eq!(geometry.y, 48.0);
        assert_eq!(geometry.width, 16.0);
        assert_eq!(geometry.height, 16.0);
        assert_eq!(geometry.source_rect, [0.0, 0.0, 1.0, 1.0]);

        // The RGBA copy path serves virtual images the same way.
        let mut buf = [0u8; 16];
        assert_eq!(state.kitty_image_rgba(7, &mut buf), 16);
    }
}
