use icrate::Foundation::{NSArray, NSObject};
use objc2::rc::Id;
use objc2::{extern_class, extern_methods, mutability, ClassType};

use super::NSWindow;

extern_class!(
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct NSWindowTabGroup;

    unsafe impl ClassType for NSWindowTabGroup {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
    }
);

extern_methods!(
    unsafe impl NSWindowTabGroup {
        #[method(selectNextTab)]
        pub fn selectNextTab(&self);

        #[method(selectPreviousTab)]
        pub fn selectPreviousTab(&self);

        #[method_id(windows)]
        pub fn tabbedWindows(&self) -> Option<Id<NSArray<NSWindow>>>;

        #[method(setSelectedWindow:)]
        pub fn setSelectedWindow(&self, window: &NSWindow);
    }
);
