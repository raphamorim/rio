# canario

**The songbird at the bottom of Rio's stack: a correct, fast, embeddable VT terminal engine that turns bytes into terminal state, and nothing more.**

`canario` is a headless, renderer-agnostic VT/ANSI terminal-state engine extracted from the [Rio](https://rioterm.com) terminal — the Rust counterpart to `alacritty_terminal`, `wezterm-term`, `libghostty-vt`, and `vt100`. You feed it PTY bytes and read a packed cell grid, cursor, scrollback, selection, modes, and graphics. It never produces pixels, owns a window, or parses TOML.

> *canário* — Portuguese for *canary*. It joins Rio's family of Brazilian-landmark crate names (**sugarloaf** renderer, **corcovado** event loop, **teletypewriter** PTY). A canary is the first thing into the mine: the small, sensitive, correctness-focused core that surfaces VT bugs before they reach the renderer.

## Status: scaffold

This is the initial extraction scaffold. What exists today:

| Crate | What it is |
|---|---|
| **`rio-core`** | The decoupling landing zone — pure value types (`ColorRgb`/`AnsiColor`/`NamedColor`, `GraphicData`/`GraphicId`/`ColorType`, geometry) relocated out of `sugarloaf` and `config`. **Real, compiling.** |
| **`rio-parser`** | The VT500 state machine. **Skeleton** — the forked `vte` machine lands in Phase 1. |
| **`canario`** | The engine. The **host-integration contract** (`TerminalHost`, `TerminalConfig`, `Alert`) is real, compiling code; the engine core (parser → `Handler` → grid) is lifted from `rio-backend` over Phases 2–5. |

## Documents

- **[`DESIGN.md`](./DESIGN.md)** — goal & positioning, a comparative analysis of `vt100` / `alacritty_terminal` / `wezterm-term` / `libghostty-vt` / `xterm.js`, the architecture, the public-API sketch, and the five severances that decouple the engine from Rio's renderer/config/window.
- **[`ROADMAP.md`](./ROADMAP.md)** — the source-verified, phased extraction plan (Phase 0 scaffold → parser → grid → host traits → PTY → re-point `rio-backend`), with per-phase verification gates.
- **[`CRITIQUE.md`](./CRITIQUE.md)** — the adversarial completeness pass over the design (features to carry over, decoupling risks, API-ergonomics gaps).

## The contract (today)

```rust
use canario::{Terminal, TerminalHost, VoidHost, Dimensions};

// A no-op host for headless/test use; real embedders implement TerminalHost.
let mut term = Terminal::new(Dimensions::new(24, 80), config, VoidHost, ());
// term.advance(b"\x1b[31mhello\x1b[0m");   // (engine lands in Phase 1–2)
```

The single seam every embedder touches is `TerminalHost` (PTY write-back, alerts, graphics, glyph decode) — the boundary that replaces Rio's `RioEvent`/`EventListener` plumbing.

## License

MIT — same as the Rio workspace.
