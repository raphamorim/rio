// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Originally retired from https://github.com/not-fl3/macroquad licensed under MIT
// https://github.com/not-fl3/macroquad/blob/master/LICENSE-MIT

// stripped mac core foundation + metal layer only whats needed

#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(clippy::all)]

pub use {
    objc::{
        class,
        declare::ClassDecl,
        msg_send,
        runtime::{Class, Object, Protocol, Sel, BOOL, NO, YES},
        sel, sel_impl, Encode, Encoding,
    },
    std::{ffi::c_void, os::raw::c_ulong, ptr::NonNull},
};

//use bitflags::bitflags;

pub type ObjcId = *mut Object;
pub const nil: ObjcId = 0 as ObjcId;

pub struct RcObjcId(NonNull<Object>);

impl RcObjcId {
    pub fn from_owned(id: NonNull<Object>) -> Self {
        Self(id)
    }

    pub fn from_unowned(id: NonNull<Object>) -> Self {
        unsafe {
            let _: () = msg_send![id.as_ptr(), retain];
        }
        Self::from_owned(id)
    }

    pub fn as_id(&self) -> ObjcId {
        self.0.as_ptr()
    }

    pub fn forget(self) -> NonNull<Object> {
        unsafe {
            let _: () = msg_send![self.0.as_ptr(), retain];
        }
        self.0
    }
}

impl Clone for RcObjcId {
    fn clone(&self) -> Self {
        Self::from_unowned(self.0)
    }
}

impl Drop for RcObjcId {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![self.0.as_ptr(), release];
        }
    }
}

#[link(name = "System")]
extern "C" {
    pub static _NSConcreteStackBlock: [*const c_void; 32];
    pub static _NSConcreteBogusBlock: [*const c_void; 32];
}

#[link(name = "Foundation", kind = "framework")]
extern "C" {
    pub static NSRunLoopCommonModes: ObjcId;
    pub static NSDefaultRunLoopMode: ObjcId;
    pub static NSProcessInfo: ObjcId;
    pub fn NSStringFromClass(class: ObjcId) -> ObjcId;

    pub fn __CFStringMakeConstantString(
        cStr: *const ::std::os::raw::c_char,
    ) -> CFStringRef;

    pub fn CFStringGetLength(theString: CFStringRef) -> u64;
    pub fn CFStringGetBytes(
        theString: CFStringRef,
        range: CFRange,
        encoding: u32,
        lossByte: u8,
        isExternalRepresentation: bool,
        buffer: *mut u8,
        maxBufLen: u64,
        usedBufLen: *mut u64,
    ) -> u64;
    pub fn NSLog(fmt: ObjcId, ...);
}

pub type CFTimeInterval = f64;
pub type CFAbsoluteTime = CFTimeInterval;

extern "C" {
    /*
     * CFRunLoop.h
     */

    pub static kCFRunLoopDefaultMode: CFStringRef;
    pub static kCFRunLoopCommonModes: CFStringRef;

    /* CFRunLoop */
    /* Getting a Run Loop */
    pub fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    pub fn CFRunLoopGetMain() -> CFRunLoopRef;
    pub fn CFAbsoluteTimeGetCurrent() -> CFAbsoluteTime;

    /* Starting and Stopping a Run Loop */
    pub fn CFRunLoopRun();
    pub fn CFRunLoopRunInMode(
        mode: CFStringRef,
        seconds: CFTimeInterval,
        returnAfterSourceHandled: BOOL,
    ) -> i32;
    pub fn CFRunLoopWakeUp(rl: CFRunLoopRef);
    pub fn CFRunLoopStop(rl: CFRunLoopRef);
    pub fn CFRunLoopIsWaiting(rl: CFRunLoopRef) -> BOOL;

    pub fn CFRelease(cftype: *const c_void);

    /* Managing Sources */
    pub fn CFRunLoopAddSource(
        rl: CFRunLoopRef,
        source: CFRunLoopSourceRef,
        mode: CFStringRef,
    );
    pub fn CFRunLoopContainsSource(
        rl: CFRunLoopRef,
        source: CFRunLoopSourceRef,
        mode: CFStringRef,
    ) -> BOOL;
    pub fn CFRunLoopRemoveSource(
        rl: CFRunLoopRef,
        source: CFRunLoopSourceRef,
        mode: CFStringRef,
    );

    /* Managing Observers */
    pub fn CFRunLoopAddObserver(
        rl: CFRunLoopRef,
        observer: CFRunLoopObserverRef,
        mode: CFStringRef,
    );
    pub fn CFRunLoopContainsObserver(
        rl: CFRunLoopRef,
        observer: CFRunLoopObserverRef,
        mode: CFStringRef,
    ) -> BOOL;
    pub fn CFRunLoopRemoveObserver(
        rl: CFRunLoopRef,
        observer: CFRunLoopObserverRef,
        mode: CFStringRef,
    );

    /* Managing Run Loop Modes */
    pub fn CFRunLoopAddCommonMode(rl: CFRunLoopRef, mode: CFStringRef);
    // pub fn CFRunLoopCopyAllModes(rl: CFRunLoopRef) -> CFArrayRef;
    pub fn CFRunLoopCopyCurrentMode(rl: CFRunLoopRef) -> CFStringRef;

    /* Managing Timers */
    pub fn CFRunLoopAddTimer(
        rl: CFRunLoopRef,
        timer: CFRunLoopTimerRef,
        mode: CFStringRef,
    );
    pub fn CFRunLoopGetNextTimerFireDate(
        rl: CFRunLoopRef,
        mode: CFStringRef,
    ) -> CFAbsoluteTime;
    pub fn CFRunLoopRemoveTimer(
        rl: CFRunLoopRef,
        timer: CFRunLoopTimerRef,
        mode: CFStringRef,
    );
    pub fn CFRunLoopContainsTimer(
        rl: CFRunLoopRef,
        timer: CFRunLoopTimerRef,
        mode: CFStringRef,
    ) -> BOOL;

    /* Scheduling Blocks */
    // fn CFRunLoopPerformBlock(rl: CFRunLoopRef, mode: CFTypeRef, block: void (^)(void));

    /* Getting the CFRunLoop Type ID */
    pub fn CFRunLoopGetTypeID() -> CFTypeID;

    /* CFRunLoopSource */
    /* CFRunLoopSource Miscellaneous Functions */
    pub fn CFRunLoopSourceCreate(
        allocator: CFAllocatorRef,
        order: CFIndex,
        context: *mut CFRunLoopSourceContext,
    ) -> CFRunLoopSourceRef;
    pub fn CFRunLoopSourceGetContext(
        source: CFRunLoopSourceRef,
        context: *mut CFRunLoopSourceContext,
    );
    pub fn CFRunLoopSourceGetOrder(source: CFRunLoopSourceRef) -> CFIndex;
    pub fn CFRunLoopSourceGetTypeID() -> CFTypeID;
    pub fn CFRunLoopSourceInvalidate(source: CFRunLoopSourceRef);
    pub fn CFRunLoopSourceIsValid(source: CFRunLoopSourceRef) -> BOOL;
    pub fn CFRunLoopSourceSignal(source: CFRunLoopSourceRef);

    /* CFRunLoopObserver */
    /* CFRunLoopObserver Miscellaneous Functions */
    // fn CFRunLoopObserverCreateWithHandler(allocator: CFAllocatorRef, activities: CFOptionFlags, repeats: Boolean, order: CFIndex, block: void (^) (CFRunLoopObserverRef observer, CFRunLoopActivity activity)) -> CFRunLoopObserverRef;
    pub fn CFRunLoopObserverCreate(
        allocator: CFAllocatorRef,
        activities: CFOptionFlags,
        repeats: BOOL,
        order: CFIndex,
        callout: CFRunLoopObserverCallBack,
        context: *mut CFRunLoopObserverContext,
    ) -> CFRunLoopObserverRef;
    pub fn CFRunLoopObserverDoesRepeat(observer: CFRunLoopObserverRef) -> BOOL;
    pub fn CFRunLoopObserverGetActivities(
        observer: CFRunLoopObserverRef,
    ) -> CFOptionFlags;
    pub fn CFRunLoopObserverGetContext(
        observer: CFRunLoopObserverRef,
        context: *mut CFRunLoopObserverContext,
    );
    pub fn CFRunLoopObserverGetOrder(observer: CFRunLoopObserverRef) -> CFIndex;
    pub fn CFRunLoopObserverGetTypeID() -> CFTypeID;
    pub fn CFRunLoopObserverInvalidate(observer: CFRunLoopObserverRef);
    pub fn CFRunLoopObserverIsValid(observer: CFRunLoopObserverRef) -> BOOL;

    /* CFRunLoopTimer */
    /* CFRunLoopTimer Miscellaneous Functions */
    // fn CFRunLoopTimerCreateWithHandler(allocator: CFAllocatorRef, fireDate: CFAbsoluteTime, interval: CFTimeInterval, flags: CFOptionFlags, order: CFIndex, block: void (^) (CFRunLoopTimerRef timer)) -> CFRunLoopTimerRef;
    pub fn CFRunLoopTimerCreate(
        allocator: CFAllocatorRef,
        fireDate: CFAbsoluteTime,
        interval: CFTimeInterval,
        flags: CFOptionFlags,
        order: CFIndex,
        callout: CFRunLoopTimerCallBack,
        context: *mut CFRunLoopTimerContext,
    ) -> CFRunLoopTimerRef;
    pub fn CFRunLoopTimerDoesRepeat(timer: CFRunLoopTimerRef) -> BOOL;
    pub fn CFRunLoopTimerGetContext(
        timer: CFRunLoopTimerRef,
        context: *mut CFRunLoopTimerContext,
    );
    pub fn CFRunLoopTimerGetInterval(timer: CFRunLoopTimerRef) -> CFTimeInterval;
    pub fn CFRunLoopTimerGetNextFireDate(timer: CFRunLoopTimerRef) -> CFAbsoluteTime;
    pub fn CFRunLoopTimerGetOrder(timer: CFRunLoopTimerRef) -> CFIndex;
    pub fn CFRunLoopTimerGetTypeID() -> CFTypeID;
    pub fn CFRunLoopTimerInvalidate(timer: CFRunLoopTimerRef);
    pub fn CFRunLoopTimerIsValid(timer: CFRunLoopTimerRef) -> BOOL;
    pub fn CFRunLoopTimerSetNextFireDate(
        timer: CFRunLoopTimerRef,
        fireDate: CFAbsoluteTime,
    );
    pub fn CFRunLoopTimerGetTolerance(timer: CFRunLoopTimerRef) -> CFTimeInterval; //macos(10.9)+
    pub fn CFRunLoopTimerSetTolerance(
        timer: CFRunLoopTimerRef,
        tolerance: CFTimeInterval,
    ); //macos(10.9)+
}

