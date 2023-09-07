pub mod global;
pub mod menu;
pub mod menubar;
pub mod menuitem;

pub use self::menubar::MenuBar;
pub use global::InitializedApplication;
pub use menu::NSMenu;
pub use menuitem::{MenuItemState, NSMenuItem};

// We need the Objectice-C symbols like NSString, NSMenu and so on to be available
#[link(name = "AppKit", kind = "framework")]
extern "C" {}
#[link(name = "Foundation", kind = "framework")]
extern "C" {}
