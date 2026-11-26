#[cfg(feature = "pty")]
pub mod capi;

pub mod key;
mod render_state;

pub use key::{
    encode as encode_key, EncodeContext, Key, KeyAction, KeyEvent, KittyFlags, Modifiers,
};
pub use render_state::{
    ImageLayer, ImagePlacement, ImageProtocol, RenderState, ViewportSelection,
};
/// What a host that draws Glyph Protocol glyphs needs: the registry's
/// entries, their placement (§8.5), and the `glyf` and `vg` decoders.
#[cfg(feature = "graphics")]
pub use rio_graphics::{
    glyph::{glyf_decode, glyph_registry},
    glyph_layout, glyph_vector,
};
pub use rio_vt::ansi::glyph_protocol::GlyphFormat;
pub use rio_vt::clipboard::ClipboardType;
pub use rio_vt::config::colors::term::TermColors;
pub use rio_vt::config::colors::{AnsiColor, ColorRgb, NamedColor};
pub use rio_vt::crosswords::grid::row::Row;
pub use rio_vt::crosswords::pos::Column;
pub use rio_vt::crosswords::square::{Extras, Square};
pub use rio_vt::crosswords::style::{Style, StyleFlags};
pub use rio_vt::event::{BlockedKind, ProgramState, ProgramStatus, ProgramStatusEvent};
pub use rio_vt::grapheme_lut::cluster_width;
pub use rio_vt::selection::SelectionRange;

pub use rio_vt::ansi::CursorShape;
pub use rio_vt::crosswords::pos::Side;
use rio_vt::crosswords::pos::{Column as PosColumn, Line, Pos};
use rio_vt::crosswords::{Crosswords, Mode};
use rio_vt::event::sync::TerminalLock;
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
use std::sync::{Arc, Mutex};
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

/// The longest notification title, in bytes, an embedder is given.
pub const NOTIFICATION_TITLE_MAX: usize = 64;
/// The longest notification body, in bytes, an embedder is given.
pub const NOTIFICATION_BODY_MAX: usize = 256;

/// Whether a character may show in a notification. Control characters,
/// bidirectional marks, overrides and isolates, and invisible spaces are
/// dropped: with them a program could disguise what it shows (reverse
/// the text, pass for another app). Joiners stay, as emoji need them.
fn shown(c: char) -> bool {
    !c.is_control()
        && !matches!(
            c,
            '\u{061C}'
                | '\u{200B}'
                | '\u{200E}'
                | '\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'
                | '\u{2066}'..='\u{2069}'
                | '\u{FEFF}'
        )
}

/// A notification's text as given to the embedder: the characters that
/// may show, at most `max` bytes of them.
fn notification_text(s: &str, max: usize) -> String {
    cut(s.chars().filter(|&c| shown(c)).collect(), max)
}

/// How often a surface may notify, as ghostty does: at most once a
/// second, and the same notification at most once in five seconds. Any
/// program can send them (over ssh, from a file being printed), and each
/// one would reach the system.
#[derive(Default)]
struct NotifyLimit {
    /// When the last one went, and a hash of its title and body.
    last: Option<(rio_vt::time::Instant, u64)>,
}

impl NotifyLimit {
    fn allow(&mut self, now: rio_vt::time::Instant, title: &str, body: &str) -> bool {
        use rio_vt::time::Duration;
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (title, body).hash(&mut hasher);
        let digest = hasher.finish();
        if let Some((at, last)) = self.last {
            let since = now.saturating_duration_since(at);
            if since < Duration::from_secs(1)
                || (last == digest && since < Duration::from_secs(5))
            {
                return false;
            }
        }
        self.last = Some((now, digest));
        true
    }
}

/// `s` cut to at most `max` bytes, at a character boundary.
fn cut(mut s: String, max: usize) -> String {
    if s.len() > max {
        let mut end = max;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
    }
    s
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
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
    /// OSC 7501 program status: a validated report, or a lifecycle event
    /// (new prompt, full reset) after which the embedder must drop
    /// records. Records themselves live in the embedder.
    ProgramStatus(ProgramStatusEvent),
    /// A shell command finished (OSC 133 `D` after a `C`). `exit_status`
    /// is `None` when the shell sent none; `duration` runs from the
    /// start of the command's output.
    CommandFinished {
        exit_status: Option<i32>,
        duration: std::time::Duration,
    },
    /// A program asked for a desktop notification: OSC 9 (iTerm2: the text
    /// after `9;` is the body) or OSC 777 `notify;title;body` (rxvt).
    /// OSC 9 text that starts with a ConEmu sub-command and a `;` (`9;4;`
    /// progress, `9;9;` working directory, `9;3;` and the others up to 12)
    /// is not a notification.
    ///
    /// `title` is empty when the program sent none; `body` is never empty
    /// (an empty one isn't sent). Control characters, bidirectional
    /// controls and invisible spaces are taken out, then the two are cut
    /// to [`NOTIFICATION_TITLE_MAX`] and [`NOTIFICATION_BODY_MAX`] bytes
    /// at a character boundary. A surface sends at most one a second, and
    /// the same one at most once in five seconds. Whether to show it, and
    /// when, is the embedder's call.
    DesktopNotification {
        title: String,
        body: String,
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
    /// The surface changed, and a [`RenderState::update`] would show it.
    ///
    /// It may not be repeated for later output until the next `update` of
    /// the surface. A host that keeps its own "needs render" flag clears it
    /// before calling `update`, not after: a wakeup that lands between the
    /// update and the clear would otherwise be lost, and with it the output
    /// that follows.
    ///
    /// Called with the surface's terminal locked, on librio's reader thread
    /// or from inside any surface call that changes what is shown, such as
    /// [`Surface::inject_output`], [`Surface::resize`] or [`Surface::scroll`].
    /// Only flag or schedule work here; calling back into the surface
    /// deadlocks.
    fn wakeup(&self, surface: SurfaceId);
    fn action(&self, _surface: SurfaceId, _action: Action) {}
    fn clipboard_write(&self, _surface: SurfaceId, _kind: ClipboardType, _text: String) {}
    fn close_surface(&self, _surface: SurfaceId) {}
    /// Whether this delegate acts on `Action::ProgramStatus` (OSC 7501).
    /// Read once per surface at creation. While false, the terminal
    /// leaves the support query unanswered, so programs don't report to
    /// a terminal that ignores them, and no program-status actions are
    /// sent.
    fn handles_program_status(&self) -> bool {
        false
    }
    /// The Glyph Protocol payload formats this delegate draws. Read once
    /// per surface at creation. Programs are offered exactly these, and
    /// what they register is read with [`RenderState::glyph_version`]
    /// and [`Surface::glyph`]. None by default, which leaves the
    /// protocol off: a program probing for it gets no answer, as from a
    /// terminal without it. Needs the `graphics` feature, which stores
    /// registrations.
    fn glyph_formats(&self) -> &'static [GlyphFormat] {
        &[]
    }
    /// A program asked what draws `cp` (Glyph Protocol `q`). Only called
    /// for a delegate that draws some format, and like [`Self::wakeup`]
    /// with the terminal locked, so the only surface call allowed here is
    /// [`Surface::glyph_query_reply`]. Answer with it once per query, in
    /// order: right here, or later from any thread when looking the
    /// codepoint up in the fonts is slow. Replies the terminal makes
    /// after the query wait for that answer, so the program gets them in
    /// the order it asked; a query never answered holds them forever.
    fn glyph_query(&self, _surface: SurfaceId, _cp: u32) {}
    /// Bytes the terminal wants delivered to the child process. Only called
    /// on non-`pty` builds, where the host owns the transport (a WebSocket
    /// to a real shell, an in-page demo interpreter, ...); with a PTY the
    /// bytes go straight to it and this never fires. Called with the
    /// terminal locked, or from inside [`Surface::glyph_query_reply`]
    /// while it keeps the replies in order, so only hand the bytes on
    /// here; calling back into the surface deadlocks.
    fn output(&self, _surface: SurfaceId, _bytes: &[u8]) {}
}

/// The pixels of the sixel/iTerm2 images on screen, by image key. The
/// terminal hands new pixels to its renderer once, through its upload
/// queues; here that renderer is the host, so the surface keeps them
/// (kitty images stay in the terminal's own store) until the terminal
/// says an image's last placement is gone. The listener applies each
/// queue as it arrives, and every render state of the surface reads it.
pub(crate) struct InlineImageStore {
    images: std::sync::Mutex<rustc_hash::FxHashMap<u64, Arc<rio_graphics::GraphicData>>>,
    /// Baseline for image change stamps (Instants aren't representable
    /// over the C ABI; nanoseconds relative to this are). One per
    /// surface, so all of its render states give an image the same
    /// stamp.
    epoch: rio_vt::time::Instant,
}

impl Default for InlineImageStore {
    fn default() -> Self {
        Self {
            images: std::sync::Mutex::default(),
            epoch: rio_vt::time::Instant::now(),
        }
    }
}

