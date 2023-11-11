// WA is a fork of https://github.com/rust-windowing/wa/
// wa is is licensed under Apache 2.0 license https://github.com/rust-windowing/wa/blob/master/LICENSE

use crate::keyboard::{KeyCode, PhysicalKey};

// TODO: Describe what this value contains for each platform

/// Additional methods for the [`PhysicalKey`] type that allow the user to access the platform-specific
/// scancode.
///
/// [`PhysicalKey`]: crate::keyboard::PhysicalKey
pub trait PhysicalKeyExtScancode {
    /// The raw value of the platform-specific physical key identifier.
    ///
    /// Returns `Some(key_id)` if the conversion was succesful; returns `None` otherwise.
    ///
    /// ## Platform-specific
    /// - **Windows:** A 16bit extended scancode
    /// - **Wayland/X11**: A 32-bit linux scancode, which is X11/Wayland keycode subtracted by 8.
    fn to_scancode(self) -> Option<u32>;

    /// Constructs a `PhysicalKey` from a platform-specific physical key identifier.
    ///
    /// Note that this conversion may be lossy, i.e. converting the returned `PhysicalKey` back
    /// using `to_scancode` might not yield the original value.
    ///
    /// ## Platform-specific
    /// - **Wayland/X11**: A 32-bit linux scancode. When building from X11/Wayland keycode subtract
    ///                    `8` to get the value you wanted.
    fn from_scancode(scancode: u32) -> PhysicalKey;
}

impl PhysicalKeyExtScancode for KeyCode
where
    PhysicalKey: PhysicalKeyExtScancode,
{
    #[inline]
    fn from_scancode(scancode: u32) -> PhysicalKey {
        <PhysicalKey as PhysicalKeyExtScancode>::from_scancode(scancode)
    }

    #[inline]
    fn to_scancode(self) -> Option<u32> {
        <PhysicalKey as PhysicalKeyExtScancode>::to_scancode(PhysicalKey::Code(self))
    }
}
