// The packed `Square(u64)` cell, its bit layout (`ContentTag`, `Wide`,
// `CellFlags`), the `Extras` side-table record, the `GridSquare`/`LineLength`/
// `ResetDiscriminant` impls, and the `Hyperlink`/`ExtrasId` cell-value helpers
// now live in the `canario` engine crate. Re-export them so existing
// `crate::crosswords::square::*` paths keep resolving unchanged. The
// host-coupled `Crosswords` struct stays in rio-backend and uses these types.
pub use canario::square::*;
