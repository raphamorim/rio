mod icon;
#[macro_use]
pub mod error;
pub mod dpi;
pub mod monitor;
pub mod keyboard;
pub mod event;
pub mod event_loop;
pub mod window;
pub mod platform;
mod platform_impl;

#[doc(hidden)]
#[derive(Clone, Debug)]
pub(crate) struct SendSyncWrapper<T>(pub(crate) T);

unsafe impl<T> Send for SendSyncWrapper<T> {}
unsafe impl<T> Sync for SendSyncWrapper<T> {}