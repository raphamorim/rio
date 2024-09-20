// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Originally retired from wez/wezterm menu.rs
// Ref: https://github.com/wez/wezterm/blob/84ae00c868e711cf97b2bfe885892428f1131a1d/window/src/os/macos/menu.rs
// The code has suffered changed like removal of cocoa and ported to usage of apple_utils.rs

use objc::class;
use objc::sel;
use objc::sel_impl;
use objc::msg_send;

pub type ObjcId = *mut Object;

#[derive(Clone, Debug, PartialEq)]
pub enum KeyAssignment {
    SpawnWindow,
}

pub const UTF8_ENCODING: usize = 4;

pub fn str_to_nsstring(str: &str) -> ObjcId {
    unsafe {
        let ns_string: ObjcId = msg_send![class!(NSString), alloc];
        let ns_string: ObjcId = msg_send![
            ns_string,
            initWithBytes: str.as_ptr()
            length: str.len()
            encoding: UTF8_ENCODING as ObjcId
        ];
        let _: () = msg_send![ns_string, autorelease];
        ns_string
    }
}

pub fn nsstring_to_string(string: ObjcId) -> String {
    unsafe {
        let utf8_string: *const std::os::raw::c_uchar = msg_send![string, UTF8String];
        let utf8_len: usize =
            msg_send![string, lengthOfBytesUsingEncoding: UTF8_ENCODING];
        let slice = std::slice::from_raw_parts(utf8_string, utf8_len);
        std::str::from_utf8_unchecked(slice).to_owned()
    }
}

#[cfg(target_pointer_width = "32")]
pub type NSInteger = libc::c_int;
#[cfg(target_pointer_width = "32")]
pub type NSUInteger = libc::c_uint;

#[cfg(target_pointer_width = "64")]
pub type NSInteger = libc::c_long;
#[cfg(target_pointer_width = "64")]
pub type NSUInteger = libc::c_ulong;

use objc::declare::ClassDecl;
use objc::rc::StrongPtr;
use objc::runtime::{Class, Object, Sel, BOOL, NO, YES};
use std::ffi::c_void;

pub type SEL = Sel;

pub struct Menu {
    menu: StrongPtr,
}

bitflags::bitflags! {
    pub struct NSEventModifierFlags: NSUInteger {
        const NSAlphaShiftKeyMask                     = 1 << 16;
        const NSShiftKeyMask                          = 1 << 17;
        const NSControlKeyMask                        = 1 << 18;
        const NSAlternateKeyMask                      = 1 << 19;
        const NSCommandKeyMask                        = 1 << 20;
        const NSNumericPadKeyMask                     = 1 << 21;
        const NSHelpKeyMask                           = 1 << 22;
        const NSFunctionKeyMask                       = 1 << 23;
        const NSDeviceIndependentModifierFlagsMask    = 0xffff0000;
    }
}

impl Menu {
    pub fn new_with_title(title: &str) -> Self {
        unsafe {
            let menu: ObjcId = msg_send![class!(NSMenu), alloc];
            let menu: ObjcId = msg_send![menu, initWithTitle: str_to_nsstring(title)];
            let menu = StrongPtr::new(menu);
            Self { menu }
        }
    }

    pub fn autorelease(self) -> *mut Object {
        self.menu.autorelease()
    }

    pub fn item_at_index(&self, index: usize) -> Option<MenuItem> {
        let index = index as NSInteger;
        let item: ObjcId = unsafe { msg_send![*self.menu, itemAtIndex: index] };
        if item.is_null() {
            None
        } else {
            Some(MenuItem {
                item: unsafe { StrongPtr::retain(item) },
            })
        }
    }

    pub fn add_item(&self, item: &MenuItem) {
        unsafe {
            let () = msg_send![*self.menu, addItem: *item.item];
        }
    }

    pub fn item_with_title(&self, title: &str) -> Option<MenuItem> {
        unsafe {
            let item: ObjcId =
                msg_send![*self.menu, itemWithTitle: str_to_nsstring(title)];
            if item.is_null() {
                None
            } else {
                Some(MenuItem {
                    item: StrongPtr::retain(item),
                })
            }
        }
    }

    pub fn get_or_create_sub_menu<F: FnOnce(&Menu)>(
        &self,
        title: &str,
        on_create: F,
    ) -> Menu {
        match self.item_with_title(title) {
            Some(m) => m.get_sub_menu().unwrap(),
            None => {
                let item = MenuItem::new_with(title, None, "");
                let menu = Menu::new_with_title(title);
                item.set_sub_menu(&menu);
                self.add_item(&item);
                on_create(&menu);
                menu
            }
        }
    }

