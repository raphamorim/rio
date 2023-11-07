// WA is a fork of https://github.com/rust-windowing/wa/
// wa is is licensed under Apache 2.0 license https://github.com/rust-windowing/wa/blob/master/LICENSE

use crate::keyboard::Key;

/// Additional methods for the `KeyEvent` which cannot be implemented on all
/// platforms.
pub trait KeyEventExtModifierSupplement {
    /// Identical to `KeyEvent::text` but this is affected by <kbd>Ctrl</kbd>.
    ///
    /// For example, pressing <kbd>Ctrl</kbd>+<kbd>a</kbd> produces `Some("\x01")`.
    fn text_with_all_modifiers(&self) -> Option<&str>;

    /// This value ignores all modifiers including,
    /// but not limited to <kbd>Shift</kbd>, <kbd>Caps Lock</kbd>,
    /// and <kbd>Ctrl</kbd>. In most cases this means that the
    /// unicode character in the resulting string is lowercase.
    ///
    /// This is useful for key-bindings / shortcut key combinations.
    ///
    /// In case `logical_key` reports `Dead`, this will still report the
    /// key as `Character` according to the current keyboard layout. This value
    /// cannot be `Dead`.
    fn key_without_modifiers(&self) -> Key;
}
