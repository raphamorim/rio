// The per-grid style intern table (`Style`, `StyleId`, `StyleSet`,
// `StyleFlags`, `DEFAULT_STYLE_ID`) now lives in the `canario` engine crate.
// Re-export it so existing `crate::crosswords::style::*` paths keep resolving
// unchanged. The host-coupled `Crosswords` struct stays in rio-backend and
// uses these types.
pub use canario::style::*;
