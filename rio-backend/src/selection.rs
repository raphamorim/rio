// The selection state machine (`Selection`, `SelectionType`, `SelectionRange`,
// and the span/anchor logic) now lives in the `canario` engine crate — it
// couples only to canario's grid/pos/square types. Re-export it so existing
// `crate::selection::*` paths — and the frontend's
// `rio_backend::selection::{Selection, SelectionRange, SelectionType}` — keep
// resolving unchanged.
pub use canario::selection::*;
