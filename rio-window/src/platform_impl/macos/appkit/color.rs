use icrate::Foundation::NSObject;
use objc2::rc::Id;
use objc2::{extern_class, extern_methods, mutability, ClassType};

extern_class!(
    /// An object that stores color data and sometimes opacity (alpha value).
    ///
    /// <https://developer.apple.com/documentation/appkit/nscolor?language=objc>
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct NSColor;

    unsafe impl ClassType for NSColor {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
    }
);

// SAFETY: Documentation clearly states:
// > Color objects are immutable and thread-safe
unsafe impl Send for NSColor {}
unsafe impl Sync for NSColor {}

extern_methods!(
    unsafe impl NSColor {
        #[method_id(clearColor)]
        pub fn clear() -> Id<Self>;
    }
);
