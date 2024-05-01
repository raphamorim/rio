// Part of this file was originally taken from menubar crate
// https://github.com/madsmtm/menubar/blob/master/LICENSE-MIT
// which is licensed under Apache 2.0 license.

use core::fmt;
use icrate::AppKit::{
    NSControlStateValueMixed, NSControlStateValueOff, NSControlStateValueOn, NSMenuItem,
};
use icrate::Foundation::NSString;
use objc2::rc::Id;
use objc2::runtime::Sel;
use objc2::ClassType;

use crate::ui::appkit::menu::MenuWrapper;

#[derive(Debug, PartialEq)]
pub enum MenuItemState {
    /// Checked
    On,
    Mixed,
    /// Unchecked
    Off,
}

#[derive(PartialEq, Eq, Hash, Clone)]
pub struct MenuItemWrapper(pub Id<NSMenuItem>);

impl MenuItemWrapper {
    // Defaults:
    //     State: NSOffState
    //     On-state image: Check mark
    //     Mixed-state image: Dash

    // Public only locally to allow for construction in Menubar
    pub(super) fn new_empty() -> Self {
        Self(unsafe { NSMenuItem::init(NSMenuItem::alloc()) })
    }

    pub fn new(title: &str, key_equivalent: &str, action: Option<Sel>) -> Self {
        let title = NSString::from_str(title);
        let key_equivalent = NSString::from_str(key_equivalent);
        Self(unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(),
                &title,
                action,
                &key_equivalent,
            )
        })
    }

    pub fn new_separator() -> Self {
        Self(unsafe { NSMenuItem::separatorItem() })
    }

    /// Whether the menu item is hidden or not.
    ///
    /// If hidden, it does not appear in a menu and does not participate in command key matching.
    pub fn hidden(&self) -> bool {
        unsafe { self.0.isHidden() }
    }

    #[allow(unused)]
    pub fn set_hidden(&self, hidden: bool) {
        unsafe { self.0.setHidden(hidden) }
    }

    // Title
    pub fn title(&self) -> String {
        unsafe { self.0.title().to_string() }
    }

    #[allow(unused)]
    pub fn set_title(&self, title: &str) {
        let title = NSString::from_str(title);
        unsafe { self.0.setTitle(&title) };
    }

    /// Get the menu item's state
    pub fn state(&self) -> MenuItemState {
        let state = unsafe { self.0.state() };
        if state == NSControlStateValueOn {
            MenuItemState::On
        } else if state == NSControlStateValueMixed {
            MenuItemState::Mixed
        } else if state == NSControlStateValueOff {
            MenuItemState::Off
        } else {
            unreachable!()
        }
    }

    /// Set the menu item's state
    #[allow(unused)]
    pub fn set_state(&self, state: MenuItemState) {
        let state = match state {
            MenuItemState::On => NSControlStateValueOn,
            MenuItemState::Mixed => NSControlStateValueMixed,
            MenuItemState::Off => NSControlStateValueOff,
        };
        unsafe { self.0.setState(state) };
    }

    pub fn submenu(&self) -> Option<MenuWrapper> {
        unsafe { self.0.submenu() }.map(MenuWrapper)
    }

    pub fn set_submenu(&self, menu: Option<MenuWrapper>) -> Option<MenuWrapper> {
        // The submenu must not already have a parent!
        unsafe { self.0.setSubmenu(menu.as_ref().map(|menu| &*menu.0)) };
        menu
    }

    pub fn separator(&self) -> bool {
        // TODO: Maybe call this is_separator?
        unsafe { self.0.isSeparatorItem() }
    }
}

impl fmt::Debug for MenuItemWrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NSMenuItem")
            .field("id", &(self as *const Self))
            .field("separator", &self.separator())
            .field("title", &self.title())
            .field("hidden", &self.hidden())
            .field("state", &self.state())
            .field("submenu", &self.submenu())
            // TODO: parent?
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use icrate::Foundation::MainThreadMarker;

    use super::*;
    use crate::ui::appkit::menu::MenuWrapper;
    pub static STRINGS: [&str; 5] = [
        "",
        "🤖",
        "test",
        "abcαβγ",
        "ศไทย中华Việt Nam β-release 🐱123",
        // Doesn't yet work properly
        // "test\0",
        // "test\0test",
    ];

    fn for_each_item(mut f: impl FnMut(&MenuItemWrapper)) {
        f(&MenuItemWrapper::new_separator());
        f(&MenuItemWrapper::new_empty());
        f(&MenuItemWrapper::new("", "", None));
    }

    #[test]
    fn test_hidden() {
        for_each_item(|item| {
            assert!(!item.hidden());
            item.set_hidden(true);
            assert!(item.hidden());
            item.set_hidden(false);
            assert!(!item.hidden());
        })
    }

    #[test]
    fn test_title() {
        for_each_item(|item| {
            STRINGS.iter().for_each(|&title| {
                item.set_title(title);
                assert_eq!(item.title(), title);
            });
        });
    }

    #[test]
    fn test_title_init() {
        STRINGS.iter().for_each(|&title| {
            let item = MenuItemWrapper::new(title, "", None);
            assert_eq!(item.title(), title);
        });
    }

    #[test]
    fn test_title_default() {
        let item = MenuItemWrapper::new_empty();
        assert_eq!(item.title(), "NSMenuItem");
        let item = MenuItemWrapper::new_separator();
        assert_eq!(item.title(), "");
    }

    #[test]
    fn test_separator() {
        let item = MenuItemWrapper::new_separator();
        assert!(item.separator());
        let item = MenuItemWrapper::new_empty();
        assert!(!item.separator());
        let item = MenuItemWrapper::new("", "", None);
        assert!(!item.separator());
    }

    #[test]
    fn test_state() {
        for_each_item(|item| {
            assert_eq!(item.state(), MenuItemState::Off);
            item.set_state(MenuItemState::On);
            assert_eq!(item.state(), MenuItemState::On);
            item.set_state(MenuItemState::Mixed);
            assert_eq!(item.state(), MenuItemState::Mixed);
            item.set_state(MenuItemState::Off);
            assert_eq!(item.state(), MenuItemState::Off);
        });
    }

    #[test]
    fn test_submenu() {
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        for_each_item(|item| {
            assert!(item.submenu().is_none());
            let menu = MenuWrapper::new(mtm);
            let menu = item.set_submenu(Some(menu));
            assert_eq!(item.submenu(), menu);
            item.set_submenu(None);
            assert!(item.submenu().is_none());
        })
    }
}
