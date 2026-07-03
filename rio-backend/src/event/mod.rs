pub mod sync;

use crate::ansi::graphics::UpdateQueues;
use crate::clipboard::ClipboardType;
use crate::config::colors::ColorRgb;
use crate::crosswords::grid::Scroll;
use crate::crosswords::pos::{Direction, Pos};
use crate::crosswords::search::{Match, RegexSearch};
use crate::error::RioError;
use rio_window::event::Event as RioWindowEvent;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::sync::Arc;
use teletypewriter::WinsizeBuilder;

use rio_window::event_loop::EventLoopProxy;

pub type WindowId = rio_window::window::WindowId;

#[derive(Debug, Clone)]
pub enum RioEventType {
    Rio(RioEvent),
    Frame,
    // Message(Message),
}

// The PTY-channel `Msg` protocol now lives in the `canario` engine crate's
// `pty` module (behind the `pty` feature). Re-export it so existing
// `crate::event::Msg` references — and the frontend's `rio_backend::event::Msg`
// imports — keep resolving unchanged. `WinsizeBuilder` itself stays in
// `teletypewriter`, which `canario`'s `pty` feature pulls in, so the variant
// payload is the same type across both crates.
pub use canario::pty::Msg;

#[derive(Debug, Eq, PartialEq)]
pub enum ClickState {
    None,
    Click,
    DoubleClick,
    TripleClick,
}

// The terminal damage hint — a coarse signal for the renderer's update path
// (skip vs incremental vs full rebuild) — now lives in the `canario` engine
// crate, which defines the identical `Noop`/`Full`/`Partial`/`CursorOnly`
// enum. Re-export it so existing `crate::event::TerminalDamage` references —
// and the frontend's `rio_backend::event::TerminalDamage` — keep resolving
// unchanged. The actual per-row decision still lives on the snapshot's
// `Row::dirty` (post-`snapshot_visible`); this enum just gates `update`:
// - `Noop` — no terminal-side change worth rendering for
// - `Full` — global state changed (resize, palette, mode flip), force a full
//   rebuild even if no individual row is dirty
// - `Partial` — at least one row's content changed; the snapshot's per-row
//   dirty bits identify which rows
// - `CursorOnly` — cursor moved/blinked, no cell content changed
pub use canario::TerminalDamage;

#[derive(Clone)]
pub enum RioEvent {
    PrepareRender(u64),
    PrepareRenderOnRoute(u64, usize),
    PrepareUpdateConfig,
    /// New terminal content available.
    Render,
    /// New terminal content available per route.
    RenderRoute(usize),
    /// Terminal content changed — lightweight notification (no damage payload).
    /// Damage stays in the terminal; renderer extracts it when it locks.
    TerminalDamaged(usize),
    /// Graphics update available from terminal.
    UpdateGraphics {
        route_id: usize,
        queues: UpdateQueues,
    },
    /// A pane's Glyph Protocol registry just became live (first
    /// `register` after session start, or first register following
    /// a clear-all). Frontend installs it into the font library so
    /// subsequent renders consult it. Fires at most once per
    /// (route_id × registry-arc) pair; the registry is Arc-shared,
    /// so further `register`/`clear` mutations made through the
    /// existing handle are visible without re-firing.
    GlyphProtocolInstalled {
        route_id: usize,
        registry: sugarloaf::font::glyph_registry::GlyphRegistry,
    },
    /// A `q` (query) request arrived from the PTY in `route_id`. The
    /// frontend computes the four-state status — System and/or
    /// Glossary coverage — by consulting both `FontLibrary` (system
    /// fonts) and the per-route glyph registry, then writes the
    /// formatted reply back to the same pane's PTY. Asynchronous
    /// because the dispatcher (in rio-backend) doesn't have access
    /// to the FontLibrary; the frontend does.
    GlyphProtocolQuery {
        route_id: usize,
        cp: u32,
    },
    Paste,
    Copy(String),
    UpdateFontSize(u8),
    Scroll(Scroll),
    ToggleFullScreen,
    ToggleAppearanceTheme,
    Minimize(bool),
    Hide,
    HideOtherApplications,
    UpdateConfig,
    CreateWindow,
    CloseWindow,
    CreateNativeTab(Option<String>),
    CreateConfigEditor,
    SelectNativeTabByIndex(usize),
    SelectNativeTabLast,
    SelectNativeTabNext,
    SelectNativeTabPrev,

    ReportToAssistant(RioError),

