use icrate::Foundation::{NSObject, NSString};
use objc2::rc::Id;
use objc2::{extern_class, extern_methods, mutability, ClassType};

type NSTextInputSourceIdentifier = NSString;

extern_class!(
    /// Main-Thread-Only!
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct NSTextInputContext;

    unsafe impl ClassType for NSTextInputContext {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
    }
);

extern_methods!(
    unsafe impl NSTextInputContext {
        #[method(invalidateCharacterCoordinates)]
        pub fn invalidateCharacterCoordinates(&self);

        #[method(discardMarkedText)]
        pub fn discardMarkedText(&self);

        #[method_id(selectedKeyboardInputSource)]
        pub fn selectedKeyboardInputSource(&self) -> Option<Id<NSTextInputSourceIdentifier>>;
    }
);