impl InlineImageStore {
    /// Every write is one insert or remove, so a thread that panicked
    /// while holding the lock left the map whole: a poisoned lock is
    /// taken all the same rather than taking a PTY thread down.
    pub(crate) fn lock(
        &self,
    ) -> std::sync::MutexGuard<
        '_,
        rustc_hash::FxHashMap<u64, Arc<rio_graphics::GraphicData>>,
    > {
        self.images
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn epoch(&self) -> rio_vt::time::Instant {
        self.epoch
    }
}

pub(crate) type InlineImages = Arc<InlineImageStore>;

/// What goes to the terminal behind a glyph query the host has yet to
/// answer, in request order. The front is always a query.
#[cfg(feature = "graphics")]
enum Held {
    /// A query for `cp`, and whether `cp` was registered when it
    /// arrived: the answer says what the program saw at that point, not
    /// what a later `r` or `c` made of it before the host answered.
    Query(u32, bool),
    Reply(Cow<'static, [u8]>),
}

#[cfg(feature = "graphics")]
type HeldReplies = Arc<std::sync::Mutex<std::collections::VecDeque<Held>>>;

/// How many queries and replies may wait on the host. Past it the
/// oldest query is answered `reason=unknown` (§5.4 lets a terminal
/// limit how many it answers) and the replies behind it go out, so a
/// program flooding `q` while the host is slow or stalled can't grow
/// the queue, or the drain an answer does, without bound.
#[cfg(feature = "graphics")]
const MAX_HELD: usize = 1024;

/// Makes room for one more held item: while full, answers the oldest
/// query `reason=unknown` and delivers the replies held behind it. The
/// front stays a query, or the queue empties.
#[cfg(feature = "graphics")]
fn make_room(
    held: &mut std::collections::VecDeque<Held>,
    deliver: impl Fn(Cow<'static, [u8]>),
) {
    while held.len() >= MAX_HELD {
        if let Some(Held::Query(cp, _)) = held.pop_front() {
            let answer = rio_vt::ansi::glyph_protocol::format_query_unknown(cp);
            deliver(Cow::Owned(answer.into_bytes()));
        }
        while let Some(Held::Reply(_)) = held.front() {
            if let Some(Held::Reply(bytes)) = held.pop_front() {
                deliver(bytes);
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct Listener {
    surface_id: SurfaceId,
    delegate: Arc<dyn SurfaceDelegate>,
    inline_images: InlineImages,
    /// The terminal's glyph registry when the delegate draws some
    /// format, so it answers glyph queries.
    #[cfg(feature = "graphics")]
    glyphs: Option<rio_graphics::glyph::glyph_registry::GlyphRegistry>,
    #[cfg(feature = "graphics")]
    held: HeldReplies,
    #[cfg(feature = "pty")]
    pty_writer: Arc<Mutex<Option<corcovado::channel::Sender<Msg>>>>,
    notify_limit: Arc<Mutex<NotifyLimit>>,
}

impl Listener {
    fn dispatch(&self, event: RioEvent) {
        match event {
            RioEvent::TerminalDamaged(_)
            | RioEvent::Render
            | RioEvent::RenderRoute(_) => {
                self.delegate.wakeup(self.surface_id);
            }
            RioEvent::Title(_, title) => {
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
            RioEvent::Bell(_) => {
                self.delegate.action(self.surface_id, Action::RingBell);
            }
            RioEvent::DesktopNotification { title, body } => {
                // A program can send an OSC as long as the parser's cap.
                let title = notification_text(&title, NOTIFICATION_TITLE_MAX);
                let body = notification_text(&body, NOTIFICATION_BODY_MAX);
                if body.is_empty() {
                    return;
                }
                let now = rio_vt::time::Instant::now();
                if !self.notify_limit.lock().unwrap().allow(now, &title, &body) {
                    return;
                }
                self.delegate
                    .action(self.surface_id, Action::DesktopNotification { title, body });
            }
            RioEvent::CursorBlinkingChange | RioEvent::CursorBlinkingChangeOnRoute(_) => {
                self.delegate
                    .action(self.surface_id, Action::CursorBlinkingChange);
            }
            RioEvent::ClipboardStore(kind, text) => {
                self.delegate.clipboard_write(self.surface_id, kind, text);
            }
            // A reply the terminal routed through here, behind a query
            // only the owner answers; passed back as an answer so the
            // terminal knows when it has every one of them.
            RioEvent::PtyWrite(_, text) => {
                self.pass_back(Cow::Owned(text.into_bytes()));
            }
            #[cfg(feature = "graphics")]
            RioEvent::GlyphProtocolQuery { cp, .. } if self.glyphs.is_some() => {
                // Under the terminal lock, so no registration can land
                // between the query and this read.
                let registered = self.glyphs.as_ref().is_some_and(|g| g.contains(cp));
                let mut held = self.held.lock().unwrap();
                make_room(&mut held, |bytes| self.deliver(bytes));
                held.push_back(Held::Query(cp, registered));
                drop(held);
                self.delegate.glyph_query(self.surface_id, cp);
            }
            // Not answered here. The terminal is told the query is
            // settled, so the replies behind it take the direct path.
            RioEvent::ColorRequest(..)
            | RioEvent::TextAreaSizeRequest(..)
            | RioEvent::ClipboardLoad(..)
            | RioEvent::GlyphProtocolQuery { .. }
            | RioEvent::GlyphProtocolGraphemeQuery { .. } => {
                self.pass_back(Cow::Borrowed(&[]));
            }
            // Delivered under the terminal lock, from the parser, so the
            // state is current and this orders with `Surface::set_focus`,
            // which also writes under the lock.
            RioEvent::FocusReport(_, focused) => {
                self.write_to_child(Cow::Borrowed(
                    rio_vt::crosswords::focus_report(focused).as_bytes(),
                ));
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
            RioEvent::ProgramStatus(event) => {
                self.delegate
                    .action(self.surface_id, Action::ProgramStatus(event));
            }
            RioEvent::UpdateGraphics { queues, .. } => {
                render_state::apply_graphics_updates(&self.inline_images, queues);
                self.delegate.wakeup(self.surface_id);
            }
            RioEvent::CommandFinished {
                exit_status,
                duration,
                ..
            } => {
                self.delegate.action(
                    self.surface_id,
                    Action::CommandFinished {
                        exit_status,
                        duration,
                    },
                );
            }
            _ => {}
        }
    }
}

impl Listener {
    /// Bytes the terminal wants the child to see: the PTY on `pty` builds,
    /// the delegate otherwise.
    fn write_to_child(&self, bytes: Cow<'static, [u8]>) {
        #[cfg(feature = "pty")]
        if let Some(channel) = self.pty_writer.lock().unwrap().as_ref() {
            let _ = channel.send(Msg::Input(bytes));
        }
        #[cfg(not(feature = "pty"))]
        self.delegate.output(self.surface_id, &bytes);
    }

    /// A query from the program that this embedding does not answer.
    /// An answer, a decline (no bytes) or a routed reply going back to
    /// the terminal: the PTY's writer settles one fenced item with it
    /// on `pty` builds, the delegate gets the bytes otherwise.
    fn pass_back(&self, bytes: Cow<'static, [u8]>) {
        #[cfg(feature = "graphics")]
        {
            let mut held = self.held.lock().unwrap();
            make_room(&mut held, |bytes| self.deliver(bytes));
            if !held.is_empty() {
                held.push_back(Held::Reply(bytes));
                return;
            }
            self.deliver(bytes);
        }
        #[cfg(not(feature = "graphics"))]
        self.deliver(bytes);
    }

    /// Hand one answer or reply to the terminal, now. Callers hold
    /// `held`, so a reply can't overtake one held behind a query.
    fn deliver(&self, bytes: Cow<'static, [u8]>) {
        #[cfg(feature = "pty")]
        if let Some(channel) = self.pty_writer.lock().unwrap().as_ref() {
            let _ = channel.send(Msg::Reply(bytes));
        }
        #[cfg(not(feature = "pty"))]
        if !bytes.is_empty() {
            self.delegate.output(self.surface_id, &bytes);
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
    /// Where the shell starts: a path (a leading `~` is the home
    /// directory), `home`, or `inherit` (as `None`) for the host's current
    /// directory. One a shell can't be started in falls back to the
    /// current directory, with a warning in the log.
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

    /// A surface running the shell `desc` names, or the user's login
    /// shell, on a new pty. Without the `pty` feature the host carries
    /// the bytes instead, and `desc`'s shell, args and working_dir are
    /// unused.
    pub fn create_surface(
        &self,
        desc: &SurfaceDesc,
    ) -> Result<Surface, Box<dyn Error + Send + Sync>> {
        #[cfg(feature = "pty")]
        return Surface::new(self, desc, spawn_pty(desc)?);
        #[cfg(not(feature = "pty"))]
        Surface::new(self, desc)
    }

    /// A surface over a pty master the host already holds, instead of a
    /// spawned shell, like foot's `--pty`. `desc`'s shell, args and
    /// working_dir are unused. Nothing of ours runs on it, so nothing is
    /// signaled or reaped and [`Surface::child_pid`] is 0, and the
    /// surface ends when the other side closes: once no process holds
    /// the slave, which on Linux includes one that has never been
    /// opened. Anything but a pty master is refused. The descriptor is
    /// made nonblocking, which a duplicate the host keeps shares, and
    /// closed on failure.
    #[cfg(all(feature = "pty", not(target_os = "windows")))]
    pub fn adopt_surface(
        &self,
        desc: &SurfaceDesc,
        main: std::os::fd::OwnedFd,
    ) -> Result<Surface, Box<dyn Error + Send + Sync>> {
        let pty = teletypewriter::create_pty_from_fd(
            main,
            desc.cols,
            desc.rows,
            desc.pixel_width,
            desc.pixel_height,
        )?;
        Surface::new(self, desc, pty)
    }
}

/// Translate the terminal's kitty keyboard flags into the encoder's own set.
/// Only the flags that change what a key produces are carried across.
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
    if modes.contains(KeyboardModes::REPORT_ALTERNATE_KEYS) {
        flags |= key::KittyFlags::REPORT_ALTERNATE_KEYS;
    }
    if modes.contains(KeyboardModes::REPORT_ASSOCIATED_TEXT) {
        flags |= key::KittyFlags::REPORT_ASSOCIATED_TEXT;
    }
    flags
}

/// Scheme-prefixed URL pattern for plain-text link detection. The engine
/// is regex-automata (no lookbehind), so trailing prose punctuation is
/// trimmed afterwards by `trailing_url_punctuation`.
const URL_REGEX: &str = "(?:https://|http://|mailto:|ftp://|file:|ssh://|ssh:|git://|tel:|magnet:|ipfs://|ipns://|gemini://|gopher://|news:)[^\u{0000}-\u{001F}\u{007F}-\u{009F}<>\"\\s{|}\\^⟨⟩`]+";

/// How many trailing characters of a URL match are prose punctuation
/// rather than URL: `.,;:!?'"` always, and a closing paren/bracket only
/// when its opener is not part of the match.
fn trailing_url_punctuation(text: &str) -> usize {
    let mut len = text.len();
    let mut trimmed = 0;
    while let Some(c) = text[..len].chars().last() {
        let cut = match c {
            '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' => true,
            ')' => text[..len].matches('(').count() < text[..len].matches(')').count(),
            ']' => text[..len].matches('[').count() < text[..len].matches(']').count(),
            _ => false,
        };
        if !cut {
            break;
        }
        len -= c.len_utf8();
        trimmed += 1;
    }
    trimmed
}

pub struct Surface {
    id: SurfaceId,
    /// The terminal's glyph registry, when the delegate draws glyphs;
    /// see [`Surface::glyph`].
    #[cfg(feature = "graphics")]
    glyphs: Option<rio_graphics::glyph::glyph_registry::GlyphRegistry>,
    /// Delivers the glyph query answers and the replies held behind them.
    #[cfg(feature = "graphics")]
    replies: Listener,
    alt_is_meta: std::sync::atomic::AtomicBool,
    terminal: Arc<TerminalLock<Crosswords<Listener>>>,
    /// Compiled URL detector, built on first hover. Hit-testing runs per
    /// pointer event, so the four lazy DFAs must not be rebuilt each time.
    url_regex: std::sync::Mutex<Option<rio_vt::crosswords::search::RegexSearch>>,
    /// VT parser for host-injected output. Persistent so escape sequences
    /// split across `inject_output` calls resume mid-sequence instead of
    /// mis-parsing (and so its sync buffer is allocated once, not per
    /// write).
    processor: std::sync::Mutex<Option<rio_vt::performer::handler::Processor>>,
    /// Sixel/iTerm2 pixels for the render states to read.
    inline_images: InlineImages,
    /// Whether the host breaks shaping runs at the cursor cell; read by
    /// the render states to mark the rows a cursor move reshapes.
    cursor_shaping_break: Arc<std::sync::atomic::AtomicBool>,
    /// Non-pty transport: `write` hands the bytes to the delegate instead
    /// of a PTY channel.
    #[cfg(not(feature = "pty"))]
    delegate: Arc<dyn SurfaceDelegate>,
    #[cfg(feature = "pty")]
    channel: corcovado::channel::Sender<Msg>,
    #[cfg(feature = "pty")]
    shell_pid: u32,
    #[cfg(all(feature = "pty", not(target_os = "windows")))]
    main_fd: std::os::fd::RawFd,
    #[cfg(all(feature = "pty", not(target_os = "windows")))]
    child_terminator: teletypewriter::ChildTerminator,
    #[cfg(feature = "pty")]
    _io_thread: std::thread::JoinHandle<(
        Machine<teletypewriter::Pty, Listener>,
        rio_vt::performer::State,
    )>,
}

/// Encode one mouse report. SGR (`CSI < b ; x ; y M`) when the program
/// asked for it, else the original X10 form, whose coordinates are
/// offset by 32 and cannot exceed 223 without the UTF-8 extension.
fn mouse_report(
    button: u8,
    col: u16,
    row: u16,
    pressed: bool,
    sgr: bool,
    utf8: bool,
) -> Vec<u8> {
    let x = col.saturating_add(1);
    let y = row.saturating_add(1);
    if sgr {
        // SGR keeps the real button and marks a release with lowercase `m`.
        let end = if pressed { 'M' } else { 'm' };
        return format!("\x1b[<{button};{x};{y}{end}").into_bytes();
    }
    // The X10/normal form has no release code: the button field collapses to
    // 3, but the modifier/motion bits (bit 3 and up) are kept, matching xterm.
    let encoded = if pressed { button } else { button | 3 };
    let mut out = vec![0x1b, b'[', b'M', 32u8.saturating_add(encoded)];
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

/// A pty with the shell `desc` names, or the user's login shell.
#[cfg(feature = "pty")]
fn spawn_pty(desc: &SurfaceDesc) -> std::io::Result<teletypewriter::Pty> {
    // No shell in the descriptor means "whatever the user's default is",
    // which teletypewriter resolves (and starts as a login shell).
    let shell = desc.shell.as_deref();

    // The child inherits the host process's environment, which for GUI
    // hosts has no TERM at all (or a stale one). Resolve it the way rio
    // does: prefer rio's terminfo when it's installed, else fall back to
    // the universally known xterm-256color so local prompts and remote
    // ssh sessions both keep working.
    #[cfg(not(target_os = "windows"))]
    {
        let terminfo = match (
            teletypewriter::terminfo_exists("xterm-rio"),
            teletypewriter::terminfo_exists("rio"),
        ) {
            (true, _) => "xterm-rio",
            (false, true) => "rio",
            (false, false) => "xterm-256color",
        };
        let env = vec![
            ("TERM".to_string(), terminfo.to_string()),
            ("COLORTERM".to_string(), "truecolor".to_string()),
        ];
        create_pty_with_spawn(
            shell,
            desc.args.clone(),
            &desc.working_dir,
            Some(env),
            desc.cols,
            desc.rows,
            desc.pixel_width,
            desc.pixel_height,
        )
    }

    #[cfg(target_os = "windows")]
    create_pty(
        shell,
        desc.args.clone(),
        &desc.working_dir,
        None,
        desc.cols,
        desc.rows,
    )
}

impl Surface {
    /// A surface over `pty`, which librio reads and writes on its own
    /// threads: [`Engine::create_surface`] spawns it and
    /// [`Engine::adopt_surface`] wraps the host's. Without the `pty`
    /// feature the host carries the bytes: it feeds the program's output
    /// with [`Surface::inject_output`] and gets the terminal's through
    /// [`SurfaceDelegate::output`].
    fn new(
        engine: &Engine,
        desc: &SurfaceDesc,
        #[cfg(feature = "pty")] pty: teletypewriter::Pty,
    ) -> Result<Surface, Box<dyn Error + Send + Sync>> {
        let id = engine.next_surface_id.fetch_add(1, Ordering::SeqCst);
        #[cfg(feature = "pty")]
        let pty_writer = Arc::new(Mutex::new(None));
        let inline_images = InlineImages::default();
        // Without `graphics` nothing stores a registration to draw.
        let glyph_formats = if cfg!(feature = "graphics") {
            engine.delegate.glyph_formats()
        } else {
            &[]
        };
        // Made now so the listener and the surface hold it; a full reset
        // empties it in place.
        #[cfg(feature = "graphics")]
        let glyphs = (!glyph_formats.is_empty())
            .then(rio_graphics::glyph::glyph_registry::GlyphRegistry::default);
        let listener = Listener {
            surface_id: id,
            delegate: engine.delegate.clone(),
            inline_images: inline_images.clone(),
            #[cfg(feature = "graphics")]
            glyphs: glyphs.clone(),
            #[cfg(feature = "graphics")]
            held: HeldReplies::default(),
            #[cfg(feature = "pty")]
            pty_writer: pty_writer.clone(),
            notify_limit: Arc::default(),
        };
        #[cfg(feature = "graphics")]
        let replies = listener.clone();

        let mut terminal = Crosswords::new(
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
        terminal.set_program_status_enabled(engine.delegate.handles_program_status());
        terminal.set_glyph_formats(glyph_formats);
        #[cfg(feature = "graphics")]
        {
            terminal.glyph_registry = glyphs.clone();
        }
        // On wasm the delegate (and so the whole graph) is single-threaded
        // by design; Arc stays because the pty build shares it with the IO
        // thread and the API is one type on every target.
        #[allow(clippy::arc_with_non_send_sync)]
        let terminal = Arc::new(TerminalLock::new(terminal));

        #[cfg(not(feature = "pty"))]
        {
            Ok(Surface {
                id,
                #[cfg(feature = "graphics")]
                glyphs,
                #[cfg(feature = "graphics")]
                replies,
                // Terminals default alt to meta; the host may override it.
                alt_is_meta: std::sync::atomic::AtomicBool::new(true),
                terminal,
                url_regex: std::sync::Mutex::new(None),
                processor: std::sync::Mutex::new(None),
                inline_images,
                cursor_shaping_break: Arc::new(std::sync::atomic::AtomicBool::new(true)),
                delegate: engine.delegate.clone(),
            })
        }

        #[cfg(feature = "pty")]
        {
            #[cfg(not(target_os = "windows"))]
            let shell_pid = *pty.child.pid.clone() as u32;
            #[cfg(target_os = "windows")]
            let shell_pid = pty.child_watcher().pid().map(|pid| pid.get()).unwrap_or(0);
            #[cfg(not(target_os = "windows"))]
            let main_fd = *pty.child.id;
            #[cfg(not(target_os = "windows"))]
            let child_terminator = pty.child.terminator();

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
                #[cfg(feature = "graphics")]
                glyphs,
                #[cfg(feature = "graphics")]
                replies,
                // Terminals default alt to meta; the host may override it.
                alt_is_meta: std::sync::atomic::AtomicBool::new(true),
                terminal,
                url_regex: std::sync::Mutex::new(None),
                processor: std::sync::Mutex::new(None),
                inline_images,
                cursor_shaping_break: Arc::new(std::sync::atomic::AtomicBool::new(true)),
                channel,
                shell_pid,
                #[cfg(not(target_os = "windows"))]
                main_fd,
                #[cfg(not(target_os = "windows"))]
                child_terminator,
                _io_thread: io_thread,
            })
        }
    }

    pub fn id(&self) -> SurfaceId {
        self.id
    }

    pub fn write<B: Into<Cow<'static, [u8]>>>(&self, bytes: B) {
        // Input snaps the view back to the live screen and drops any
        // selection, the scroll-to-bottom / clear-on-typing convention
        // (only reached when a key actually produced PTY bytes).
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
        self.send(bytes);
    }

    /// The Glyph Protocol registration a cell was printed with, by the
    /// version [`RenderState::glyph_version`] gives for it. A cell keeps
    /// that registration (spec §6.3): registering or clearing the
    /// codepoint again changes only text printed afterwards, and doesn't
    /// damage the screen. A version always means the same glyph, so it
    /// can key a raster cache; versions are unique within one surface.
    /// `None` without a delegate that draws glyphs, or once no cell
    /// shows the registration any more. This reads the registry under
    /// its lock and clones the entry, so look a version up once and
    /// cache what it draws, not once per cell and frame.
    #[cfg(feature = "graphics")]
    pub fn glyph(
        &self,
        version: u32,
    ) -> Option<rio_graphics::glyph::glyph_registry::RegisteredGlyph> {
        self.glyphs.as_ref()?.get_version(version)
    }

    /// The codepoint of the oldest unanswered
    /// [`SurfaceDelegate::glyph_query`], so a host can answer every
    /// waiting query in one pass instead of one hop per callback. Past
    /// 1024 waiting, librio answers the oldest itself (`reason=unknown`),
    /// so the codepoint a callback reported may be gone: answer what
    /// this returns.
    #[cfg(feature = "graphics")]
    pub fn glyph_query_pending(&self) -> Option<u32> {
        match self.replies.held.lock().unwrap().front() {
            Some(&Held::Query(cp, _)) => Some(cp),
            _ => None,
        }
    }

    /// Answer the oldest unanswered [`SurfaceDelegate::glyph_query`]:
    /// `system` says whether the host's fonts cover its codepoint, and
    /// whether a program registered it is filled in here. The replies
    /// the terminal made after the query follow it. Returns `false`, and
    /// sends nothing, when no query is waiting.
    #[cfg(feature = "graphics")]
    pub fn glyph_query_reply(&self, system: bool) -> bool {
        use rio_vt::ansi::glyph_protocol::{format_query_response, QueryStatus};
        if self.glyphs.is_none() {
            return false;
        }
        let mut held = self.replies.held.lock().unwrap();
        let Some(&Held::Query(cp, registered)) = held.front() else {
            return false;
        };
        held.pop_front();
        let status = QueryStatus::new(registered, system);
        let answer = format_query_response(cp, status).into_bytes();
        self.replies.deliver(Cow::Owned(answer));
        while let Some(Held::Reply(_)) = held.front() {
            if let Some(Held::Reply(bytes)) = held.pop_front() {
                self.replies.deliver(bytes);
            }
        }
        true
    }

    /// Bytes for the child, as they are: the PTY on `pty` builds, the
    /// delegate otherwise.
    fn send<B: Into<Cow<'static, [u8]>>>(&self, bytes: B) {
        #[cfg(feature = "pty")]
        let _ = self.channel.send(Msg::Input(bytes.into()));
        #[cfg(not(feature = "pty"))]
        self.delegate.output(self.id, &bytes.into());
    }

    pub fn text(&self, text: &str) {
        self.write(text.as_bytes().to_vec());
    }

    /// Paste text the way Rio does, see [`rio_vt::paste::encode_in_place`]:
    /// its control bytes as spaces so it can't end the bracket or act as
    /// tty input, between the bracketed paste markers when the program
    /// asked for them (mode 2004) with its line endings as they are, and
    /// otherwise with LF as CR, what Enter sends.
    ///
    /// A `String` is encoded in its own buffer and sent as is, never
    /// copied; a `&str` is copied once, since the writer thread needs the
    /// bytes after this returns.
    pub fn paste(&self, text: impl Into<String>) {
        let text = text.into();
        if text.is_empty() {
            return;
        }
        let bracketed = self.terminal.lock().mode().contains(Mode::BRACKETED_PASTE);
        let encoded = rio_vt::paste::encode_owned(text, bracketed);
        // `write` snaps the view back once, for the whole paste; the rest
        // follows in order.
        if encoded.start.is_empty() {
            self.write(encoded.text);
        } else {
            self.write(encoded.start);
            self.send(encoded.text);
            self.send(encoded.end);
        }
    }

    /// Whether `text` can be pasted without asking the user first, for a
    /// host that confirms pastes that could run commands: bracketed (mode
    /// 2004), only a bracketed paste end is suspect; otherwise any line
    /// break is too, since each line would run. See
    /// [`rio_vt::paste::is_safe_with`]; a host that wants to ask about
    /// bracketed line breaks too can use [`rio_vt::paste::is_safe`].
    pub fn paste_is_safe(&self, text: &str) -> bool {
        let bracketed = self.terminal.lock().mode().contains(Mode::BRACKETED_PASTE);
        rio_vt::paste::is_safe_with(text, bracketed)
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

    /// Tell the terminal whether this surface has keyboard focus. A change
    /// is reported to a program that asked for focus events (DECSET 1004)
    /// as CSI I or CSI O, and a program that enables them is told the
    /// current state right away; nothing else is sent. A surface starts
    /// unfocused, and a repeated value is ignored. Call it for window focus
    /// and for the host's own tab or split switches alike: to the program,
    /// a surface in a background tab is unfocused. Unlike [`Surface::write`]
    /// this leaves the view and the selection alone; losing focus must not
    /// scroll or deselect.
    pub fn set_focus(&self, focused: bool) {
        // Written under the lock, like the reply to a program enabling
        // focus events, so the two cannot cross.
        let mut terminal = self.terminal.lock();
        if let Some(report) = terminal.set_focus(focused) {
            self.send(report.as_bytes());
        }
    }

    /// Configure the default for grapheme cluster processing (DEC
    /// private mode 2027). On by default; embedders whose renderers
    /// assume legacy wcwidth cell layout can turn it off. Applied
    /// immediately and restored by RIS; a program's DECSET/DECRST
    /// still wins at runtime.
    pub fn set_grapheme_clustering(&self, enabled: bool) {
        self.terminal.lock().set_grapheme_clustering(enabled);
    }

    /// Whether the host breaks shaping runs around the cursor cell, as
    /// a renderer does so a ligature under the cursor shows its
    /// characters one by one. When it does, a render state update marks
    /// the row the cursor left and the row it is on dirty on a cursor
    /// move, without copying either from the terminal, since neither
    /// changed. A host that shapes rows the same wherever the cursor is
    /// turns this off and a cursor move marks no row. On by default.
    pub fn set_cursor_shaping_break(&self, enabled: bool) {
        self.cursor_shaping_break
            .store(enabled, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn key(&self, event: &KeyEvent) -> bool {
        // The encoding depends on terminal state the embedder does not track,
        // which is the reason this lives here and not in the host.
        let ctx = {
            let terminal = self.terminal.lock();
            key::EncodeContext {
                app_cursor: terminal.mode().contains(Mode::APP_CURSOR),
                app_keypad: terminal.mode().contains(Mode::APP_KEYPAD),
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
    /// Three cases, in order:
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
                out.extend_from_slice(&mouse_report(button, col, row, true, sgr, utf8));
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

    /// Report a mouse button press/release to the program when it asked for
    /// mouse events (DEC 1000/1002/1003, or X10/9). `button` is 0=left,
    /// 1=middle, 2=right. Returns true when a report was written, so the host
    /// should not start a local selection. Returns false when no program
    /// is grabbing the mouse, or shift is held to force a local selection
    /// (shift-to-bypass).
    pub fn mouse_button(
        &self,
        col: u16,
        row: u16,
        button: u8,
        pressed: bool,
        mods: Modifiers,
    ) -> bool {
        let (mouse_mode, x10, sgr, utf8) = {
            let mode = self.terminal.lock().mode();
            (
                mode.intersects(Mode::MOUSE_MODE),
                mode.contains(Mode::MOUSE_REPORT_X10),
                mode.contains(Mode::SGR_MOUSE),
                mode.contains(Mode::UTF8_MOUSE),
            )
        };
        if !mouse_mode || mods.contains(Modifiers::SHIFT) {
            return false;
        }
        // X10 (mode 9) reports only presses of the three main buttons, with
        // no modifiers and no release.
        if x10 && (!pressed || button > 2) {
            return false;
        }
        let mut encoded = button;
        if !x10 {
            if mods.contains(Modifiers::ALT) {
                encoded += 8;
            }
            if mods.contains(Modifiers::CTRL) {
                encoded += 16;
            }
        }
        self.write(mouse_report(encoded, col, row, pressed, sgr, utf8));
        true
    }

    /// Report pointer motion for button-event (1002, a button held) and
    /// any-event (1003, bare motion) modes. `button` is 0/1/2 for the button
    /// held during a drag, or 3 when none is held. Returns true when a report
    /// was written.
    pub fn mouse_motion(&self, col: u16, row: u16, button: u8, mods: Modifiers) -> bool {
        let (drag, motion, sgr, utf8) = {
            let mode = self.terminal.lock().mode();
            (
                mode.contains(Mode::MOUSE_DRAG),
                mode.contains(Mode::MOUSE_MOTION),
                mode.contains(Mode::SGR_MOUSE),
                mode.contains(Mode::UTF8_MOUSE),
            )
        };
        if mods.contains(Modifiers::SHIFT) {
            return false;
        }
        // 1002 reports motion only while a button is down; 1003 reports all.
        let wanted = if button >= 3 { motion } else { drag || motion };
        if !wanted {
            return false;
        }
        // The motion bit (32) rides on top of the button.
        let mut encoded = button.saturating_add(32);
        if mods.contains(Modifiers::ALT) {
            encoded += 8;
        }
        if mods.contains(Modifiers::CTRL) {
            encoded += 16;
        }
        self.write(mouse_report(encoded, col, row, true, sgr, utf8));
        true
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
        term.request_render();
    }

    pub fn selection_update(&self, viewport_line: i32, col: usize, side: Side) {
        let mut term = self.terminal.lock();
        let offset = term.display_offset() as i32;
        let pos = Pos::new(Line(viewport_line - offset), PosColumn(col));
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

    /// Lines currently held in scrollback (the ring, not counting
    /// eviction). Search coordinates are relative to the top of this ring.
    pub fn history_size(&self) -> usize {
        self.terminal.lock().history_size()
    }

    /// Serialize scrollback + screen to a byte stream that reconstructs
    /// content, SGR styling, and OSC 8 hyperlinks when replayed into a
    /// same-width terminal (`inject_output`). The style state machine is
    /// reset-then-set per change: verbose but unambiguous.
    pub fn serialize(&self) -> String {
        use rio_vt::crosswords::grid::{Dimensions, GridSquare};
        use rio_vt::crosswords::square::{CellFlags, Wide};

        let term = self.terminal.lock();
        let grid = &term.grid;
        let cols = grid.columns();
        let history = term.history_size() as i32;
        let rows = term.screen_lines() as i32;

        // Trim trailing all-empty screen rows, like dump().
        let mut last = rows - 1;
        'trim: while last > -history {
            let row = &grid[Line(last)];
            for col in 0..cols {
                let square = row[PosColumn(col)];
                if !square.is_empty() {
                    break 'trim;
                }
            }
            last -= 1;
        }

        let mut out = String::new();
        let mut style = Style::default();
        let mut link: Option<String> = None;
        out.push_str("\x1b[0m");

        for line in -history..=last {
            let row = &grid[Line(line)];
            let wrapped = row[PosColumn(cols - 1)]
                .cell_flags()
                .contains(CellFlags::WRAPLINE);

            // Within a row, drop the trailing run of default empty cells
            // (unless the row wraps, where every cell is content).
            let mut end = cols;
            if !wrapped {
                while end > 0 {
                    let square = row[PosColumn(end - 1)];
                    if !square.is_empty() {
                        break;
                    }
                    end -= 1;
                }
            }

            for col in 0..end {
                let square = row[PosColumn(col)];
                match square.wide() {
                    Wide::Spacer | Wide::LeadingSpacer => continue,
                    _ => {}
                }

                let cell_style = grid.style_of(&square);
                if cell_style != style {
                    rio_vt::crosswords::formatter::write_sgr(&mut out, &cell_style);
                    style = cell_style;
                }

                let cell_link = square
                    .extras_id_checked()
                    .and_then(|id| grid.extras_table.get(id))
                    .and_then(|extras| extras.hyperlink.as_ref())
                    .map(|h| h.uri().to_string());
                if cell_link != link {
                    match &cell_link {
                        Some(uri) => {
                            out.push_str("\x1b]8;;");
                            out.push_str(uri);
                            out.push_str("\x1b\\");
                        }
                        None => out.push_str("\x1b]8;;\x1b\\"),
                    }
                    link = cell_link;
                }

                let c = square.c();
                out.push(if c == '\0' { ' ' } else { c });
                if let Some(extras) = square
                    .extras_id_checked()
                    .and_then(|id| grid.extras_table.get(id))
                {
                    for z in &extras.zerowidth {
                        out.push(*z);
                    }
                }
            }

            if !wrapped && line < last {
                out.push_str("\r\n");
            }
        }

        if link.is_some() {
            out.push_str("\x1b]8;;\x1b\\");
        }
        out.push_str("\x1b[0m");
        out
    }

    /// All regex matches across scrollback + screen, top to bottom, as
    /// `(start_line, start_col, end_line, end_col)` with lines relative to
    /// the top of the scrollback ring. `None` when the pattern is invalid.
    pub fn search(&self, pattern: &str, max: usize) -> Option<Vec<(u32, u16, u32, u16)>> {
        use rio_vt::crosswords::pos::Direction;
        use rio_vt::crosswords::search::{RegexIter, RegexSearch};

        let mut regex = RegexSearch::new(pattern).ok()?;
        let term = self.terminal.lock();
        let history = term.history_size() as i32;
        let rows = term.screen_lines() as i32;
        let cols = term.columns();
        let start = Pos::new(Line(-history), PosColumn(0));
        let end = Pos::new(Line(rows - 1), PosColumn(cols - 1));

        let mut matches = Vec::new();
        for m in RegexIter::new(start, end, Direction::Right, &term, &mut regex) {
            let (s, e) = (m.start(), m.end());
            matches.push((
                (s.row.0 + history) as u32,
                s.col.0 as u16,
                (e.row.0 + history) as u32,
                e.col.0 as u16,
            ));
            if matches.len() >= max {
                break;
            }
        }
        Some(matches)
    }

    /// The pid of the program this surface spawned (the shell, or the
    /// configured `shell` program), for identity and diagnostics. Do not
    /// signal it on teardown: dropping the surface already hangs up the
    /// process group, and the reader thread escalates to SIGKILL and
    /// reaps, so a host-side killpg would race that escalation and can
    /// hit a recycled pid; a host that wants an extra signal must use
    /// [`Surface::hangup_child`], which goes through the lifecycle
    /// guard. On Windows it is the conpty child's process id; 0 if the
    /// pid was unavailable.
    #[cfg(feature = "pty")]
    pub fn child_pid(&self) -> u32 {
        self.shell_pid
    }

    /// Hang up the child's process group through the lifecycle guard:
    /// a no-op once the child was reaped, so a host can signal on its
    /// own teardown without racing the reader thread's escalation or
    /// hitting a recycled pid. Returns false when delivery failed.
    #[cfg(all(feature = "pty", not(target_os = "windows")))]
    pub fn hangup_child(&self) -> bool {
        self.child_terminator.hangup().is_ok()
    }

    /// The foreground process's name (the program the user is running
    /// right now: `claude`, `vim`, or the shell itself), from the kernel.
    /// Hosts use it to tell what a pane is running without any shell
    /// integration.
    #[cfg(feature = "pty")]
    pub fn foreground_process_name(&self) -> String {
        #[cfg(target_os = "windows")]
        return String::new();

        #[cfg(not(target_os = "windows"))]
        teletypewriter::foreground_process_name(self.main_fd, self.shell_pid)
    }

    /// Inject bytes into the terminal's DISPLAY (the VT parser), as if they
    /// came from the child process — NOT into the PTY input. Used to replay
    /// saved scrollback on restore; the shell never sees these bytes, so it
    /// can't execute them. Bytes are plain output (convert `\n` to `\r\n`
    /// upstream if you want proper line starts).
    ///
    /// The parser is persistent: an escape sequence split across two
    /// calls resumes where it left off, exactly like PTY chunking.
    pub fn inject_output(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let mut guard = self.processor.lock().unwrap();
        let processor = guard.get_or_insert_with(Default::default);
        let mut term = self.terminal.lock();
        processor.advance(&mut *term, bytes);
        // With a PTY, the replies these bytes produce go to its writer
        // thread as the parser produces them, in their place among
        // everything else bound for it; without one they are events.
        // No PTY thread announces injected bytes; the host that
        // injected them is told what the reader thread would tell it.
        if term.peek_damage_event().is_some() {
            term.request_render();
        }
    }

    /// Plain-text URL under a viewport cell, with the run to underline on
    /// the hovered row: `(uri, start_col, end_col)`. Detection is regex
    /// over the logical (unwrapped) line containing the cell, so wrapped
    /// URLs resolve whole. Hit-test shaped: call it from pointer events;
    /// OSC 8 links should be checked first, they are explicit.
    pub fn url_at(&self, viewport_line: u16, col: u16) -> Option<(String, u16, u16)> {
        use rio_vt::crosswords::pos::Direction;
        use rio_vt::crosswords::search::{RegexIter, RegexSearch};
        use rio_vt::crosswords::square::CellFlags;

        // A fully wrapped scrollback (one endless logical line) must not
        // turn a hover into a whole-buffer regex scan.
        const MAX_WRAP_SCAN: i32 = 100;

        let mut guard = self.url_regex.lock().unwrap();
        let regex = match guard.as_mut() {
            Some(regex) => regex,
            None => guard.insert(
                RegexSearch::new(URL_REGEX).expect("static URL pattern compiles"),
            ),
        };

        let term = self.terminal.lock();
        let cols = term.columns();
        let rows = term.screen_lines() as i32;
        let history = term.history_size() as i32;
        let display_offset = term.display_offset() as i32;
        let grid_line = viewport_line as i32 - display_offset;
        if grid_line >= rows || col as usize >= cols {
            return None;
        }
        let point = Pos::new(Line(grid_line), PosColumn(col as usize));

        let wraps = |line: i32| {
            term.grid[Line(line)][PosColumn(cols - 1)]
                .cell_flags()
                .contains(CellFlags::WRAPLINE)
        };
        let mut start_line = grid_line;
        while start_line > -history
            && grid_line - start_line < MAX_WRAP_SCAN
            && wraps(start_line - 1)
        {
            start_line -= 1;
        }
        let mut end_line = grid_line;
        while end_line < rows - 1
            && end_line - grid_line < MAX_WRAP_SCAN
            && wraps(end_line)
        {
            end_line += 1;
        }

        let from = Pos::new(Line(start_line), PosColumn(0));
        let to = Pos::new(Line(end_line), PosColumn(cols - 1));
        for m in RegexIter::new(from, to, Direction::Right, &term, regex) {
            let (s, mut e) = (*m.start(), *m.end());
            if point < s {
                break;
            }
            let text = term.bounds_to_string(s, e);
            // The pattern is greedy about trailing punctuation; prose is
            // not: `https://rio.dev,` links without the comma, and a paren
            // only belongs to the URL when it was opened inside it.
            let trim = trailing_url_punctuation(&text);
            for _ in 0..trim {
                if e.col.0 == 0 {
                    e.row.0 -= 1;
                    e.col = PosColumn(cols - 1);
                } else {
                    e.col.0 -= 1;
                }
            }
            if point > e {
                continue;
            }
            let uri: String = {
                let chars = text.chars().count() - trim;
                text.chars().take(chars).collect()
            };
            let start_col = if s.row.0 < grid_line {
                0
            } else {
                s.col.0 as u16
            };
            let end_col = if e.row.0 > grid_line {
                (cols - 1) as u16
            } else {
                e.col.0 as u16
            };
            return Some((uri, start_col, end_col));
        }
        None
    }

    /// Dump the whole buffer (scrollback + screen) to plain text, so a
    /// frontend can persist it and replay it as inert scrollback on
    /// restore. Trailing blank rows are trimmed by `bounds_to_string`.
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

    pub(crate) fn terminal(&self) -> Arc<TerminalLock<Crosswords<Listener>>> {
        self.terminal.clone()
    }

    pub(crate) fn inline_images(&self) -> InlineImages {
        self.inline_images.clone()
    }

    pub(crate) fn cursor_shaping_break(&self) -> Arc<std::sync::atomic::AtomicBool> {
        self.cursor_shaping_break.clone()
    }
}

#[cfg(feature = "pty")]
impl Drop for Surface {
    fn drop(&mut self) {
        let _ = self.channel.send(Msg::Shutdown);
        // Also hang up synchronously: an embedder may exit the process
        // right after dropping the surface, before the reader thread can
        // run its shutdown escalation. The handle is a no-op once the
        // child was reaped, so no stale PID is ever signaled.
        #[cfg(not(target_os = "windows"))]
        let _ = self.child_terminator.hangup();
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
        fn handles_program_status(&self) -> bool {
            true
        }
    }

    // A selection and injected output change what is shown without a
    // PTY thread to announce it, so the surface wakes the host itself.
    #[test]
    fn selection_and_injected_output_wake_the_host() {
        let delegate = Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        });
        let engine = Engine::new(delegate.clone());
        // A child that prints nothing, so every wakeup counted here is
        // the surface's own and none comes from a shell prompt.
        let surface = engine
            .create_surface(&SurfaceDesc {
                shell: Some("/bin/sh".to_string()),
                args: vec!["-c".to_string(), "sleep 300".to_string()],
                ..SurfaceDesc::default()
            })
            .expect("spawn sleeper");

        let before = delegate.wakeups.load(Ordering::SeqCst);
        surface.selection_begin(0, 0, SelectionKind::Simple, Side::Left);
        assert!(delegate.wakeups.load(Ordering::SeqCst) > before);

        let before = delegate.wakeups.load(Ordering::SeqCst);
        surface.selection_update(0, 3, Side::Right);
        assert!(delegate.wakeups.load(Ordering::SeqCst) > before);

        let before = delegate.wakeups.load(Ordering::SeqCst);
        surface.selection_clear();
        assert!(delegate.wakeups.load(Ordering::SeqCst) > before);

        let before = delegate.wakeups.load(Ordering::SeqCst);
        surface.inject_output(b"restored\x1b[2J");
        assert!(delegate.wakeups.load(Ordering::SeqCst) > before);
    }

    // Issue #2119: yazi pushes kitty flags 1|4|8|16 and matches keys on the
    // shifted character. The flags it sets must reach the encoder, or
    // shift+g arrives as a bare `g` and shift looks dead.
    #[test]
    fn yazi_keyboard_flags_reach_the_encoder() {
        let engine = Engine::new(Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        }));
        let surface = engine
            .create_surface(&SurfaceDesc {
                shell: Some("/bin/sh".to_string()),
                args: vec!["-c".to_string(), "sleep 300".to_string()],
                ..SurfaceDesc::default()
            })
            .expect("spawn sleeper");

        surface.inject_output(b"\x1b[>29u");
        let flags = kitty_flags(surface.terminal.lock().keyboard_mode());
        assert_eq!(
            flags,
            key::KittyFlags::DISAMBIGUATE
                | key::KittyFlags::REPORT_ALL_AS_ESC
                | key::KittyFlags::REPORT_ALTERNATE_KEYS
                | key::KittyFlags::REPORT_ASSOCIATED_TEXT
        );

        let shift_g = KeyEvent {
            key: Some(key::Key::Char('g')),
            mods: Modifiers::SHIFT,
            text: Some("G".to_string()),
            ..KeyEvent::default()
        };
        let ctx = key::EncodeContext {
            kitty: flags,
            ..key::EncodeContext::default()
        };
        assert_eq!(
            key::encode(&shift_g, &ctx),
            Some(b"\x1b[103:71;2;71u".to_vec())
        );

        // Leaving, yazi pops its flags: shift+g is plain text again.
        surface.inject_output(b"\x1b[<u");
        let flags = kitty_flags(surface.terminal.lock().keyboard_mode());
        assert!(flags.is_empty());
        let ctx = key::EncodeContext {
            kitty: flags,
            ..key::EncodeContext::default()
        };
        assert_eq!(key::encode(&shift_g, &ctx), Some(b"G".to_vec()));
    }

    // DECKPAM and DECKPNM reach the encoder: a numpad key sends its SS3
    // code while a program has the application keypad on, and types again
    // once it's off.
    #[test]
    fn application_keypad_mode_reaches_the_encoder() {
        let engine = Engine::new(Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        }));
        let surface = engine
            .create_surface(&SurfaceDesc {
                shell: Some("/bin/sh".to_string()),
                args: vec!["-c".to_string(), "sleep 300".to_string()],
                ..SurfaceDesc::default()
            })
            .expect("spawn sleeper");
        let keypad_1 = KeyEvent {
            key: Some(key::Key::Keypad(key::Keypad::Digit(1))),
            text: Some("1".to_string()),
            ..KeyEvent::default()
        };
        let ctx = |surface: &Surface| {
            let terminal = surface.terminal.lock();
            key::EncodeContext {
                app_keypad: terminal.mode().contains(Mode::APP_KEYPAD),
                ..key::EncodeContext::default()
            }
        };

        surface.inject_output(b"\x1b=");
        assert_eq!(
            key::encode(&keypad_1, &ctx(&surface)),
            Some(b"\x1bOq".to_vec())
        );
        surface.inject_output(b"\x1b>");
        assert_eq!(key::encode(&keypad_1, &ctx(&surface)), Some(b"1".to_vec()));
    }

    // Each kitty flag a program can set maps to the encoder's own, one to
    // one, so no flag is silently dropped on the way.
    #[test]
    fn every_kitty_flag_reaches_the_encoder() {
        use rio_vt::ansi::KeyboardModes;
        let cases = [
            (
                KeyboardModes::DISAMBIGUATE_ESC_CODES,
                key::KittyFlags::DISAMBIGUATE,
            ),
            (
                KeyboardModes::REPORT_EVENT_TYPES,
                key::KittyFlags::REPORT_EVENT_TYPES,
            ),
            (
                KeyboardModes::REPORT_ALTERNATE_KEYS,
                key::KittyFlags::REPORT_ALTERNATE_KEYS,
            ),
            (
                KeyboardModes::REPORT_ALL_KEYS_AS_ESC,
                key::KittyFlags::REPORT_ALL_AS_ESC,
            ),
            (
                KeyboardModes::REPORT_ASSOCIATED_TEXT,
                key::KittyFlags::REPORT_ASSOCIATED_TEXT,
            ),
        ];
        for (mode, flag) in cases {
            assert_eq!(kitty_flags(mode), flag, "{mode:?}");
        }
        assert!(kitty_flags(KeyboardModes::empty()).is_empty());
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

    /// The measurement entry through the C ABI: count plus width via
    /// out-param, and null tolerance on both pointers.
    #[test]
    fn cluster_width_through_c_abi() {
        let cps: [u32; 3] = [0x1F468, 0x200D, 0x1F33E];
        let mut width: u8 = 0xFF;
        let len = unsafe { capi::rio_cluster_width(cps.as_ptr(), cps.len(), &mut width) };
        assert_eq!((len, width), (3, 2));

        // The width out-param is optional.
        let len = unsafe {
            capi::rio_cluster_width(cps.as_ptr(), cps.len(), core::ptr::null_mut())
        };
        assert_eq!(len, 3);

        // A null or empty buffer measures as nothing.
        let mut width: u8 = 0xFF;
        let len = unsafe { capi::rio_cluster_width(core::ptr::null(), 5, &mut width) };
        assert_eq!((len, width), (0, 0));
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

    /// A surface and render state for the synchronized update tests. The
    /// engine is returned too, since the surface must not outlive it.
    fn hold_fixture() -> (Engine, Surface, RenderState) {
        let engine = Engine::new(Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        }));
        let surface = engine
            .create_surface(&SurfaceDesc::default())
            .expect("spawn shell");
        let state = RenderState::new(&surface);
        (engine, surface, state)
    }

    fn row_10(state: &RenderState) -> Option<char> {
        state.square(9, 0).map(|sq| sq.c())
    }

    // A cursor move marks the row the cursor left and the row it is on,
    // which a shaping break at the cursor changes, without the terminal
    // copying either. A host that does not break at the cursor says so:
    // the row shaped around the cursor is marked once more, and a
    // cursor move then leaves every row clean.
    #[test]
    fn the_cursor_shaping_break_decides_whether_a_move_dirties_rows() {
        let engine = Engine::new(Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        }));
        // A child that prints nothing, so only injected output and the
        // cursor touch the rows.
        let surface = engine
            .create_surface(&SurfaceDesc {
                shell: Some("/bin/sh".to_string()),
                args: vec!["-c".to_string(), "sleep 300".to_string()],
                ..SurfaceDesc::default()
            })
            .expect("spawn sleeper");
        let mut state = RenderState::new(&surface);
        surface.inject_output(b"ab\r\n\r\ncd\x1b[3;2H");
        state.update();
        state.reset_dirty();

        surface.set_cursor_shaping_break(false);
        state.update();
        assert!(state.row_dirty(2), "the row shaped around the cursor");
        assert!(!state.row_dirty(0));
        state.reset_dirty();
        surface.inject_output(b"\x1b[1;2H");
        state.update();
        assert_eq!(state.cursor(), (0, 1), "line 0, column 1");
        assert!(
            !state.row_dirty(0) && !state.row_dirty(2),
            "no break, no row work"
        );

        surface.set_cursor_shaping_break(true);
        surface.inject_output(b"\x1b[3;2H");
        state.update();
        assert!(state.row_dirty(2), "the row the cursor is on");
        assert!(
            !state.row_dirty(0),
            "the row it left was shaped without a cursor"
        );
        state.reset_dirty();
        surface.inject_output(b"\x1b[1;2H");
        state.update();
        assert!(state.row_dirty(0), "the row the cursor is on");
        assert!(state.row_dirty(2), "the row the cursor left");
    }

    /// Move the active update's last BSU far enough back that it expired.
    fn expire_hold(surface: &Surface) {
        surface.terminal().lock().sync_update_started = rio_vt::time::Instant::now()
            .checked_sub(rio_vt::time::Duration::from_secs(2));
    }

    /// A synchronized update holds the last snapshot until it ends, so an
    /// embedder never draws a half-written frame.
    #[test]
    fn render_state_holds_during_synchronized_update() {
        let (_engine, surface, mut state) = hold_fixture();

        surface.inject_output(b"\x1b[10;1Ha");
        state.update();
        assert_eq!(row_10(&state), Some('a'));

        surface.inject_output(b"\x1b[?2026h\x1b[10;1Hb");
        state.update();
        assert_eq!(row_10(&state), Some('a'), "held during the update");

        surface.inject_output(b"\x1b[?2026l");
        state.update();
        assert_eq!(row_10(&state), Some('b'));
    }

    /// A resize that changes nothing keeps the update; a pixel size
    /// change ends it even when the grid keeps its size.
    #[test]
    fn render_state_resize_ends_hold_only_on_change() {
        let (_engine, surface, mut state) = hold_fixture();
        let desc = SurfaceDesc::default();

        surface.inject_output(b"\x1b[10;1Ha");
        state.update();
        surface.inject_output(b"\x1b[?2026h\x1b[10;1Hb");
        state.update();
        assert_eq!(row_10(&state), Some('a'));

        surface.resize(desc.cols, desc.rows, desc.pixel_width, desc.pixel_height);
        state.update();
        assert_eq!(row_10(&state), Some('a'), "unchanged size keeps the hold");

        surface.resize(
            desc.cols,
            desc.rows,
            desc.pixel_width * 2,
            desc.pixel_height,
        );
        state.update();
        assert_eq!(row_10(&state), Some('b'));
    }

    /// A program that never ends its update cannot hold the snapshot for
    /// more than a second.
    #[test]
    fn render_state_gives_up_on_a_stuck_hold() {
        let (_engine, surface, mut state) = hold_fixture();

        surface.inject_output(b"\x1b[10;1Ha");
        state.update();
        surface.inject_output(b"\x1b[?2026h\x1b[10;1Hb");
        state.update();
        assert_eq!(row_10(&state), Some('a'));

        expire_hold(&surface);
        state.update();
        assert_eq!(row_10(&state), Some('b'));
        assert!(!surface.terminal().lock().synchronized_output());
    }

    /// Every BSU re-arms the limit, so a stream of back-to-back updates is
    /// not cut off mid-frame.
    #[test]
    fn render_state_hold_limit_rearms_on_bsu() {
        let (_engine, surface, mut state) = hold_fixture();

        surface.inject_output(b"\x1b[10;1Ha");
        state.update();
        surface.inject_output(b"\x1b[?2026h\x1b[10;1Hb");
        expire_hold(&surface);

        surface.inject_output(b"\x1b[?2026l\x1b[?2026h\x1b[10;1Hc");
        state.update();
        assert_eq!(row_10(&state), Some('a'), "the new update is still held");
        assert!(surface.terminal().lock().synchronized_output());
    }

    // The wasm renderer draws a cell's full cluster text (base +
    // attached codepoints) when the wire flag says one exists; the
    // accessor is the source of that text.
    #[test]
    fn render_state_exposes_cluster_text() {
        let engine = Engine::new(Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        }));
        let surface = engine
            .create_surface(&SurfaceDesc::default())
            .expect("spawn shell");
        let mut state = RenderState::new(&surface);

        // Mode 2027 on, then a ZWJ emoji and a decomposed accent.
        surface
            .inject_output("\x1b[?2027h\u{1F9D1}\u{200D}\u{1F33E}e\u{301}x".as_bytes());
        state.update();

        assert_eq!(
            state.cell_cluster_text(0, 0).as_deref(),
            Some("\u{1F9D1}\u{200D}\u{1F33E}")
        );
        assert_eq!(state.cell_cluster_text(0, 2).as_deref(), Some("e\u{301}"));
        // Plain cells report nothing.
        assert_eq!(state.cell_cluster_text(0, 3), None);
    }

    // SGR is the modern form; the X10 fallback offsets by 32.
    #[test]
    fn mouse_reports_encode_both_forms() {
        // SGR press keeps the button and ends in `M`; release ends in `m`.
        assert_eq!(
            mouse_report(64, 4, 2, true, true, false),
            b"\x1b[<64;5;3M".to_vec()
        );
        assert_eq!(
            mouse_report(0, 4, 2, false, true, false),
            b"\x1b[<0;5;3m".to_vec()
        );
        // X10 press offsets by 32; release collapses to button 3.
        assert_eq!(
            mouse_report(65, 0, 0, true, false, false),
            vec![0x1b, b'[', b'M', 32 + 65, 33, 33]
        );
        assert_eq!(
            mouse_report(0, 0, 0, false, false, false),
            vec![0x1b, b'[', b'M', 32 + 3, 33, 33]
        );
        // A legacy release keeps the modifier bits: ctrl+left release is
        // 16 | 3 = 19, not a bare 3.
        assert_eq!(
            mouse_report(16, 0, 0, false, false, false),
            vec![0x1b, b'[', b'M', 32 + 19, 33, 33]
        );
    }

    // Clicks and drags report only when the program asked; shift bypasses to
    // a local selection, and motion waits for 1002/1003.
    #[test]
    fn clicks_report_when_the_program_asks() {
        let engine = Engine::new(Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        }));
        let surface = engine
            .create_surface(&SurfaceDesc::default())
            .expect("spawn shell");

        // Nothing grabs the mouse yet.
        assert!(!surface.mouse_button(4, 2, 0, true, Modifiers::empty()));

        surface.inject_output(b"\x1b[?1000h\x1b[?1006h");
        assert!(surface.mouse_button(4, 2, 0, true, Modifiers::empty()));
        assert!(surface.mouse_button(4, 2, 0, false, Modifiers::empty()));
        // Shift forces a local selection instead of a report.
        assert!(!surface.mouse_button(4, 2, 0, true, Modifiers::SHIFT));
        // 1000 is click-only: motion isn't wanted until 1002/1003.
        assert!(!surface.mouse_motion(5, 2, 0, Modifiers::empty()));

        surface.inject_output(b"\x1b[?1002h");
        assert!(surface.mouse_motion(5, 2, 0, Modifiers::empty()));
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

    // OSC 8 links must resolve through the pulled render state: URI under
    // a cell, and the row-run a renderer underlines on hover.
    #[test]
    fn osc8_links_resolve_from_render_state() {
        let delegate = Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        });
        let engine = Engine::new(delegate);
        let surface = engine
            .create_surface(&SurfaceDesc::default())
            .expect("spawn shell");
        let mut state = RenderState::new(&surface);

        surface.inject_output(
            b"\x1b[2J\x1b[Hpre \x1b]8;;https://rioterm.com\x1b\\rio link\x1b]8;;\x1b\\ post",
        );
        state.update();

        assert_eq!(state.link_at(0, 0), None);
        assert_eq!(state.link_at(0, 4), Some("https://rioterm.com"));
        assert_eq!(state.link_at(0, 11), Some("https://rioterm.com"));
        assert_eq!(state.link_at(0, 13), None);
        assert_eq!(state.link_run(0, 6), Some((4, 11)));
        assert_eq!(state.link_run(0, 0), None);
    }

