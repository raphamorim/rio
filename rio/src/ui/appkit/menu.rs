use core::fmt;
use core::marker::PhantomData;
use core::mem;
use objc2::ffi::{NSInteger, NSUInteger};
use objc2::rc::{autoreleasepool, AutoreleasePool, Id, Owned, Shared};
use objc2::runtime::Object;
use objc2::{class, msg_send, sel};
use objc2::{Encoding, Message, RefEncode};
use objc2_foundation::NSString;

use super::menuitem::NSMenuItem;

struct MenuDelegate;

struct USize {
    height: f64,
    width: f64,
}

/// The maximum number of items a menu can hold is 65534
#[repr(C)]
pub struct NSMenu {
    _priv: [u8; 0],
}

unsafe impl RefEncode for NSMenu {
    const ENCODING_REF: Encoding<'static> = Encoding::Object;
}

unsafe impl Message for NSMenu {}

unsafe impl Send for NSMenu {}
unsafe impl Sync for NSMenu {}

/// Creating menus
impl NSMenu {
    fn alloc() -> *mut Self {
        unsafe { msg_send![class!(NSMenu), alloc] }
    }

    pub fn new() -> Id<Self, Owned> {
        let ptr = Self::alloc();
        unsafe { Id::new(msg_send![ptr, init]).unwrap() }
    }

    // Public only locally to allow for construction in Menubar
    #[doc(alias = "initWithTitle")]
    #[doc(alias = "initWithTitle:")]
    pub(super) fn new_with_title(title: &str) -> Id<Self, Owned> {
        let title = NSString::from_str(title);
        let ptr = Self::alloc();
        unsafe { Id::new(msg_send![ptr, initWithTitle: &*title]).unwrap() }
    }

    // Title (only useful for MenuBar!)

    pub(super) fn title<'p>(&self, pool: &'p AutoreleasePool) -> &'p str {
        let title: &'p NSString = unsafe { msg_send![self, title] };
        title.as_str(pool)
    }

    #[doc(alias = "setTitle")]
    #[doc(alias = "setTitle:")]
    pub(super) fn set_title(&mut self, title: &str) {
        let title = NSString::from_str(title);
        unsafe { msg_send![self, setTitle: &*title] }
    }
}

/// Managing items
impl NSMenu {
    /// Insert an item at the specified index.
    ///
    /// Panics if `index > menu.len()`.
    #[doc(alias = "insertItem")]
    #[doc(alias = "insertItem:atIndex:")]
    // TODO: Reorder arguments to match `Vec::insert`?
    pub fn insert(
        &mut self,
        item: Id<NSMenuItem, Owned>,
        index: usize,
    ) -> Id<NSMenuItem, Shared> {
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
        let _: () =
            unsafe { msg_send![self, insertItem: &*item, atIndex: index as NSInteger] };
        // The item is now shared, so it's no longer safe to hold a mutable pointer to it
        item.into()
    }

    #[doc(alias = "addItem")]
    #[doc(alias = "addItem:")]
    pub fn add(&mut self, item: Id<NSMenuItem, Owned>) -> Id<NSMenuItem, Shared> {
        // Same safety concerns as above
        let _: () = unsafe { msg_send![self, addItem: &*item] };
        // The item is now shared, so it's no longer safe to hold a mutable pointer to it
        item.into()
    }

    // There exists `addItemWithTitle_action_keyEquivalent`

    // Can't use this yet, we need to find a way to let users have references to menu items safely!
    // #[doc(alias = "removeItem")]
    // #[doc(alias = "removeItem:")]
    // fn remove(&mut self, item: &mut NSMenuItem) {
    //     unsafe { msg_send![self, removeItem: item] }
    // }
    // #[doc(alias = "removeItemAtIndex")]
    // #[doc(alias = "removeItemAtIndex:")]
    // fn remove_at_index(&mut self, at: isize) {
    //     unimplemented!()
    // }

    /// Does not post notifications.
    #[doc(alias = "removeAllItems")]
    pub fn remove_all(&mut self) {
        // SAFETY: Reference is valid
        unsafe { msg_send![self, removeAllItems] }
    }

    // Finding items

