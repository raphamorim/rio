//! Host integration — **the decoupling boundary**.
//!
//! Everything the engine needs from its embedder is expressed through these
//! traits, so the engine names no `sugarloaf`, `rio_window`, `config`, or
//! `copypasta` type. This is the contract that replaces Rio's fat
//! `RioEvent` + `EventListener` plumbing (DESIGN §4, §5 Severances 4 & 5).
//!
//! These traits compile today; the engine that *calls* them is lifted from
//! `rio-backend` over Phases 2–5.

use crate::ansi::glyph_protocol::{GlyphPayload, RegisterError};
use crate::ansi::graphics::UpdateQueues;
use rio_core::{ColorRgb, GraphicData};
use std::sync::Arc;

/// Everything the engine needs from its embedder. Methods are defaulted
/// no-op so a minimal/headless host implements nothing.
///
/// Modeled on wezterm's small post-construction trait set, but folded into
/// one object so `Terminal` is not generic-viral the way alacritty's
/// `EventListener` is.
pub trait TerminalHost {
    /// Opaque routing/window identity — canario never names
    /// `rio_window::WindowId`. (Severance 5.)
    type WindowId: Copy + Eq + std::hash::Hash + Send + 'static;

    /// Engine → program reply bytes (DA/DSR, color/size replies, bracketed
    /// paste, mouse/key reports). The host writes these to the PTY; the engine
    /// formats the bytes itself and owns the *write side* conceptually but
    /// performs no I/O. Replaces the per-event `RioEvent::PtyWrite`.
    fn write_pty(&mut self, _id: Self::WindowId, _bytes: &[u8]) {}

    /// Out-of-band, fire-and-forget side effects, all through one rich enum.
    fn alert(&mut self, _id: Self::WindowId, _alert: Alert) {}

    /// Synchronous query: the text-area geometry, for XTWINOPS replies. The
    /// engine formats the CSI reply from this and `write_pty`s it — replacing
    /// the old `RioEvent::TextAreaSizeRequest(Arc<dyn Fn>)` reply-closure.
    fn text_area_size(&self, _id: Self::WindowId) -> WindowSize {
        WindowSize::default()
    }

    /// Synchronous query: a resolved palette color, for OSC 4/10/11 *queries*.
    /// The engine formats the reply itself. (This is for one-off queries; the
    /// per-cell palette is snapshotted into the engine — never read here.)
    fn color(&self, _id: Self::WindowId, _index: usize) -> Option<ColorRgb> {
        None
    }

    /// A decoded graphic ready for the host to upload to a texture. The large
    /// pixel blob never enters the grid (the cell holds only a `GraphicId`).
    fn insert_graphic(&mut self, _id: Self::WindowId, _graphic: GraphicData) {}

    /// Glyph-protocol outline decode is injected — the engine never owns the
    /// `sugarloaf` font type (Severance 4). Returns an opaque handle the engine
    /// stores by registered codepoint.
    fn decode_glyph(&mut self, _payload: &[u8]) -> Result<GlyphHandle, GlyphReject> {
        Err(GlyphReject::Unsupported)
    }

    /// Synchronous query: whether the host can render a codepoint, for the
    /// glyph-protocol support query. The engine formats the reply itself.
    fn glyph_support(&self, _cp: u32) -> GlyphSupport {
        GlyphSupport::None
    }

    // ---------------------------------------------------------------------
    // Asynchronous reply requests.
    //
    // A subset of embedders (notably Rio's multi-pane frontend) can only
    // answer palette / geometry / clipboard queries from state that lives
    // behind an event loop, off the terminal thread — so they cannot be
    // serviced by the synchronous `color`/`text_area_size` accessors above.
    // For those embedders the engine hands the host a formatting closure;
    // the host resolves the value asynchronously, formats the reply with the
    // closure, and writes the bytes back to the originating PTY itself. The
    // closure carries the exact wire formatting the engine would otherwise
    // apply, so the reply is byte-identical regardless of which path runs.
    // Headless/synchronous hosts leave these as no-ops and rely on the
    // accessors instead.
    // ---------------------------------------------------------------------

    /// OSC 4/10/11 colour *query* reply, resolved asynchronously. `format`
    /// turns the resolved [`ColorRgb`] into the OSC reply bytes.
    fn color_request(
        &mut self,
        _id: Self::WindowId,
        _index: usize,
        _format: Arc<dyn Fn(ColorRgb) -> String + Send + Sync + 'static>,
    ) {
    }

    /// XTWINOPS text-area-size reply, resolved asynchronously. `format`
    /// turns the resolved [`WindowSize`] into the CSI reply bytes.
    fn text_area_size_request(
        &mut self,
        _id: Self::WindowId,
        _format: Arc<dyn Fn(WindowSize) -> String + Send + Sync + 'static>,
    ) {
    }

    /// OSC 52 paste reply, resolved asynchronously. The host reads the
    /// requested clipboard buffer, runs `format` over the contents (base64 +
    /// OSC framing), and writes the bytes back to the originating PTY.
    fn clipboard_load_request(
        &mut self,
        _id: Self::WindowId,
        _kind: ClipboardKind,
        _format: Arc<dyn Fn(&str) -> String + Send + Sync + 'static>,
    ) {
    }

