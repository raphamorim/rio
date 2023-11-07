// WA is a fork of https://github.com/rust-windowing/wa/
// wa is is licensed under Apache 2.0 license https://github.com/rust-windowing/wa/blob/master/LICENSE

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