#[link(name = "ImageIO", kind = "framework")]
extern "C" {
    pub static kUTTypePNG: ObjcId;
    pub fn CGImageDestinationCreateWithURL(
        url: ObjcId,
        ty: ObjcId,
        count: u64,
        options: ObjcId,
    ) -> ObjcId;
    pub fn CGImageDestinationAddImage(dest: ObjcId, img: ObjcId, props: ObjcId);
    pub fn CGImageDestinationFinalize(dest: ObjcId) -> bool;
}

#[cfg(target_os = "macos")]
#[link(name = "AppKit", kind = "framework")]
extern "C" {
    pub static NSPasteboardURLReadingFileURLsOnlyKey: ObjcId;
    pub static NSTrackingArea: ObjcId;
    pub static NSStringPboardType: ObjcId;
    pub static NSFilenamesPboardType: ObjcId;
    pub static NSURLPboardType: ObjcId;
    pub static NSPasteboardTypeFileURL: ObjcId;
}

#[cfg(target_os = "ios")]
#[link(name = "GLKit", kind = "framework")]
extern "C" {}

pub const GLKViewDrawableColorFormatRGBA8888: i32 = 0;

#[repr(i32)]
pub enum GLKViewDrawableDepthFormat {
    FormatNone = 0,
    Format16,
    Format24,
}
#[repr(i32)]
pub enum GLKViewDrawableStencilFormat {
    FormatNone = 0,
    Format8,
}
#[cfg(target_os = "ios")]
#[link(name = "UIKit", kind = "framework")]
extern "C" {
    pub static UIKeyboardDidShowNotification: ObjcId;
    pub static UIKeyboardWillHideNotification: ObjcId;
    pub static UIKeyboardDidChangeFrameNotification: ObjcId;
    pub fn UIApplicationMain(
        argc: i32,
        argv: *mut *mut i8,
        principal_class_name: ObjcId,
        delegate_class_name: ObjcId,
    );
}

#[link(name = "Vision", kind = "framework")]
extern "C" {
    pub static VNImageRequestHandler: ObjcId;
    pub static VNRecognizeTextRequest: ObjcId;
}

pub const kCGEventLeftMouseDown: u32 = 1;
pub const kCGEventLeftMouseUp: u32 = 2;
pub const kCGMouseEventClickState: u32 = 1;
//pub const kCGEventSourceStateHIDSystemState: u32 = 1;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    pub fn CGEventSourceCreate(state_id: u32) -> ObjcId;
    pub fn CGEventSetIntegerValueField(event: ObjcId, field: u32, value: u64);
    pub fn CGEventCreateMouseEvent(
        source: ObjcId,
        mouse_type: u32,
        pos: NSPoint,
        button: u32,
    ) -> ObjcId;
    pub fn CGEventCreateScrollWheelEvent(
        source: ObjcId,
        is_line: u32,
        wheel_count: u32,
        wheel1: i32,
        wheel2: i32,
        wheel3: i32,
    ) -> ObjcId;
    pub fn CGEventPostToPid(pid: u32, event: ObjcId);
    pub fn CGEventPost(tap: u32, event: ObjcId);

    pub fn CGWindowListCreateImage(
        rect: NSRect,
        options: u32,
        window_id: u32,
        imageoptions: u32,
    ) -> ObjcId;
    pub fn CGMainDisplayID() -> u32;
    pub fn CGDisplayPixelsHigh(display: u32) -> u64;
    pub fn CGColorCreateGenericRGB(red: f64, green: f64, blue: f64, alpha: f64)
        -> ObjcId;
    pub fn CGAssociateMouseAndMouseCursorPosition(connected: bool);
    pub fn CGWarpMouseCursorPosition(newCursorPosition: NSPoint);

    // Wildly used private APIs; Apple uses them for their Terminal.app.
    pub fn CGSMainConnectionID() -> *mut ObjcId;
    pub fn CGSSetWindowBackgroundBlurRadius(
        connection_id: *mut ObjcId,
        window_id: i32,
        radius: i64,
    ) -> i32;
}

