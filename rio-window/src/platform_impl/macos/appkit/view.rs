use std::ffi::c_void;
use std::num::NonZeroIsize;
use std::ptr;

use icrate::Foundation::{NSObject, NSPoint, NSRect};
use objc2::rc::Id;
use objc2::runtime::AnyObject;
use objc2::{extern_class, extern_methods, mutability, ClassType};

use super::{NSCursor, NSResponder, NSTextInputContext, NSWindow};

extern_class!(
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct NSView;

    unsafe impl ClassType for NSView {
        #[inherits(NSObject)]
        type Super = NSResponder;
        type Mutability = mutability::InteriorMutable;
    }
);

// Documented as "Main Thread Only".
// > generally thread safe, although operations on views such as creating,
// > resizing, and moving should happen on the main thread.
// <https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/CocoaFundamentals/AddingBehaviortoaCocoaProgram/AddingBehaviorCocoa.html#//apple_ref/doc/uid/TP40002974-CH5-SW47>
//
// > If you want to use a thread to draw to a view, bracket all drawing code
// > between the lockFocusIfCanDraw and unlockFocus methods of NSView.
// <https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/Multithreading/ThreadSafetySummary/ThreadSafetySummary.html#//apple_ref/doc/uid/10000057i-CH12-123351-BBCFIIEB>

extern_methods!(
    /// Getter methods
    unsafe impl NSView {
        #[method(frame)]
        pub fn frame(&self) -> NSRect;

        #[method(bounds)]
        pub fn bounds(&self) -> NSRect;

        #[method_id(inputContext)]
        pub fn inputContext(
            &self,
            // _mtm: MainThreadMarker,
        ) -> Option<Id<NSTextInputContext>>;

        #[method(hasMarkedText)]
        pub fn hasMarkedText(&self) -> bool;

        #[method(convertPoint:fromView:)]
        pub fn convertPoint_fromView(&self, point: NSPoint, view: Option<&NSView>) -> NSPoint;

        #[method_id(window)]
        pub fn window(&self) -> Option<Id<NSWindow>>;
    }

    unsafe impl NSView {
        #[method(setWantsBestResolutionOpenGLSurface:)]
        pub fn setWantsBestResolutionOpenGLSurface(&self, value: bool);

        #[method(setWantsLayer:)]
        pub fn setWantsLayer(&self, wants_layer: bool);

        #[method(setPostsFrameChangedNotifications:)]
        pub fn setPostsFrameChangedNotifications(&self, value: bool);

        #[method(removeTrackingRect:)]
        pub fn removeTrackingRect(&self, tag: NSTrackingRectTag);

        #[method(addTrackingRect:owner:userData:assumeInside:)]
        unsafe fn inner_addTrackingRect(
            &self,
            rect: NSRect,
            owner: &AnyObject,
            user_data: *mut c_void,
            assume_inside: bool,
        ) -> Option<NSTrackingRectTag>;

        pub fn add_tracking_rect(&self, rect: NSRect, assume_inside: bool) -> NSTrackingRectTag {
            // SAFETY: The user data is NULL, so it is valid
            unsafe { self.inner_addTrackingRect(rect, self, ptr::null_mut(), assume_inside) }
                .expect("failed creating tracking rect")
        }

        #[method(addCursorRect:cursor:)]
        // NSCursor safe to take by shared reference since it is already immutable
        pub fn addCursorRect(&self, rect: NSRect, cursor: &NSCursor);

        #[method(setHidden:)]
        pub fn setHidden(&self, hidden: bool);
    }
);

/// <https://developer.apple.com/documentation/appkit/nstrackingrecttag?language=objc>
pub type NSTrackingRectTag = NonZeroIsize; // NSInteger, but non-zero!
