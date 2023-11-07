use icrate::ns_string;
use icrate::Foundation::{CGFloat, NSArray, NSDictionary, NSNumber, NSObject, NSRect, NSString};
use objc2::rc::Id;
use objc2::runtime::AnyObject;
use objc2::{extern_class, extern_methods, mutability, ClassType};

extern_class!(
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct NSScreen;

    unsafe impl ClassType for NSScreen {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
    }
);

// TODO: Main thread marker!

extern_methods!(
    unsafe impl NSScreen {
        /// The application object must have been created.
        #[method_id(mainScreen)]
        pub fn main() -> Option<Id<Self>>;

        /// The application object must have been created.
        #[method_id(screens)]
        pub fn screens() -> Id<NSArray<Self>>;

        #[method(frame)]
        pub fn frame(&self) -> NSRect;

        #[method(visibleFrame)]
        pub fn visibleFrame(&self) -> NSRect;

        #[method_id(deviceDescription)]
        pub fn deviceDescription(&self) -> Id<NSDictionary<NSDeviceDescriptionKey, AnyObject>>;

        pub fn display_id(&self) -> u32 {
            let key = ns_string!("NSScreenNumber");

            objc2::rc::autoreleasepool(|_| {
                let device_description = self.deviceDescription();

                // Retrieve the CGDirectDisplayID associated with this screen
                //
                // SAFETY: The value from @"NSScreenNumber" in deviceDescription is guaranteed
                // to be an NSNumber. See documentation for `deviceDescription` for details:
                // <https://developer.apple.com/documentation/appkit/nsscreen/1388360-devicedescription?language=objc>
                let obj = device_description
                    .get(key)
                    .expect("failed getting screen display id from device description");
                let obj: *const AnyObject = obj;
                let obj: *const NSNumber = obj.cast();
                let obj: &NSNumber = unsafe { &*obj };

                obj.as_u32()
            })
        }

        #[method(backingScaleFactor)]
        pub fn backingScaleFactor(&self) -> CGFloat;
    }
);

pub type NSDeviceDescriptionKey = NSString;
