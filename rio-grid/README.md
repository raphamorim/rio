# rio-grid

Rio's shared grid-emission layer: turns a row of terminal cells plus its
resolved styles into the GPU cell primitives (`CellBg` / `CellText`)
that [Rio](https://github.com/raphamorim/rio)'s renderer draws. One
pass emits backgrounds, one run-building pass emits glyphs (shaping
runs, drawable sprites, underlines), with selection, search-highlight,
and hint-label handling applied per row.

It sits between the terminal core
([`rio-vt`](https://crates.io/crates/rio-vt)) and the renderer, and is
consumed by both the Rio terminal itself and `libsugarloaf`'s C ABI so
embedders share the exact same emission logic.
