// TODO: Upstream these

#![allow(dead_code, non_snake_case, non_upper_case_globals)]

use std::ffi::c_void;

use core_foundation::array::CFArrayRef;
use core_foundation::dictionary::CFDictionaryRef;
use core_foundation::string::CFStringRef;
use core_foundation::uuid::CFUUIDRef;
use core_graphics::base::CGError;
use core_graphics::display::{CGDirectDisplayID, CGDisplayConfigRef};
use objc2::ffi::NSInteger;
use objc2::runtime::AnyObject;

pub type CGDisplayFadeInterval = f32;
pub type CGDisplayReservationInterval = f32;
pub type CGDisplayBlendFraction = f32;

pub const kCGDisplayBlendNormal: f32 = 0.0;
pub const kCGDisplayBlendSolidColor: f32 = 1.0;

pub type CGDisplayFadeReservationToken = u32;
pub const kCGDisplayFadeReservationInvalidToken: CGDisplayFadeReservationToken = 0;

pub type Boolean = u8;
pub const FALSE: Boolean = 0;
pub const TRUE: Boolean = 1;

pub const kCGErrorSuccess: i32 = 0;
pub const kCGErrorFailure: i32 = 1000;
pub const kCGErrorIllegalArgument: i32 = 1001;
pub const kCGErrorInvalidConnection: i32 = 1002;
pub const kCGErrorInvalidContext: i32 = 1003;
pub const kCGErrorCannotComplete: i32 = 1004;
pub const kCGErrorNotImplemented: i32 = 1006;
pub const kCGErrorRangeCheck: i32 = 1007;
pub const kCGErrorTypeCheck: i32 = 1008;
pub const kCGErrorInvalidOperation: i32 = 1010;
pub const kCGErrorNoneAvailable: i32 = 1011;

pub const IO1BitIndexedPixels: &str = "P";
pub const IO2BitIndexedPixels: &str = "PP";
pub const IO4BitIndexedPixels: &str = "PPPP";
pub const IO8BitIndexedPixels: &str = "PPPPPPPP";
pub const IO16BitDirectPixels: &str = "-RRRRRGGGGGBBBBB";
pub const IO32BitDirectPixels: &str = "--------RRRRRRRRGGGGGGGGBBBBBBBB";

pub const kIO30BitDirectPixels: &str = "--RRRRRRRRRRGGGGGGGGGGBBBBBBBBBB";
pub const kIO64BitDirectPixels: &str = "-16R16G16B16";

pub const kIO16BitFloatPixels: &str = "-16FR16FG16FB16";
pub const kIO32BitFloatPixels: &str = "-32FR32FG32FB32";

pub const IOYUV422Pixels: &str = "Y4U2V2";
pub const IO8BitOverlayPixels: &str = "O8";

pub type CGWindowLevel = i32;
pub type CGDisplayModeRef = *mut c_void;

// `CGDisplayCreateUUIDFromDisplayID` comes from the `ColorSync` framework.
// However, that framework was only introduced "publicly" in macOS 10.13.
//
// Since we want to support older versions, we can't link to `ColorSync`
// directly. Fortunately, it has always been available as a subframework of
// `ApplicationServices`, see:
// https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/OSX_Technology_Overview/SystemFrameworks/SystemFrameworks.html#//apple_ref/doc/uid/TP40001067-CH210-BBCFFIEG
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    pub fn CGDisplayCreateUUIDFromDisplayID(display: CGDirectDisplayID) -> CFUUIDRef;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    pub fn CGRestorePermanentDisplayConfiguration();
    pub fn CGDisplayCapture(display: CGDirectDisplayID) -> CGError;
    pub fn CGDisplayRelease(display: CGDirectDisplayID) -> CGError;
    pub fn CGConfigureDisplayFadeEffect(
        config: CGDisplayConfigRef,
        fadeOutSeconds: CGDisplayFadeInterval,
        fadeInSeconds: CGDisplayFadeInterval,
        fadeRed: f32,
        fadeGreen: f32,
        fadeBlue: f32,
    ) -> CGError;
    pub fn CGAcquireDisplayFadeReservation(
        seconds: CGDisplayReservationInterval,
        token: *mut CGDisplayFadeReservationToken,
    ) -> CGError;
    pub fn CGDisplayFade(
        token: CGDisplayFadeReservationToken,
        duration: CGDisplayFadeInterval,
        startBlend: CGDisplayBlendFraction,
        endBlend: CGDisplayBlendFraction,
        redBlend: f32,
        greenBlend: f32,
        blueBlend: f32,
        synchronous: Boolean,
    ) -> CGError;
    pub fn CGReleaseDisplayFadeReservation(
        token: CGDisplayFadeReservationToken,
    ) -> CGError;
    pub fn CGShieldingWindowLevel() -> CGWindowLevel;
    pub fn CGDisplaySetDisplayMode(
        display: CGDirectDisplayID,
        mode: CGDisplayModeRef,
        options: CFDictionaryRef,
    ) -> CGError;
    pub fn CGDisplayCopyAllDisplayModes(
        display: CGDirectDisplayID,
        options: CFDictionaryRef,
    ) -> CFArrayRef;
    pub fn CGDisplayModeGetPixelWidth(mode: CGDisplayModeRef) -> usize;
    pub fn CGDisplayModeGetPixelHeight(mode: CGDisplayModeRef) -> usize;
    pub fn CGDisplayModeGetRefreshRate(mode: CGDisplayModeRef) -> f64;
    pub fn CGDisplayModeCopyPixelEncoding(mode: CGDisplayModeRef) -> CFStringRef;
    pub fn CGDisplayModeRetain(mode: CGDisplayModeRef);
    pub fn CGDisplayModeRelease(mode: CGDisplayModeRef);

    // Wildly used private APIs; Apple uses them for their Terminal.app.
    pub fn CGSMainConnectionID() -> *mut AnyObject;
    pub fn CGSSetWindowBackgroundBlurRadius(
        connection_id: *mut AnyObject,
        window_id: NSInteger,
        radius: i64,
    ) -> i32;
}

