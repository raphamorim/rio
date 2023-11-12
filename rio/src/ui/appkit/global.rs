use core::cell::UnsafeCell;

use objc2::rc::{AutoreleasePool, Id, Shared};
use objc2::runtime::Bool;
use objc2::{class, msg_send};
use objc2::{Encoding, Message, RefEncode};

use super::menu::NSMenu;
use super::menubar::MenuBar;

/// Helper to make various functions on the global application object safe.
#[repr(C)]
pub struct InitializedApplication {
    /// The application contains syncronization primitives that allows mutable
    /// access with an immutable reference, and hence need to be `UnsafeCell`.
    ///
    /// TODO: Verify this claim.
    _priv: UnsafeCell<[u8; 0]>,
}

unsafe impl RefEncode for InitializedApplication {
    const ENCODING_REF: Encoding<'static> = Encoding::Object;
}

unsafe impl Message for InitializedApplication {}
unsafe impl Sync for InitializedApplication {}

impl InitializedApplication {
    /// # Safety
    ///
    /// This must not be called before `applicationDidFinishLaunching`.
    ///
    /// In `winit`, this is at or after
    /// [`winit::event::StartCause::Init`] has been emitted.
    pub unsafe fn new() -> &'static Self {
        msg_send![class!(NSApplication), sharedApplication]
    }

    #[allow(unused)]
    pub fn menubar<'p>(&self, _pool: &'p AutoreleasePool) -> Option<&'p NSMenu> {
        unsafe { msg_send![self, mainMenu] }
    }

    /// Setting the menubar to `null` does not work properly, so we don't allow
    /// that functionality here!
    #[allow(unused)]
    pub fn set_menubar(&self, menubar: MenuBar) -> Id<NSMenu, Shared> {
        let menu = menubar.into_raw();
        let _: () = unsafe { msg_send![self, setMainMenu: &*menu] };
        menu.into()
    }

    /// Returns the first menu set with [`set_window_menu`]
    #[allow(unused)]
    pub fn window_menu<'p>(&self, _pool: &'p AutoreleasePool) -> Option<&'p NSMenu> {
        unsafe { msg_send![self, windowsMenu] }
    }

    /// Set the global window menu.
    ///
    /// The "Window: menu has items and keyboard shortcuts for entering
    /// fullscreen, managing tabs (e.g. "Show Next Tab") and a list of the
    /// application's windows.
    ///
    /// Should be called before [`set_menubar`], otherwise the window menu
    /// won't be properly populated.
    ///
    /// Un-setting the window menu (to `null`) does not work properly, so we
    /// don't expose that functionality here.
    ///
    /// Additionally, you can have luck setting the window menu more than once,
    /// though this is not recommended.
    #[allow(unused)]
    pub fn set_window_menu(&self, menu: &NSMenu) {
        // TODO: Is it safe to immutably set this?
        unsafe { msg_send![self, setWindowsMenu: menu] }
    }

    /// Returns the first menu set with [`set_services_menu`]
    #[allow(unused)]
    pub fn services_menu<'p>(&self, _pool: &'p AutoreleasePool) -> Option<&'p NSMenu> {
        unsafe { msg_send![self, servicesMenu] }
    }

    /// Set the global services menu.
    ///
    /// The user can have a number of system configured services and
    /// corresponding keyboard shortcuts that can be accessed from this menu.
    ///
    /// Un-setting the services menu (to `null`) does not work properly, so we
    /// don't expose that functionality here.
    ///
    /// Additionally, you can sometimes have luck setting the services menu
    /// more than once, but this is really flaky.
    #[allow(unused)]
    pub fn set_services_menu(&self, menu: &NSMenu) {
        // TODO: Is it safe to immutably set this?
        // TODO: The menu should (must?) not contain any items!
        // TODO: Setting this and pressing the close button doesn't work in winit
        unsafe { msg_send![self, setServicesMenu: menu] }
    }

    // TODO: registerServicesMenuSendTypes

    /// Get the menu that is currently assigned as the help menu, or `None` if the system is configured to autodetect this.
    #[allow(unused)]
    pub fn help_menu<'p>(&self, _pool: &'p AutoreleasePool) -> Option<&'p NSMenu> {
        unsafe { msg_send![self, helpMenu] }
    }

    /// Set the global menu that should have the spotlight Help Search
    /// functionality at the top of it.
    ///
    /// If this is set to `None`, the system will place the search bar somewhere
    /// else, usually on an item named "Help" (unknown if localization applies).
    /// To prevent this, specify a menu that does not appear anywhere.
    #[allow(unused)]
    pub fn set_help_menu(&self, menu: Option<&NSMenu>) {
        // TODO: Is it safe to immutably set this?
        unsafe { msg_send![self, setHelpMenu: menu] }
    }

    // TODO: applicationDockMenu (the application delegate should implement this function)

    #[allow(unused)]
    pub fn menubar_visible(&self) -> bool {
        let visible: Bool = unsafe { msg_send![class!(NSMenu), menuBarVisible] };
        visible.is_true()
    }

    /// Hide or show the menubar for the entire application.
    /// This also hides or shows the yellow minimize button.
    ///
    /// Might silently fail to set the menubar visible if in fullscreen mode or similar.
    #[allow(unused)]
    pub fn set_menubar_visible(&self, visible: bool) {
        let visible = Bool::new(visible);
        unsafe { msg_send![class!(NSMenu), setMenuBarVisible: visible] }
    }

    // Only available on the global menu bar object
    // pub fn global_height(&self) -> f64 {
    //     let height: CGFloat = unsafe { msg_send![self, menuBarHeight] };
    //     height
    // }
}

#[cfg(test)]
mod tests {
    use objc2::rc::autoreleasepool;
    use objc2::rc::Owned;

    use super::*;

    fn init_app() -> &'static InitializedApplication {
        unimplemented!()
    }

    fn create_menu() -> Id<NSMenu, Owned> {
        unimplemented!()
    }

    #[test]
    #[ignore = "not implemented"]
    fn test_services_menu() {
        let app = init_app();
        let menu1 = create_menu();
        let menu2 = create_menu();

        autoreleasepool(|pool| {
            assert!(app.services_menu(pool).is_none());

            app.set_services_menu(&menu1);
            assert_eq!(app.services_menu(pool).unwrap(), &*menu1);

            app.set_services_menu(&menu2);
            assert_eq!(app.services_menu(pool).unwrap(), &*menu2);

            // At this point `menu1` still shows as a services menu...
        });
    }
}
