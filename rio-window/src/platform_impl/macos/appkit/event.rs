use std::os::raw::c_ushort;

use icrate::Foundation::{
    CGFloat, NSCopying, NSInteger, NSObject, NSPoint, NSString, NSTimeInterval, NSUInteger,
};
use objc2::encode::{Encode, Encoding};
use objc2::rc::Id;
use objc2::{extern_class, extern_methods, mutability, ClassType};

extern_class!(
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct NSEvent;

    unsafe impl ClassType for NSEvent {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
    }
);

// > Safely handled only on the same thread, whether that be the main thread
// > or a secondary thread; otherwise you run the risk of having events get
// > out of sequence.
// <https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/CocoaFundamentals/AddingBehaviortoaCocoaProgram/AddingBehaviorCocoa.html#//apple_ref/doc/uid/TP40002974-CH5-SW47>
// <https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/Multithreading/ThreadSafetySummary/ThreadSafetySummary.html#//apple_ref/doc/uid/10000057i-CH12-123383>

extern_methods!(
    unsafe impl NSEvent {
        #[method_id(
            otherEventWithType:
            location:
            modifierFlags:
            timestamp:
            windowNumber:
            context:
            subtype:
            data1:
            data2:
        )]
        unsafe fn otherEventWithType(
            type_: NSEventType,
            location: NSPoint,
            flags: NSEventModifierFlags,
            time: NSTimeInterval,
            window_num: NSInteger,
            context: Option<&NSObject>, // NSGraphicsContext
            subtype: NSEventSubtype,
            data1: NSInteger,
            data2: NSInteger,
        ) -> Id<Self>;

        pub fn dummy() -> Id<Self> {
            unsafe {
                Self::otherEventWithType(
                    NSEventType::NSApplicationDefined,
                    NSPoint::new(0.0, 0.0),
                    NSEventModifierFlags::empty(),
                    0.0,
                    0,
                    None,
                    NSEventSubtype::NSWindowExposedEventType,
                    0,
                    0,
                )
            }
        }

        #[method_id(
            keyEventWithType:
            location:
            modifierFlags:
            timestamp:
            windowNumber:
            context:
            characters:
            charactersIgnoringModifiers:
            isARepeat:
            keyCode:
        )]
        pub fn keyEventWithType(
            type_: NSEventType,
            location: NSPoint,
            modifier_flags: NSEventModifierFlags,
            timestamp: NSTimeInterval,
            window_num: NSInteger,
            context: Option<&NSObject>,
            characters: &NSString,
            characters_ignoring_modifiers: &NSString,
            is_a_repeat: bool,
            scancode: c_ushort,
        ) -> Id<Self>;

        #[method(locationInWindow)]
        pub fn locationInWindow(&self) -> NSPoint;

        // TODO: MainThreadMarker
        #[method(pressedMouseButtons)]
        pub fn pressedMouseButtons() -> NSUInteger;

        #[method(modifierFlags)]
        pub fn modifierFlags(&self) -> NSEventModifierFlags;

        #[method(type)]
        pub fn type_(&self) -> NSEventType;

        #[method(keyCode)]
        pub fn key_code(&self) -> c_ushort;

        #[method(magnification)]
        pub fn magnification(&self) -> CGFloat;

        #[method(phase)]
        pub fn phase(&self) -> NSEventPhase;

        #[method(momentumPhase)]
        pub fn momentumPhase(&self) -> NSEventPhase;

        #[method(deltaX)]
        pub fn deltaX(&self) -> CGFloat;

        #[method(deltaY)]
        pub fn deltaY(&self) -> CGFloat;

        #[method(buttonNumber)]
        pub fn buttonNumber(&self) -> NSInteger;

        #[method(scrollingDeltaX)]
        pub fn scrollingDeltaX(&self) -> CGFloat;

        #[method(scrollingDeltaY)]
        pub fn scrollingDeltaY(&self) -> CGFloat;

        #[method(hasPreciseScrollingDeltas)]
        pub fn hasPreciseScrollingDeltas(&self) -> bool;

        #[method(rotation)]
        pub fn rotation(&self) -> f32;

        #[method(pressure)]
        pub fn pressure(&self) -> f32;

        #[method(stage)]
        pub fn stage(&self) -> NSInteger;

        #[method(isARepeat)]
        pub fn is_a_repeat(&self) -> bool;

        #[method(windowNumber)]
        pub fn window_number(&self) -> NSInteger;

        #[method(timestamp)]
        pub fn timestamp(&self) -> NSTimeInterval;

        #[method_id(characters)]
        pub fn characters(&self) -> Option<Id<NSString>>;

        #[method_id(charactersIgnoringModifiers)]
        pub fn charactersIgnoringModifiers(&self) -> Option<Id<NSString>>;

        pub fn lshift_pressed(&self) -> bool {
            let raw_modifiers = self.modifierFlags().bits() as u32;
            raw_modifiers & NX_DEVICELSHIFTKEYMASK != 0
        }

        pub fn rshift_pressed(&self) -> bool {
            let raw_modifiers = self.modifierFlags().bits() as u32;
            raw_modifiers & NX_DEVICERSHIFTKEYMASK != 0
        }

        pub fn lctrl_pressed(&self) -> bool {
            let raw_modifiers = self.modifierFlags().bits() as u32;
            raw_modifiers & NX_DEVICELCTLKEYMASK != 0
        }

        pub fn rctrl_pressed(&self) -> bool {
            let raw_modifiers = self.modifierFlags().bits() as u32;
            raw_modifiers & NX_DEVICERCTLKEYMASK != 0
        }

        pub fn lalt_pressed(&self) -> bool {
            let raw_modifiers = self.modifierFlags().bits() as u32;
            raw_modifiers & NX_DEVICELALTKEYMASK != 0
        }

        pub fn ralt_pressed(&self) -> bool {
            let raw_modifiers = self.modifierFlags().bits() as u32;
            raw_modifiers & NX_DEVICERALTKEYMASK != 0
        }

        pub fn lcmd_pressed(&self) -> bool {
            let raw_modifiers = self.modifierFlags().bits() as u32;
            raw_modifiers & NX_DEVICELCMDKEYMASK != 0
        }

        pub fn rcmd_pressed(&self) -> bool {
            let raw_modifiers = self.modifierFlags().bits() as u32;
            raw_modifiers & NX_DEVICERCMDKEYMASK != 0
        }
    }
);

