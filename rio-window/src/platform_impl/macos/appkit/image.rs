use icrate::Foundation::{NSData, NSObject, NSString};
use objc2::rc::Id;
use objc2::{extern_class, extern_methods, msg_send_id, mutability, ClassType};

extern_class!(
    // TODO: Can this be mutable?
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct NSImage;

    unsafe impl ClassType for NSImage {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
    }
);

// Documented Thread-Unsafe, but:
// > One thread can create an NSImage object, draw to the image buffer,
// > and pass it off to the main thread for drawing. The underlying image
// > cache is shared among all threads.
// <https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/Multithreading/ThreadSafetySummary/ThreadSafetySummary.html#//apple_ref/doc/uid/10000057i-CH12-126728>
//
// So really only unsafe to mutate on several threads.
unsafe impl Send for NSImage {}
unsafe impl Sync for NSImage {}

extern_methods!(
    unsafe impl NSImage {
        pub fn new_by_referencing_file(path: &NSString) -> Id<Self> {
            unsafe { msg_send_id![Self::alloc(), initByReferencingFile: path] }
        }

        pub fn new_with_data(data: &NSData) -> Id<Self> {
            unsafe { msg_send_id![Self::alloc(), initWithData: data] }
        }
    }
);
