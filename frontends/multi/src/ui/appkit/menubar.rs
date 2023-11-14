use super::menu::NSMenu;
use super::menuitem::NSMenuItem;
use objc2::rc::{Id, Owned, Shared};

/// Helper to make constructing the menu bar easier
#[derive(Debug)]
pub struct MenuBar(Id<NSMenu, Owned>);

impl MenuBar {
    #[allow(unused)]
    pub unsafe fn from_raw(menu: Id<NSMenu, Owned>) -> Self {
        Self(menu)
    }

    pub fn into_raw(self) -> Id<NSMenu, Owned> {
        self.0
    }

    pub fn new(f: impl FnOnce(&mut NSMenu)) -> Self {
        // The root menu title is irrelevant
        let menu = NSMenu::new();
        let mut menubar = Self(menu);
        // The first item's title is irrelevant.
        // Not sure if this is the best way to represent this?
        let mut first = NSMenu::new();
        f(&mut first);
        menubar.add_menu(first);
        menubar
    }

    fn add_menu(&mut self, menu: Id<NSMenu, Owned>) -> Id<NSMenu, Shared> {
        // All parameters on menu items irrelevant in the menu bar
        let mut item = NSMenuItem::new_empty();
        let menu = item.set_submenu(Some(menu)).unwrap();
        let _item = self.0.add(item);
        menu
    }

    pub fn add(
        &mut self,
        title: &str,
        f: impl FnOnce(&mut NSMenu),
    ) -> Id<NSMenu, Shared> {
        let mut menu = NSMenu::new_with_title(title);
        f(&mut menu);
        self.add_menu(menu)
    }

    // How do we handle this???
    // pub fn title(index) {}
    // pub fn set_title(index, title) {}
}
