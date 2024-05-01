// Part of this file was originally taken from menubar crate
// https://github.com/madsmtm/menubar/blob/master/LICENSE-MIT
// which is licensed under Apache 2.0 license.

use crate::ui::appkit::menu::MenuWrapper;
use crate::ui::appkit::menuitem::MenuItemWrapper;
use icrate::Foundation::MainThreadMarker;

/// Helper to make constructing the menu bar easier
#[derive(Debug)]
pub struct MenuBar(MenuWrapper);

impl MenuBar {
    pub fn into_raw(self) -> MenuWrapper {
        self.0
    }

    pub fn new(mtm: MainThreadMarker, f: impl FnOnce(&mut MenuWrapper)) -> Self {
        // The root menu title is irrelevant
        let menu = MenuWrapper::new(mtm);
        let mut menubar = Self(menu);
        // The first item's title is irrelevant.
        // Not sure if this is the best way to represent this?
        let mut first = MenuWrapper::new(mtm);
        f(&mut first);
        menubar.add_menu(first);
        menubar
    }

    fn add_menu(&mut self, menu: MenuWrapper) -> MenuWrapper {
        // All parameters on menu items irrelevant in the menu bar
        let item = MenuItemWrapper::new_empty();
        let menu = item.set_submenu(Some(menu)).unwrap();
        self.0.add(item);
        menu
    }

    pub fn add(&mut self, title: &str, f: impl FnOnce(&mut MenuWrapper)) -> MenuWrapper {
        let mtm = MainThreadMarker::new().expect("adding to menu on main thread");
        let mut menu = MenuWrapper::new_with_title(mtm, title);
        f(&mut menu);
        self.add_menu(menu)
    }
}
