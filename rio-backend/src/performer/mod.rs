// The `Handler` trait + `Processor` performer and the OSC parsing helpers
// live in the `canario` engine crate. Re-export `handler` so existing
// `crate::performer::handler::{Handler, Processor, ...}` paths keep resolving
// unchanged. (`osc` is an internal detail of `handler` and has no rio-backend
// consumers.)
pub use canario::handler;

// The PTY driver — the `Machine` event loop, its `State`, the `Msg` channel
// protocol, and the `spawn_named` helper — now lives in the `canario` engine
// crate's `pty` module (behind the `pty` feature, which rio-backend enables).
// Re-export them so existing `crate::performer::{Machine, State, Msg,
// spawn_named}` paths — and the frontend's `rio_backend::performer::*`
// imports — keep resolving unchanged.
pub use canario::pty::{spawn_named, Machine, Msg, State};