    // mode_bits mirrors the private modes programs toggle at runtime.
    #[test]
    fn mode_bits_track_private_modes() {
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

    // Focus reports ride the terminal's reply path, so they reach the PTY
    // without the typing side effects of write(). The sleeper child never
    // reads; what lands in the grid is the tty's echo, ^[[I and ^[[O.
    #[test]
    fn focus_reports_reach_the_pty_once_per_change() {
        let surface = quiet_surface(60, 10);
        let mut state = RenderState::new(&surface);
        std::thread::sleep(Duration::from_millis(150));
        // Enabling the mode on an unfocused surface reports ^[[O at once.
        surface.inject_output(b"\x1b[?1004h");

        // Not a change, a change, a repeat, a change: ^[[I then ^[[O.
        surface.set_focus(false);
        surface.set_focus(true);
        surface.set_focus(true);
        surface.set_focus(false);

        let deadline = Instant::now() + Duration::from_secs(8);
        let text = loop {
            state.update();
            let text = (0..state.lines())
                .map(|i| state.text_row(i))
                .collect::<Vec<_>>()
                .join("\n");
            if text.matches("^[[O").count() == 2 {
                break text;
            }
            assert!(
                Instant::now() < deadline,
                "focus reports never echoed: {text:?}"
            );
            std::thread::sleep(Duration::from_millis(25));
        };
        assert_eq!(text.matches("^[[I").count(), 1, "{text:?}");
    }

    // A host confirms pastes that could run commands: any line break
    // while the program takes pastes as typing, only a bracket end once it
    // asks for bracketed paste.
    #[test]
    fn paste_is_safe_follows_bracketed_paste() {
        let surface = quiet_surface(20, 5);
        assert!(surface.paste_is_safe("ls -la"));
        assert!(!surface.paste_is_safe("ls\nrm -rf ~"));
        surface.inject_output(b"\x1b[?2004h");
        assert!(surface.paste_is_safe("ls\nrm -rf ~"));
        assert!(!surface.paste_is_safe("a\x1b[201~rm"));
    }

    // paste() keys off live terminal state: markers go to the child only
    // once the program turned mode 2004 on. The sleeper child never reads,
    // so what lands in the grid is the tty line discipline's echo, which
    // renders the ESC of each marker as ^[ (ECHOCTL).
    #[test]
    fn paste_brackets_when_the_program_asks() {
        let surface = quiet_surface(60, 10);
        let mut state = RenderState::new(&surface);
        std::thread::sleep(Duration::from_millis(150));

        let grid_rows = |state: &mut RenderState, needle: &str| {
            let deadline = Instant::now() + Duration::from_secs(8);
            loop {
                state.update();
                let rows: Vec<String> =
                    (0..state.lines()).map(|i| state.text_row(i)).collect();
                if rows.iter().any(|row| row.contains(needle)) {
                    return rows;
                }
                if Instant::now() >= deadline {
                    panic!("echo of {needle:?} never reached the grid: {rows:?}");
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        };

        surface.paste("plain\n");
        let rows = grid_rows(&mut state, "plain");
        assert!(
            !rows.iter().any(|row| row.contains("^[[200~")),
            "unbracketed paste must not emit markers: {rows:?}"
        );

        surface.inject_output(b"\x1b[?2004h");
        surface.paste("wrapped");
        grid_rows(&mut state, "^[[200~wrapped^[[201~");
    }

    // A child that never writes, so buffer contents are exactly what the
    // test injected and comparisons can't race the shell prompt. Spawned
    // via /bin/sh, the one binary the nix build sandbox provides.
    fn quiet_surface(cols: u16, rows: u16) -> Surface {
        let engine = Engine::new(Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        }));
        engine
            .create_surface(&SurfaceDesc {
                shell: Some("/bin/sh".to_string()),
                args: vec!["-c".to_string(), "sleep 300".to_string()],
                cols,
                rows,
                ..SurfaceDesc::default()
            })
            .expect("spawn sleeper")
    }

    // serialize() must reconstruct content, styling, and hyperlinks when
    // replayed into a fresh same-width terminal. Serializing the replica
    // again is the equality check: identical bytes means identical state.
    #[test]
    fn serialize_round_trips_styles_and_links() {
        let surface = quiet_surface(40, 10);
        surface.inject_output(
            b"\x1b[31mred\x1b[0m plain \x1b[1;4;38;5;208morange\x1b[0m\r\n\
              \x1b[48;2;10;20;30mrgb bg\x1b[0m \
              \x1b]8;;https://rioterm.com\x1b\\link\x1b]8;;\x1b\\\r\n\
              \x1b[5mslow\x1b[0m \x1b[6mfast\x1b[0m\r\n\
              wide: \xe4\xbd\xa0\xe5\xa5\xbd",
        );
        let first = surface.serialize();
        assert!(first.contains("\x1b]8;;https://rioterm.com\x1b\\"));
        assert!(first.contains(";5m"), "missing slow blink: {first:?}");
        assert!(first.contains(";6m"), "missing rapid blink: {first:?}");

        let replica = quiet_surface(40, 10);
        replica.inject_output(first.as_bytes());
        assert_eq!(replica.dump(), surface.dump());
        assert_eq!(replica.serialize(), first);
    }

    // The C ABI marks cells carrying an explicit SGR 58 underline color
    // and hands the color out; unmarked cells take the glyph's color.
    #[test]
    fn capi_reports_underline_color() {
        use crate::capi::{
            rio_render_state_cell, rio_render_state_cell_underline_color,
            RIO_CELL_HAS_UNDERLINE_COLOR, RIO_COLOR_INDEXED, RIO_COLOR_NONE,
            RIO_COLOR_RGB,
        };

        let surface = quiet_surface(10, 3);
        surface.inject_output(b"\x1b[4;58;2;10;20;30ma\x1b[0mb\x1b[4;58;5;196mc\x1b[0m");
        let mut state = RenderState::new(&surface);
        state.update();

        let marked = unsafe { rio_render_state_cell(&state, 0, 0) };
        assert_ne!(marked.style_flags & RIO_CELL_HAS_UNDERLINE_COLOR, 0);
        let color = unsafe { rio_render_state_cell_underline_color(&state, 0, 0) };
        assert_eq!(color.kind, RIO_COLOR_RGB);
        assert_eq!((color.r, color.g, color.b), (10, 20, 30));

        let plain = unsafe { rio_render_state_cell(&state, 0, 1) };
        assert_eq!(plain.style_flags & RIO_CELL_HAS_UNDERLINE_COLOR, 0);
        // The NONE sentinel promises zeroed value and rgb.
        let absent = unsafe { rio_render_state_cell_underline_color(&state, 0, 1) };
        assert_eq!(absent.kind, RIO_COLOR_NONE);
        assert_eq!(absent.value, 0);
        assert_eq!((absent.r, absent.g, absent.b), (0, 0, 0));

        // Indexed colors keep their form and still resolve rgb (256-color
        // cube index 196 is pure red).
        let indexed = unsafe { rio_render_state_cell_underline_color(&state, 0, 2) };
        assert_eq!(indexed.kind, RIO_COLOR_INDEXED);
        assert_eq!(indexed.value, 196);
        assert_eq!((indexed.r, indexed.g, indexed.b), (255, 0, 0));

        // A NULL state and an out-of-range cell also answer NONE, and a
        // missing cell's fg/bg carry the NONE kind.
        let null =
            unsafe { rio_render_state_cell_underline_color(std::ptr::null(), 0, 0) };
        assert_eq!(null.kind, RIO_COLOR_NONE);
        let oob = unsafe { rio_render_state_cell_underline_color(&state, 99, 99) };
        assert_eq!(oob.kind, RIO_COLOR_NONE);
        let missing = unsafe { rio_render_state_cell(&state, 99, 99) };
        assert_eq!(missing.fg.kind, RIO_COLOR_NONE);
        assert_eq!(missing.bg.kind, RIO_COLOR_NONE);
    }

    // Scrollback rows come first, and wrapped rows are emitted without a
    // newline so the replica re-wraps them at the same width.
    #[test]
    fn serialize_covers_scrollback_and_rewraps() {
        let surface = quiet_surface(20, 5);
        for i in 0..12 {
            surface.inject_output(format!("history line {i}\r\n").as_bytes());
        }
        surface.inject_output(b"abcdefghijklmnopqrstuvwxyz");
        assert!(surface.history_size() > 0);
        let first = surface.serialize();

        let replica = quiet_surface(20, 5);
        replica.inject_output(first.as_bytes());
        assert_eq!(replica.dump(), surface.dump());
        assert_eq!(replica.history_size(), surface.history_size());
        assert_eq!(replica.serialize(), first);
    }

    // Search coordinates are ring-relative: line 0 is the top of the
    // scrollback, so hits stay valid however the viewport is scrolled.
    #[test]
    fn search_reports_ring_relative_coordinates() {
        let surface = quiet_surface(40, 5);
        for i in 0..8 {
            surface.inject_output(format!("filler {i}\r\n").as_bytes());
        }
        surface.inject_output(b"needle at last");

        let fillers = surface.search("filler", 100).expect("valid pattern");
        assert_eq!(fillers.len(), 8);
        assert_eq!(fillers[0].0, 0);
        assert_eq!(fillers[7].0, 7);

        let matches = surface.search("needle", 10).expect("valid pattern");
        assert_eq!(matches.len(), 1);
        let (start_line, start_col, end_line, end_col) = matches[0];
        assert_eq!(start_line as usize, surface.history_size() + 4);
        assert_eq!(start_col, 0);
        assert_eq!(end_line, start_line);
        assert_eq!(end_col, 5);

        assert_eq!(surface.search("filler", 3).unwrap().len(), 3);
        assert!(surface.search("[", 10).is_none());
    }

    // The parser must survive chunk boundaries: transports split output
    // arbitrarily, including mid-escape-sequence. A discarded parser
    // would print the tail of the sequence as literal text.
    #[test]
    fn escape_sequences_survive_chunk_boundaries() {
        let surface = quiet_surface(40, 5);
        // Split an SGR sequence in the middle: "\x1b[31m" + "red".
        surface.inject_output(b"\x1b[3");
        surface.inject_output(b"1mred");
        let dump = surface.dump();
        assert_eq!(dump.trim_end(), "red", "no literal escape tail: {dump:?}");
        // The style applied: serialize carries the red foreground (the
        // default bg is covered by the leading reset).
        assert!(surface.serialize().contains("\x1b[0;31m"));

        // Split inside an OSC title too.
        surface.inject_output(b"\x1b]0;hel");
        surface.inject_output(b"lo\x07after");
        assert!(surface.dump().contains("redafter"));
    }

    // DECTCEM and a scrolled viewport both hide the cursor from
    // renderers; showing it again restores visibility.
    #[test]
    fn cursor_visibility_tracks_dectcem_and_scroll() {
        let surface = quiet_surface(40, 5);
        let mut state = RenderState::new(&surface);

        state.update();
        assert!(state.cursor_visible());

        surface.inject_output(b"\x1b[?25l");
        state.update();
        assert!(!state.cursor_visible());

        surface.inject_output(b"\x1b[?25h");
        state.update();
        assert!(state.cursor_visible());

        for i in 0..20 {
            surface.inject_output(format!("line {i}\r\n").as_bytes());
        }
        surface.scroll(5);
        state.update();
        assert!(!state.cursor_visible(), "scrolled view has no live cursor");
        surface.scroll(-100);
        state.update();
        assert!(state.cursor_visible());
    }

    // Plain-text URLs resolve under the pointer, prose punctuation stays
    // prose, and non-link text misses.
    #[test]
    fn urls_resolve_under_the_pointer() {
        let surface = quiet_surface(60, 5);
        surface.inject_output(b"see https://rio.dev, or (http://a.b/c).");

        let (uri, start, end) = surface.url_at(0, 10).expect("hover on the url");
        assert_eq!(uri, "https://rio.dev");
        assert_eq!((start, end), (4, 18));
        // The comma after the URL is prose, not link.
        assert!(surface.url_at(0, 19).is_none());
        // Parenthesized URL: closing paren and period trimmed.
        let (uri, start, end) = surface.url_at(0, 30).expect("hover in parens");
        assert_eq!(uri, "http://a.b/c");
        assert_eq!((start, end), (25, 36));
        // Plain words miss.
        assert!(surface.url_at(0, 0).is_none());
    }

    // A pipe ends a URL: `curl https://x.dev|jq` must not swallow the
    // pipeline into the link.
    #[test]
    fn urls_stop_at_pipes() {
        let surface = quiet_surface(60, 5);
        surface.inject_output(b"curl https://x.dev|jq .");
        let (uri, start, end) = surface.url_at(0, 10).expect("hover on url");
        assert_eq!(uri, "https://x.dev");
        assert_eq!((start, end), (5, 17));
        assert!(surface.url_at(0, 18).is_none());
    }

    // Hit-testing takes the scroll position into account: viewport
    // coordinates keep resolving after the view moves into history.
    #[test]
    fn urls_resolve_while_scrolled() {
        let surface = quiet_surface(40, 5);
        surface.inject_output(b"https://rio.dev/x first\r\n");
        for i in 0..8 {
            surface.inject_output(format!("filler {i}\r\n").as_bytes());
        }
        assert!(surface.url_at(0, 3).is_none(), "url is above the viewport");
        surface.scroll(100);
        let (uri, start, end) = surface.url_at(0, 3).expect("scrolled to top");
        assert_eq!(uri, "https://rio.dev/x");
        assert_eq!((start, end), (0, 16));
    }

    // A URL that wraps across rows resolves whole from either row, with
    // per-row run bounds for hover underlining.
    #[test]
    fn urls_span_wrapped_rows() {
        let surface = quiet_surface(20, 5);
        surface.inject_output(b"x https://example.com/abcdef end");

        let (uri, start, end) = surface.url_at(1, 3).expect("hover on second row");
        assert_eq!(uri, "https://example.com/abcdef");
        assert_eq!((start, end), (0, 7));
        let (uri, start, end) = surface.url_at(0, 5).expect("hover on first row");
        assert_eq!(uri, "https://example.com/abcdef");
        assert_eq!((start, end), (2, 19));
        // Past the URL on the second row is plain text again.
        assert!(surface.url_at(1, 10).is_none());
    }

    // A scheme URL mid-line followed by a parenthesized note: the hover
    // run must sit exactly under the URL, not drift into the scheme or
    // the note.
    #[test]
    fn urls_underline_alignment_mid_line() {
        let surface = quiet_surface(90, 5);
        surface.inject_output(
            b"Dev server is listening at http://localhost:1313/ (bind address 127.0.0.1)",
        );

        // "Dev server is listening at " is 27 cells, so the URL occupies
        // cols 27..=48 and the note after it is prose.
        for col in [27, 31, 35, 48] {
            let (uri, start, end) = surface
                .url_at(0, col)
                .unwrap_or_else(|| panic!("hover at col {col} misses the url"));
            assert_eq!(uri, "http://localhost:1313/");
            assert_eq!((start, end), (27, 48), "hover at col {col}");
        }
        assert!(surface.url_at(0, 26).is_none(), "space before the scheme");
        assert!(surface.url_at(0, 50).is_none(), "note after the url");
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

    // A delegate that doesn't handle OSC 7501 gets no program-status
    // actions, and the terminal doesn't claim support to programs.
    #[test]
    fn program_status_needs_the_delegate_to_opt_in() {
        struct OptedOut(Mutex<Vec<Action>>);
        impl SurfaceDelegate for OptedOut {
            fn wakeup(&self, _surface: SurfaceId) {}
            fn action(&self, _surface: SurfaceId, action: Action) {
                self.0.lock().unwrap().push(action);
            }
        }
        let delegate = Arc::new(OptedOut(Mutex::new(Vec::new())));
        let engine = Engine::new(delegate.clone());
        let surface = engine
            .create_surface(&SurfaceDesc::default())
            .expect("spawn shell");

        surface.inject_output(b"\x1b]7501;state=working\x07");
        surface.inject_output(b"\x1b]133;A\x07");

        assert!(!delegate
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|a| matches!(a, Action::ProgramStatus(_))));
    }

    // OSC 7501 reaches the embedder as validated reports plus the
    // lifecycle events that tell it to drop records.
    #[test]
    fn program_status_reaches_the_delegate() {
        let delegate = Arc::new(ActionRecorder {
            actions: Mutex::new(Vec::new()),
        });
        let engine = Engine::new(delegate.clone());
        let surface = engine
            .create_surface(&SurfaceDesc::default())
            .expect("spawn shell");

        surface.inject_output(b"\x1b]7501;state=working:app=cargo:progress=40\x07");
        surface.inject_output(b"\x1b]7501;state=nope\x07");
        surface.inject_output(b"\x1b]133;A\x07");

        let actions = delegate.actions.lock().unwrap();
        let status: Vec<&ProgramStatusEvent> = actions
            .iter()
            .filter_map(|a| match a {
                Action::ProgramStatus(event) => Some(event),
                _ => None,
            })
            .collect();
        assert_eq!(
            status,
            vec![
                &ProgramStatusEvent::Report(ProgramStatus {
                    state: ProgramState::Working,
                    kind: None,
                    app: Some("cargo".into()),
                    msg: None,
                    title: None,
                    progress: Some(40),
                    id: None,
                }),
                &ProgramStatusEvent::PromptStart,
            ]
        );
    }

    /// What reaches the delegate when a fresh surface (one with no
    /// notification sent yet) reads `bytes`.
    fn actions_for(bytes: &[u8]) -> Vec<Action> {
        let delegate = Arc::new(ActionRecorder {
            actions: Mutex::new(Vec::new()),
        });
        let engine = Engine::new(delegate.clone());
        let surface = engine
            .create_surface(&SurfaceDesc {
                shell: Some("/bin/sh".to_string()),
                args: vec!["-c".to_string(), "sleep 300".to_string()],
                ..SurfaceDesc::default()
            })
            .expect("spawn sleeper");
        surface.inject_output(bytes);
        let actions = delegate.actions.lock().unwrap().clone();
        actions
    }

    fn notes_for(bytes: &[u8]) -> Vec<(String, String)> {
        actions_for(bytes)
            .into_iter()
            .filter_map(|a| match a {
                Action::DesktopNotification { title, body } => Some((title, body)),
                _ => None,
            })
            .collect()
    }

    fn note(title: &str, body: &str) -> Vec<(String, String)> {
        vec![(title.to_string(), body.to_string())]
    }

    // OSC 9 and OSC 777 notify reach the embedder, ended with BEL or with
    // ST (ESC \\: the parser ends the string at the ESC).
    #[test]
    fn desktop_notifications_reach_the_delegate() {
        assert_eq!(notes_for(b"\x1b]9;build done\x07"), note("", "build done"));
        assert_eq!(
            notes_for(b"\x1b]9;tests; 3 failed\x1b\\"),
            note("", "tests; 3 failed")
        );
        assert_eq!(
            notes_for(b"\x1b]777;notify;Deploy;it shipped\x07"),
            note("Deploy", "it shipped")
        );
        assert_eq!(
            notes_for(b"\x1b]777;notify;Title;a;b\x1b\\"),
            note("Title", "a;b")
        );
    }

    // OSC 9;4 stays progress, and an empty body is no notification.
    #[test]
    fn desktop_notifications_not_sent() {
        let actions = actions_for(b"\x1b]9;4;1;50\x07");
        assert!(!actions
            .iter()
            .any(|a| matches!(a, Action::DesktopNotification { .. })));
        assert!(actions.contains(&Action::Progress {
            state: 1,
            value: 50
        }));
        assert!(notes_for(b"\x1b]9;\x07").is_empty());
        assert!(notes_for(b"\x1b]777;notify;Title;\x07").is_empty());
        // Only characters that can't show.
        assert!(notes_for("\x1b]9;\u{202E}\u{200B}\x07".as_bytes()).is_empty());
    }

    // Controls and bidirectional overrides are taken out; an emoji's
    // joiner stays.
    #[test]
    fn desktop_notifications_show_only_plain_text() {
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        let sent =
            format!("\x1b]777;notify;\u{202E}Mail\u{0085};ok\u{7F}\u{200B} {family}\x07");
        assert_eq!(
            notes_for(sent.as_bytes()),
            note("Mail", &format!("ok {family}"))
        );
    }

    // A surface sends one a second: the second of a burst is dropped.
    #[test]
    fn desktop_notifications_are_limited() {
        let notes = notes_for(b"\x1b]9;one\x07\x1b]9;two\x07");
        assert_eq!(notes, note("", "one"));
    }

    #[test]
    fn notify_limit_timing() {
        use rio_vt::time::{Duration, Instant};
        let t = Instant::now();
        let mut limit = NotifyLimit::default();
        assert!(limit.allow(t, "", "a"));
        assert!(!limit.allow(t + Duration::from_millis(500), "", "b"));
        assert!(limit.allow(t + Duration::from_millis(1100), "", "b"));
        // The same again within five seconds of the last one sent.
        assert!(!limit.allow(t + Duration::from_secs(3), "", "b"));
        assert!(limit.allow(t + Duration::from_secs(7), "", "b"));
        // Different text after a second.
        assert!(limit.allow(t + Duration::from_millis(8100), "", "c"));
    }

    // A long notification arrives cut, never inside a character: a
    // 3-byte character straddling the limit goes whole.
    #[test]
    fn desktop_notifications_are_cut() {
        let title = format!("{}\u{20ac}", "t".repeat(NOTIFICATION_TITLE_MAX - 1));
        let body = "b".repeat(NOTIFICATION_BODY_MAX + 100);
        let sent = format!("\x1b]777;notify;{title};{body}\x07");
        assert_eq!(
            notes_for(sent.as_bytes()),
            note(
                &"t".repeat(NOTIFICATION_TITLE_MAX - 1),
                &"b".repeat(NOTIFICATION_BODY_MAX)
            )
        );
    }

    #[test]
    fn cut_keeps_characters_whole() {
        assert_eq!(cut("abc".into(), 5), "abc");
        assert_eq!(cut("abc".into(), 2), "ab");
        assert_eq!(cut("a\u{20ac}".into(), 3), "a");
        assert_eq!(cut("a\u{20ac}".into(), 4), "a\u{20ac}");
        assert_eq!(cut(String::new(), 0), "");
    }

    // OSC 133 C then D reaches the embedder as one finished command with
    // its exit status; a D with no C before it is not a command.
    #[test]
    fn command_finished_reaches_the_delegate() {
        let delegate = Arc::new(ActionRecorder {
            actions: Mutex::new(Vec::new()),
        });
        let engine = Engine::new(delegate.clone());
        let surface = engine
            .create_surface(&SurfaceDesc::default())
            .expect("spawn shell");

        surface.inject_output(b"\x1b]133;D;0\x07");
        surface.inject_output(b"\x1b]133;C\x07out\r\n\x1b]133;D;2\x07");
        surface.inject_output(b"\x1b]133;C\x07\x1b]133;D\x07");

        let actions = delegate.actions.lock().unwrap();
        let finished: Vec<Option<i32>> = actions
            .iter()
            .filter_map(|a| match a {
                Action::CommandFinished { exit_status, .. } => Some(*exit_status),
                _ => None,
            })
            .collect();
        assert_eq!(finished, vec![Some(2), None]);
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
        let el_fill = state.style_at(5, last, state.square(5, last).unwrap());
        assert_eq!(el_fill.bg, AnsiColor::Indexed(2));

        let el2_fill = state.style_at(6, last, state.square(6, last).unwrap());
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

    fn image_surface() -> (Surface, RenderState) {
        let delegate = Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        });
        let engine = Engine::new(delegate);
        let surface = engine
            .create_surface(&SurfaceDesc::default())
            .expect("spawn shell");
        let state = RenderState::new(&surface);
        (surface, state)
    }

