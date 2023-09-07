use core::mem;
use core::{ffi, fmt, ptr};
use objc2::ffi::NSInteger;
use objc2::rc::{autoreleasepool, AutoreleasePool, Id, Owned, Shared};
use objc2::runtime::{Bool, Object};
use objc2::{class, msg_send, sel};
use objc2::{Encoding, Message, RefEncode};
use objc2_foundation::NSString;
use std::ptr::NonNull;

use super::menu::NSMenu;

struct Target; // Normal NSObject. Should return YES in worksWhenModal.
struct ActionSelector; // objc::Sel - a method selector
struct Image;

#[derive(Debug, PartialEq)]
pub enum MenuItemState {
    /// Checked
    On,
    Mixed,
    /// Unchecked
    Off,
}

#[repr(C)]
pub struct NSMenuItem {
    _priv: [u8; 0],
}

unsafe impl RefEncode for NSMenuItem {
    const ENCODING_REF: Encoding<'static> = Encoding::Object;
}

unsafe impl Message for NSMenuItem {}

unsafe impl Send for NSMenuItem {}
unsafe impl Sync for NSMenuItem {}

impl NSMenuItem {
    // Defaults:
    //     State: NSOffState
    //     On-state image: Check mark
    //     Mixed-state image: Dash

    fn alloc() -> *mut Self {
        unsafe { msg_send![class!(NSMenuItem), alloc] }
    }

    // Public only locally to allow for construction in Menubar
    pub(super) fn new_empty() -> Id<Self, Owned> {
        let ptr = Self::alloc();
        unsafe { Id::new(msg_send![ptr, init]).unwrap() }
    }

    #[doc(alias = "initWithTitle")]
    #[doc(alias = "initWithTitle:action:keyEquivalent:")]
    pub fn new(
        title: &str,
        key_equivalent: &str,
        action: Option<NonNull<ffi::c_void>>,
    ) -> Id<Self, Owned> {
        let title = NSString::from_str(title);
        let key_equivalent = NSString::from_str(key_equivalent);
        let action = if let Some(p) = action {
            p.as_ptr()
        } else {
            ptr::null_mut()
        };
        let ptr = Self::alloc();
        unsafe {
            Id::new(msg_send![
                ptr,
                initWithTitle: &*title,
                action: action,
                keyEquivalent: &*key_equivalent,
            ])
            .unwrap()
        }
    }

    #[doc(alias = "separatorItem")]
    pub fn new_separator() -> Id<Self, Owned> {
        let ptr: *mut Self = unsafe { msg_send![class!(NSMenuItem), separatorItem] };
        // TODO: Find an ergonomic API where we don't need to retain. Also,
        // this has a memory leak if there's no `autoreleasepool` to release
        // the returned pointer.
        unsafe { Id::retain(ptr).unwrap_unchecked() }
    }

    // fn new_separator<'p>(pool: &'p AutoreleasePool) -> &'p mut Self {
    //     unsafe { msg_send![class!(NSMenuItem), separatorItem] }
    // }

    // Enabling

    fn enabled(&self) -> bool {
        unimplemented!()
    }

    #[doc(alias = "setEnabled")]
    #[doc(alias = "setEnabled:")]
    fn set_enabled(&mut self, state: bool) {
        unimplemented!()
    }

    // Managing Hidden Status

    /// Whether the menu item is hidden or not.
    ///
    /// If hidden, it does not appear in a menu and does not participate in command key matching.
    pub fn hidden(&self) -> bool {
        let hidden: Bool = unsafe { msg_send![self, isHidden] };
        hidden.is_true()
    }

    #[doc(alias = "setHidden")]
    #[doc(alias = "setHidden:")]
    pub fn set_hidden(&mut self, hidden: bool) {
        let hidden = Bool::new(hidden);
        unsafe { msg_send![self, setHidden: hidden] }
    }

    // #[doc(alias = "hiddenOrHasHiddenAncestor")]
    // fn hidden_or_has_hidden_ancestor(&self) -> bool {
    //     unimplemented!()
    // }

    // Target and action

    fn target(&self) -> Target {
        unimplemented!()
    }

    #[doc(alias = "setTarget")]
    #[doc(alias = "setTarget:")]
    fn set_target(&mut self, target: Target) {
        unimplemented!()
    }

    fn action(&self) -> ActionSelector {
        unimplemented!()
    }

    #[doc(alias = "setAction")]
    #[doc(alias = "setAction:")]
    fn set_action(&mut self, action: ActionSelector) {
        unimplemented!()
    }

    // Title

