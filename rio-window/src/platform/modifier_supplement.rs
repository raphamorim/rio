use crate::event::KeyEvent;
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

    /// The key resolved against a Latin (ASCII-capable) keyboard layout,
    /// ignoring all modifiers.
    ///
    /// When the active layout cannot produce ASCII for this key (Cyrillic,
    /// Greek, Arabic, ...), this resolves the key through a layout that can:
    /// another configured xkb layout group on Linux, the ASCII-capable
    /// keyboard layout on macOS. Useful for matching shortcuts such as
    /// `Ctrl+Shift+V` independently of the active layout.
    ///
    /// Returns `None` when no configured layout produces ASCII for this key,
    /// or on platforms without layout introspection (Windows); callers may
    /// then fall back to the key's position in the standard PC-101 layout.
    fn base_layout_key(&self) -> Option<Key>;
}

impl KeyEventExtModifierSupplement for KeyEvent {
    #[inline]
    fn text_with_all_modifiers(&self) -> Option<&str> {
        self.platform_specific
            .text_with_all_modifiers
            .as_ref()
            .map(|s| s.as_str())
    }

    #[inline]
    fn key_without_modifiers(&self) -> Key {
        self.platform_specific.key_without_modifiers.clone()
    }

    #[inline]
    fn base_layout_key(&self) -> Option<Key> {
        self.platform_specific.base_layout_key.clone()
    }
}