    // The default surface is 720x432 pixels for 80x24 cells.
    const CELL_W: f32 = 9.0;
    const CELL_H: f32 = 18.0;

    // A 2x2 sixel: raster attributes fix the size, color register 0 is
    // red, and `B` (sixel value 3, bits 0 and 1) paints the two rows of
    // each of its two columns.
    const SIXEL_2X2_RED: &[u8] = b"\x1bPq\"1;1;2;2#0;2;100;0;0#0BB\x1b\\";

    // A sixel image lands in the unified placement list with its
    // geometry, layer and pixels, the way a kitty image does; the kitty
    // accessors do not see it.
    #[test]
    fn sixel_image_reaches_render_state() {
        let (surface, mut state) = image_surface();
        state.update();
        assert_eq!(state.image_placements_generation(), 0);

        surface.inject_output(b"\x1b[6;5H");
        surface.inject_output(SIXEL_2X2_RED);
        state.update();

        assert_eq!(state.image_count(), 1);
        assert_eq!(state.kitty_count(), 0);
        assert_eq!(state.image_placements_generation(), 1);
        let placement = state
            .image_placement(0, CELL_W, CELL_H)
            .expect("placement in view");
        assert_eq!(placement.protocol, ImageProtocol::Inline);
        assert_eq!(placement.layer, ImageLayer::BelowText);
        assert_eq!(placement.z_index, -1);
        assert_eq!(placement.geometry.x, 4.0 * CELL_W);
        assert_eq!(placement.geometry.y, 5.0 * CELL_H);
        assert_eq!(placement.geometry.width, 2.0);
        assert_eq!(placement.geometry.height, 2.0);
        assert_eq!(placement.geometry.source_rect, [0.0, 0.0, 1.0, 1.0]);

        let (width, height, stamp) = state
            .image_info(ImageProtocol::Inline, placement.image_id)
            .expect("stored image");
        assert_eq!((width, height), (2, 2));
        assert_eq!(stamp, placement.image_stamp);
        let mut buf = [0u8; 16];
        assert_eq!(
            state.image_rgba(ImageProtocol::Inline, placement.image_id, &mut buf),
            16
        );
        assert_eq!(buf, [0xFF, 0x00, 0x00, 0xFF].repeat(4).as_slice());
        let mut small = [0u8; 4];
        assert_eq!(
            state.image_rgba(ImageProtocol::Inline, placement.image_id, &mut small),
            0
        );
        // Kitty ids are a different namespace.
        assert_eq!(
            state.image_info(ImageProtocol::Kitty, placement.image_id),
            None
        );
    }