#[link(name = "Metal", kind = "framework")]
extern "C" {
    pub fn MTLCreateSystemDefaultDevice() -> ObjcId;
    #[cfg(not(target_os = "ios"))]
    pub fn MTLCopyAllDevices() -> ObjcId; //TODO: Array
}

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {
    pub static AVAudioUnitComponentManager: ObjcId;
    pub static AVAudioUnit: ObjcId;
}

// Foundation

#[repr(C)]
#[derive(Copy, Clone)]
pub struct CFRange {
    pub location: u64,
    pub length: u64,
}

pub const kCFStringEncodingUTF8: u32 = 134217984;
pub const kCGWindowListOptionIncludingWindow: u32 = 1 << 3;
pub const kCGWindowImageBoundsIgnoreFraming: u32 = 1 << 0;

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct NSPoint {
    pub x: f64,
    pub y: f64,
}

unsafe impl Encode for NSPoint {
    fn encode() -> Encoding {
        let encoding = format!(
            "{{CGPoint={}{}}}",
            f64::encode().as_str(),
            f64::encode().as_str()
        );
        unsafe { Encoding::from_str(&encoding) }
    }
}

#[repr(C)]
#[derive(Copy, Debug, Clone)]
pub struct NSSize {
    pub width: f64,
    pub height: f64,
}

unsafe impl Encode for NSSize {
    fn encode() -> Encoding {
        let encoding = format!(
            "{{CGSize={}{}}}",
            f64::encode().as_str(),
            f64::encode().as_str()
        );
        unsafe { Encoding::from_str(&encoding) }
    }
}

#[repr(C)]
#[derive(Copy, Debug, Clone)]
pub struct NSRect {
    pub origin: NSPoint,
    pub size: NSSize,
}
impl NSRect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> NSRect {
        NSRect {
            origin: NSPoint { x, y },
            size: NSSize {
                width: w,
                height: h,
            },
        }
    }
}
unsafe impl Encode for NSRect {
    fn encode() -> Encoding {
        let encoding = format!(
            "{{CGRect={}{}}}",
            NSPoint::encode().as_str(),
            NSSize::encode().as_str()
        );
        unsafe { Encoding::from_str(&encoding) }
    }
}

#[repr(u64)] // NSUInteger
pub enum NSEventModifierFlags {
    NSAlphaShiftKeyMask = 1 << 16,
    NSShiftKeyMask = 1 << 17,
    NSControlKeyMask = 1 << 18,
    NSAlternateKeyMask = 1 << 19,
    NSCommandKeyMask = 1 << 20,
    NSNumericPadKeyMask = 1 << 21,
    NSHelpKeyMask = 1 << 22,
    NSFunctionKeyMask = 1 << 23,
    NSDeviceIndependentModifierFlagsMask = 0xffff0000,
}

pub const NSTrackignActiveAlways: u64 = 0x80;
pub const NSTrackingInVisibleRect: u64 = 0x200;
pub const NSTrackingMouseEnteredAndExited: u64 = 0x01;
pub const NSTrackingMouseMoved: u64 = 0x02;
pub const NSTrackingCursorUpdate: u64 = 0x04;

pub const UTF8_ENCODING: usize = 4;

#[repr(u64)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NSWindowTitleVisibility {
    NSWindowTitleVisible = 0,
    NSWindowTitleHidden = 1,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u64)] // NSUInteger
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

#[repr(u64)] // NSUInteger
pub enum NSEventMask {
    NSLeftMouseDownMask = 1 << NSEventType::NSLeftMouseDown as u64,
    NSLeftMouseUpMask = 1 << NSEventType::NSLeftMouseUp as u64,
    NSRightMouseDownMask = 1 << NSEventType::NSRightMouseDown as u64,
    NSRightMouseUpMask = 1 << NSEventType::NSRightMouseUp as u64,
    NSMouseMovedMask = 1 << NSEventType::NSMouseMoved as u64,
    NSLeftMouseDraggedMask = 1 << NSEventType::NSLeftMouseDragged as u64,
    NSRightMouseDraggedMask = 1 << NSEventType::NSRightMouseDragged as u64,
    NSMouseEnteredMask = 1 << NSEventType::NSMouseEntered as u64,
    NSMouseExitedMask = 1 << NSEventType::NSMouseExited as u64,
    NSKeyDownMask = 1 << NSEventType::NSKeyDown as u64,
    NSKeyUpMask = 1 << NSEventType::NSKeyUp as u64,
    NSFlagsChangedMask = 1 << NSEventType::NSFlagsChanged as u64,
    NSAppKitDefinedMask = 1 << NSEventType::NSAppKitDefined as u64,
    NSSystemDefinedMask = 1 << NSEventType::NSSystemDefined as u64,
    NSApplicationDefinedMask = 1 << NSEventType::NSApplicationDefined as u64,
    NSPeriodicMask = 1 << NSEventType::NSPeriodic as u64,
    NSCursorUpdateMask = 1 << NSEventType::NSCursorUpdate as u64,
    NSScrollWheelMask = 1 << NSEventType::NSScrollWheel as u64,
    NSTabletPointMask = 1 << NSEventType::NSTabletPoint as u64,
    NSTabletProximityMask = 1 << NSEventType::NSTabletProximity as u64,
    NSOtherMouseDownMask = 1 << NSEventType::NSOtherMouseDown as u64,
    NSOtherMouseUpMask = 1 << NSEventType::NSOtherMouseUp as u64,
    NSOtherMouseDraggedMask = 1 << NSEventType::NSOtherMouseDragged as u64,
    NSEventMaskGesture = 1 << NSEventType::NSEventTypeGesture as u64,
    NSEventMaskSwipe = 1 << NSEventType::NSEventTypeSwipe as u64,
    NSEventMaskRotate = 1 << NSEventType::NSEventTypeRotate as u64,
    NSEventMaskBeginGesture = 1 << NSEventType::NSEventTypeBeginGesture as u64,
    NSEventMaskEndGesture = 1 << NSEventType::NSEventTypeEndGesture as u64,
    NSEventMaskPressure = 1 << NSEventType::NSEventTypePressure as u64,
    NSAnyEventMask = 0xffffffffffffffff,
}

#[repr(u64)] // NSUInteger
pub enum NSWindowStyleMask {
    NSBorderlessWindowMask = 0,
    NSTitledWindowMask = 1 << 0,
    NSClosableWindowMask = 1 << 1,
    NSMiniaturizableWindowMask = 1 << 2,
    NSResizableWindowMask = 1 << 3,

    NSTexturedBackgroundWindowMask = 1 << 8,

