#[cfg(use_wa)]
pub mod bindings_wa;

#[cfg(use_wa)]
pub use bindings_wa::*;

#[cfg(not(use_wa))]
pub mod bindings_winit;

#[cfg(not(use_wa))]
pub use bindings_winit::*;