    // An iTerm2 inline image (OSC 1337 File=) takes the same path; its
    // PNG is decoded to RGBA for the host.
    #[test]
    fn iterm2_image_reaches_render_state() {
        let (surface, mut state) = image_surface();
        // 2x2 RGBA PNG: red, green / blue, white.
        surface.inject_output(
            b"\x1b]1337;File=inline=1;size=75:iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEklEQVR42mP4z8DwHwyBNBgAAEnICfcD2WTxAAAAAElFTkSuQmCC\x07",
        );
        state.update();

        assert_eq!(state.image_count(), 1);
        let placement = state
            .image_placement(0, CELL_W, CELL_H)
            .expect("placement in view");
        assert_eq!(placement.protocol, ImageProtocol::Inline);
        assert_eq!((placement.geometry.x, placement.geometry.y), (0.0, 0.0));
        let (width, height, _) = state
            .image_info(ImageProtocol::Inline, placement.image_id)
            .expect("stored image");
        assert_eq!((width, height), (2, 2));
        let mut buf = [0u8; 16];
        assert_eq!(
            state.image_rgba(ImageProtocol::Inline, placement.image_id, &mut buf),
            16
        );
        assert_eq!(
            buf,
            [
                0xFF, 0x00, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF, //
                0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            ]
        );
    }

