use icrate::Foundation::{NSObject, NSString};
use objc2::rc::Id;
use objc2::{extern_class, extern_methods, mutability, ClassType};

extern_class!(
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct NSPasteboard;

    unsafe impl ClassType for NSPasteboard {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
    }
);

extern_methods!(
    unsafe impl NSPasteboard {
        #[method_id(propertyListForType:)]
        pub fn propertyListForType(&self, type_: &NSPasteboardType) -> Id<NSObject>;
    }
);

pub type NSPasteboardType = NSString;

extern "C" {
    pub static NSFilenamesPboardType: &'static NSPasteboardType;
}
