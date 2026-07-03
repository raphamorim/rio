// The VT geometry primitives (`Pos`, `Line`, `Column`, `Direction`/`Side`,
// `Cursor`, `CursorState`, `Charsets`, `CharsetIndex`, `StandardCharset`,
// `Boundary`) now live in the `canario` engine crate. Re-export them so
// existing `crate::crosswords::pos::*` paths keep resolving unchanged.
pub use canario::pos::*;
