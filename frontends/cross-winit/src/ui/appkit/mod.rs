pub mod global;
pub mod menu;
pub mod menubar;
pub mod menuitem;

// use core::ptr::NonNull;
use objc2::rc::autoreleasepool;

pub use self::menubar::MenuBar;
pub use global::InitializedApplication;
pub use menuitem::NSMenuItem;

// We need the Objectice-C symbols like NSString, NSMenu and so on to be available
#[link(name = "AppKit", kind = "framework")]
extern "C" {}
#[link(name = "Foundation", kind = "framework")]
extern "C" {}

pub fn create_toolbar() {
    autoreleasepool(|_pool| {
        let app = unsafe { InitializedApplication::new() };
        // let menubar = app.menubar(pool).unwrap();
        // // Yeah, this is not ok but we'll do it for now
        // let menubar: Id<NSMenu, Owned> =
        //     unsafe { Id::retain(NonNull::from(menubar)) };
        // let mut menubar = unsafe { MenuBar::from_raw(menubar) };
        let mut menubar = MenuBar::new(|menu| {
            menu.add(NSMenuItem::new(env!("CARGO_PKG_VERSION"), "", None));
            menu.add(NSMenuItem::new_separator());
            menu.add(NSMenuItem::new("Hide Rio", "h", None));
            menu.add(NSMenuItem::new("Quit Rio", "q", None));
        });

        // let oi = NonNull::new(print as *mut std::ffi::c_void);

        // menubar.add("Shell", |menu| {
        // let mut item = NSMenuItem::new("Will be above the window data", "a", oi);
        // item.set_enabled(true);
        // item.set_target(1)
        // menu.add(item);
        // });

        // menubar.add("Edit", |menu| {
        // menu.add(NSMenuItem::new("Will be above the window data", "", None));
        // });

        menubar.add("View", |_menu| {
            // menu.add(NSMenuItem::new("Will be above the window data", "", None));
        });

        let window_menu = menubar.add("Window", |_menu| {
            // menu.add(NSMenuItem::new("Will be above the window data", "", None));
        });

        let help_menu = menubar.add("Help", |_menu| {
            // menu.add(NSMenuItem::new("Item 2 : 1", "", None));
            // menu.add(NSMenuItem::new(
            //     "Search or report issue on Github",
            //     "",
            //     None,
            // ));
        });

        app.set_window_menu(&window_menu);
        app.set_help_menu(Some(&help_menu));
        app.set_menubar(menubar);
    });
}
