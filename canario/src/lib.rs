//! # canario
//!
//! **The songbird at the bottom of Rio's stack: a correct, fast, embeddable
//! VT terminal engine that turns bytes into terminal state, and nothing
//! more.**
//!
//! `canario` is a headless, renderer-agnostic, window-agnostic,
//! config-agnostic VT/ANSI terminal-state engine extracted from the
//! [Rio](https://rioterm.com) terminal — the Rust counterpart to
//! `alacritty_terminal`, `wezterm-term`, `libghostty-vt`, and `vt100`. You
//! feed it PTY bytes and read a packed cell grid, cursor, scrollback,
//! selection, modes, and graphics; it never produces pixels, owns a window,
//! or parses TOML.
//!
//! ## Status
//!
//! **Scaffold.** The *decoupling boundary* — the [`host`] traits — is real,
//! compiling code. The engine itself (parser → `Handler` → grid) is lifted
//! out of `rio-backend` over the phased plan in
//! [`ROADMAP.md`](https://github.com/raphamorim/rio). See
//! [`DESIGN.md`](https://github.com/raphamorim/rio) for the full
//! architecture and the comparative analysis of the reference engines.
//!
//! ## Shape of the API (target)
//!
//! ```ignore
//! let mut term = Terminal::new(dims, config, host, window_id);
//! term.advance(b"\x1b[31mhello\x1b[0m");   // bytes in
//! let dmg = term.damage();                  // what changed
//! // render thread: lock briefly, snapshot dirty rows, unlock, paint.
//! ```
//!
//! See `examples/` and `DESIGN.md` §4 for the full surface.

pub mod ansi;
pub mod attr;
pub mod codepoint_width;
pub mod colors;
pub mod crosswords;
pub mod damage;
pub mod grid;
pub mod handler;
pub mod host;
pub mod osc;
pub mod pos;
pub mod selection;
pub mod simd_base64;
pub mod square;
pub mod style;
pub mod sync;

/// The PTY driver — the `Machine` event loop that pumps PTY bytes into the
/// engine. Gated behind `pty` because it pulls in `teletypewriter` +
/// `corcovado`; headless embedders don't need it.
#[cfg(feature = "pty")]
pub mod pty;

// Re-export the shared value types so embedders depend only on `canario`.
pub use rio_core;
pub use rio_core::{AnsiColor, ColorRgb, GraphicData, NamedColor};

pub use colors::TermColors;
pub use damage::{
    CursorShape, CursorState, Dimensions, LineDamage, TerminalDamage,
};
pub use host::{
    Alert, ClipboardKind, GlyphHandle, GlyphReject, GlyphSupport, ProgressReport,
    ProgressState, TerminalConfig, TerminalHost, VoidHost, WindowSize,
};

/// The headless terminal engine — *bytes in, terminal state out*.
///
/// Generic over the embedder's [`TerminalHost`]. The engine internals (the
/// alacritty-derived `Grid<Square>` ring buffer, the `Handler` impl, modes,
/// scrollback, selection) live in [`crosswords`]; the ported type keeps the
/// name [`Crosswords`](crosswords::Crosswords) so Rio's history and vocabulary
/// survive the move (DESIGN §6). `Terminal` is the embedder-facing name for
/// the same type.
pub type Terminal<H> = crosswords::Crosswords<H>;