mod core_video {
    use super::*;

    #[link(name = "CoreVideo", kind = "framework")]
    extern "C" {}

    // CVBase.h

    pub type CVTimeFlags = i32; // int32_t
    pub const kCVTimeIsIndefinite: CVTimeFlags = 1 << 0;

    #[repr(C)]
    #[derive(Debug, Clone)]
    pub struct CVTime {
        pub time_value: i64, // int64_t
        pub time_scale: i32, // int32_t
        pub flags: i32,      // int32_t
    }

    // CVReturn.h

    pub type CVReturn = i32; // int32_t
    pub const kCVReturnSuccess: CVReturn = 0;

    // CVDisplayLink.h

    pub type CVDisplayLinkRef = *mut c_void;

    extern "C" {
        pub fn CVDisplayLinkCreateWithCGDisplay(
            displayID: CGDirectDisplayID,
            displayLinkOut: *mut CVDisplayLinkRef,
        ) -> CVReturn;
        pub fn CVDisplayLinkGetNominalOutputVideoRefreshPeriod(
            displayLink: CVDisplayLinkRef,
        ) -> CVTime;
        pub fn CVDisplayLinkRelease(displayLink: CVDisplayLinkRef);
    }
}

pub use core_video::*;
#[repr(transparent)]
pub struct TISInputSource(std::ffi::c_void);
pub type TISInputSourceRef = *mut TISInputSource;

#[repr(transparent)]
pub struct UCKeyboardLayout(std::ffi::c_void);

pub type OptionBits = u32;
pub type UniCharCount = std::os::raw::c_ulong;
pub type UniChar = std::os::raw::c_ushort;
pub type OSStatus = i32;

#[allow(non_upper_case_globals)]
pub const kUCKeyActionDisplay: u16 = 3;
#[allow(non_upper_case_globals)]
pub const kUCKeyTranslateNoDeadKeysMask: OptionBits = 1;

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    pub static kTISPropertyUnicodeKeyLayoutData: CFStringRef;

    #[allow(non_snake_case)]
    pub fn TISGetInputSourceProperty(
        inputSource: TISInputSourceRef,
        propertyKey: CFStringRef,
    ) -> *mut c_void;

    pub fn TISCopyCurrentKeyboardLayoutInputSource() -> TISInputSourceRef;

    pub fn LMGetKbdType() -> u8;

    #[allow(non_snake_case)]
    pub fn UCKeyTranslate(
        keyLayoutPtr: *const UCKeyboardLayout,
        virtualKeyCode: u16,
        keyAction: u16,
        modifierKeyState: u32,
        keyboardType: u32,
        keyTranslateOptions: OptionBits,
        deadKeyState: *mut u32,
        maxStringLength: UniCharCount,
        actualStringLength: *mut UniCharCount,
        unicodeString: *mut UniChar,
    ) -> OSStatus;
}

// CGWindowLevel.h
//
// Note: There are two different things at play in this header:
// `CGWindowLevel` and `CGWindowLevelKey`.
//
// It seems like there was a push towards using "key" values instead of the
// raw window level values, and then you were supposed to use
// `CGWindowLevelForKey` to get the actual level.
//
// But the values that `NSWindowLevel` has are compiled in, and as such has
// to remain ABI compatible, so they're safe for us to hardcode as well.
#[allow(dead_code, non_upper_case_globals)]
mod window_level {
    const kCGNumReservedWindowLevels: i32 = 16;
    const kCGNumReservedBaseWindowLevels: i32 = 5;

    pub const kCGBaseWindowLevel: i32 = i32::MIN;
    pub const kCGMinimumWindowLevel: i32 =
        kCGBaseWindowLevel + kCGNumReservedBaseWindowLevels;
    pub const kCGMaximumWindowLevel: i32 = i32::MAX - kCGNumReservedWindowLevels;

    pub const kCGDesktopWindowLevel: i32 = kCGMinimumWindowLevel + 20;
    pub const kCGDesktopIconWindowLevel: i32 = kCGDesktopWindowLevel + 20;
    pub const kCGBackstopMenuLevel: i32 = -20;
    pub const kCGNormalWindowLevel: i32 = 0;
    pub const kCGFloatingWindowLevel: i32 = 3;
    pub const kCGTornOffMenuWindowLevel: i32 = 3;
    pub const kCGModalPanelWindowLevel: i32 = 8;
    pub const kCGUtilityWindowLevel: i32 = 19;
    pub const kCGDockWindowLevel: i32 = 20;
    pub const kCGMainMenuWindowLevel: i32 = 24;
    pub const kCGStatusWindowLevel: i32 = 25;
    pub const kCGPopUpMenuWindowLevel: i32 = 101;
    pub const kCGOverlayWindowLevel: i32 = 102;
    pub const kCGHelpWindowLevel: i32 = 200;
    pub const kCGDraggingWindowLevel: i32 = 500;
    pub const kCGScreenSaverWindowLevel: i32 = 1000;
    pub const kCGAssistiveTechHighWindowLevel: i32 = 1500;
    pub const kCGCursorWindowLevel: i32 = kCGMaximumWindowLevel - 1;
}

pub use window_level::*;
