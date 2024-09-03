use icrate::Foundation::NSObject;
use objc2::{extern_class, extern_methods, mutability, ClassType};

use super::{NSResponder, NSView};

extern_class!(
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct NSControl;

    unsafe impl ClassType for NSControl {
        #[inherits(NSResponder, NSObject)]
        type Super = NSView;
        type Mutability = mutability::InteriorMutable;
    }
);

extern_methods!(
    unsafe impl NSControl {
        #[method(setEnabled:)]
        pub fn setEnabled(&self, enabled: bool);

        #[method(isEnabled)]
        pub fn isEnabled(&self) -> bool;
    }
);