    pub fn get_sub_menu(&self, title: &str) -> Menu {
        self.item_with_title(title).unwrap().get_sub_menu().unwrap()
    }

    pub fn remove_all_items(&self) {
        unsafe {
            let () = msg_send![*self.menu, removeAllItems];
        }
    }

    pub fn remove_item(&self, item: &MenuItem) {
        unsafe {
            let () = msg_send![*self.menu, removeItem:*item.item];
        }
    }

    pub fn items(&self) -> Vec<MenuItem> {
        unsafe {
            let n: NSInteger = msg_send![*self.menu, numberOfItems];
            let mut items = vec![];
            for i in 0..n {
                items.push(self.item_at_index(i as _).expect("index to be valid"));
            }
            items
        }
    }

    pub fn index_of_item_with_represented_object(&self, object: ObjcId) -> Option<usize> {
        unsafe {
            let n: NSInteger =
                msg_send![*self.menu, indexOfItemWithRepresentedObject: object];
            if n == -1 {
                None
            } else {
                Some(n as usize)
            }
        }
    }

    pub fn index_of_item_with_represented_item(
        &self,
        item: &RepresentedItem,
    ) -> Option<usize> {
        let wrapped = item.clone().wrap();
        self.index_of_item_with_represented_object(*wrapped)
    }

    pub fn get_item_with_represented_item(
        &self,
        item: &RepresentedItem,
    ) -> Option<MenuItem> {
        let idx = self.index_of_item_with_represented_item(item)?;
        self.item_at_index(idx)
    }
}

pub struct MenuItem {
    item: StrongPtr,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RepresentedItem {
    KeyAssignment(KeyAssignment),
}

#[cfg(target_vendor = "apple")]
unsafe impl Send for RepresentedItem {}
#[cfg(target_vendor = "apple")]
unsafe impl Sync for RepresentedItem {}

impl RepresentedItem {
    fn wrap(self) -> StrongPtr {
        let wrapper: ObjcId = unsafe { msg_send![get_wrapper_class(), alloc] };
        let wrapper = unsafe { StrongPtr::new(wrapper) };
        let item = Box::new(self);
        let item: *const RepresentedItem = Box::into_raw(item);
        let item = item as *const c_void;
        unsafe {
            (**wrapper).set_ivar(WRAPPER_FIELD_NAME, item);
        }
        wrapper
    }

    fn ref_item(wrapper: ObjcId) -> Option<RepresentedItem> { unsafe {
        let item = (*wrapper).get_ivar::<*const c_void>(WRAPPER_FIELD_NAME);
        let item = (*item) as *const RepresentedItem;
        if item.is_null() {
            None
        } else {
            Some((*item).clone())
        }
    }}
}

impl MenuItem {
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn with_menu_item(item: ObjcId) -> Self {
        let item = unsafe { StrongPtr::retain(item) };
        Self { item }
    }

    pub fn new_separator() -> Self {
        unsafe {
            let menu_item_separator: ObjcId =
                msg_send![class!(NSMenuItem), separatorItem];
            let item = StrongPtr::new(menu_item_separator);
            Self { item }
        }
    }

    pub fn new_with(title: &str, action: Option<SEL>, key: &str) -> Self {
        unsafe {
            let item: ObjcId = msg_send![class!(NSMenuItem), alloc];
            let title = str_to_nsstring(title);
            let action = action.unwrap_or_else(|| SEL::from_ptr(std::ptr::null()));
            let key = str_to_nsstring(key);
            let item: ObjcId =
                msg_send![item, initWithTitle:title action:action keyEquivalent:key];

            Self {
                item: StrongPtr::new(item),
            }
        }
    }

    pub fn get_action(&self) -> Option<SEL> {
        unsafe {
            let s: SEL = msg_send![*self.item, action];
            if s.as_ptr().is_null() {
                None
            } else {
                Some(s)
            }
        }
    }

    pub fn set_tool_tip(&self, tip: &str) {
        unsafe {
            let () = msg_send![*self.item, setToolTip:str_to_nsstring(tip)];
        }
    }

    pub fn set_target(&self, target: ObjcId) {
        unsafe {
            let () = msg_send![*self.item, setTarget: target];
        }
    }

