use icrate::Foundation::{NSObject, NSString};
use objc2::rc::Id;
use objc2::runtime::Sel;
use objc2::{extern_class, extern_methods, msg_send_id, mutability, ClassType};

use super::{NSEventModifierFlags, NSMenu};

extern_class!(
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct NSMenuItem;

    unsafe impl ClassType for NSMenuItem {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
    }
);

extern_methods!(
    unsafe impl NSMenuItem {
        #[method_id(new)]
        pub fn new() -> Id<Self>;

        pub fn newWithTitle(title: &NSString, action: Sel, key_equivalent: &NSString) -> Id<Self> {
            unsafe {
                msg_send_id![
                    Self::alloc(),
                    initWithTitle: title,
                    action: action,
                    keyEquivalent: key_equivalent,
                ]
            }
        }

        #[method_id(separatorItem)]
        pub fn separatorItem() -> Id<Self>;

        #[method(setKeyEquivalentModifierMask:)]
        pub fn setKeyEquivalentModifierMask(&self, mask: NSEventModifierFlags);

        #[method(setSubmenu:)]
        pub fn setSubmenu(&self, submenu: &NSMenu);
    }
);
