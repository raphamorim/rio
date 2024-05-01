// Part of this file was originally taken from menubar crate
// https://github.com/madsmtm/menubar/blob/master/LICENSE-MIT
// which is licensed under Apache 2.0 license.

pub mod global;
pub mod menu;
pub mod menubar;
pub mod menuitem;

use crate::ui::appkit::global::InitializedApplication;
use crate::ui::appkit::menubar::MenuBar;
use crate::ui::appkit::menuitem::MenuItemWrapper;
use icrate::Foundation::MainThreadMarker;

#[link(name = "AppKit", kind = "framework")]
extern "C" {}
#[link(name = "Foundation", kind = "framework")]
extern "C" {}

pub fn create_toolbar() {
    let mtm = MainThreadMarker::new().unwrap();
    let app = unsafe { InitializedApplication::new(mtm) };

    let mut menubar = MenuBar::new(mtm, |menu| {
        menu.add(MenuItemWrapper::new(env!("CARGO_PKG_VERSION"), "", None));
        menu.add(MenuItemWrapper::new_separator());
        menu.add(MenuItemWrapper::new("Hide Rio", "h", None));
        menu.add(MenuItemWrapper::new("Quit Rio", "q", None));
    });

    menubar.add("View", |_menu| {
        // menu.add(MenuItemWrapper::new("Will be above the window data", "", None));
    });

    let window_menu = menubar.add("Window", |_menu| {
        // menu.add(MenuItemWrapper::new("Will be above the window data", "", None));
    });

    let help_menu = menubar.add("Help", |_menu| {
        // menu.add(MenuItemWrapper::new("Item 2 : 1", "", None));
        // menu.add(MenuItemWrapper::new(
        //     "Search or report issue on Github",
        //     "",
        //     None,
        // ));
    });

    app.set_window_menu(&window_menu);
    app.set_help_menu(Some(&help_menu));
    app.set_menubar(menubar);
}