    pub fn title<'p>(&self, pool: &'p AutoreleasePool) -> &'p str {
        let title: &NSString = unsafe { msg_send![self, title] };
        title.as_str(pool)
    }

    #[doc(alias = "setTitle")]
    #[doc(alias = "setTitle:")]
    pub fn set_title(&mut self, title: &str) {
        let title = NSString::from_str(title);
        unsafe { msg_send![self, setTitle: &*title] }
    }

    // #[doc(alias = "attributedTitle")]
    // pub fn attributed_title(&self) -> ??? { unimplemented!() }
    // #[doc(alias = "setAttributedTitle")]
    // #[doc(alias = "setAttributedTitle:")]
    // pub fn set_attributed_title(&mut self, title: ???) { unimplemented!() }

    // Tag

    fn tag(&self) -> isize {
        unimplemented!()
    }

    #[doc(alias = "setTag")]
    #[doc(alias = "setTag:")]
    fn set_tag(&mut self, tag: isize) {
        unimplemented!()
    }

    /// Get the menu item's state
    pub fn state(&self) -> MenuItemState {
        let state: NSInteger = unsafe { msg_send![self, state] };
        match state {
            1 => MenuItemState::On,
            -1 => MenuItemState::Mixed,
            0 => MenuItemState::Off,
            _ => unreachable!(),
        }
    }

    /// Set the menu item's state
    #[doc(alias = "setState")]
    #[doc(alias = "setState:")]
    pub fn set_state(&mut self, state: MenuItemState) {
        // TODO: Link or something to these?
        // static const NSControlStateValue NSControlStateValueMixed = -1;
        // static const NSControlStateValue NSControlStateValueOff = 0;
        // static const NSControlStateValue NSControlStateValueOn = 1;

        let state: NSInteger = match state {
            MenuItemState::On => 1,
            MenuItemState::Mixed => -1,
            MenuItemState::Off => 0,
        };
        unsafe { msg_send![self, setState: state] }
    }

    // Images

    fn image(&self) -> Option<&Image> {
        unimplemented!()
    }

    #[doc(alias = "setImage")]
    #[doc(alias = "setImage:")]
    fn set_image(&mut self, image: Option<&Image>) {
        unimplemented!()
    }

    #[doc(alias = "onStateImage")]
    #[doc(alias = "offStateImage")]
    #[doc(alias = "mixedStateImage")]
    fn image_for_state<'p>(
        &self,
        pool: &'p AutoreleasePool,
        state: MenuItemState,
    ) -> Option<&'p Image> {
        unimplemented!()
    }

    #[doc(alias = "setOnStateImage")]
    #[doc(alias = "setOnStateImage:")]
    #[doc(alias = "setOffStateImage")]
    #[doc(alias = "setOffStateImage:")]
    #[doc(alias = "setMixedStateImage")]
    #[doc(alias = "setMixedStateImage:")]
    fn set_image_for_state(&mut self, state: MenuItemState, image: Option<&Image>) {
        unimplemented!()
    }

    // Submenus

    pub fn submenu<'p>(&self, pool: &'p AutoreleasePool) -> Option<&'p NSMenu> {
        unsafe { msg_send![self, submenu] }
    }

    #[doc(alias = "setSubmenu")]
    #[doc(alias = "setSubmenu:")]
    pub fn set_submenu(
        &mut self,
        mut menu: Option<Id<NSMenu, Owned>>,
    ) -> Option<Id<NSMenu, Shared>> {
        // The submenu must not already have a parent!
        let ptr = match menu {
            Some(ref mut menu) => &mut **menu as *mut NSMenu,
            None => ptr::null_mut(),
        };
        let _: () = unsafe { msg_send![self, setSubmenu: ptr] };
        menu.map(|obj| obj.into())
    }

    #[doc(alias = "hasSubmenu")]
    fn has_submenu(&self) -> bool {
        unimplemented!()
    }

    /// The parent submenu's menuitem
    #[doc(alias = "parentItem")]
    fn parent_item<'p>(&self, pool: &'p AutoreleasePool) -> Option<&'p NSMenuItem> {
        unimplemented!()
    }

    #[doc(alias = "isSeparatorItem")]
    pub fn separator(&self) -> bool {
        // TODO: Maybe call this is_separator?
        let is_separator: Bool = unsafe { msg_send![self, isSeparatorItem] };
        is_separator.is_true()
    }

    // Owning menu

    #[doc(alias = "menu")]
    fn parent_menu<'p>(&self, pool: &'p AutoreleasePool) -> &'p NSMenu {
        unimplemented!()
    }

    #[doc(alias = "setMenu")]
    #[doc(alias = "setMenu:")]
    fn set_parent_menu(&mut self, menu: &mut NSMenu) {
        unimplemented!()
    }

    // Handling keyboard events

    // fn key_equvalent()
    // fn key_equvalent_something_modifiers()
    // fn something_user_key_equvalents
    // fn user_key_equvalent() (readonly)

    // Marks the menu item as an alternate to the previous menu item

    fn alternate(&self) -> bool {
        unimplemented!()
    }

    #[doc(alias = "setAlternate")]
    #[doc(alias = "setAlternate:")]
    fn set_alternate(&mut self, alternate: bool) {
        unimplemented!()
    }

    // Indentation level (0-15)

    #[doc(alias = "indentationLevel")]
    fn indentation_level(&self) -> isize {
        unimplemented!()
    }

    #[doc(alias = "setIndentationLevel")]
    #[doc(alias = "setIndentationLevel:")]
    fn set_indentation_level(&mut self, level: isize) {
        unimplemented!()
    }

    // Tooltop / help tag

    #[doc(alias = "toolTip")]
    fn tooltip(&self) -> &str {
        unimplemented!()
    }

    #[doc(alias = "setToolTip")]
    #[doc(alias = "setToolTip:")]
    fn set_tooltip(&mut self, tooltip: &str) {
        unimplemented!()
    }

    // Represented object (kinda like tags)

    #[doc(alias = "representedObject")]
    fn represented_object(&self) -> *const Object {
        unimplemented!()
    }

    #[doc(alias = "setRepresentedObject")]
    #[doc(alias = "setRepresentedObject:")]
    fn set_represented_object(&mut self, tooltip: *mut Object) {
        unimplemented!()
    }

    // View - most other attributes are ignore if this is set

    fn view(&self) -> *const Object {
        unimplemented!()
    }

    #[doc(alias = "setView")]
    #[doc(alias = "setView:")]
    fn set_view(&mut self, tooltip: *mut Object) {
        unimplemented!()
    }

    /// Get whether the menu should be drawn highlighted
    ///
    /// You should probably use the [`NSMenu`] delegate method "willHighlightItem"
    #[doc(alias = "isHighlighted")]
    fn highlighted(&self) -> bool {
        unimplemented!()
    }

    // Protocols: Same as NSMenu + "NSValidatedUserInterfaceItem"
    // This will have to be researched, is the way for the system to
    // automatically enable and disable items based on context
}