    NSUnifiedTitleAndToolbarWindowMask = 1 << 12,

    NSFullScreenWindowMask = 1 << 14,

    NSFullSizeContentViewWindowMask = 1 << 15,
}

#[repr(u64)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NSApplicationActivationOptions {
    NSApplicationActivateAllWindows = 1 << 0,
    NSApplicationActivateIgnoringOtherApps = 1 << 1,
}

#[repr(i64)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NSApplicationActivationPolicy {
    NSApplicationActivationPolicyRegular = 0,
    NSApplicationActivationPolicyAccessory = 1,
    NSApplicationActivationPolicyProhibited = 2,
    NSApplicationActivationPolicyERROR = -1,
}

#[repr(u64)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NSBackingStoreType {
    NSBackingStoreRetained = 0,
    NSBackingStoreNonretained = 1,
    NSBackingStoreBuffered = 2,
}

#[repr(C)]
pub struct NSRange {
    pub location: u64,
    pub length: u64,
}
impl NSRange {
    #[inline]
    pub fn new(location: u64, length: u64) -> NSRange {
        NSRange { location, length }
    }
}

unsafe impl Encode for NSRange {
    fn encode() -> Encoding {
        let encoding = format!(
            // TODO: Verify that this is correct
            "{{NSRange={}{}}}",
            u64::encode().as_str(),
            u64::encode().as_str(),
        );
        unsafe { Encoding::from_str(&encoding) }
    }
}

pub type CFHashCode = usize;
pub type CFIndex = isize;
pub type CFAllocatorRef = *const c_void;
pub type mach_port_t = std::os::raw::c_uint;
pub type CFTypeID = usize;
pub type CFOptionFlags = usize;

#[repr(C)]
pub struct __CFRunLoop(c_void);

pub type CFRunLoopRef = *mut __CFRunLoop;

#[repr(C)]
pub struct __CFRunLoopSource(c_void);

pub type CFRunLoopSourceRef = *mut __CFRunLoopSource;

#[repr(C)]
pub struct __CFRunLoopObserver(c_void);

pub type CFRunLoopObserverRef = *mut __CFRunLoopObserver;

// Reasons for CFRunLoopRunInMode() to Return
pub const kCFRunLoopRunFinished: i32 = 1;
pub const kCFRunLoopRunStopped: i32 = 2;
pub const kCFRunLoopRunTimedOut: i32 = 3;
pub const kCFRunLoopRunHandledSource: i32 = 4;

// Run Loop Observer Activities
//typedef CF_OPTIONS(CFOptionFlags, CFRunLoopActivity) {
pub type CFRunLoopActivity = CFOptionFlags;
pub const kCFRunLoopEntry: CFOptionFlags = 1 << 0;
pub const kCFRunLoopBeforeTimers: CFOptionFlags = 1 << 1;
pub const kCFRunLoopBeforeSources: CFOptionFlags = 1 << 2;
pub const kCFRunLoopBeforeWaiting: CFOptionFlags = 1 << 5;
pub const kCFRunLoopAfterWaiting: CFOptionFlags = 1 << 6;
pub const kCFRunLoopExit: CFOptionFlags = 1 << 7;
pub const kCFRunLoopAllActivities: CFOptionFlags = 0x0FFFFFFF;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CFRunLoopSourceContext {
    pub version: CFIndex,
    pub info: *mut c_void,
    pub retain: Option<extern "C" fn(info: *const c_void) -> *const c_void>,
    pub release: Option<extern "C" fn(info: *const c_void)>,
    pub copyDescription: Option<extern "C" fn(info: *const c_void) -> CFStringRef>,
    pub equal: Option<extern "C" fn(info1: *const c_void, info2: *const c_void) -> BOOL>,
    pub hash: Option<extern "C" fn(info: *const c_void) -> CFHashCode>,
    pub schedule:
        Option<extern "C" fn(info: *const c_void, rl: CFRunLoopRef, mode: CFStringRef)>,
    pub cancel:
        Option<extern "C" fn(info: *const c_void, rl: CFRunLoopRef, mode: CFStringRef)>,
    pub perform: extern "C" fn(info: *const c_void),
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CFRunLoopSourceContext1 {
    pub version: CFIndex,
    pub info: *mut c_void,
    pub retain: Option<extern "C" fn(info: *const c_void) -> *const c_void>,
    pub release: Option<extern "C" fn(info: *const c_void)>,
    pub copyDescription: Option<extern "C" fn(info: *const c_void) -> CFStringRef>,
    pub equal: Option<extern "C" fn(info1: *const c_void, info2: *const c_void) -> BOOL>,
    pub hash: Option<extern "C" fn(info: *const c_void) -> CFHashCode>,

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub getPort: extern "C" fn(info: *mut c_void) -> mach_port_t,
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub perform: extern "C" fn(
        msg: *mut c_void,
        size: CFIndex,
        allocator: CFAllocatorRef,
        info: *mut c_void,
    ) -> *mut c_void,

    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    pub getPort: extern "C" fn(info: *mut c_void) -> *mut c_void,
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    pub perform: extern "C" fn(info: *mut c_void) -> *mut c_void,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CFRunLoopObserverContext {
    pub version: CFIndex,
    pub info: *mut c_void,
    pub retain: Option<extern "C" fn(info: *const c_void) -> *const c_void>,
    pub release: Option<extern "C" fn(info: *const c_void)>,
    pub copyDescription: Option<extern "C" fn(info: *const c_void) -> CFStringRef>,
}

pub type CFRunLoopObserverCallBack = extern "C" fn(
    observer: CFRunLoopObserverRef,
    activity: CFRunLoopActivity,
    info: *mut c_void,
);

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CFRunLoopTimerContext {
    pub version: CFIndex,
    pub info: *mut c_void,
    pub retain: Option<extern "C" fn(info: *const c_void) -> *const c_void>,
    pub release: Option<extern "C" fn(info: *const c_void)>,
    pub copyDescription: Option<extern "C" fn(info: *const c_void) -> CFStringRef>,
}

pub type CFRunLoopTimerCallBack =
    extern "C" fn(timer: CFRunLoopTimerRef, info: *mut c_void);

#[repr(C)]
pub struct __CFRunLoopTimer(c_void);

pub type CFRunLoopTimerRef = *mut __CFRunLoopTimer;

pub trait NSMutableAttributedString: Sized {
    unsafe fn alloc(_: Self) -> ObjcId {
        msg_send![class!(NSMutableAttributedString), alloc]
    }

    unsafe fn init(self) -> ObjcId;
    // *mut NSMutableAttributedString
    unsafe fn init_with_string(self, string: ObjcId) -> ObjcId;
    unsafe fn init_with_attributed_string(self, string: ObjcId) -> ObjcId;

    unsafe fn string(self) -> ObjcId;
    // *mut NSString
    unsafe fn mutable_string(self) -> ObjcId;
    // *mut NSMutableString
    unsafe fn length(self) -> u64;
}

impl NSMutableAttributedString for ObjcId {
    unsafe fn init(self) -> ObjcId {
        msg_send![self, init]
    }

    unsafe fn init_with_string(self, string: ObjcId) -> ObjcId {
        msg_send![self, initWithString: string]
    }

    unsafe fn init_with_attributed_string(self, string: ObjcId) -> ObjcId {
        msg_send![self, initWithAttributedString: string]
    }

    unsafe fn string(self) -> ObjcId {
        msg_send![self, string]
    }

    unsafe fn mutable_string(self) -> ObjcId {
        msg_send![self, mutableString]
    }

    unsafe fn length(self) -> u64 {
        msg_send![self, length]
    }
}

// Metal API

#[repr(u64)]
#[derive(Clone, Debug)]
pub enum MTLLoadAction {
    DontCare = 0,
    Load = 1,
    Clear = 2,
}

#[repr(u64)]
#[derive(Clone, Debug)]
pub enum MTLStoreAction {
    DontCare = 0,
    Store = 1,
    MultisampleResolve = 2,
    StoreAndMultisampleResolve = 3,
    Unknown = 4,
    CustomSampleDepthStore = 5,
}

#[repr(C)]
#[derive(Clone, Debug)]
pub struct MTLClearColor {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}
impl MTLClearColor {
    pub fn new(r: f64, g: f64, b: f64, a: f64) -> MTLClearColor {
        MTLClearColor {
            red: r,
            green: g,
            blue: b,
            alpha: a,
        }
    }
}

#[repr(u64)]
#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum MTLPixelFormat {
    BGRA8Unorm = 80,
    Depth32Float = 252,
    Stencil8 = 253,
    Depth24Unorm_Stencil8 = 255,
    Depth32Float_Stencil8 = 260,
    RGBA8Unorm = 70,
    RGBA10A2Unorm = 75,
    RGBA16Float = 115,
}

/// See <https://developer.apple.com/documentation/metal/mtlsamplerminmagfilter>
#[repr(u64)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum MTLSamplerMinMagFilter {
    Nearest = 0,
    Linear = 1,
}

