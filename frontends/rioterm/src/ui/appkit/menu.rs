use core::fmt;
use core::marker::PhantomData;

use objc2::ffi::{NSInteger, NSUInteger};
use objc2::rc::{autoreleasepool, AutoreleasePool, Id, Owned, Shared};
use objc2::runtime::Object;
use objc2::{class, msg_send};
use objc2::{Encoding, Message, RefEncode};
use objc2_foundation::NSString;

use super::menuitem::NSMenuItem;

#[allow(unused)]
struct MenuDelegate;

#[allow(unused)]
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

    #[allow(unused)]
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
    // TODO: Reorder arguments to match `Vec::insert`?
    #[allow(unused)]
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

    #[allow(unused)]
    pub fn add(&mut self, item: Id<NSMenuItem, Owned>) -> Id<NSMenuItem, Shared> {
        // Same safety concerns as above
        let _: () = unsafe { msg_send![self, addItem: &*item] };
        // The item is now shared, so it's no longer safe to hold a mutable pointer to it
        item.into()
    }

    // There exists `addItemWithTitle_action_keyEquivalent`

    // Can't use this yet, we need to find a way to let users have references to menu items safely!
    // fn remove(&mut self, item: &mut NSMenuItem) {
    //     unsafe { msg_send![self, removeItem: item] }
    // }
    // fn remove_at_index(&mut self, at: isize) {
    //     unimplemented!()
    // }

    /// Does not post notifications.
    #[allow(unused)]
    pub fn remove_all(&mut self) {
        // SAFETY: Reference is valid
        unsafe { msg_send![self, removeAllItems] }
    }

    // Finding items

    #[allow(unused)]
    fn find_by_tag<'p>(
        &self,
        _pool: &'p AutoreleasePool,
        _tag: isize,
    ) -> Option<&'p NSMenuItem> {
        unimplemented!()
    }

    #[allow(unused)]
    fn find_by_title<'p>(
        &self,
        _pool: &'p AutoreleasePool,
        _title: &str,
    ) -> Option<&'p NSMenuItem> {
        unimplemented!()
    }

    #[allow(unused)]
    unsafe fn get_at_index<'p>(
        &self,
        _pool: &'p AutoreleasePool,
        _at: isize,
    ) -> &'p NSMenuItem {
        unimplemented!()
    }

    // Getting all items

    /// Number of items in this menu, including separators
    #[allow(unused)]
    pub fn len(&self) -> usize {
        let number_of_items: NSInteger = unsafe { msg_send![self, numberOfItems] };
        number_of_items as usize
    }

    #[inline]
    #[allow(unused)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[allow(unused)]
    fn get_all_items<'p>(&self, _pool: &'p AutoreleasePool) -> &'p [&'p NSMenuItem] {
        unimplemented!()
    }

    #[allow(unused)]
    pub fn iter<'p>(
        &self,
        _pool: &'p AutoreleasePool,
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

    #[allow(unused)]
    fn index_of(&self, _item: &NSMenuItem) -> Option<isize> {
        unimplemented!()
    }

    #[allow(unused)]
    fn index_of_by_title(&self, _title: &str) -> Option<isize> {
        unimplemented!()
    }

    #[allow(unused)]
    fn index_of_by_tag(&self, _tag: isize) -> Option<isize> {
        unimplemented!()
    }

    // fn index_of_by_action_and_target(&self, ...) -> isize {}
    // fn index_of_item_by_represented_object(&self, ...) -> isize {}

    #[allow(unused)]
    fn index_of_submenu(&self, _submenu: &NSMenu) -> Option<isize> {
        unimplemented!()
    }

    // Managing submenus

    // Unsure about this!
    #[allow(unused)]
    fn set_submenu(&self, _submenu: &mut NSMenu, _for_item: &mut NSMenuItem) {
        unimplemented!()
    }

    // fn submenuAction(&self) {} // Overridable!

    #[allow(unused)]
    fn get_parent<'p>(&self, _pool: &'p AutoreleasePool) -> Option<&'p NSMenu> {
        unimplemented!()
    }

    // Has more deprecated methods!

    // Enable/disable items

    /// Default on
    #[allow(unused)]
    fn autoenables_items(&self) -> bool {
        unimplemented!()
    }

    #[allow(unused)]
    fn set_autoenables_items(&mut self, _state: bool) {
        unimplemented!()
    }

    #[allow(unused)]
    fn update_enabled_state_of_items(&self) {
        unimplemented!()
    }

    // Control fonts for this and subitems

    // fn font() -> Font {}

    // fn set_font(&mut self, font: Font) {}

    // Handling keyboard events

    // fn perform_key_equivalent(&self, event: KeyEvent) -> bool {}

    // Simulating mouse clicks

    // fn perform_action_for_item_at(&self, index: isize) {}

    // Size

    #[allow(unused)]
    fn min_width(&self) -> Option<f64> {
        // None / zero when not set
        unimplemented!()
    }

    #[allow(unused)]
    fn set_min_width(&mut self, _width: Option<f64>) {
        // None ~= zero
        unimplemented!()
    }

    #[allow(unused)]
    fn size(&self) -> USize {
        unimplemented!()
    }

    #[allow(unused)]
    fn set_size(&mut self, _size: USize) {
        // Might change the size if too big (or small?)
        unimplemented!()
    }

    // propertiesToUpdate - for efficiency when updating items

    #[allow(unused)]
    fn allows_context_menu_plug_ins(&self) -> bool {
        unimplemented!()
    }

    #[allow(unused)]
    fn set_allows_context_menu_plug_ins(&mut self, _state: bool) {
        unimplemented!()
    }

    // fn displayPopUpContextMenu(&mut self, event: Event, view: Option<&View>) {}
    // fn displayPopUpContextMenuWithFont(&mut self, event: Event, view: Option<&View>, font: Font) {}
    // fn displayPopUpAtMenuPositioningItem(&mut self, position_item: Option<&NSMenuItem>, event: Event, view: Option<&View>)

    // Whether the menu displays the state column (the "Checkmark" column for items?)
    #[allow(unused)]
    fn show_state_column(&self) -> bool {
        unimplemented!()
    }

    #[allow(unused)]
    fn set_show_state_column(&mut self, _show: bool) {
        unimplemented!()
    }

    #[allow(unused)]
    fn currently_highlighted_item<'p>(
        &self,
        _pool: &'p AutoreleasePool,
    ) -> Option<&'p NSMenuItem> {
        unimplemented!()
    }

    // Should honestly probably not be changed! (userInterfaceLayoutDirection)
    // fn layout_direction() {}
    // fn set_layout_direction() {}

    // You can use the delegate to populate a menu just before it is drawn
    // and to check for key equivalents without creating a menu item.
    #[allow(unused)]
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

    #[allow(unused)]
    fn set_delegate(&mut self, _delegate: &mut MenuDelegate) {
        unimplemented!()
    }

    // Handling tracking? Perhaps just means closing/dismissing the menu?

    #[allow(unused)]
    fn cancel_tracking(&mut self) {
        unimplemented!()
    }

    #[allow(unused)]
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

    #[test]
    fn test_title() {
        autoreleasepool(|pool| {
            let strings: [&str; 5] = [
                "",
                "🤖",
                "test",
                "abcαβγ",
                "ศไทย中华Việt Nam β-release 🐱123",
            ];
            let mut menu = NSMenu::new();
            assert_eq!(menu.title(pool), "");
            strings.iter().for_each(|&title| {
                menu.set_title(title);
                assert_eq!(menu.title(pool), title);
            });
        });
    }

    #[test]
    fn test_title_init() {
        let strings: [&str; 5] = [
            "",
            "🤖",
            "test",
            "abcαβγ",
            "ศไทย中华Việt Nam β-release 🐱123",
        ];
        autoreleasepool(|pool| {
            strings.iter().for_each(|&title| {
                let menu = NSMenu::new_with_title(title);
                assert_eq!(menu.title(pool), title);
            });
        });
    }

    #[test]
    fn test_length() {
        autoreleasepool(|_pool| {
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
