// WA is a fork of https://github.com/rust-windowing/wa/
// wa is is licensed under Apache 2.0 license https://github.com/rust-windowing/wa/blob/master/LICENSE

use icrate::Foundation::NSObject;
use objc2::rc::Id;
use objc2::{extern_class, extern_methods, mutability, ClassType};

use super::NSMenuItem;

extern_class!(
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct NSMenu;

    unsafe impl ClassType for NSMenu {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
    }
);

extern_methods!(
    unsafe impl NSMenu {
        #[method_id(new)]
        pub fn new() -> Id<Self>;

        #[method(addItem:)]
        pub fn addItem(&self, item: &NSMenuItem);
    }
);
