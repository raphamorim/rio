//! One terminal: rio-vt's `Crosswords` and the PTY machine that feeds it,
//! and the listener that turns its events into main-loop events.

use rio_backend::config::Config;
use rio_vt::clipboard::ClipboardType;
use rio_vt::config::colors::term::List;
use rio_vt::crosswords::grid::Scroll;
use rio_vt::crosswords::pos::{Column, Line, Pos, Side};
use rio_vt::crosswords::{Crosswords, CrosswordsSize, Mode};
use rio_vt::event::sync::TerminalLock;
use rio_vt::event::{EventListener, Msg, RioEvent, WindowId, WindowSize};
use rio_vt::key::{EncodeContext, KeyEvent, KittyFlags, Modifiers};
use rio_vt::performer::Machine;
use rio_vt::selection::{Selection, SelectionType};
use std::borrow::Cow;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

pub type Terminal = Arc<TerminalLock<Crosswords<Listener>>>;
pub type Formatter = Arc<dyn Fn(&str) -> String + Sync + Send + 'static>;

pub enum Event {
    Wakeup,
    Graphics(rio_vt::ansi::graphics::UpdateQueues),
    Title(String),
    Bell,
    Clipboard(ClipboardType, String),
    ClipboardLoad(ClipboardType, Formatter),
    CursorBlinking,
    Progress(Option<u8>, u8),
    Notification(String, String),
    CommandFinished(Option<i32>, std::time::Duration),
    Exited,
}

pub trait Sink: Send + Sync + 'static {
    fn send(&self, id: usize, event: Event);
    /// Whether a wakeup for `id` should go out: false while one is queued.
    fn wake(&self, id: usize) -> bool;
}

#[derive(Clone)]
pub struct Listener {
    id: usize,
    sink: Arc<dyn Sink>,
    pty: Arc<Mutex<Option<corcovado::channel::Sender<Msg>>>>,
    /// Text area in pixels, for XTWINOPS size reports.
    pixels: Arc<AtomicU32>,
}

impl Listener {
    fn pty(&self, msg: Msg) {
        if let Some(channel) = self.pty.lock().unwrap().as_ref() {
            let _ = channel.send(msg);
        }
    }
}

impl EventListener for Listener {
    fn send_event(&self, event: RioEvent, _id: WindowId) {
        let event = match event {
            RioEvent::UpdateGraphics { queues, .. } => Event::Graphics(queues),
            RioEvent::TerminalDamaged(_)
            | RioEvent::Render
            | RioEvent::RenderRoute(_) => {
                if !self.sink.wake(self.id) {
                    return;
                }
                Event::Wakeup
            }
            RioEvent::Title(_, title) | RioEvent::TitleWithSubtitle(title, _) => {
                Event::Title(title)
            }
            RioEvent::ResetTitle => Event::Title(String::new()),
            RioEvent::Bell(_) => Event::Bell,
            RioEvent::ClipboardStore(kind, text) => Event::Clipboard(kind, text),
            RioEvent::ClipboardLoad(_, kind, format) => {
                Event::ClipboardLoad(kind, format)
            }
            RioEvent::PtyWrite(_, text) => {
                self.pty(Msg::Reply(Cow::Owned(text.into_bytes())));
                return;
            }
            RioEvent::TextAreaSizeRequest(_, format) => {
                let pixels = self.pixels.load(Ordering::Relaxed);
                let size = WindowSize {
                    cols: 0,
                    rows: 0,
                    width: (pixels >> 16) as u16,
                    height: pixels as u16,
                };
                self.pty(Msg::Reply(Cow::Owned(format(size).into_bytes())));
                return;
            }
            RioEvent::ColorRequest(..)
            | RioEvent::GlyphProtocolQuery { .. }
            | RioEvent::GlyphProtocolGraphemeQuery { .. } => {
                self.pty(Msg::Reply(Cow::Borrowed(&[])));
                return;
            }
            RioEvent::FocusReport(_, focused) => {
                self.pty(Msg::Input(Cow::Borrowed(
                    rio_vt::crosswords::focus_report(focused).as_bytes(),
                )));
                return;
            }
            RioEvent::CursorBlinkingChange | RioEvent::CursorBlinkingChangeOnRoute(_) => {
                Event::CursorBlinking
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
                Event::Progress(report.progress, state)
            }
            RioEvent::DesktopNotification { title, body } => {
                Event::Notification(title, body)
            }
            RioEvent::CommandFinished {
                exit_status,
                duration,
                ..
            } => Event::CommandFinished(exit_status, duration),
            RioEvent::CloseTerminal(_) | RioEvent::Exit | RioEvent::ChildExited(..) => {
                Event::Exited
            }
            _ => return,
        };
        self.sink.send(self.id, event);
    }

