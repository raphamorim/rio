# librio-sugarloaf

Terminal renderer for `librio-vt` render state, powered by
[sugarloaf](https://crates.io/crates/sugarloaf), Rio's GPU renderer.
Consumes the render-state pull API only — anything librio-sugarloaf can do,
a third-party renderer can do too.

Status: public alpha. No API stability promise; versioned independently of
the Rio terminal.

See `examples/term.rs` for a complete embedding: a live shell in a window in
under 200 lines. A C ABI is available behind the `capi` feature and ships as
`RioKit.xcframework` for Swift consumers.
