#[cfg(all(unix, not(target_os = "macos")))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(all(unix, not(target_os = "macos")))]
pub use linux::OsBackdropProvider;
#[cfg(target_os = "macos")]
pub use macos::OsBackdropProvider;
#[cfg(target_os = "windows")]
pub use windows::OsBackdropProvider;
