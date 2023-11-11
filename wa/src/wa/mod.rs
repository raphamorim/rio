// WA is a fork of https://github.com/rust-windowing/wa/
// wa is is licensed under Apache 2.0 license https://github.com/rust-windowing/wa/blob/master/LICENSE

mod icon;
#[macro_use]
pub mod error;
pub mod dpi;
pub mod event;
pub mod event_loop;
pub mod keyboard;
pub mod monitor;
pub mod platform;
mod platform_impl;
pub mod window;

#[doc(hidden)]
#[derive(Clone, Debug)]
pub(crate) struct SendSyncWrapper<T>(pub(crate) T);

unsafe impl<T> Send for SendSyncWrapper<T> {}
unsafe impl<T> Sync for SendSyncWrapper<T> {}