    /// Grid has changed possibly requiring a mouse cursor shape change.
    MouseCursorDirty,

    /// Window title change.
    Title(String),

    /// Window title change.
    TitleWithSubtitle(String, String),

    /// Reset to the default window title.
    ResetTitle,

    /// Request to store a text string in the clipboard.
    ClipboardStore(ClipboardType, String),

    /// Request to write the contents of the clipboard to the PTY.
    ///
    /// `route_id` identifies the panel that emitted the request so
    /// the bytes land on the originating PTY rather than whichever
    /// panel happens to be focused. The attached function is a
    /// formatter which transforms the clipboard content into the
    /// expected escape-sequence form.
    ClipboardLoad(
        usize,
        ClipboardType,
        Arc<dyn Fn(&str) -> String + Sync + Send + 'static>,
    ),

    /// Request to write the RGB value of a color to the PTY.
    ///
    /// `route_id` identifies the panel that emitted the request so
    /// the reply lands on the originating PTY. The attached function
    /// is a formatter which transforms the RGB color into the
    /// expected escape-sequence form.
    ColorRequest(
        usize,
        usize,
        Arc<dyn Fn(ColorRgb) -> String + Sync + Send + 'static>,
    ),

    /// Write some text to the PTY identified by `route_id`. Routing
    /// by panel (rather than the focused context) is required so
    /// CSI / OSC reply bytes land on the shell that asked for them
    /// even if the user focuses a different split mid-flight.
    PtyWrite(usize, String),

    /// Request to write the text area size to the PTY of `route_id`.
    TextAreaSizeRequest(
        usize,
        Arc<dyn Fn(WinsizeBuilder) -> String + Sync + Send + 'static>,
    ),

    /// Cursor blinking state has changed.
    CursorBlinkingChange,

    CursorBlinkingChangeOnRoute(usize),

    /// Progress bar report from OSC 9;4 sequence
    ProgressReport(ProgressReport),

    /// Terminal bell ring.
    Bell,

    /// Desktop notification from OSC 9 or OSC 777.
    DesktopNotification {
        title: String,
        body: String,
    },

    /// Shutdown request.
    Exit,

    /// Quit request.
    Quit,

    /// Leave current terminal.
    CloseTerminal(usize),

    BlinkCursor(u64, usize),

    /// Selection scroll tick — auto-scroll while dragging outside viewport.
    SelectionScrollTick,

    /// Update window titles.
    UpdateTitles,

    /// Update terminal screen colors.
    ///
    /// The first usize is the route_id, the second is the color index to change.
    /// Color index: 0 for foreground, 1 for background, 2 for cursor color.
    ColorChange(usize, usize, Option<ColorRgb>),

    // No operation
    Noop,
}