/// See <https://developer.apple.com/documentation/metal/mtlsamplermipfilter>
#[repr(u64)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum MTLSamplerMipFilter {
    NotMipmapped = 0,
    Nearest = 1,
    Linear = 2,
}

/// See <https://developer.apple.com/documentation/metal/mtlsampleraddressmode>
#[repr(u64)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum MTLSamplerAddressMode {
    ClampToEdge = 0,
    MirrorClampToEdge = 1,
    Repeat = 2,
    MirrorRepeat = 3,
    ClampToZero = 4,
    ClampToBorderColor = 5,
}

#[repr(u64)]
#[allow(non_camel_case_types)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum MTLVertexFormat {
    Invalid = 0,
    UChar2 = 1,
    UChar3 = 2,
    UChar4 = 3,
    Char2 = 4,
    Char3 = 5,
    Char4 = 6,
    UChar2Normalized = 7,
    UChar3Normalized = 8,
    UChar4Normalized = 9,
    Char2Normalized = 10,
    Char3Normalized = 11,
    Char4Normalized = 12,
    UShort2 = 13,
    UShort3 = 14,
    UShort4 = 15,
    Short2 = 16,
    Short3 = 17,
    Short4 = 18,
    UShort2Normalized = 19,
    UShort3Normalized = 20,
    UShort4Normalized = 21,
    Short2Normalized = 22,
    Short3Normalized = 23,
    Short4Normalized = 24,
    Half2 = 25,
    Half3 = 26,
    Half4 = 27,
    Float = 28,
    Float2 = 29,
    Float3 = 30,
    Float4 = 31,
    Int = 32,
    Int2 = 33,
    Int3 = 34,
    Int4 = 35,
    UInt = 36,
    UInt2 = 37,
    UInt3 = 38,
    UInt4 = 39,
    Int1010102Normalized = 40,
    UInt1010102Normalized = 41,
    UChar4Normalized_BGRA = 42,
    UChar = 45,
    Char = 46,
    UCharNormalized = 47,
    CharNormalized = 48,
    UShort = 49,
    Short = 50,
    UShortNormalized = 51,
    ShortNormalized = 52,
    Half = 53,
}

#[repr(u64)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum MTLVertexStepFunction {
    Constant = 0,
    PerVertex = 1,
    PerInstance = 2,
    PerPatch = 3,
    PerPatchControlPoint = 4,
}
#[repr(u64)]
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MTLBlendFactor {
    Zero = 0,
    One = 1,
    SourceColor = 2,
    OneMinusSourceColor = 3,
    SourceAlpha = 4,
    OneMinusSourceAlpha = 5,
    DestinationColor = 6,
    OneMinusDestinationColor = 7,
    DestinationAlpha = 8,
    OneMinusDestinationAlpha = 9,
    SourceAlphaSaturated = 10,
    BlendColor = 11,
    OneMinusBlendColor = 12,
    BlendAlpha = 13,
    OneMinusBlendAlpha = 14,
    Source1Color = 15,
    OneMinusSource1Color = 16,
    Source1Alpha = 17,
    OneMinusSource1Alpha = 18,
}

#[repr(u64)]
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MTLBlendOperation {
    Add = 0,
    Subtract = 1,
    ReverseSubtract = 2,
    Min = 3,
    Max = 4,
}

#[repr(u64)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum MTLPrimitiveType {
    Point = 0,
    Line = 1,
    LineStrip = 2,
    Triangle = 3,
    TriangleStrip = 4,
}

#[repr(u64)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum MTLIndexType {
    UInt16 = 0,
    UInt32 = 1,
}

#[repr(u64)]
pub enum MTLCompareFunction {
    Never = 0,
    Less = 1,
    Equal = 2,
    LessEqual = 3,
    Greater = 4,
    NotEqual = 5,
    GreaterEqual = 6,
    Always = 7,
}

#[repr(u64)]
#[allow(non_camel_case_types)]
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub enum MTLTextureType {
    D1 = 0,
    D1Array = 1,
    D2 = 2,
    D2Array = 3,
    D2Multisample = 4,
    Cube = 5,
    CubeArray = 6,
    D3 = 7,
}

#[repr(u64)]
#[allow(non_camel_case_types)]
pub enum MTLTextureUsage {
    Unknown = 0x0000,
    ShaderRead = 0x0001,
    ShaderWrite = 0x0002,
    RenderTarget = 0x0004,
    PixelFormatView = 0x0010,
}