    pub fn set_sub_menu(&self, menu: &Menu) {
        unsafe {
            let () = msg_send![*self.item, setSubmenu: *menu.menu];
        }
    }

    pub fn get_sub_menu(&self) -> Option<Menu> {
        unsafe {
            let menu: ObjcId = msg_send![*self.item, submenu];
            if menu.is_null() {
                None
            } else {
                Some(Menu {
                    menu: StrongPtr::retain(menu),
                })
            }
        }
    }

    pub fn get_parent_item(&self) -> Option<Self> {
        unsafe {
            let item: ObjcId = msg_send![*self.item, parentItem];
            if item.is_null() {
                None
            } else {
                Some(Self {
                    item: StrongPtr::retain(item),
                })
            }
        }
    }

    pub fn get_menu(&self) -> Option<Menu> {
        unsafe {
            let item: ObjcId = msg_send![*self.item, menu];
            if item.is_null() {
                None
            } else {
                Some(Menu {
                    menu: StrongPtr::retain(item),
                })
            }
        }
    }

    /// Set an integer tag to identify this item
    pub fn set_tag(&self, tag: NSInteger) {
        unsafe {
            let () = msg_send![*self.item, setTag: tag];
        }
    }

    pub fn get_title(&self) -> String {
        unsafe {
            let title: ObjcId = msg_send![*self.item, title];
            nsstring_to_string(title).to_string()
        }
    }

    pub fn set_title(&self, title: &str) {
        unsafe {
            let () = msg_send![*self.item, setTitle:str_to_nsstring(title)];
        }
    }

    pub fn set_key_equivalent(&self, equiv: &str) {
        unsafe {
            let () = msg_send![*self.item, setKeyEquivalent:str_to_nsstring(equiv)];
        }
    }

    pub fn get_tag(&self) -> NSInteger {
        unsafe { msg_send![*self.item, tag] }
    }

    /// Associate the item to an object
    fn set_represented_object(&self, object: ObjcId) {
        unsafe {
            let () = msg_send![*self.item, setRepresentedObject: object];
        }
    }

    fn get_represented_object(&self) -> Option<StrongPtr> {
        unsafe {
            let object: ObjcId = msg_send![*self.item, representedObject];
            if object.is_null() {
                None
            } else {
                Some(StrongPtr::retain(object))
            }
        }
    }

    pub fn set_represented_item(&self, item: RepresentedItem) {
        let wrapper = item.wrap();
        self.set_represented_object(*wrapper);
    }

    pub fn get_represented_item(&self) -> Option<RepresentedItem> {
        let wrapper = self.get_represented_object()?;
        RepresentedItem::ref_item(*wrapper)
    }

    pub fn set_key_equiv_modifier_mask(&self, mods: NSEventModifierFlags) {
        unsafe {
            let () = msg_send![*self.item, setKeyEquivalentModifierMask: mods];
        }
    }
}

const WRAPPER_CLS_NAME: &str = "WaRepresentedItem";
const WRAPPER_FIELD_NAME: &str = "item";
/// Wraps RepresentedItem in an NSObject so that we can associate
/// it with a MenuItem
fn get_wrapper_class() -> &'static Class {
    Class::get(WRAPPER_CLS_NAME).unwrap_or_else(|| {
        let mut cls = ClassDecl::new(WRAPPER_CLS_NAME, class!(NSObject))
            .expect("Unable to register class");

        extern "C" fn dealloc(this: &mut Object, _sel: Sel) {
            unsafe {
                let item = this.get_ivar::<*mut c_void>(WRAPPER_FIELD_NAME);
                let item = (*item) as *mut RepresentedItem;
                let item = Box::from_raw(item);
                drop(item);
                let superclass = class!(NSObject);
                let () = msg_send![super(this, superclass), dealloc];
            }
        }

        extern "C" fn is_equal(this: &mut Object, _sel: Sel, that: *mut Object) -> BOOL {
            let this_item = RepresentedItem::ref_item(this);
            let that_item = RepresentedItem::ref_item(that);
            if this_item == that_item {
                YES
            } else {
                NO
            }
        }

        cls.add_ivar::<*mut c_void>(WRAPPER_FIELD_NAME);
        unsafe {
            cls.add_method(sel!(dealloc), dealloc as extern "C" fn(&mut Object, Sel));
            cls.add_method(
                sel!(isEqual:),
                is_equal as extern "C" fn(&mut Object, Sel, *mut Object) -> BOOL,
            );
        }
        cls.register()
    })
}