    // Sixel and iTerm2 images sit on the grid plane, under a kitty
    // image at the same z; the list is in draw order and the kitty
    // accessors index the kitty subset.
    #[test]
    fn image_list_orders_inline_under_kitty() {
        let (surface, mut state) = image_surface();
        surface.inject_output(b"\x1b[6;5H");
        surface.inject_output(SIXEL_2X2_RED);
        surface.inject_output(
            b"\x1b[2;2H\x1b_Gf=32,s=2,v=2,i=7,a=T,z=-1;/wAA/wD/AP8AAP///////w==\x1b\\",
        );
        state.update();

        assert_eq!(state.image_count(), 2);
        let first = state.image_placement(0, CELL_W, CELL_H).unwrap();
        let second = state.image_placement(1, CELL_W, CELL_H).unwrap();
        assert_eq!(first.protocol, ImageProtocol::Inline);
        assert_eq!(second.protocol, ImageProtocol::Kitty);
        assert_eq!(second.image_id, 7);
        assert_eq!((second.layer, second.z_index), (ImageLayer::BelowText, -1));
        assert_eq!(second.geometry.x, 1.0 * CELL_W);

        assert_eq!(state.kitty_count(), 1);
        let (image_id, _, geometry) = state.kitty_geometry(0, CELL_W, CELL_H).unwrap();
        assert_eq!(image_id, 7);
        assert_eq!(geometry, second.geometry);
        let (_, _, stamp) = state.image_info(ImageProtocol::Kitty, 7).unwrap();
        assert_eq!(stamp, second.image_stamp);
        let mut buf = [0u8; 16];
        assert_eq!(state.image_rgba(ImageProtocol::Kitty, 7, &mut buf), 16);
    }

