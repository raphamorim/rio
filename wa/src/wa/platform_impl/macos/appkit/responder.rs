use icrate::Foundation::{NSArray, NSObject};
use objc2::{extern_class, extern_methods, mutability, ClassType};

use super::NSEvent;

extern_class!(
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub struct NSResponder;

    unsafe impl ClassType for NSResponder {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
    }
);

// Documented as "Thread-Unsafe".

extern_methods!(
    unsafe impl NSResponder {
        #[method(interpretKeyEvents:)]
        pub(crate) unsafe fn interpretKeyEvents(&self, events: &NSArray<NSEvent>);
    }
);