impl Debug for RioEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RioEvent::ClipboardStore(ty, text) => {
                write!(f, "ClipboardStore({ty:?}, {text})")
            }
            RioEvent::ClipboardLoad(route_id, ty, _) => {
                write!(f, "ClipboardLoad(route={route_id}, {ty:?})")
            }
            RioEvent::TextAreaSizeRequest(route_id, _) => {
                write!(f, "TextAreaSizeRequest(route={route_id})")
            }
            RioEvent::ColorRequest(route_id, index, _) => {
                write!(f, "ColorRequest(route={route_id}, idx={index})")
            }
            RioEvent::PtyWrite(route_id, text) => {
                write!(f, "PtyWrite(route={route_id}, {text})")
            }
            RioEvent::Title(title) => write!(f, "Title({title})"),
            RioEvent::TitleWithSubtitle(title, subtitle) => {
                write!(f, "TitleWithSubtitle({title}, {subtitle})")
            }
            RioEvent::Minimize(cond) => write!(f, "Minimize({cond})"),
            RioEvent::Hide => write!(f, "Hide)"),
            RioEvent::HideOtherApplications => write!(f, "HideOtherApplications)"),
            RioEvent::CursorBlinkingChange => write!(f, "CursorBlinkingChange"),
            RioEvent::CursorBlinkingChangeOnRoute(route_id) => {
                write!(f, "CursorBlinkingChangeOnRoute {route_id}")
            }
            RioEvent::ProgressReport(report) => {
                write!(f, "ProgressReport({:?})", report)
            }
            RioEvent::MouseCursorDirty => write!(f, "MouseCursorDirty"),
            RioEvent::ResetTitle => write!(f, "ResetTitle"),
            RioEvent::PrepareUpdateConfig => write!(f, "PrepareUpdateConfig"),
            RioEvent::PrepareRender(millis) => write!(f, "PrepareRender({millis})"),
            RioEvent::PrepareRenderOnRoute(millis, route) => {
                write!(f, "PrepareRender({millis} on route {route})")
            }
            RioEvent::Render => write!(f, "Render"),
            RioEvent::RenderRoute(route) => write!(f, "Render route {route}"),
            RioEvent::TerminalDamaged(route_id) => {
                write!(f, "TerminalDamaged route {route_id}")
            }
            RioEvent::GlyphProtocolInstalled { route_id, .. } => {
                write!(f, "GlyphProtocolInstalled route {route_id}")
            }
            RioEvent::GlyphProtocolQuery { route_id, cp } => {
                write!(f, "GlyphProtocolQuery route {route_id} cp {cp:#x}")
            }
            RioEvent::Scroll(scroll) => write!(f, "Scroll {scroll:?}"),
            RioEvent::Bell => write!(f, "Bell"),
            RioEvent::DesktopNotification { title, body } => {
                write!(f, "DesktopNotification({title}, {body})")
            }
            RioEvent::Exit => write!(f, "Exit"),
            RioEvent::Quit => write!(f, "Quit"),
            RioEvent::CloseTerminal(route) => write!(f, "CloseTerminal {route}"),
            RioEvent::CreateWindow => write!(f, "CreateWindow"),
            RioEvent::CloseWindow => write!(f, "CloseWindow"),
            RioEvent::CreateNativeTab(_) => write!(f, "CreateNativeTab"),
            RioEvent::SelectNativeTabByIndex(tab_index) => {
                write!(f, "SelectNativeTabByIndex({tab_index})")
            }
            RioEvent::SelectNativeTabLast => write!(f, "SelectNativeTabLast"),
            RioEvent::SelectNativeTabNext => write!(f, "SelectNativeTabNext"),
            RioEvent::SelectNativeTabPrev => write!(f, "SelectNativeTabPrev"),
            RioEvent::CreateConfigEditor => write!(f, "CreateConfigEditor"),
            RioEvent::UpdateConfig => write!(f, "ReloadConfiguration"),
            RioEvent::ReportToAssistant(error_report) => {
                write!(f, "ReportToAssistant({})", error_report.report)
            }
            RioEvent::ToggleFullScreen => write!(f, "FullScreen"),
            RioEvent::ToggleAppearanceTheme => write!(f, "ToggleAppearanceTheme"),
            RioEvent::BlinkCursor(timeout, route_id) => {
                write!(f, "BlinkCursor {timeout} {route_id}")
            }
            RioEvent::SelectionScrollTick => write!(f, "SelectionScrollTick"),
            RioEvent::UpdateTitles => write!(f, "UpdateTitles"),
            RioEvent::Noop => write!(f, "Noop"),
            RioEvent::Copy(_) => write!(f, "Copy"),
            RioEvent::Paste => write!(f, "Paste"),
            RioEvent::UpdateFontSize(action) => write!(f, "UpdateFontSize({action:?})"),
            RioEvent::UpdateGraphics { route_id, .. } => {
                write!(f, "UpdateGraphics({route_id})")
            }
            RioEvent::ColorChange(route_id, color, rgb) => {
                write!(f, "ColorChange({route_id}, {color:?}, {rgb:?})")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct EventPayload {
    /// Event payload.
    pub payload: RioEventType,
    pub window_id: WindowId,
}

impl EventPayload {
    pub fn new(payload: RioEventType, window_id: WindowId) -> Self {
        Self { payload, window_id }
    }
}

impl From<EventPayload> for RioWindowEvent<EventPayload> {
    fn from(event: EventPayload) -> Self {
        RioWindowEvent::UserEvent(event)
    }
}

pub trait OnResize {
    fn on_resize(&mut self, window_size: WinsizeBuilder);
}

/// Event Loop for notifying the renderer about terminal events.
pub trait EventListener {
    fn event(&self) -> (Option<RioEvent>, bool);

    fn send_event(&self, _event: RioEvent, _id: WindowId) {}

    fn send_event_with_high_priority(&self, _event: RioEvent, _id: WindowId) {}

    fn send_redraw(&self, _id: WindowId) {}

    fn send_global_event(&self, _event: RioEvent) {}
}

/// Convenience bound for Rio's frontend: a type that is both the legacy
/// `EventListener` (for non-engine UI events) and the decoupled
/// [`canario::host::TerminalHost`] (for the terminal engine), reporting
/// against the `(WindowId, route_id)` [`HostId`]. `EventProxy` satisfies it;
/// the frontend's generic terminal code bounds on this single trait so it
/// can both drive `Crosswords<T>` and emit window/UI events.
pub trait HostEventListener:
    EventListener + canario::host::TerminalHost<WindowId = HostId>
{
}

impl<T> HostEventListener for T where
    T: EventListener + canario::host::TerminalHost<WindowId = HostId>
{
}

#[derive(Clone)]
pub struct VoidListener;

impl From<RioEvent> for RioEventType {
    fn from(rio_event: RioEvent) -> Self {
        Self::Rio(rio_event)
    }
}

impl EventListener for VoidListener {
    fn event(&self) -> (std::option::Option<RioEvent>, bool) {
        (None, false)
    }
}

/// `VoidListener` doubles as a no-op [`canario::host::TerminalHost`] so the
/// engine's headless tests can construct `Crosswords<VoidListener>` with the
/// raw `WindowId`. (Rioterm's UI tests, which build `Context`/`ContextManager`
/// and therefore need the [`HostEventListener`] `(window, route)` identity,
/// use their own `HostId`-typed void host.) Every host method is the
/// defaulted no-op.
impl canario::host::TerminalHost for VoidListener {
    type WindowId = WindowId;
}

/// A no-op listener+host reporting against [`HostId`] — the frontend
/// counterpart to [`VoidListener`]. Rio's `Context`/`ContextManager` are
/// generic over [`HostEventListener`] (which fixes `WindowId = HostId`), so
/// their tests can't use `VoidListener` (whose `WindowId` is the raw
/// `WindowId`). `VoidHost` fills that gap: every method is a no-op.
#[derive(Default, Clone)]
pub struct VoidHost;

impl EventListener for VoidHost {
    fn event(&self) -> (std::option::Option<RioEvent>, bool) {
        (None, false)
    }
}

impl canario::host::TerminalHost for VoidHost {
    type WindowId = HostId;
}

#[derive(Debug, Clone)]
pub struct EventProxy {
    proxy: EventLoopProxy<EventPayload>,
    /// Per-route Glyph Protocol glossaries. The engine no longer owns the
    /// `sugarloaf` font types, so the host (this proxy) owns the registry the
    /// renderer reads back. Lazily populated per route the first time a
    /// program in that route registers a glyph, mirroring the engine's old
    /// lazy-init exactly; on first init a `GlyphProtocolInstalled` event
    /// hands the Arc-shared registry to the frontend's font library.
    glyph_registries: Arc<
        parking_lot::Mutex<
            std::collections::HashMap<
                usize,
                sugarloaf::font::glyph_registry::GlyphRegistry,
            >,
        >,
    >,
}

impl EventProxy {
    pub fn new(proxy: EventLoopProxy<EventPayload>) -> Self {
        Self {
            proxy,
            glyph_registries: Arc::new(parking_lot::Mutex::new(
                std::collections::HashMap::new(),
            )),
        }
    }

    pub fn send_event(&self, event: RioEventType, id: WindowId) {
        let _ = self.proxy.send_event(EventPayload::new(event, id));
    }
}

impl EventListener for EventProxy {
    fn event(&self) -> (std::option::Option<RioEvent>, bool) {
        (None, false)
    }

    fn send_event(&self, event: RioEvent, id: WindowId) {
        let _ = self.proxy.send_event(EventPayload::new(event.into(), id));
    }
}

/// The engine reports against an opaque `(window, route)` identity. The
/// frontend's `RioEvent` flow routes replies by `route_id` and side effects
/// by `WindowId`, so the proxy carries both inside `TerminalHost::WindowId`.
pub type HostId = (WindowId, usize);

/// `EventProxy` is the host that bridges the decoupled `canario` engine back
/// onto Rio's existing `RioEvent` flow. Each host call is the inverse of the
/// taxonomy applied inside the engine: side effects become `Alert`-equivalent
/// `RioEvent`s, replies re-emit the same reply-closure events, and graphics /
/// glyph operations route to the same handlers as before — so behaviour is
/// identical to the pre-severance `EventListener` path.
impl canario::host::TerminalHost for EventProxy {
    type WindowId = HostId;

    fn write_pty(&mut self, id: Self::WindowId, bytes: &[u8]) {
        let (window_id, route_id) = id;
        let text = String::from_utf8_lossy(bytes).into_owned();
        EventListener::send_event(self, RioEvent::PtyWrite(route_id, text), window_id);
    }

    fn alert(&mut self, id: Self::WindowId, alert: canario::host::Alert) {
        use canario::host::Alert;
        let (window_id, route_id) = id;
        let event = match alert {
            Alert::Bell => RioEvent::Bell,
            Alert::Damaged => RioEvent::TerminalDamaged(route_id),
            Alert::MouseCursorDirty => RioEvent::MouseCursorDirty,
            Alert::CursorBlinkingChanged => RioEvent::CursorBlinkingChange,
            Alert::PaletteChanged { index, color } => {
                RioEvent::ColorChange(route_id, index, color)
            }
            Alert::Progress(report) => RioEvent::ProgressReport(report),
            Alert::ChildExited => RioEvent::CloseTerminal(route_id),
            Alert::Notification { title, body } => {
                RioEvent::DesktopNotification { title, body }
            }
            Alert::ClipboardStore { kind, data } => {
                RioEvent::ClipboardStore(kind.into(), data)
            }
            Alert::ClipboardLoad { .. } => {
                // OSC 52 loads always arrive through `clipboard_load_request`
                // (they carry the reply formatter); a bare `ClipboardLoad`
                // alert has no formatter and is never emitted by the engine.
                return;
            }
        };
        EventListener::send_event(self, event, window_id);
    }

    fn color_request(
        &mut self,
        id: Self::WindowId,
        index: usize,
        format: Arc<dyn Fn(ColorRgb) -> String + Send + Sync + 'static>,
    ) {
        let (window_id, route_id) = id;
        EventListener::send_event(
            self,
            RioEvent::ColorRequest(route_id, index, format),
            window_id,
        );
    }

    fn text_area_size_request(
        &mut self,
        id: Self::WindowId,
        format: Arc<
            dyn Fn(canario::host::WindowSize) -> String + Send + Sync + 'static,
        >,
    ) {
        let (window_id, route_id) = id;
        // The engine formats from a `canario::host::WindowSize`; the existing
        // event carries a `WinsizeBuilder`. Adapt the builder into the engine
        // shape so the formatter is byte-identical.
        let adapter: Arc<dyn Fn(WinsizeBuilder) -> String + Send + Sync + 'static> =
            Arc::new(move |ws: WinsizeBuilder| {
                format(canario::host::WindowSize {
                    columns: ws.cols,
                    lines: ws.rows,
                    width_px: ws.width,
                    height_px: ws.height,
                })
            });
        EventListener::send_event(
            self,
            RioEvent::TextAreaSizeRequest(route_id, adapter),
            window_id,
        );
    }

    fn clipboard_load_request(
        &mut self,
        id: Self::WindowId,
        kind: canario::host::ClipboardKind,
        format: Arc<dyn Fn(&str) -> String + Send + Sync + 'static>,
    ) {
        let (window_id, route_id) = id;
        EventListener::send_event(
            self,
            RioEvent::ClipboardLoad(route_id, kind.into(), format),
            window_id,
        );
    }

    fn update_graphics(
        &mut self,
        id: Self::WindowId,
        queues: crate::ansi::graphics::UpdateQueues,
    ) {
        let (window_id, route_id) = id;
        EventListener::send_event(
            self,
            RioEvent::UpdateGraphics { route_id, queues },
            window_id,
        );
    }

    fn glyph_register(
        &mut self,
        id: Self::WindowId,
        cp: u32,
        payload: crate::ansi::glyph_protocol::GlyphPayload,
    ) -> Result<(), crate::ansi::glyph_protocol::RegisterError> {
        use crate::ansi::glyph_protocol::{GlyphPayload, RegisterError};
        use sugarloaf::font::glyf_decode;
        use sugarloaf::font::glyph_registry::{RegisterRejection, StoredPayload};

        let (window_id, route_id) = id;

        // Translate a glyf_decode error into the protocol's `reason=` codes.
        fn translate(err: glyf_decode::DecodeError) -> RegisterError {
            match err {
                glyf_decode::DecodeError::Composite => {
                    RegisterError::CompositeUnsupported
                }
                glyf_decode::DecodeError::Hinted => RegisterError::HintingUnsupported,
                glyf_decode::DecodeError::Malformed => RegisterError::MalformedPayload,
            }
        }

        // Validate the monochrome `glyf` payload at register time; COLR
        // containers are validated render-time only (see the protocol notes).
        let (stored, upm) = match payload {
            GlyphPayload::Glyf { glyf, upm } => {
                glyf_decode::decode(&glyf).map_err(translate)?;
                (StoredPayload::Glyf { glyf }, upm)
            }
            GlyphPayload::ColrV0 { container, upm } => (
                StoredPayload::ColrV0 {
                    glyphs: container.glyphs,
                    colr: container.colr,
                    cpal: container.cpal,
                },
                upm,
            ),
            GlyphPayload::ColrV1 { container, upm } => (
                StoredPayload::ColrV1 {
                    glyphs: container.glyphs,
                    colr: container.colr,
                    cpal: container.cpal,
                },
                upm,
            ),
        };

        // Lazily allocate the per-route registry; emit GlyphProtocolInstalled
        // exactly once per route so the frontend wires it into the font
        // library a single time per session.
        let mut registries = self.glyph_registries.lock();
        let was_uninitialised = !registries.contains_key(&route_id);
        let registry = registries.entry(route_id).or_default();

        let result = match registry.register(cp, stored, upm) {
            Ok(_evicted) => Ok(()),
            Err(RegisterRejection::OutOfNamespace) => {
                Err(RegisterError::OutOfNamespace)
            }
        };

        if was_uninitialised && result.is_ok() {
            let registry = registry.clone();
            drop(registries);
            EventListener::send_event(
                self,
                RioEvent::GlyphProtocolInstalled { route_id, registry },
                window_id,
            );
        }

        result
    }

    fn glyph_clear(&mut self, id: Self::WindowId, cp: Option<u32>) {
        let (_window_id, route_id) = id;
        let registries = self.glyph_registries.lock();
        let Some(registry) = registries.get(&route_id) else {
            return;
        };
        match cp {
            None => registry.clear_all(),
            Some(cp) => registry.clear_one(cp),
        }
    }

    fn glyph_query(&mut self, id: Self::WindowId, cp: u32) {
        let (window_id, route_id) = id;
        EventListener::send_event(
            self,
            RioEvent::GlyphProtocolQuery { route_id, cp },
            window_id,
        );
    }
}

/// Regex search state.
pub struct SearchState {
    /// Search direction.
    pub direction: Direction,

    /// Current position in the search history.
    pub history_index: Option<usize>,

    /// Change in display offset since the beginning of the search.
    pub display_offset_delta: i32,

    /// Search origin in viewport coordinates relative to original display offset.
    pub origin: Pos,

    /// Focused match during active search.
    pub focused_match: Option<Match>,

    /// Search regex and history.
    ///
    /// During an active search, the first element is the user's current input.
    ///
    /// While going through history, the [`SearchState::history_index`] will point to the element
    /// in history which is currently being previewed.
    pub history: VecDeque<String>,

    /// Compiled search automatons.
    pub dfas: Option<RegexSearch>,
}

impl SearchState {
    /// Search regex text if a search is active.
    pub fn regex(&self) -> Option<&String> {
        self.history_index.and_then(|index| self.history.get(index))
    }

    /// Direction of the search from the search origin.
    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// Focused match during vi-less search.
    pub fn focused_match(&self) -> Option<&Match> {
        self.focused_match.as_ref()
    }

    /// Clear the focused match.
    pub fn clear_focused_match(&mut self) {
        self.focused_match = None;
    }

    /// Active search dfas.
    pub fn dfas_mut(&mut self) -> Option<&mut RegexSearch> {
        self.dfas.as_mut()
    }

    /// Active search dfas.
    pub fn dfas(&self) -> Option<&RegexSearch> {
        self.dfas.as_ref()
    }

    /// Search regex text if a search is active.
    pub fn regex_mut(&mut self) -> Option<&mut String> {
        self.history_index
            .and_then(move |index| self.history.get_mut(index))
    }
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            direction: Direction::Right,
            display_offset_delta: Default::default(),
            focused_match: Default::default(),
            history_index: Default::default(),
            history: Default::default(),
            origin: Default::default(),
            dfas: Default::default(),
        }
    }
}

// Progress reporting types (OSC 9;4) now live in the `canario` engine crate.
// Re-export them so existing `crate::event::{ProgressReport, ProgressState}`
// references — and the frontend's `rio_backend::event::ProgressReport`
// imports — keep resolving unchanged.
pub use canario::host::{ProgressReport, ProgressState};
