#[cfg(target_os = "macos")]
pub mod bindings_wa;

#[cfg(target_os = "macos")]
pub use bindings_wa::*;

#[cfg(not(target_os = "macos"))]
pub mod bindings_winit;

#[cfg(not(target_os = "macos"))]
pub use bindings_winit::*;