unsafe impl NSCopying for NSEvent {}

// The values are from the https://github.com/apple-oss-distributions/IOHIDFamily/blob/19666c840a6d896468416ff0007040a10b7b46b8/IOHIDSystem/IOKit/hidsystem/IOLLEvent.h#L258-L259
const NX_DEVICELCTLKEYMASK: u32 = 0x00000001;
const NX_DEVICELSHIFTKEYMASK: u32 = 0x00000002;
const NX_DEVICERSHIFTKEYMASK: u32 = 0x00000004;
const NX_DEVICELCMDKEYMASK: u32 = 0x00000008;
const NX_DEVICERCMDKEYMASK: u32 = 0x00000010;
const NX_DEVICELALTKEYMASK: u32 = 0x00000020;
const NX_DEVICERALTKEYMASK: u32 = 0x00000040;
const NX_DEVICERCTLKEYMASK: u32 = 0x00002000;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct NSEventModifierFlags: NSUInteger {
        const NSAlphaShiftKeyMask                     = 1 << 16;
        const NSShiftKeyMask                          = 1 << 17;
        const NSControlKeyMask                        = 1 << 18;
        const NSAlternateKeyMask                      = 1 << 19;
        const NSCommandKeyMask                        = 1 << 20;
        const NSNumericPadKeyMask                     = 1 << 21;
        const NSHelpKeyMask                           = 1 << 22;
        const NSFunctionKeyMask                       = 1 << 23;
        const NSDeviceIndependentModifierFlagsMask    = 0xffff0000;
    }
}

unsafe impl Encode for NSEventModifierFlags {
    const ENCODING: Encoding = NSUInteger::ENCODING;
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct NSEventPhase: NSUInteger {
       const NSEventPhaseNone        = 0;
       const NSEventPhaseBegan       = 0x1 << 0;
       const NSEventPhaseStationary  = 0x1 << 1;
       const NSEventPhaseChanged     = 0x1 << 2;
       const NSEventPhaseEnded       = 0x1 << 3;
       const NSEventPhaseCancelled   = 0x1 << 4;
       const NSEventPhaseMayBegin    = 0x1 << 5;
    }
}

unsafe impl Encode for NSEventPhase {
    const ENCODING: Encoding = NSUInteger::ENCODING;
}

#[allow(dead_code)]
#[repr(i16)] // short
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NSEventSubtype {
    // TODO: Not sure what these values are
    // NSMouseEventSubtype           = NX_SUBTYPE_DEFAULT,
    // NSTabletPointEventSubtype     = NX_SUBTYPE_TABLET_POINT,
    // NSTabletProximityEventSubtype = NX_SUBTYPE_TABLET_PROXIMITY
    // NSTouchEventSubtype           = NX_SUBTYPE_MOUSE_TOUCH,
    NSWindowExposedEventType = 0,
    NSApplicationActivatedEventType = 1,
    NSApplicationDeactivatedEventType = 2,
    NSWindowMovedEventType = 4,
    NSScreenChangedEventType = 8,
    NSAWTEventType = 16,
}

unsafe impl Encode for NSEventSubtype {
    const ENCODING: Encoding = i16::ENCODING;
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(usize)] // NSUInteger
pub enum NSEventType {
    NSLeftMouseDown = 1,
    NSLeftMouseUp = 2,
    NSRightMouseDown = 3,
    NSRightMouseUp = 4,
    NSMouseMoved = 5,
    NSLeftMouseDragged = 6,
    NSRightMouseDragged = 7,
    NSMouseEntered = 8,
    NSMouseExited = 9,
    NSKeyDown = 10,
    NSKeyUp = 11,
    NSFlagsChanged = 12,
    NSAppKitDefined = 13,
    NSSystemDefined = 14,
    NSApplicationDefined = 15,
    NSPeriodic = 16,
    NSCursorUpdate = 17,
    NSScrollWheel = 22,
    NSTabletPoint = 23,
    NSTabletProximity = 24,
    NSOtherMouseDown = 25,
    NSOtherMouseUp = 26,
    NSOtherMouseDragged = 27,
    NSEventTypeGesture = 29,
    NSEventTypeMagnify = 30,
    NSEventTypeSwipe = 31,
    NSEventTypeRotate = 18,
    NSEventTypeBeginGesture = 19,
    NSEventTypeEndGesture = 20,
    NSEventTypePressure = 34,
}

unsafe impl Encode for NSEventType {
    const ENCODING: Encoding = NSUInteger::ENCODING;
}