    #[doc(alias = "itemWithTag")]
    #[doc(alias = "itemWithTag:")]
    fn find_by_tag<'p>(
        &self,
        pool: &'p AutoreleasePool,
        tag: isize,
    ) -> Option<&'p NSMenuItem> {
        unimplemented!()
    }

    #[doc(alias = "itemWithTitle")]
    #[doc(alias = "itemWithTitle:")]
    fn find_by_title<'p>(
        &self,
        pool: &'p AutoreleasePool,
        title: &str,
    ) -> Option<&'p NSMenuItem> {
        unimplemented!()
    }

    #[doc(alias = "itemAtIndex")]
    #[doc(alias = "itemAtIndex:")]
    unsafe fn get_at_index<'p>(
        &self,
        pool: &'p AutoreleasePool,
        at: isize,
    ) -> &'p NSMenuItem {
        unimplemented!()
    }

    // Getting all items

    /// Number of items in this menu, including separators
    #[doc(alias = "numberOfItems")]
    pub fn len(&self) -> usize {
        let number_of_items: NSInteger = unsafe { msg_send![self, numberOfItems] };
        number_of_items as usize
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[doc(alias = "itemArray")]
    fn get_all_items<'p>(&self, pool: &'p AutoreleasePool) -> &'p [&'p NSMenuItem] {
        unimplemented!()
    }

    #[doc(alias = "itemArray")]
    pub fn iter<'p>(
        &self,
        pool: &'p AutoreleasePool,
    ) -> impl Iterator<Item = &'p NSMenuItem> + 'p {
        let array: *const Object = unsafe { msg_send![self, itemArray] };
        let enumerator: *mut Object = unsafe { msg_send![array, objectEnumerator] };
        Iter {
            array,
            enumerator,
            _p: PhantomData,
        }
    }

    // Finding indices of elements

    #[doc(alias = "indexOfItem")]
    #[doc(alias = "indexOfItem:")]
    fn index_of(&self, item: &NSMenuItem) -> Option<isize> {
        unimplemented!()
    }

    #[doc(alias = "indexOfItemWithTitle")]
    #[doc(alias = "indexOfItemWithTitle:")]
    fn index_of_by_title(&self, title: &str) -> Option<isize> {
        unimplemented!()
    }

    #[doc(alias = "indexOfItemWithTag")]
    #[doc(alias = "indexOfItemWithTag:")]
    fn index_of_by_tag(&self, tag: isize) -> Option<isize> {
        unimplemented!()
    }

    // fn index_of_by_action_and_target(&self, ...) -> isize {}
    // fn index_of_item_by_represented_object(&self, ...) -> isize {}

    #[doc(alias = "indexOfItemWithSubmenu")]
    #[doc(alias = "indexOfItemWithSubmenu:")]
    fn index_of_submenu(&self, submenu: &NSMenu) -> Option<isize> {
        unimplemented!()
    }

    // Managing submenus

    #[doc(alias = "setSubmenu")]
    #[doc(alias = "setSubmenu:forItem:")]
    // Unsure about this!
    fn set_submenu(&self, submenu: &mut NSMenu, for_item: &mut NSMenuItem) {
        unimplemented!()
    }

    // fn submenuAction(&self) {} // Overridable!

    #[doc(alias = "supermenu")]
    fn get_parent<'p>(&self, pool: &'p AutoreleasePool) -> Option<&'p NSMenu> {
        unimplemented!()
    }

    // Has more deprecated methods!

    // Enable/disable items

    /// Default on
    #[doc(alias = "autoenablesItems")]
    fn autoenables_items(&self) -> bool {
        unimplemented!()
    }

    #[doc(alias = "setAutoenablesItems")]
    #[doc(alias = "setAutoenablesItems:")]
    fn set_autoenables_items(&mut self, state: bool) {
        unimplemented!()
    }

    #[doc(alias = "update")]
    fn update_enabled_state_of_items(&self) {
        unimplemented!()
    }

    // Control fonts for this and subitems

    // fn font() -> Font {}

    // #[doc(alias = "setFont")]
    // #[doc(alias = "setFont:")]
    // fn set_font(&mut self, font: Font) {}

    // Handling keyboard events

    // #[doc(alias = "performKeyEquivalent")]
    // #[doc(alias = "performKeyEquivalent:")]
    // fn perform_key_equivalent(&self, event: KeyEvent) -> bool {}

    // Simulating mouse clicks

    // #[doc(alias = "performActionForItemAtIndex")]
    // #[doc(alias = "performActionForItemAtIndex:")]
    // fn perform_action_for_item_at(&self, index: isize) {}

    // Size

    #[doc(alias = "minimumWidth")]
    fn min_width(&self) -> Option<f64> {
        // None / zero when not set
        unimplemented!()
    }

    #[doc(alias = "setMinimumWidth")]
    #[doc(alias = "setMinimumWidth:")]
    fn set_min_width(&mut self, width: Option<f64>) {
        // None ~= zero
        unimplemented!()
    }

    fn size(&self) -> USize {
        unimplemented!()
    }

    #[doc(alias = "setSize")]
    #[doc(alias = "setSize:")]
    fn set_size(&mut self, size: USize) {
        // Might change the size if too big (or small?)
        unimplemented!()
    }

    // propertiesToUpdate - for efficiency when updating items

    #[doc(alias = "allowsContextMenuPlugIns")]
    fn allows_context_menu_plug_ins(&self) -> bool {
        unimplemented!()
    }

    #[doc(alias = "setAllowsContextMenuPlugIns")]
    #[doc(alias = "setAllowsContextMenuPlugIns:")]
    fn set_allows_context_menu_plug_ins(&mut self, state: bool) {
        unimplemented!()
    }

    // #[doc(alias = "popUpContextMenu:withEvent:forView:")]
    // fn displayPopUpContextMenu(&mut self, event: Event, view: Option<&View>) {}
    // #[doc(alias = "popUpContextMenu:withEvent:forView:withFont:")]
    // fn displayPopUpContextMenuWithFont(&mut self, event: Event, view: Option<&View>, font: Font) {}
    // #[doc(alias = "popUpMenuPositioningItem:atLocation:inView:")]
    // fn displayPopUpAtMenuPositioningItem(&mut self, position_item: Option<&NSMenuItem>, event: Event, view: Option<&View>)

    // Whether the menu displays the state column (the "Checkmark" column for items?)
    #[doc(alias = "showsStateColumn")]
    fn show_state_column(&self) -> bool {
        unimplemented!()
    }

    #[doc(alias = "setShowsStateColumn")]
    #[doc(alias = "setShowsStateColumn:")]
    fn set_show_state_column(&mut self, show: bool) {
        unimplemented!()
    }

    #[doc(alias = "highlightedItem")]
    fn currently_highlighted_item<'p>(
        &self,
        pool: &'p AutoreleasePool,
    ) -> Option<&'p NSMenuItem> {
        unimplemented!()
    }

    // Should honestly probably not be changed! (userInterfaceLayoutDirection)
    // fn layout_direction() {}
    // fn set_layout_direction() {}

    // You can use the delegate to populate a menu just before it is drawn
    // and to check for key equivalents without creating a menu item.
    fn delegate(&self) -> &MenuDelegate {
        // Tied to a pool or the current item?
        unimplemented!()

        // Events / things this delegate can respond to
        // - menuHasKeyEquivalent:forEvent:target:action:
        // - menu:updateItem:atIndex:shouldCancel: (update_item_before_displayed)
        // - confinementRectForMenu:onScreen: (display_location)
        // - menu:willHighlightItem: (before_highlight_item)
        // - menuWillOpen: (before_open)
        // - menuDidClose: (after_close)
        // - numberOfItemsInMenu: // Works together with updateItemBeforeDisplayed
        //     Newly created items are blank, and then updateItemBeforeDisplayed populates them
        // - menuNeedsUpdate: // Alternatively, if the population can happen basically instantly
        //     (and don't need to do a lot of processing beforehand), this can just be used
    }

    #[doc(alias = "setDelegate")]
    #[doc(alias = "setDelegate:")]
    fn set_delegate(&mut self, delegate: &mut MenuDelegate) {
        unimplemented!()
    }

    // Handling tracking? Perhaps just means closing/dismissing the menu?

    #[doc(alias = "cancelTracking")]
    fn cancel_tracking(&mut self) {
        unimplemented!()
    }

    #[doc(alias = "cancelTrackingWithoutAnimation")]
    #[doc(alias = "cancelTrackingWithoutAnimation:")]
    fn cancel_tracking_without_animation(&mut self) {
        unimplemented!()
    }

    // "Notifications" - not sure what these are yet!
    // - https://developer.apple.com/documentation/foundation/nsnotificationcenter?language=objc
    // - https://developer.apple.com/documentation/foundation/nsnotificationname?language=objc
    // - https://developer.apple.com/documentation/foundation/nsnotification?language=objc

    // Conforms to protocols:
    //     NSAccessibility - wow big guy [...]
    //     NSAccessibilityElement
    //     NSAppearanceCustomization - we should probably not allow editing this?
    //     NSCoding
    //     NSCopying
    //     NSUserInterfaceItemIdentification - May become important!
}