#[repr(u64)]
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MTLStorageMode {
    Shared = 0,
    Managed = 1,
    Private = 2,
    Memoryless = 3,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MTLOrigin {
    pub x: u64,
    pub y: u64,
    pub z: u64,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MTLSize {
    pub width: u64,
    pub height: u64,
    pub depth: u64,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MTLRegion {
    pub origin: MTLOrigin,
    pub size: MTLSize,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}

#[repr(u64)]
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MTLCPUCacheMode {
    DefaultCache = 0,
    WriteCombined = 1,
}

#[repr(u64)]
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MTLHazardTrackingMode {
    Default = 0,
    Untracked = 1,
    Tracked = 2,
}

pub const MTLResourceCPUCacheModeShift: u64 = 0;
pub const MTLResourceCPUCacheModeMask: u64 = 0xf << MTLResourceCPUCacheModeShift;

pub const MTLResourceStorageModeShift: u64 = 4;
pub const MTLResourceStorageModeMask: u64 = 0xf << MTLResourceStorageModeShift;

pub const MTLResourceHazardTrackingModeShift: u64 = 8;
pub const MTLResourceHazardTrackingModeMask: u64 =
    0x3 << MTLResourceHazardTrackingModeShift;

#[allow(non_upper_case_globals)]
pub struct MTLResourceOptions;
impl MTLResourceOptions {
    pub const CPUCacheModeDefaultCache: u64 =
        (MTLCPUCacheMode::DefaultCache as u64) << MTLResourceCPUCacheModeShift;
    pub const CPUCacheModeWriteCombined: u64 =
        (MTLCPUCacheMode::WriteCombined as u64) << MTLResourceCPUCacheModeShift;

    pub const HazardTrackingModeUntracked: u64 =
        (MTLHazardTrackingMode::Untracked as u64) << MTLResourceHazardTrackingModeShift;
    pub const HazardTrackingModeTracked: u64 =
        (MTLHazardTrackingMode::Tracked as u64) << MTLResourceHazardTrackingModeShift;

    pub const StorageModeShared: u64 =
        (MTLStorageMode::Shared as u64) << MTLResourceStorageModeShift;
    pub const StorageModeManaged: u64 =
        (MTLStorageMode::Managed as u64) << MTLResourceStorageModeShift;
    pub const StorageModePrivate: u64 =
        (MTLStorageMode::Private as u64) << MTLResourceStorageModeShift;
    pub const StorageModeMemoryless: u64 =
        (MTLStorageMode::Memoryless as u64) << MTLResourceStorageModeShift;
}

#[repr(u64)]
pub enum NSDragOperation {
    None = 0,
    Copy = 1,
    Link = 2,
    Move = 16,
}

unsafe impl Encode for NSDragOperation {
    fn encode() -> Encoding {
        let encoding = format!("Q");
        unsafe { Encoding::from_str(&encoding) }
    }
}

#[repr(C)]
#[derive(Copy, Debug, Clone)]
pub struct MTLScissorRect {
    pub x: u64,
    pub y: u64,
    pub width: u64,
    pub height: u64,
}

// CORE AUDIO

pub const kAudioUnitManufacturer_Apple: u32 = 1634758764;

#[repr(C)]
pub struct OpaqueAudioComponent([u8; 0]);
pub type CAudioComponent = *mut OpaqueAudioComponent;

#[repr(C)]
pub struct ComponentInstanceRecord([u8; 0]);
pub type CAudioComponentInstance = *mut ComponentInstanceRecord;
pub type CAudioUnit = CAudioComponentInstance;

pub type OSStatus = i32;

#[repr(C)]
pub struct CAudioStreamBasicDescription {
    pub mSampleRate: f64,
    pub mFormatID: AudioFormatId,
    pub mFormatFlags: u32,
    pub mBytesPerPacket: u32,
    pub mFramesPerPacket: u32,
    pub mBytesPerFrame: u32,
    pub mChannelsPerFrame: u32,
    pub mBitsPerChannel: u32,
    pub mReserved: u32,
}

#[repr(u32)]
pub enum AudioFormatId {
    LinearPCM = 1819304813,
    AC3 = 1633889587,
    F60958AC3 = 1667326771,
    AppleIMA4 = 1768775988,
    MPEG4AAC = 1633772320,
    MPEG4CELP = 1667591280,
    MPEG4HVXC = 1752594531,
    MPEG4TwinVQ = 1953986161,
    MACE3 = 1296122675,
    MACE6 = 1296122678,
    ULaw = 1970037111,
    ALaw = 1634492791,
    QDesign = 1363430723,
    QDesign2 = 1363430706,
    QUALCOMM = 1365470320,
    MPEGLayer1 = 778924081,
    MPEGLayer2 = 778924082,
    MPEGLayer3 = 778924083,
    TimeCode = 1953066341,
    MIDIStream = 1835623529,
    ParameterValueStream = 1634760307,
    AppleLossless = 1634492771,
    MPEG4AAC_HE = 1633772392,
    MPEG4AAC_LD = 1633772396,
    MPEG4AAC_ELD = 1633772389,
    MPEG4AAC_ELD_SBR = 1633772390,
    MPEG4AAC_ELD_V2 = 1633772391,
    MPEG4AAC_HE_V2 = 1633772400,
    MPEG4AAC_Spatial = 1633772403,
    AMR = 1935764850,
    AMR_WB = 1935767394,
    Audible = 1096107074,
    iLBC = 1768710755,
    DVIIntelIMA = 1836253201,
    MicrosoftGSM = 1836253233,
    AES3 = 1634038579,
}

struct F60958AC3Flags;
impl F60958AC3Flags {
    const IS_FLOAT: u32 = 1;
    const IS_BIG_ENDIAN: u32 = 2;
    const IS_SIGNED_INTEGER: u32 = 4;
    const IS_PACKED: u32 = 8;
    const IS_ALIGNED_HIGH: u32 = 16;
    const IS_NON_INTERLEAVED: u32 = 32;
    const IS_NON_MIXABLE: u32 = 64;
}

pub struct LinearPcmFlags;
impl LinearPcmFlags {
    const IS_FLOAT: u32 = 1;
    const IS_BIG_ENDIAN: u32 = 2;
    const IS_SIGNED_INTEGER: u32 = 4;
    const IS_PACKED: u32 = 8;
    const IS_ALIGNED_HIGH: u32 = 16;
    const IS_NON_INTERLEAVED: u32 = 32;
    const IS_NON_MIXABLE: u32 = 64;
    const FLAGS_SAMPLE_FRACTION_SHIFT: u32 = 7;
    const FLAGS_SAMPLE_FRACTION_MASK: u32 = 8064;
}

pub struct AppleLosslessFlags;
impl AppleLosslessFlags {
    const BIT_16_SOURCE_DATA: u32 = 1;
    const BIT_20_SOURCE_DATA: u32 = 2;
    const BIT_24_SOURCE_DATA: u32 = 3;
    const BIT_32_SOURCE_DATA: u32 = 4;
}

#[repr(u32)]
pub enum Mpeg4ObjectId {
    AAC_Main = 1,
    AAC_LC = 2,
    AAC_SSR = 3,
    AAC_LTP = 4,
    AAC_SBR = 5,
    AAC_Scalable = 6,
    TwinVQ = 7,
    CELP = 8,
    HVXC = 9,
}

pub struct AudioTimeStampFlags;
impl AudioTimeStampFlags {
    const SAMPLE_TIME_VALID: u32 = 1;
    const HOST_TIME_VALID: u32 = 2;
    const RATE_SCALAR_VALID: u32 = 4;
    const WORLD_CLOCK_TIME_VALID: u32 = 8;
    const SMPTE_TIME_VALID: u32 = 16;
}

#[derive(Debug, PartialEq, Copy, Clone)]
#[repr(C)]
pub struct CAudioComponentDescription {
    pub componentType: CAudioUnitType,
    pub componentSubType: CAudioUnitSubType,
    pub componentManufacturer: u32,
    pub componentFlags: u32,
    pub componentFlagsMask: u32,
}

impl CAudioComponentDescription {
    pub fn new_apple(ty: CAudioUnitType, sub: CAudioUnitSubType) -> Self {
        Self {
            componentType: ty,
            componentSubType: sub,
            componentManufacturer: kAudioUnitManufacturer_Apple,
            componentFlags: 0,
            componentFlagsMask: 0,
        }
    }
    pub fn new_all_manufacturers(ty: CAudioUnitType, sub: CAudioUnitSubType) -> Self {
        Self {
            componentType: ty,
            componentSubType: sub,
            componentManufacturer: 0,
            componentFlags: 0,
            componentFlagsMask: 0,
        }
    }
}

#[derive(Debug, Default)]
#[repr(C)]
pub struct SMPTETime {
    pub mSubframes: i16,
    pub mSubframeDivisor: i16,
    pub mCounter: u32,
    pub mType: u32,
    pub mFlags: u32,
    pub mHours: i16,
    pub mMinutes: i16,
    pub mSeconds: i16,
    pub mFrames: i16,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct _AudioBuffer {
    pub mNumberChannels: u32,
    pub mDataByteSize: u32,
    pub mData: *mut ::std::os::raw::c_void,
}

pub const MAX_AUDIO_BUFFERS: usize = 8;
#[repr(C)]
pub struct CAudioBufferList {
    pub mNumberBuffers: u32,
    pub mBuffers: [_AudioBuffer; MAX_AUDIO_BUFFERS],
}

#[derive(Debug)]
#[repr(C)]
pub struct CAudioTimeStamp {
    pub mSampleTime: f64,
    pub mHostTime: u64,
    pub mRateScalar: f64,
    pub mWordClockTime: u64,
    pub mSMPTETime: SMPTETime,
    pub mFlags: u32,
    pub mReserved: u32,
}

#[derive(Debug, PartialEq, Copy, Clone)]
#[repr(u32)]
pub enum CAudioUnitType {
    IO = 1635086197,
    MusicDevice = 1635085685,
    MusicEffect = 1635085670,
    FormatConverter = 1635083875,
    Effect = 1635083896,
    Mixer = 1635085688,
    Panner = 1635086446,
    Generator = 1635084142,
    OfflineEffect = 1635086188,
}

#[derive(Debug, PartialEq, Copy, Clone)]
#[repr(u32)]
pub enum CAudioUnitSubType {
    Undefined = 0,

    PeakLimiter = 1819112562,
    DynamicsProcessor = 1684237680,
    LowPassFilter = 1819304307,
    HighPassFilter = 1752195443,
    BandPassFilter = 1651532147,
    HighShelfFilter = 1752393830,
    LowShelfFilter = 1819502694,
    ParametricEQ = 1886217585,
    Distortion = 1684632436,
    Delay = 1684368505,
    SampleDelay = 1935961209,
    GraphicEQ = 1735550321,
    MultiBandCompressor = 1835232624,
    MatrixReverb = 1836213622,
    Pitch = 1953329268,
    AUFilter = 1718185076,
    NetSend = 1853058660,
    RogerBeep = 1919903602,
    NBandEQ = 1851942257,

    //pub enum FormatConverterType
    AUConverter = 1668247158,
    NewTimePitch = 1853191280,
    //TimePitch = 1953329268,
    DeferredRenderer = 1684366962,
    Splitter = 1936747636,
    Merger = 1835364967,
    Varispeed = 1986097769,
    AUiPodTimeOther = 1768977519,

    //pub enum MixerType
    MultiChannelMixer = 1835232632,
    StereoMixer = 1936554098,
    Mixer3D = 862219640,
    MatrixMixer = 1836608888,

    //pub enum GeneratorType {
    ScheduledSoundPlayer = 1936945260,
    AudioFilePlayer = 1634103404,

    //pub enum MusicDeviceType {
    DLSSynth = 1684828960,
    Sampler = 1935764848,

    //pub enum IOType {
    GenericOutput = 1734700658,
    HalOutput = 1634230636,
    DefaultOutput = 1684366880,
    SystemOutput = 1937339168,
    VoiceProcessingIO = 1987078511,
    RemoteIO = 1919512419,
}

#[derive(Debug)]
#[repr(i32)]
pub enum OSError {
    Unimplemented = -4,
    FileNotFound = -43,
    FilePermission = -54,
    TooManyFilesOpen = -42,

    Unspecified = -1500,
    SystemSoundClientMessageTimeout = -1501,

    BadFilePath = 561017960,
    Param = -50,
    MemFull = -108,

    FormatUnspecified = 2003329396,
    UnknownProperty = 2003332927,
    BadPropertySize = 561211770,
    IllegalOperation = 1852797029,
    UnsupportedFormat = 560226676,
    State = 561214580,
    NotEnoughBufferSpace = 560100710,

    UnsupportedDataFormat = 1718449215,

    InvalidProperty = -10879,
    InvalidParameter = -10878,
    InvalidElement = -10877,
    NoConnection = -10876,
    FailedInitialization = -10875,
    TooManyFramesToProcess = -10874,
    InvalidFile = -10871,
    FormatNotSupported = -10868,
    Uninitialized = -10867,
    InvalidScope = -10866,
    PropertyNotWritable = -10865,
    CannotDoInCurrentContext = -10863,
    InvalidPropertyValue = -10851,
    PropertyNotInUse = -10850,
    Initialized = -10849,
    InvalidOfflineRender = -10848,
    Unauthorized = -10847,

    NoMatchingDefaultAudioUnitFound,

    Unknown,
}

pub const kAudioComponentInstantiation_LoadInProcess: u32 = 2;
pub const kAudioComponentInstantiation_LoadOutOfProcess: u32 = 1;

impl OSError {
    pub fn from(result: i32) -> Result<(), Self> {
        Err(match result {
            0 => return Ok(()),
            x if x == Self::Unimplemented as i32 => Self::Unimplemented,
            x if x == Self::FileNotFound as i32 => Self::FileNotFound,
            x if x == Self::FilePermission as i32 => Self::FilePermission,
            x if x == Self::TooManyFilesOpen as i32 => Self::TooManyFilesOpen,

            x if x == Self::Unspecified as i32 => Self::Unspecified,
            x if x == Self::SystemSoundClientMessageTimeout as i32 => {
                Self::SystemSoundClientMessageTimeout
            }

            x if x == Self::BadFilePath as i32 => Self::BadFilePath,
            x if x == Self::Param as i32 => Self::Param,
            x if x == Self::MemFull as i32 => Self::MemFull,

            x if x == Self::FormatUnspecified as i32 => Self::FormatUnspecified,
            x if x == Self::UnknownProperty as i32 => Self::UnknownProperty,
            x if x == Self::BadPropertySize as i32 => Self::BadPropertySize,
            x if x == Self::IllegalOperation as i32 => Self::IllegalOperation,
            x if x == Self::UnsupportedFormat as i32 => Self::UnsupportedFormat,
            x if x == Self::State as i32 => Self::State,
            x if x == Self::NotEnoughBufferSpace as i32 => Self::NotEnoughBufferSpace,

            x if x == Self::UnsupportedDataFormat as i32 => Self::UnsupportedDataFormat,

            x if x == Self::InvalidProperty as i32 => Self::InvalidProperty,
            x if x == Self::InvalidParameter as i32 => Self::InvalidParameter,
            x if x == Self::InvalidElement as i32 => Self::InvalidElement,
            x if x == Self::NoConnection as i32 => Self::NoConnection,
            x if x == Self::FailedInitialization as i32 => Self::FailedInitialization,
            x if x == Self::TooManyFramesToProcess as i32 => Self::TooManyFramesToProcess,
            x if x == Self::InvalidFile as i32 => Self::InvalidFile,
            x if x == Self::FormatNotSupported as i32 => Self::FormatNotSupported,
            x if x == Self::Uninitialized as i32 => Self::Uninitialized,
            x if x == Self::InvalidScope as i32 => Self::InvalidScope,
            x if x == Self::PropertyNotWritable as i32 => Self::PropertyNotWritable,
            x if x == Self::CannotDoInCurrentContext as i32 => {
                Self::CannotDoInCurrentContext
            }
            x if x == Self::InvalidPropertyValue as i32 => Self::InvalidPropertyValue,
            x if x == Self::PropertyNotInUse as i32 => Self::PropertyNotInUse,
            x if x == Self::Initialized as i32 => Self::Initialized,
            x if x == Self::InvalidOfflineRender as i32 => Self::InvalidOfflineRender,
            x if x == Self::Unauthorized as i32 => Self::Unauthorized,
            _ => Self::Unknown,
        })
    }

    pub fn from_nserror(ns_error: ObjcId) -> Result<(), Self> {
        if ns_error != nil {
            let code: i32 = unsafe { msg_send![ns_error, code] };
            Self::from(code)
        } else {
            Ok(())
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct __CFString {
    _unused: [u8; 0],
}
pub type CFStringRef = *const __CFString;
pub type ItemCount = u64;
pub type MIDIObjectRef = u32;
pub type MIDIClientRef = MIDIObjectRef;
pub type MIDIPortRef = MIDIObjectRef;
pub type MIDIEndpointRef = MIDIObjectRef;
pub type MIDIProtocolID = i32;
pub type MIDITimeStamp = u64;
pub const kMIDIProtocol_1_0: i32 = 1;
pub const kMIDIProtocol_2_0: i32 = 2;

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct MIDINotification {
    pub messageID: i32,
    pub messageSize: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct MIDIEventList {
    pub protocol: MIDIProtocolID,
    pub numPackets: u32,
    pub packet: [MIDIEventPacket; 1usize],
}

#[repr(C, packed(4))]
#[derive(Copy, Clone)]
pub struct MIDIEventPacket {
    pub timeStamp: MIDITimeStamp,
    pub wordCount: u32,
    pub words: [u32; 64usize],
}

#[link(name = "CoreMIDI", kind = "framework")]
extern "C" {
    pub static kMIDIPropertyManufacturer: CFStringRef;
    pub static kMIDIPropertyDisplayName: CFStringRef;
    pub static kMIDIPropertyUniqueID: CFStringRef;

    pub fn MIDIGetNumberOfSources() -> ItemCount;
    pub fn MIDIGetSource(sourceIndex0: ItemCount) -> MIDIEndpointRef;

    pub fn MIDIGetNumberOfDestinations() -> ItemCount;
    pub fn MIDIGetDestination(sourceIndex0: ItemCount) -> MIDIEndpointRef;

    pub fn MIDIClientCreateWithBlock(
        name: CFStringRef,
        outClient: *mut MIDIClientRef,
        notifyBlock: ObjcId,
    ) -> OSStatus;

    pub fn MIDIInputPortCreateWithProtocol(
        client: MIDIClientRef,
        portName: CFStringRef,
        protocol: MIDIProtocolID,
        outPort: *mut MIDIPortRef,
        receiveBlock: ObjcId,
    ) -> OSStatus;

    pub fn MIDIOutputPortCreate(
        client: MIDIClientRef,
        portName: CFStringRef,
        outPort: *mut MIDIPortRef,
    ) -> OSStatus;

    pub fn MIDIObjectGetStringProperty(
        obj: MIDIObjectRef,
        propertyID: CFStringRef,
        str_: *mut CFStringRef,
    ) -> OSStatus;

    pub fn MIDIObjectGetIntegerProperty(
        obj: MIDIObjectRef,
        propertyID: CFStringRef,
        outValue: *mut i32,
    ) -> OSStatus;

    pub fn MIDIPortConnectSource(
        port: MIDIPortRef,
        source: MIDIEndpointRef,
        connRefCon: *mut ::std::os::raw::c_void,
    ) -> OSStatus;
}

pub const NSOpenGLContextParameterSwapInterval: i32 = 222;

#[repr(u64)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NSOpenGLPixelFormatAttribute {
    NSOpenGLPFAAllRenderers = 1,
    NSOpenGLPFATripleBuffer = 3,
    NSOpenGLPFADoubleBuffer = 5,
    NSOpenGLPFAStereo = 6,
    NSOpenGLPFAAuxBuffers = 7,
    NSOpenGLPFAColorSize = 8,
    NSOpenGLPFAAlphaSize = 11,
    NSOpenGLPFADepthSize = 12,
    NSOpenGLPFAStencilSize = 13,
    NSOpenGLPFAAccumSize = 14,
    NSOpenGLPFAMinimumPolicy = 51,
    NSOpenGLPFAMaximumPolicy = 52,
    NSOpenGLPFAOffScreen = 53,
    NSOpenGLPFAFullScreen = 54,
    NSOpenGLPFASampleBuffers = 55,
    NSOpenGLPFASamples = 56,
    NSOpenGLPFAAuxDepthStencil = 57,
    NSOpenGLPFAColorFloat = 58,
    NSOpenGLPFAMultisample = 59,
    NSOpenGLPFASupersample = 60,
    NSOpenGLPFASampleAlpha = 61,
    NSOpenGLPFARendererID = 70,
    NSOpenGLPFASingleRenderer = 71,
    NSOpenGLPFANoRecovery = 72,
    NSOpenGLPFAAccelerated = 73,
    NSOpenGLPFAClosestPolicy = 74,
    NSOpenGLPFARobust = 75,
    NSOpenGLPFABackingStore = 76,
    NSOpenGLPFAMPSafe = 78,
    NSOpenGLPFAWindow = 80,
    NSOpenGLPFAMultiScreen = 81,
    NSOpenGLPFACompliant = 83,
    NSOpenGLPFAScreenMask = 84,
    NSOpenGLPFAPixelBuffer = 90,
    NSOpenGLPFARemotePixelBuffer = 91,
    NSOpenGLPFAAllowOfflineRenderers = 96,
    NSOpenGLPFAAcceleratedCompute = 97,
    NSOpenGLPFAOpenGLProfile = 99,
    NSOpenGLPFAVirtualScreenCount = 128,
}

#[repr(u64)]
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NSOpenGLPFAOpenGLProfiles {
    NSOpenGLProfileVersionLegacy = 0x1000,
    NSOpenGLProfileVersion3_2Core = 0x3200,
    NSOpenGLProfileVersion4_1Core = 0x4100,
}
