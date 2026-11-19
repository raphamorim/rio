// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Font concerns shared by Rio's renderers, with no rendering, GPU, or
//! windowing dependencies.
//!
//! - [`nerd_font`]: the Nerd Fonts patcher's per-glyph scaling and
//!   alignment rules (by way of ghostty's generated table), so every
//!   renderer places Powerline separators and icon glyphs identically.
//! - The symbols-only Nerd Font itself, embedded behind the
//!   `symbols-nerd-font` feature for renderers that want icon glyphs
//!   available with no font file shipped or installed (the same call
//!   libghostty makes).

#![forbid(unsafe_code)]

pub mod nerd_font;

/// Symbols-only Nerd Font as raw TTF bytes.
#[cfg(feature = "symbols-nerd-font")]
pub static SYMBOLS_NERD_FONT: &[u8] =
    include_bytes!("../resources/SymbolsNerdFontMono/SymbolsNerdFontMono-Regular.ttf");
