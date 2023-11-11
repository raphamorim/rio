// WA is a fork of https://github.com/rust-windowing/wa/
// wa is is licensed under Apache 2.0 license https://github.com/rust-windowing/wa/blob/master/LICENSE

use icrate::Foundation::NSObject;
use objc2::{extern_class, mutability, ClassType};

use super::{NSControl, NSResponder, NSView};

extern_class!(
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct NSButton;

    unsafe impl ClassType for NSButton {
        #[inherits(NSView, NSResponder, NSObject)]
        type Super = NSControl;
        type Mutability = mutability::InteriorMutable;
    }
);