impl PartialEq for NSMenuItem {
    /// Pointer equality
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self, other)
    }
}

impl fmt::Debug for NSMenuItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        autoreleasepool(|pool| {
            f.debug_struct("NSMenuItem")
                .field("id", &(self as *const Self))
                .field("separator", &self.separator())
                .field("title", &self.title(pool))
                .field("hidden", &self.hidden())
                .field("state", &self.state())
                .field("submenu", &self.submenu(pool))
                // TODO: parent?
                .finish()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::STRINGS;

    fn for_each_item(_pool: &AutoreleasePool, mut f: impl FnMut(&mut NSMenuItem)) {
        f(&mut *NSMenuItem::new_separator());
        f(&mut *NSMenuItem::new_empty());
        f(&mut *NSMenuItem::new("", "", None));
    }

    #[test]
    fn test_hidden() {
        autoreleasepool(|pool| {
            for_each_item(pool, |item| {
                assert!(!item.hidden());
                item.set_hidden(true);
                assert!(item.hidden());
                item.set_hidden(false);
                assert!(!item.hidden());
            })
        });
    }

    #[test]
    fn test_title() {
        autoreleasepool(|pool| {
            for_each_item(pool, |item| {
                STRINGS.iter().for_each(|&title| {
                    item.set_title(title);
                    assert_eq!(item.title(pool), title);
                });
            });
        });
    }

    #[test]
    fn test_title_init() {
        autoreleasepool(|pool| {
            STRINGS.iter().for_each(|&title| {
                let item = NSMenuItem::new(title, "", None);
                assert_eq!(item.title(pool), title);
            });
        });
    }

    #[test]
    fn test_title_default() {
        autoreleasepool(|pool| {
            let item = NSMenuItem::new_empty();
            assert_eq!(item.title(pool), "NSMenuItem");
            let item = NSMenuItem::new_separator();
            assert_eq!(item.title(pool), "");
        });
    }

    #[test]
    fn test_separator() {
        autoreleasepool(|_| {
            let item = NSMenuItem::new_separator();
            assert!(item.separator());
            let item = NSMenuItem::new_empty();
            assert!(!item.separator());
            let item = NSMenuItem::new("", "", None);
            assert!(!item.separator());
        });
    }

    #[test]
    fn test_state() {
        autoreleasepool(|pool| {
            for_each_item(pool, |item| {
                assert_eq!(item.state(), MenuItemState::Off);
                item.set_state(MenuItemState::On);
                assert_eq!(item.state(), MenuItemState::On);
                item.set_state(MenuItemState::Mixed);
                assert_eq!(item.state(), MenuItemState::Mixed);
                item.set_state(MenuItemState::Off);
                assert_eq!(item.state(), MenuItemState::Off);
            });
        });
    }

    #[test]
    fn test_submenu() {
        autoreleasepool(|pool| {
            for_each_item(pool, |item| {
                assert!(item.submenu(pool).is_none());
                let menu = NSMenu::new();
                let menu = item.set_submenu(Some(menu));
                assert_eq!(item.submenu(pool), menu.as_deref());
                item.set_submenu(None);
                assert!(item.submenu(pool).is_none());
            })
        });
    }
}
