// The pure grid/cell data model (`Grid`, `Row`, `Storage`, `GridSquare`, the
// `Dimensions` trait, resize/reflow, scroll, the per-grid `ExtrasTable`, and
// the grid tests) now lives in the `canario` engine crate. Re-export the whole
// module — including its `row`/`storage`/`resize` submodules — so existing
// `crate::crosswords::grid::*` and `crate::crosswords::grid::row::Row` paths
// keep resolving unchanged. The host-coupled `Crosswords` struct (in
// `crosswords/mod.rs`) stays in rio-backend and uses these re-exported types.
pub use canario::grid::*;
pub use canario::grid::{resize, row, storage};
