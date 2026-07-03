// The host-clean terminal engine (`Crosswords<H>`, `CrosswordsSize`, the VT
// `Mode`/`PrivateMode` flags, damage iteration, the `Handler` impl, and the
// `search`/`vi_mode` submodules) now lives in the `canario` engine crate.
// Re-export the whole module so existing `crate::crosswords::*` paths —
// `crate::crosswords::Crosswords`, `CrosswordsSize`, `crosswords::search`,
// `crosswords::vi_mode`, etc. — keep resolving unchanged.
//
// The `attr`/`grid`/`pos`/`square`/`style` data-model submodules are declared
// below as thin shim modules (each re-exporting the matching `canario` crate
// module). They are declared explicitly so the historical
// `crate::crosswords::grid::row::Row` style paths keep resolving and so they
// take precedence over the same names brought in by the glob below (an
// explicit item always shadows a glob re-export).
pub mod attr;
pub mod grid;
pub mod pos;
pub mod square;
pub mod style;

pub use canario::crosswords::*;
