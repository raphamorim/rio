// The terminal core (VT state machine, parser, grid, selection, search,
// PTY event model, colors) now lives in the `rio-vt` crate. Re-export
// every module at its original `crate::`/`rio_backend::` path so nothing
// downstream (rioterm, tests) has to change. This crate keeps the
// user-facing config and the sugarloaf renderer glue.
pub use rio_vt::{
    ansi, clipboard, codepoint_width, crosswords, error, event, paste, performer,
    selection, simd_base64, simd_utf8,
};

pub mod config;
pub mod hints;
#[cfg(not(target_arch = "wasm32"))]
pub mod shell_integration;

#[cfg(test)]
mod graphics;

#[cfg(feature = "renderer")]
pub use sugarloaf;
