// Part of this file was originally taken from menubar crate
// https://github.com/madsmtm/menubar/blob/master/LICENSE-MIT
// which is licensed under Apache 2.0 license.

use crate::ui::appkit::menuitem::MenuItemWrapper;
use core::fmt;
use icrate::AppKit::{NSMenu, NSMenuItem};
use icrate::Foundation::{MainThreadMarker, NSArray, NSString};
use objc2::rc::Id;

/// The maximum number of items a menu can hold is 65534
#[derive(PartialEq, Eq, Hash, Clone)]
pub struct MenuWrapper(pub Id<NSMenu>);

/// Creating menus
impl MenuWrapper {
    pub fn new(_mtm: MainThreadMarker) -> Self {
        Self(unsafe { NSMenu::new() })
    }

    // Public only locally to allow for construction in Menubar
    #[doc(alias = "initWithTitle")]
    #[doc(alias = "initWithTitle:")]
    pub(super) fn new_with_title(mtm: MainThreadMarker, title: &str) -> Self {
        let title = NSString::from_str(title);
        let menu = unsafe { NSMenu::initWithTitle(mtm.alloc(), &title) };
        Self(menu)
    }

    // Title (only useful for MenuBar!)

    pub(super) fn title(&self) -> String {
        unsafe { self.0.title() }.to_string()
    }

    #[allow(unused)]
    pub(super) fn set_title(&self, title: &str) {
        let title = NSString::from_str(title);
        unsafe { self.0.setTitle(&title) };
    }
}

/// Managing items
impl MenuWrapper {
    #[allow(unused)]
    pub fn insert(&self, item: MenuItemWrapper, index: usize) {
        let length = self.len();
        if index > length {
            panic!(
                "Failed inserting item: Index {} larger than number of items {}",
                index, length
            );
        }
        // SAFETY:
        // - References are valid
        // - The item must not exist in another menu!!!!!
        //     - We need to ensure this somehow, for now we'll just consume the item!
        //     - Should maybe return a reference to the menu, where the reference is now bound to self?
        // - 0 <= index <= self.len()
        // TODO: Thread safety!
        unsafe { self.0.insertItem_atIndex(&item.0, index as isize) };
    }

    pub fn add(&self, item: MenuItemWrapper) {
        // Same safety concerns as above
        unsafe { self.0.addItem(&item.0) }
    }

    #[allow(unused)]
    pub fn remove_all(&self) {
        // SAFETY: Reference is valid
        unsafe { self.0.removeAllItems() }
    }

    #[allow(unused)]
    pub fn len(&self) -> usize {
        unsafe { self.0.numberOfItems() as usize }
    }

    fn get_all_items(&self) -> Id<NSArray<NSMenuItem>> {
        unsafe { self.0.itemArray() }
    }
}

impl fmt::Debug for MenuWrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NSMenu")
            .field("id", &(self as *const Self))
            .field("title", &self.title())
            // TODO: parent?
            // TODO: size and stuff
            .field("items", &self.get_all_items())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::appkit::menuitem::MenuItemWrapper;
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

    #[test]
    fn test_title() {
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let menu = MenuWrapper::new(mtm);
        assert_eq!(menu.title(), "");
        STRINGS.iter().for_each(|&title| {
            menu.set_title(title);
            assert_eq!(menu.title(), title);
        });
    }

    #[test]
    fn test_title_init() {
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        STRINGS.iter().for_each(|&title| {
            let menu = MenuWrapper::new_with_title(mtm, title);
            assert_eq!(menu.title(), title);
        });
    }

    #[test]
    fn test_length() {
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let menu = MenuWrapper::new(mtm);
        assert_eq!(menu.len(), 0);
        menu.add(MenuItemWrapper::new_empty());
        assert_eq!(menu.len(), 1);
        menu.add(MenuItemWrapper::new_separator());
        assert_eq!(menu.len(), 2);
        menu.add(MenuItemWrapper::new("test", "", None));
        assert_eq!(menu.len(), 3);
        menu.insert(MenuItemWrapper::new("test", "", None), 2);
        assert_eq!(menu.len(), 4);
        menu.remove_all();
        assert_eq!(menu.len(), 0);
    }

    #[test]
    fn test_iter() {
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let menu = MenuWrapper::new(mtm);
        assert!(menu.get_all_items().is_empty());

        // A few different iterations
        menu.add(MenuItemWrapper::new_empty());
        menu.add(MenuItemWrapper::new_empty());
        menu.add(MenuItemWrapper::new_separator());
        let mut iter = menu.get_all_items().into_iter();
        assert_eq!(iter.size_hint(), (0, Some(3)));
        assert!(unsafe { !iter.next().unwrap().isSeparatorItem() });
        assert!(unsafe { !iter.next().unwrap().isSeparatorItem() });
        assert!(unsafe { iter.next().unwrap().isSeparatorItem() });
        assert!(iter.next().is_none());

        // Modifying after creating the iterator (the iterator is unaffected)
        let mut iter = menu.get_all_items().into_iter();

        menu.add(MenuItemWrapper::new_empty());
        assert_eq!(iter.size_hint(), (0, Some(3)));
        assert!(unsafe { !iter.next().unwrap().isSeparatorItem() });

        menu.add(MenuItemWrapper::new_separator());
        assert_eq!(iter.size_hint(), (2, Some(3)));
        assert!(unsafe { !iter.next().unwrap().isSeparatorItem() });

        menu.remove_all();
        assert_eq!(iter.size_hint(), (1, Some(3)));
        assert!(unsafe { iter.next().unwrap().isSeparatorItem() });

        menu.add(MenuItemWrapper::new_separator());
        assert_eq!(iter.size_hint(), (0, Some(3)));
        assert!(iter.next().is_none());

        // Test fused-ness
        assert!(iter.next().is_none());
        assert!(iter.next().is_none());
        assert!(iter.next().is_none());
        assert!(iter.next().is_none());
    }

    #[test]
    fn test_max_count() {
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let menu = MenuWrapper::new(mtm);
        const COUNT: usize = 65534;
        for i in 1..=COUNT {
            menu.add(MenuItemWrapper::new(&format!("item {}", i), "", None));
        }
        assert_eq!(menu.len(), COUNT);

        menu.add(MenuItemWrapper::new(
            &format!("item {}", COUNT + 1),
            "",
            None,
        ));

        // The menu item should fail rendering, and we should get an error similar to the following logged:
        // 2021-01-01 00:00:00.000 my_program[12345:678901] InsertMenuItemTextWithCFString(_principalMenuRef, (useAccessibilityTitleDescriptionTrick ? CFSTR("") : (CFStringRef)title), carbonIndex - 1, attributes, [self _menuItemCommandID]) returned error -108 on line 2638 in -[NSCarbonMenuImpl _carbonMenuInsertItem:atCarbonIndex:]
    }
}