    // Pixels stay for as long as a placement shows the image and leave
    // with it; quiet frames keep the placements generation, a change
    // bumps it.
    #[test]
    fn inline_images_leave_with_their_placements() {
        let (surface, mut state) = image_surface();
        surface.inject_output(SIXEL_2X2_RED);
        state.update();
        let placement = state.image_placement(0, CELL_W, CELL_H).unwrap();
        let generation = state.image_placements_generation();

        // Output away from the image changes nothing about it.
        surface.inject_output(b"\x1b[10;1Hquiet frame");
        state.update();
        assert_eq!(state.image_placements_generation(), generation);
        assert!(state
            .image_info(ImageProtocol::Inline, placement.image_id)
            .is_some());

        // A full reset drops every placement, and the store follows.
        surface.inject_output(b"\x1bc");
        state.update();
        assert_eq!(state.image_count(), 0);
        assert_ne!(state.image_placements_generation(), generation);
        assert_eq!(
            state.image_info(ImageProtocol::Inline, placement.image_id),
            None
        );
        let mut buf = [0u8; 16];
        assert_eq!(
            state.image_rgba(ImageProtocol::Inline, placement.image_id, &mut buf),
            0
        );
    }

    // The pixels belong to the surface: a render state made after the
    // image arrived, or a second one, reads them too, and a surface no
    // one renders still drops them with their placements.
    #[test]
    fn inline_pixels_are_shared_by_render_states() {
        let (surface, mut first) = image_surface();
        surface.inject_output(SIXEL_2X2_RED);
        assert_eq!(surface.inline_images().lock().len(), 1);

        let mut second = RenderState::new(&surface);
        first.update();
        second.update();
        let placement = first.image_placement(0, CELL_W, CELL_H).unwrap();
        assert_ne!(placement.image_stamp, 0);
        // Stamps count from the surface's epoch, so a texture cache
        // shared by the two states sees one stamp per image.
        for state in [&first, &second] {
            let (width, height, stamp) = state
                .image_info(ImageProtocol::Inline, placement.image_id)
                .expect("pixels shared");
            assert_eq!((width, height), (2, 2));
            assert_eq!(stamp, placement.image_stamp);
        }

        drop(first);
        drop(second);
        surface.inject_output(b"\x1bc");
        assert!(surface.inline_images().lock().is_empty());
    }

