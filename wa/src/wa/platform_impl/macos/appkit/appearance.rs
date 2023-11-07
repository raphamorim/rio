use icrate::Foundation::{NSArray, NSObject, NSString};
use objc2::rc::Id;
use objc2::{extern_class, extern_methods, mutability, ClassType};

extern_class!(
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct NSAppearance;

    unsafe impl ClassType for NSAppearance {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
    }
);

type NSAppearanceName = NSString;

extern_methods!(
    unsafe impl NSAppearance {
        #[method_id(appearanceNamed:)]
        pub fn appearanceNamed(name: &NSAppearanceName) -> Id<Self>;

        #[method_id(bestMatchFromAppearancesWithNames:)]
        pub fn bestMatchFromAppearancesWithNames(
            &self,
            appearances: &NSArray<NSAppearanceName>,
        ) -> Id<NSAppearanceName>;
    }
);