impl PartialEq for NSMenu {
    /// Pointer equality
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self, other)
    }
}

impl fmt::Debug for NSMenu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        autoreleasepool(|pool| {
            f.debug_struct("NSMenu")
                .field("id", &(self as *const Self))
                .field("title", &self.title(pool))
                // TODO: parent?
                // TODO: size and stuff
                .field("items", &self.iter(pool).collect::<Vec<_>>())
                .finish()
        })
    }
}

struct Iter<'p> {
    array: *const Object,
    enumerator: *mut Object,
    _p: PhantomData<&'p [&'p NSMenuItem]>,
}

impl<'p> Iterator for Iter<'p> {
    type Item = &'p NSMenuItem;

    fn next(&mut self) -> Option<Self::Item> {
        let item: *const NSMenuItem = unsafe { msg_send![self.enumerator, nextObject] };

        if item.is_null() {
            None
        } else {
            Some(unsafe { &*item })
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let length: NSUInteger = unsafe { msg_send![self.array, count] };
        (length as usize, Some(length))
    }
}

impl ExactSizeIterator for Iter<'_> {}

impl std::iter::FusedIterator for Iter<'_> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::STRINGS;

    #[test]
    fn test_title() {
        autoreleasepool(|pool| {
            let mut menu = NSMenu::new();
            assert_eq!(menu.title(pool), "");
            STRINGS.iter().for_each(|&title| {
                menu.set_title(title);
                assert_eq!(menu.title(pool), title);
            });
        });
    }