    // The alternate screen has placements of its own: a program running
    // there hides the main screen's image without dropping its pixels,
    // and leaving brings the image back.
    #[test]
    fn inline_placement_waits_out_the_alternate_screen() {
        let (surface, mut state) = image_surface();
        surface.inject_output(SIXEL_2X2_RED);
        state.update();
        let placement = state.image_placement(0, CELL_W, CELL_H).unwrap();
        let generation = state.image_placements_generation();

        surface.inject_output(b"\x1b[?1049h");
        state.update();
        assert_eq!(state.image_count(), 0);
        assert_ne!(state.image_placements_generation(), generation);
        assert!(state
            .image_info(ImageProtocol::Inline, placement.image_id)
            .is_some());

        surface.inject_output(b"\x1b[?1049l");
        state.update();
        assert_eq!(state.image_count(), 1);
        assert_eq!(state.image_placement(0, CELL_W, CELL_H).unwrap(), placement);
    }

    // A placement scrolled into history stays in the list but does not
    // resolve; scrolling the view back brings it in and moves the
    // generation, since the host's draw list has to move too.
    #[test]
    fn inline_placement_follows_scrollback() {
        let (surface, mut state) = image_surface();
        surface.inject_output(SIXEL_2X2_RED);
        state.update();
        let generation = state.image_placements_generation();

        surface.inject_output(&b"\n".repeat(30));
        state.update();
        assert_eq!(state.image_count(), 1);
        assert!(state.image_placement(0, CELL_W, CELL_H).is_none());
        assert_ne!(state.image_placements_generation(), generation);

        // Output that scrolls the screen while the image is out of view
        // moves nothing the host can see.
        let generation = state.image_placements_generation();
        surface.inject_output(&b"\n".repeat(5));
        state.update();
        assert_eq!(state.image_placements_generation(), generation);

        surface.scroll(35);
        state.update();
        let placement = state
            .image_placement(0, CELL_W, CELL_H)
            .expect("back in view");
        assert_eq!(placement.geometry.y, 0.0);
    }

    /// Registers `cp=100000` as the smallest `glyf`, one contour with one
    /// point, at `upm`.
    #[cfg(feature = "graphics")]
    fn register_glyf(surface: &Surface, upm: u16) {
        surface.inject_output(
            format!("\x1b_25a1;r;cp=100000;reply=0;upm={upm};AAEAAAAAAAAAAAAAAAABAAAAAA==\x1b\\")
                .as_bytes(),
        );
    }

    // A delegate that draws no glyphs leaves Glyph Protocol off: nothing
    // a program registers is kept or bound to its text.
    #[cfg(feature = "graphics")]
    #[test]
    fn glyph_protocol_needs_the_delegate_to_opt_in() {
        let surface = quiet_surface(80, 24);
        let mut state = RenderState::new(&surface);
        register_glyf(&surface, 1000);
        surface.inject_output("\u{100000}".as_bytes());
        state.update();
        let cell = *state.square(0, 0).unwrap();
        assert_eq!(state.glyph_version(&cell), None);
        assert!(surface.glyph(0).is_none());
        assert!(!surface.glyph_query_reply(false));
    }

    #[cfg(feature = "graphics")]
    struct GlyphHost(Mutex<Vec<u32>>);

    #[cfg(feature = "graphics")]
    impl SurfaceDelegate for GlyphHost {
        fn wakeup(&self, _surface: SurfaceId) {}
        fn glyph_formats(&self) -> &'static [GlyphFormat] {
            &[GlyphFormat::Glyf]
        }
        fn glyph_query(&self, _surface: SurfaceId, cp: u32) {
            self.0.lock().unwrap().push(cp);
        }
    }

    // Each cell draws the registration it was printed with; text printed
    // before a registration draws as text.
    #[cfg(feature = "graphics")]
    #[test]
    fn glyph_protocol_cells_keep_the_registration_they_were_printed_with() {
        let engine = Engine::new(Arc::new(GlyphHost(Mutex::new(Vec::new()))));
        let surface = engine
            .create_surface(&SurfaceDesc {
                shell: Some("/bin/sh".into()),
                args: vec!["-c".into(), "sleep 300".into()],
                ..SurfaceDesc::default()
            })
            .expect("spawn sleeper");
        let mut state = RenderState::new(&surface);
        surface.inject_output("\u{100000}".as_bytes());
        register_glyf(&surface, 1000);
        surface.inject_output("\u{100000}".as_bytes());
        register_glyf(&surface, 2048);
        surface.inject_output("\u{100000}".as_bytes());
        state.update();
        let version = |col| {
            let cell = *state.square(0, col).unwrap();
            state.glyph_version(&cell)
        };
        assert_eq!(version(0), None);
        let (first, second) = (version(1).unwrap(), version(2).unwrap());
        assert_ne!(first, second);
        assert_eq!(surface.glyph(first).unwrap().upm, 1000);
        assert_eq!(surface.glyph(second).unwrap().upm, 2048);
    }

    // The host draws `glyf` alone, so that is all programs are offered,
    // and its answer to a query reaches the program ahead of the replies
    // made after the query.
    #[cfg(feature = "graphics")]
    #[test]
    fn glyph_protocol_query_answers_keep_request_order() {
        let delegate = Arc::new(GlyphHost(Mutex::new(Vec::new())));
        let engine = Engine::new(delegate.clone());
        // The child shows the replies it is sent.
        let surface = engine
            .create_surface(&SurfaceDesc {
                shell: Some("/bin/sh".into()),
                args: vec!["-c".into(), "stty raw -echo; printf READY; cat -v".into()],
                ..SurfaceDesc::default()
            })
            .expect("spawn cat");
        let shown = |text: &str| {
            let deadline = Instant::now() + Duration::from_secs(8);
            while !surface.dump().contains(text) {
                assert!(
                    Instant::now() < deadline,
                    "no {text:?} in {:?}",
                    surface.dump()
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        };
        shown("READY");

        register_glyf(&surface, 1000);
        surface.inject_output(b"\x1b_25a1;q;cp=100000\x1b\\\x1b_25a1;s\x1b\\");
        assert_eq!(*delegate.0.lock().unwrap(), [0x100000]);
        // The `s` reply waits behind the unanswered query.
        {
            let held = surface.replies.held.lock().unwrap();
            assert_eq!(held.len(), 2);
            assert!(matches!(held[0], Held::Query(0x100000, true)));
            assert!(matches!(held[1], Held::Reply(_)));
        }
        assert!(surface.glyph_query_reply(false));
        assert!(!surface.glyph_query_reply(false), "one answer per query");
        shown("q;cp=100000;status=glossary^[\\^[_25a1;s;fmt=glyf^[\\");
    }

    // A query answers for the registry as it was when the query arrived:
    // a registration the program sends right after it, before the host
    // answers, doesn't show up in the answer.
    #[cfg(feature = "graphics")]
    #[test]
    fn glyph_protocol_query_answers_for_the_moment_it_was_asked() {
        let engine = Engine::new(Arc::new(GlyphHost(Mutex::new(Vec::new()))));
        let surface = engine
            .create_surface(&SurfaceDesc {
                shell: Some("/bin/sh".into()),
                args: vec!["-c".into(), "sleep 300".into()],
                ..SurfaceDesc::default()
            })
            .expect("spawn sleeper");
        surface.inject_output(b"\x1b_25a1;q;cp=100000\x1b\\");
        register_glyf(&surface, 1000);
        surface.inject_output(b"\x1b_25a1;q;cp=100000\x1b\\");
        let held = surface.replies.held.lock().unwrap();
        assert!(matches!(held[0], Held::Query(0x100000, false)));
        assert!(matches!(held[1], Held::Query(0x100000, true)));
    }

    // A flood of queries, with replies behind them, that the host
    // hasn't answered stops growing the queue: the oldest are answered
    // `unknown`, in order, and the host drains the rest in one pass.
    #[cfg(feature = "graphics")]
    #[test]
    fn glyph_protocol_query_flood_is_bounded() {
        let engine = Engine::new(Arc::new(GlyphHost(Mutex::new(Vec::new()))));
        let surface = engine
            .create_surface(&SurfaceDesc {
                shell: Some("/bin/sh".into()),
                args: vec!["-c".into(), "sleep 300".into()],
                ..SurfaceDesc::default()
            })
            .expect("spawn sleeper");
        // Queries with replies behind them (DA1), well past the bound.
        let flood = b"\x1b_25a1;q;cp=e0a0\x1b\\\x1b[c".repeat(4 * MAX_HELD);
        surface.inject_output(&flood);
        {
            let held = surface.replies.held.lock().unwrap();
            assert!(held.len() <= MAX_HELD, "{} held", held.len());
            assert!(matches!(held.front(), Some(Held::Query(0xe0a0, false))));
        }
        let mut answered = 0;
        while surface.glyph_query_pending() == Some(0xe0a0) {
            assert!(surface.glyph_query_reply(true));
            answered += 1;
        }
        assert!(answered > 0 && answered <= MAX_HELD / 2 + 1);
        assert!(surface.replies.held.lock().unwrap().is_empty());
    }

    // A host whose font lookup is cheap answers inside the callback.
    #[cfg(feature = "graphics")]
    #[test]
    fn glyph_protocol_query_can_be_answered_from_the_callback() {
        struct EagerHost(std::sync::OnceLock<std::sync::Weak<Surface>>);
        impl SurfaceDelegate for EagerHost {
            fn wakeup(&self, _surface: SurfaceId) {}
            fn glyph_formats(&self) -> &'static [GlyphFormat] {
                &[GlyphFormat::Vg]
            }
            fn glyph_query(&self, _surface: SurfaceId, _cp: u32) {
                let surface = self.0.get().and_then(std::sync::Weak::upgrade);
                assert!(surface.expect("surface").glyph_query_reply(true));
            }
        }
        let delegate = Arc::new(EagerHost(std::sync::OnceLock::new()));
        let engine = Engine::new(delegate.clone());
        let surface = Arc::new(
            engine
                .create_surface(&SurfaceDesc {
                    shell: Some("/bin/sh".into()),
                    args: vec![
                        "-c".into(),
                        "stty raw -echo; printf READY; cat -v".into(),
                    ],
                    ..SurfaceDesc::default()
                })
                .expect("spawn cat"),
        );
        let _ = delegate.0.set(Arc::downgrade(&surface));
        let shown = |text: &str| {
            let deadline = Instant::now() + Duration::from_secs(8);
            while !surface.dump().contains(text) {
                assert!(
                    Instant::now() < deadline,
                    "no {text:?} in {:?}",
                    surface.dump()
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        };
        shown("READY");
        surface.inject_output(b"\x1b_25a1;q;cp=e0a0\x1b\\\x1b_25a1;s\x1b\\");
        shown("q;cp=e0a0;status=system^[\\^[_25a1;s;fmt=vg^[\\");
    }

    /// A pty pair: the main (master) and sub (slave) ends.
    fn openpty() -> (std::os::fd::OwnedFd, std::fs::File) {
        use std::os::fd::FromRawFd;
        let (mut main, mut sub) = (0, 0);
        let opened = unsafe {
            libc::openpty(
                &mut main,
                &mut sub,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert_eq!(opened, 0);
        unsafe {
            (
                std::os::fd::OwnedFd::from_raw_fd(main),
                std::fs::File::from_raw_fd(sub),
            )
        }
    }

    struct ClosingDelegate(std::sync::atomic::AtomicBool);

    impl SurfaceDelegate for ClosingDelegate {
        fn wakeup(&self, _surface: SurfaceId) {}
        fn close_surface(&self, _surface: SurfaceId) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    // A pty master the host holds drives a surface like a spawned shell;
    // the test writes from the other side, and closing that side ends
    // the surface.
    #[test]
    fn adopts_an_existing_pty() {
        use std::io::Write;
        let delegate =
            Arc::new(ClosingDelegate(std::sync::atomic::AtomicBool::new(false)));
        let engine = Engine::new(delegate.clone());
        let (main, mut sub) = openpty();
        let surface = engine
            .adopt_surface(&SurfaceDesc::default(), main)
            .expect("adopt the pty");
        assert_eq!(surface.child_pid(), 0);
        let mut state = RenderState::new(&surface);

        sub.write_all(b"from the other side").unwrap();
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            state.update();
            if (0..state.lines())
                .any(|i| state.text_row(i).contains("from the other side"))
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the output never reached the grid"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        drop(sub);
        let deadline = Instant::now() + Duration::from_secs(4);
        while !delegate.0.load(Ordering::SeqCst) {
            assert!(Instant::now() < deadline, "the surface did not close");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn adopting_refuses_what_is_not_a_pty_master() {
        let engine = Engine::new(Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        }));
        let (read_end, _write_end) = std::io::pipe().unwrap();
        assert!(engine
            .adopt_surface(&SurfaceDesc::default(), read_end.into())
            .is_err());
    }
}