    fn send_event_with_high_priority(&self, event: RioEvent, id: WindowId) {
        self.send_event(event, id);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionKind {
    Simple,
    Block,
    Semantic,
    Lines,
}

pub struct Session {
    pub id: usize,
    pub terminal: Terminal,
    channel: corcovado::channel::Sender<Msg>,
    pixels: Arc<AtomicU32>,
    #[cfg(unix)]
    main_fd: std::os::fd::RawFd,
    #[cfg(unix)]
    shell_pid: u32,
    #[cfg(unix)]
    terminator: teletypewriter::ChildTerminator,
    shell: String,
}

/// Set once at startup, before any shell runs, as Rio does: the shells
/// inherit it.
pub fn setup_environment(config: &Config) {
    #[cfg(unix)]
    {
        let terminfo = match (
            teletypewriter::terminfo_exists("xterm-rio"),
            teletypewriter::terminfo_exists("rio"),
        ) {
            (true, _) => "xterm-rio",
            (false, true) => "rio",
            (false, false) => "xterm-256color",
        };
        std::env::set_var("TERM", terminfo);
    }
    std::env::set_var("TERM_PROGRAM", "rio");
    std::env::set_var("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
    std::env::set_var("COLORTERM", "truecolor");
    std::env::remove_var("DESKTOP_STARTUP_ID");
    std::env::remove_var("XDG_ACTIVATION_TOKEN");
    for var in &config.env_vars {
        if let Some((key, value)) = var.split_once('=') {
            std::env::set_var(key, value);
        }
    }
}

impl Session {
    pub fn spawn(
        id: usize,
        config: &Config,
        working_dir: Option<String>,
        sink: Arc<dyn Sink>,
        size: CrosswordsSize,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let pty_channel = Arc::new(Mutex::new(None));
        let pixels = Arc::new(AtomicU32::new(pack(size.width, size.height)));
        let listener = Listener {
            id,
            sink,
            pty: pty_channel.clone(),
            pixels: pixels.clone(),
        };
        let (cols, rows, width, height) = (
            size.columns as u16,
            size.screen_lines as u16,
            size.width as u16,
            size.height as u16,
        );
        let mut terminal = Crosswords::new(
            size,
            config.cursor.shape,
            listener.clone(),
            WindowId::from(id as u64),
            id,
            config.scrollback_history_limit,
        );
        terminal.set_grapheme_clustering(config.grapheme_clustering);
        terminal.set_default_colors(List::from(&config.colors));
        terminal.set_default_cursor_style(config.cursor.shape, config.cursor.blinking);
        let terminal = Arc::new(TerminalLock::new(terminal));

        let working_dir = working_dir.or_else(|| config.working_dir.clone());
        let integration = if config.shell_integration {
            rio_backend::shell_integration::prepare(
                config.shell.program.as_deref(),
                &config.shell.args,
            )
        } else {
            Default::default()
        };
        let (program, args) = match &integration.command {
            Some((program, args)) => (program.as_deref(), args.clone()),
            None => (config.shell.program.as_deref(), config.shell.args.clone()),
        };
        let env = (!integration.env.is_empty()).then(|| integration.env.clone());
        #[cfg(unix)]
        let pty = teletypewriter::create_pty_with_spawn(
            program,
            args,
            &working_dir,
            env,
            cols,
            rows,
            width,
            height,
        )?;
        #[cfg(windows)]
        let pty =
            teletypewriter::create_pty(program, args, &working_dir, env, cols, rows)?;

        #[cfg(unix)]
        let (main_fd, shell_pid, terminator) =
            (*pty.child.id, *pty.child.pid as u32, pty.child.terminator());

        let machine = Machine::new(
            Arc::clone(&terminal),
            pty,
            listener,
            WindowId::from(id as u64),
            id,
        )?;
        let channel = machine.channel();
        *pty_channel.lock().unwrap() = Some(channel.clone());
        machine.spawn();

        Ok(Self {
            id,
            terminal,
            channel,
            pixels,
            #[cfg(unix)]
            main_fd,
            #[cfg(unix)]
            shell_pid,
            #[cfg(unix)]
            terminator,
            shell: shell_name(config),
        })
    }

    pub fn reconfigure(&self, config: &Config) {
        let mut term = self.terminal.lock();
        term.set_default_colors(List::from(&config.colors));
        term.set_default_cursor_style(config.cursor.shape, config.cursor.blinking);
        term.set_grapheme_clustering(config.grapheme_clustering);
        term.mark_fully_damaged();
    }

    /// Input from the user: back to the live screen, selection dropped.
    pub fn write<B: Into<Cow<'static, [u8]>>>(&self, bytes: B) {
        let bytes = bytes.into();
        if bytes.is_empty() {
            return;
        }
        {
            let mut term = self.terminal.lock();
            if term.display_offset() != 0 {
                term.scroll_display(Scroll::Bottom);
            }
            if term.selection.take().is_some() {
                term.mark_fully_damaged();
            }
        }
        let _ = self.channel.send(Msg::Input(bytes));
    }

    pub fn reply(&self, bytes: Vec<u8>) {
        let _ = self.channel.send(Msg::Reply(Cow::Owned(bytes)));
    }

    pub fn resize(&self, size: CrosswordsSize) {
        let window = WindowSize {
            cols: size.columns as u16,
            rows: size.screen_lines as u16,
            width: size.width as u16,
            height: size.height as u16,
        };
        self.pixels
            .store(pack(size.width, size.height), Ordering::Relaxed);
        self.terminal.lock().resize(size);
        let _ = self.channel.send(Msg::Resize(window));
    }

    pub fn key(&self, event: &KeyEvent) -> bool {
        let ctx = {
            let term = self.terminal.lock();
            let mode = term.mode();
            EncodeContext {
                app_cursor: mode.contains(Mode::APP_CURSOR),
                app_keypad: mode.contains(Mode::APP_KEYPAD),
                kitty: kitty_flags(term.keyboard_mode()),
                modify_other_keys: term.modify_other_keys(),
                alt_is_meta: true,
            }
        };
        match rio_vt::key::encode(event, &ctx) {
            Some(bytes) => {
                self.write(bytes);
                true
            }
            None => false,
        }
    }

    pub fn text(&self, text: &str) {
        self.write(text.as_bytes().to_vec());
    }

    pub fn mode(&self) -> Mode {
        self.terminal.lock().mode()
    }

    pub fn paste(&self, text: String) {
        if text.is_empty() {
            return;
        }
        let bracketed = self.mode().contains(Mode::BRACKETED_PASTE);
        let encoded = rio_vt::paste::encode_owned(text, bracketed);
        if encoded.start.is_empty() {
            self.write(encoded.text);
        } else {
            self.write(encoded.start);
            let _ = self.channel.send(Msg::Input(encoded.text.into()));
            let _ = self.channel.send(Msg::Input(encoded.end.into()));
        }
    }

    pub fn paste_is_safe(&self, text: &str) -> bool {
        rio_vt::paste::is_safe_with(text, self.mode().contains(Mode::BRACKETED_PASTE))
    }

    pub fn set_focus(&self, focused: bool) {
        let mut term = self.terminal.lock();
        if let Some(report) = term.set_focus(focused) {
            let _ = self
                .channel
                .send(Msg::Input(Cow::Borrowed(report.as_bytes())));
        }
    }

    pub fn wheel(&self, lines: i32, col: u16, row: u16, mods: Modifiers) {
        match rio_vt::mouse::wheel(self.mode(), lines, col, row, mods) {
            rio_vt::mouse::Wheel::Program(bytes) => self.write(bytes),
            rio_vt::mouse::Wheel::Scrollback => self.scroll(Scroll::Delta(lines)),
        }
    }

    pub fn mouse_button(
        &self,
        col: u16,
        row: u16,
        button: u8,
        pressed: bool,
        mods: Modifiers,
    ) -> bool {
        match rio_vt::mouse::button(self.mode(), col, row, button, pressed, mods) {
            Some(report) => {
                let _ = self.channel.send(Msg::Input(Cow::Owned(report)));
                true
            }
            None => false,
        }
    }

    pub fn mouse_motion(&self, col: u16, row: u16, button: u8, mods: Modifiers) -> bool {
        match rio_vt::mouse::motion(self.mode(), col, row, button, mods) {
            Some(report) => {
                let _ = self.channel.send(Msg::Input(Cow::Owned(report)));
                true
            }
            None => false,
        }
    }

    /// Scroll to the previous or next prompt the shell marked.
    pub fn scroll_to_prompt(&self, forward: bool) {
        let mut term = self.terminal.lock();
        term.scroll_to_prompt(forward);
        term.mark_fully_damaged();
        term.request_render();
    }

    pub fn scroll(&self, scroll: Scroll) {
        let mut term = self.terminal.lock();
        term.scroll_display(scroll);
        term.mark_fully_damaged();
        term.request_render();
    }

    fn pos(term: &Crosswords<Listener>, row: u16, col: u16) -> Pos {
        let offset = term.display_offset() as i32;
        Pos::new(Line(row as i32 - offset), Column(col as usize))
    }

    pub fn selection_begin(&self, row: u16, col: u16, side: Side, kind: SelectionKind) {
        let mut term = self.terminal.lock();
        let pos = Self::pos(&term, row, col);
        let kind = match kind {
            SelectionKind::Simple => SelectionType::Simple,
            SelectionKind::Block => SelectionType::Block,
            SelectionKind::Semantic => SelectionType::Semantic,
            SelectionKind::Lines => SelectionType::Lines,
        };
        term.selection = Some(Selection::new(kind, pos, side));
        term.mark_fully_damaged();
        term.request_render();
    }

    pub fn selection_update(&self, row: u16, col: u16, side: Side) {
        let mut term = self.terminal.lock();
        let pos = Self::pos(&term, row, col);
        if let Some(selection) = &mut term.selection {
            selection.update(pos, side);
            term.mark_fully_damaged();
            term.request_render();
        }
    }

    pub fn selection_clear(&self) {
        let mut term = self.terminal.lock();
        if term.selection.take().is_some() {
            term.mark_fully_damaged();
            term.request_render();
        }
    }

    /// Select from `start` to `end`, both inclusive, in grid lines.
    pub fn select_range(&self, start: Pos, end: Pos) {
        let mut term = self.terminal.lock();
        let mut selection = Selection::new(SelectionType::Simple, start, Side::Left);
        selection.update(end, Side::Right);
        term.selection = Some(selection);
        term.mark_fully_damaged();
        term.request_render();
    }

    /// Matches of `pattern` across scrollback and screen, top to bottom,
    /// at most `max`. A pattern that isn't a valid regex is searched as
    /// text.
    pub fn search(&self, pattern: &str, max: usize) -> Vec<(Pos, Pos)> {
        use rio_vt::crosswords::pos::Direction;
        use rio_vt::crosswords::search::{RegexIter, RegexSearch};
        if pattern.is_empty() {
            return Vec::new();
        }
        let Some(mut regex) = RegexSearch::new(pattern)
            .or_else(|_| RegexSearch::new(&escape(pattern)))
            .ok()
        else {
            return Vec::new();
        };
        let term = self.terminal.lock();
        let start = Pos::new(Line(-(term.history_size() as i32)), Column(0));
        let end = Pos::new(
            Line(term.screen_lines() as i32 - 1),
            Column(term.columns().saturating_sub(1)),
        );
        RegexIter::new(start, end, Direction::Right, &term, &mut regex)
            .take(max)
            .map(|m| (*m.start(), *m.end()))
            .collect()
    }

    /// Scroll so grid line `line` is on screen, centered when it wasn't.
    pub fn reveal(&self, line: Line) {
        let mut term = self.terminal.lock();
        let lines = term.screen_lines() as i32;
        let offset = term.display_offset() as i32;
        let row = line.0 + offset;
        if (0..lines).contains(&row) {
            return;
        }
        let wanted = (lines / 2 - line.0).clamp(0, term.history_size() as i32);
        term.scroll_display(Scroll::Delta(wanted - offset));
        term.mark_fully_damaged();
        term.request_render();
    }

    /// Run `program` detached, in the shell's directory.
    pub fn exec(&self, program: &str, args: &[String]) {
        #[cfg(unix)]
        let result =
            teletypewriter::spawn_daemon(program, args, self.main_fd, self.shell_pid);
        #[cfg(windows)]
        let result = teletypewriter::spawn_daemon(program, args);
        if let Err(err) = result {
            tracing::warn!("could not run {program}: {err}");
        }
    }

    pub fn open(&self, target: &str) {
        #[cfg(target_os = "macos")]
        self.exec("open", &[target.to_string()]);
        #[cfg(not(target_os = "macos"))]
        self.exec("xdg-open", &[target.to_string()]);
    }

    pub fn current_directory(&self) -> Option<std::path::PathBuf> {
        self.terminal.lock().current_directory.clone()
    }

    pub fn selection_text(&self) -> Option<String> {
        self.terminal
            .lock()
            .selection_to_string()
            .filter(|text| !text.is_empty())
    }

    /// The shell's working directory: OSC 7, else the foreground
    /// process's.
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
        #[cfg(unix)]
        {
            teletypewriter::foreground_process_path(self.main_fd, self.shell_pid)
                .ok()
                .map(|path| path.to_string_lossy().into_owned())
        }
        #[cfg(not(unix))]
        None
    }

    /// The program in the foreground, when it isn't the shell itself.
    pub fn foreground_program(&self) -> Option<String> {
        #[cfg(unix)]
        {
            let group = unsafe { libc::tcgetpgrp(self.main_fd) };
            if group < 0 || group as u32 == self.shell_pid {
                return None;
            }
            let name =
                teletypewriter::foreground_process_name(self.main_fd, self.shell_pid);
            let name = name.trim_start_matches('-');
            (!name.is_empty() && name != self.shell).then(|| name.to_string())
        }
        #[cfg(not(unix))]
        None
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.channel.send(Msg::Shutdown);
        #[cfg(unix)]
        let _ = self.terminator.hangup();
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn shell_name(config: &Config) -> String {
    let program = config
        .shell
        .program
        .clone()
        .or_else(|| std::env::var("SHELL").ok())
        .unwrap_or_default();
    std::path::Path::new(&program)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or(program)
}

fn pack(width: u32, height: u32) -> u32 {
    (width.min(u16::MAX as u32) << 16) | height.min(u16::MAX as u32)
}

fn kitty_flags(modes: rio_vt::ansi::KeyboardModes) -> KittyFlags {
    use rio_vt::ansi::KeyboardModes;
    let mut flags = KittyFlags::empty();
    for (mode, flag) in [
        (
            KeyboardModes::DISAMBIGUATE_ESC_CODES,
            KittyFlags::DISAMBIGUATE,
        ),
        (
            KeyboardModes::REPORT_EVENT_TYPES,
            KittyFlags::REPORT_EVENT_TYPES,
        ),
        (
            KeyboardModes::REPORT_ALL_KEYS_AS_ESC,
            KittyFlags::REPORT_ALL_AS_ESC,
        ),
        (
            KeyboardModes::REPORT_ALTERNATE_KEYS,
            KittyFlags::REPORT_ALTERNATE_KEYS,
        ),
        (
            KeyboardModes::REPORT_ASSOCIATED_TEXT,
            KittyFlags::REPORT_ASSOCIATED_TEXT,
        ),
    ] {
        if modes.contains(mode) {
            flags |= flag;
        }
    }
    flags
}
