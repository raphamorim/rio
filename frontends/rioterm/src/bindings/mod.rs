#[cfg(use_wa)]
pub mod bindings_wa;

#[cfg(use_wa)]
pub use bindings_wa::*;

#[cfg(not(use_wa))]
pub mod bindings_window;

#[cfg(not(use_wa))]
pub use bindings_window::*;
