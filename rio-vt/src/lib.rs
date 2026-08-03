// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Rio's embeddable terminal core: the VT state machine (`crosswords`),
//! ANSI/escape parser (`ansi` + `performer`), grid, selection, search,
//! PTY-facing event model, and the color palette. No rendering, GPU, or
//! font-shaping dependencies — pair it with a renderer of your choice
//! (Rio uses `sugarloaf`) or drive it purely from the pulled grid state.
//!
//! `rio-backend` builds on this crate and re-exports every module at the
//! same path, so it stays the home of the user-facing config and the
//! sugarloaf renderer glue while this crate carries the reusable core.

pub mod ansi;
pub mod clipboard;
pub mod codepoint_width;
pub mod config;
pub mod crosswords;
pub mod error;
pub mod event;
pub mod performer;
pub mod selection;
pub mod simd_base64;
pub mod simd_utf8;

// avoid double dep requirement
pub use corcovado;
pub use teletypewriter;

// The app-level `RioErrorType::FontsNotFound` variant references a
// sugarloaf font type; expose the crate under this path (as rio-backend
// did) so that renderer-gated code resolves `crate::sugarloaf`.
#[cfg(feature = "renderer")]
pub use sugarloaf;
