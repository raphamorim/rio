// Fork of wezterm-char-props (MIT, Wez Furlong).
// Upstream: https://github.com/wezterm/wezterm/tree/main/wezterm-char-props
//
// Reduced to the two data tables Rio needs for handling Unicode emoji
// variation selectors (VS15 / VS16) at the terminal grid level:
//   - `emoji_presentation`: Emoji_Presentation=Yes set (Unicode 16.0.0)
//   - `emoji_variation`:    entries of emoji-variation-sequences.txt
//
// `Presentation` in `emoji` is the thin wrapper around those two tables
// that `rio-backend`'s `input()` path uses to decide whether a VS15 / VS16
// should promote or narrow the preceding cell's width.
//
// Nerd Fonts are intentionally NOT handled here — Rio sizes those at
// render time via `pua_constraint_width` in the rioterm renderer rather
// than promoting their grid cells, which avoids the wcwidth disagreement
// problem that VS16 has for emoji.
//
// To refresh the tables for a newer Unicode revision, regenerate them
// against upstream wezterm-char-props' `codegen/` crate (which parses
// `emoji-variation-sequences.txt` and `DerivedCoreProperties.txt`) and
// drop the generated files in.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod emoji;
pub mod emoji_presentation;
pub mod emoji_variation;