    /// A batch of decoded graphics (sixel/iTerm2/kitty) ready for the host
    /// to upload. Rio batches per frame through [`UpdateQueues`]; the large
    /// pixel blobs never enter the grid.
    fn update_graphics(&mut self, _id: Self::WindowId, _queues: UpdateQueues) {}

    /// Glyph-protocol register: validate + store a custom PUA glyph. The
    /// host owns the render-side glossary so the engine names no font type.
    /// `Err(reason)` makes the dispatcher emit an error reply.
    fn glyph_register(
        &mut self,
        _id: Self::WindowId,
        _cp: u32,
        _payload: GlyphPayload,
    ) -> Result<(), RegisterError> {
        Ok(())
    }

    /// Glyph-protocol clear: drop one registration (`Some(cp)`) or all
    /// (`None`).
    fn glyph_clear(&mut self, _id: Self::WindowId, _cp: Option<u32>) {}

    /// Glyph-protocol `q` (query): forward to the host, which has the
    /// FontLibrary access needed to classify coverage and write the reply.
    fn glyph_query(&mut self, _id: Self::WindowId, _cp: u32) {}
}

/// Text-area geometry returned by [`TerminalHost::text_area_size`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowSize {
    pub columns: u16,
    pub lines: u16,
    pub width_px: u16,
    pub height_px: u16,
}

/// Glyph-protocol support classification (reply to the support query).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GlyphSupport {
    #[default]
    None,
    System,
    Glossary,
    Both,
}

/// Terminal-semantic, fire-and-forget side effects. UI/window events (new
/// tab, fullscreen, font-size, create/close window) are emitted by the
/// frontend, never the engine, so they stay in `RioEvent` and are NOT here.
/// Title/cwd are pull-based (`Terminal::title()`), not alerts.
#[derive(Debug, Clone)]
pub enum Alert {
    /// Terminal bell (BEL / visual bell).
    Bell,
    /// Terminal content changed — repaint hint.
    Damaged,
    /// The OS mouse-cursor icon should be re-evaluated (e.g. URL hover).
    MouseCursorDirty,
    /// Cursor blink enablement changed (config/DECSCUSR).
    CursorBlinkingChanged,
    /// A palette slot changed via OSC 4/104/10/11 — re-resolve + repaint.
    PaletteChanged { index: usize, color: Option<ColorRgb> },
    /// OSC 9;4 progress report.
    Progress(ProgressReport),
    /// The child process exited; the terminal should be closed.
    ChildExited,
    /// OSC 9 / OSC 777 desktop notification.
    Notification { title: String, body: String },
    /// OSC 52 set-clipboard.
    ClipboardStore { kind: ClipboardKind, data: String },
    /// OSC 52 paste request — host replies via `Terminal::paste_clipboard`.
    ClipboardLoad { kind: ClipboardKind },
}

/// Which clipboard buffer an OSC 52 / paste targets.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ClipboardKind {
    Clipboard,
    Primary,
    Selection,
}

/// Progress bar state for OSC 9;4 ConEmu/Windows Terminal progress reporting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressState {
    /// Remove/hide the progress bar (state 0)
    Remove,
    /// Set progress with a specific percentage (state 1)
    Set,
    /// Show error state (state 2)
    Error,
    /// Indeterminate/pulsing progress (state 3)
    Indeterminate,
    /// Paused progress (state 4)
    Pause,
}

/// Progress report from OSC 9;4 sequence
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProgressReport {
    /// The progress bar state
    pub state: ProgressState,
    /// Optional progress percentage (0-100), only used with Set, Error, and Pause states
    pub progress: Option<u8>,
}

/// Opaque handle to a host-decoded glyph-protocol outline. The engine stores
/// it by registered codepoint and never inspects it.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct GlyphHandle(pub u64);

/// Why a glyph-protocol registration was rejected by the host.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum GlyphReject {
    Unsupported,
    OutOfNamespace,
    DecodeFailed,
}

/// Engine configuration as a single trait with defaulted methods plus a
/// generation counter (wezterm pattern): new knobs are added as defaulted
/// methods without breaking implementers, and the engine flushes derived
/// caches when `generation()` bumps. The frontend's TOML config implements
/// this; the engine never sees serde or `ColorBuilder`.
pub trait TerminalConfig: Send + Sync {
    fn generation(&self) -> u64 {
        0
    }
    fn scrollback_lines(&self) -> usize {
        10_000
    }
    /// The runtime palette. The engine **snapshots this into a cached array at
    /// construction and on each `generation()` bump** — it is NEVER read per
    /// cell through this trait object, so the `Arc<dyn TerminalConfig>` vtable
    /// stays off the hot path. Slot count matches Rio's `term::COUNT` (269: 16
    /// ANSI + 240 indexed + the special UI/dim/light slots).
    fn palette(&self) -> &[ColorRgb; PALETTE_LEN];
    fn unicode_version(&self) -> u8 {
        9
    }
    fn kitty_keyboard(&self) -> bool {
        false
    }
}

/// Number of palette slots (`rio-backend::config::colors::term::COUNT`).
pub const PALETTE_LEN: usize = 269;

/// A no-op [`TerminalHost`] for tests, fuzzing, and batch use — the canario
/// analog of wezterm's `VoidListener`. `WindowId = ()`.
#[derive(Debug, Default, Clone, Copy)]
pub struct VoidHost;

impl TerminalHost for VoidHost {
    type WindowId = ();
}
