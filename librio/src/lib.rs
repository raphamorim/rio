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
use rio_vt::crosswords::pos::{Column as PosColumn, Line, Pos, Side};
use rio_vt::crosswords::{Crosswords, Mode};
use rio_vt::event::sync::FairMutex;
use rio_vt::event::WindowSize;
use rio_vt::event::{EventListener, Msg, RioEvent, WindowId};
use rio_vt::performer::Machine;
use rio_vt::selection::{Selection, SelectionType};
use std::borrow::Cow;
use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(target_os = "windows")]
use teletypewriter::create_pty;
#[cfg(not(target_os = "windows"))]
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

pub trait SurfaceDelegate: Send + Sync + 'static {
    fn wakeup(&self, surface: SurfaceId);
    fn action(&self, _surface: SurfaceId, _action: Action) {}
    fn clipboard_write(&self, _surface: SurfaceId, _kind: ClipboardType, _text: String) {}
    fn close_surface(&self, _surface: SurfaceId) {}
}

#[derive(Clone)]
pub(crate) struct Listener {
    surface_id: SurfaceId,
    delegate: Arc<dyn SurfaceDelegate>,
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
                if let Some(channel) = self.pty_writer.lock().unwrap().as_ref() {
                    let _ = channel.send(Msg::Input(Cow::Owned(text.into_bytes())));
                }
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
    channel: corcovado::channel::Sender<Msg>,
    #[cfg(not(target_os = "windows"))]
    shell_pid: u32,
    #[cfg(not(target_os = "windows"))]
    main_fd: std::os::fd::RawFd,
    _io_thread: std::thread::JoinHandle<(
        Machine<teletypewriter::Pty, Listener>,
        rio_vt::performer::State,
    )>,
}

impl Surface {
    fn new(
        engine: &Engine,
        desc: &SurfaceDesc,
    ) -> Result<Surface, Box<dyn Error + Send + Sync>> {
        let id = engine.next_surface_id.fetch_add(1, Ordering::SeqCst);
        let pty_writer = Arc::new(Mutex::new(None));
        let listener = Listener {
            surface_id: id,
            delegate: engine.delegate.clone(),
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
        let terminal = Arc::new(FairMutex::new(terminal));

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
        let _ = self.channel.send(Msg::Input(bytes.into()));
    }

    pub fn text(&self, text: &str) {
        self.write(text.as_bytes().to_vec());
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
        let _ = self.channel.send(Msg::Resize(WindowSize {
            rows,
            cols,
            width: pixel_width,
            height: pixel_height,
        }));
    }

    pub fn scroll(&self, delta_lines: i32) {
        use rio_vt::crosswords::grid::Scroll;
        self.terminal
            .lock()
            .scroll_display(Scroll::Delta(delta_lines));
    }

    pub fn selection_begin(&self, viewport_line: i32, col: usize, kind: SelectionKind) {
        let mut term = self.terminal.lock();
        let offset = term.display_offset() as i32;
        let pos = Pos::new(Line(viewport_line - offset), PosColumn(col));
        term.selection = Some(Selection::new(kind.to_type(), pos, Side::Left));
        term.mark_fully_damaged();
    }

    pub fn selection_update(&self, viewport_line: i32, col: usize) {
        let mut term = self.terminal.lock();
        let offset = term.display_offset() as i32;
        let pos = Pos::new(Line(viewport_line - offset), PosColumn(col));
        if let Some(selection) = &mut term.selection {
            selection.update(pos, Side::Right);
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
        #[cfg(not(target_os = "windows"))]
        {
            teletypewriter::foreground_process_path(self.main_fd, self.shell_pid)
                .ok()
                .map(|path| path.to_string_lossy().into_owned())
        }
        #[cfg(target_os = "windows")]
        None
    }

    /// Dump the whole buffer (scrollback + screen) to plain text, so a
    /// frontend can persist it and replay it as inert scrollback on
    /// restore. Trailing blank rows are trimmed by `bounds_to_string`.
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

impl Drop for Surface {
    fn drop(&mut self) {
        let _ = self.channel.send(Msg::Shutdown);
        #[cfg(not(target_os = "windows"))]
        teletypewriter::kill_pid(self.shell_pid as i32);
    }
}

#[cfg(all(test, not(target_os = "windows")))]
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

        surface.selection_begin(0, 0, SelectionKind::Simple);
        surface.selection_update(0, 9);
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
