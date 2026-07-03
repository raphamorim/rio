// The SGR attribute enum (`Attr`) now lives in the `canario` engine crate.
// Re-export it so existing `crate::crosswords::attr::*` paths keep resolving
// unchanged.
pub use canario::attr::*;
