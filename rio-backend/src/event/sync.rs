// The `FairMutex` (originally taken from Alacritty) now lives in the `canario`
// engine crate. Re-export it so existing `crate::event::sync::FairMutex`
// references — and the frontend's `rio_backend::event::sync::FairMutex` —
// keep resolving unchanged.
pub use canario::sync::*;
