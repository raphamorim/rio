//! `Instant` for every target. std's panics on wasm32-unknown-unknown
//! ("time not implemented"), so wasm builds use `web-time`, which has the
//! same API backed by `performance.now()`. Everything in this crate that
//! timestamps (synchronized-update timeouts, kitty image bookkeeping) goes
//! through this alias instead of `std::time` directly.

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::{Duration, Instant};

#[cfg(target_arch = "wasm32")]
pub use web_time::{Duration, Instant};