    #[test]
    fn test_title_init() {
        autoreleasepool(|pool| {
            STRINGS.iter().for_each(|&title| {
                let menu = NSMenu::new_with_title(title);
                assert_eq!(menu.title(pool), title);
            });
        });
    }

    #[test]
    fn test_length() {
        autoreleasepool(|pool| {
            let mut menu = NSMenu::new();
            assert_eq!(menu.len(), 0);
            menu.add(NSMenuItem::new_empty());
            assert_eq!(menu.len(), 1);
            menu.add(NSMenuItem::new_separator());
            assert_eq!(menu.len(), 2);
            menu.add(NSMenuItem::new("test", "", None));
            assert_eq!(menu.len(), 3);
            menu.insert(NSMenuItem::new("test", "", None), 2);
            assert_eq!(menu.len(), 4);
            menu.remove_all();
            assert_eq!(menu.len(), 0);
        });
    }

    #[test]
    fn test_iter() {
        autoreleasepool(|pool| {
            let mut menu = NSMenu::new();
            assert!(menu.iter(pool).next().is_none());

            // A few different iterations
            menu.add(NSMenuItem::new_empty());
            menu.add(NSMenuItem::new_empty());
            menu.add(NSMenuItem::new_separator());
            let mut iter = menu.iter(pool);
            assert_eq!(iter.size_hint(), (3, Some(3)));
            assert!(!iter.next().unwrap().separator());
            assert!(!iter.next().unwrap().separator());
            assert!(iter.next().unwrap().separator());
            assert!(iter.next().is_none());

            // Modifying after creating the iterator (the iterator is unaffected)
            let mut iter = menu.iter(pool);

            menu.add(NSMenuItem::new_empty());
            assert_eq!(iter.size_hint(), (3, Some(3)));
            assert!(!iter.next().unwrap().separator());

            menu.add(NSMenuItem::new_separator());
            assert_eq!(iter.size_hint(), (3, Some(3)));
            assert!(!iter.next().unwrap().separator());

            menu.remove_all();
            assert_eq!(iter.size_hint(), (3, Some(3)));
            assert!(iter.next().unwrap().separator());

            menu.add(NSMenuItem::new_separator());
            assert_eq!(iter.size_hint(), (3, Some(3)));
            assert!(iter.next().is_none());

            // Test fused-ness
            assert!(iter.next().is_none());
            assert!(iter.next().is_none());
            assert!(iter.next().is_none());
            assert!(iter.next().is_none());
        });
    }

    #[test]
    fn test_max_count() {
        autoreleasepool(|_| {
            let mut menu = NSMenu::new();
            const COUNT: usize = 65534;
            for i in 1..=COUNT {
                menu.add(NSMenuItem::new(&format!("item {}", i), "", None));
            }
            assert_eq!(menu.len(), COUNT);

            // The menu, if we could render it at this point, should render fine

            menu.add(NSMenuItem::new(&format!("item {}", COUNT + 1), "", None));

            // The menu item should fail rendering, and we should get an error similar to the following logged:
            // 2021-01-01 00:00:00.000 my_program[12345:678901] InsertMenuItemTextWithCFString(_principalMenuRef, (useAccessibilityTitleDescriptionTrick ? CFSTR("") : (CFStringRef)title), carbonIndex - 1, attributes, [self _menuItemCommandID]) returned error -108 on line 2638 in -[NSCarbonMenuImpl _carbonMenuInsertItem:atCarbonIndex:]
        });
    }
}
