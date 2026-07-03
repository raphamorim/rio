pub mod ansi;
pub mod clipboard;
pub mod config;
pub mod crosswords;
pub mod error;
pub mod event;
pub mod performer;
pub mod selection;
// `codepoint_width` and `simd_base64` now live in the `canario` engine crate.
// Re-export them so existing `crate::codepoint_width` / `crate::simd_base64`
// references keep resolving unchanged.
pub use canario::{codepoint_width, simd_base64};
pub use rio_parser::simd_utf8;

#[cfg(test)]
mod graphics;

pub use sugarloaf;
