use icrate::Foundation::{MainThreadMarker, NSArray, NSInteger, NSObject, NSUInteger};
use objc2::rc::Id;
use objc2::runtime::AnyObject;
use objc2::{extern_class, extern_methods, msg_send_id, mutability, ClassType};
use objc2::{Encode, Encoding};

use super::{NSAppearance, NSEvent, NSMenu, NSResponder, NSWindow};

extern_class!(
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct NSApplication;

    unsafe impl ClassType for NSApplication {
        #[inherits(NSObject)]
        type Super = NSResponder;
        type Mutability = mutability::InteriorMutable;
    }
);

pub(crate) fn NSApp() -> Id<NSApplication> {
    // TODO: Only allow access from main thread
    NSApplication::shared(unsafe { MainThreadMarker::new_unchecked() })
}

extern_methods!(
    unsafe impl NSApplication {
        /// This can only be called on the main thread since it may initialize
        /// the application and since it's parameters may be changed by the main
        /// thread at any time (hence it is only safe to access on the main thread).
        pub fn shared(_mtm: MainThreadMarker) -> Id<Self> {
            let app: Option<_> = unsafe { msg_send_id![Self::class(), sharedApplication] };
            // SAFETY: `sharedApplication` always initializes the app if it isn't already
            unsafe { app.unwrap_unchecked() }
        }

        #[method_id(currentEvent)]
        pub fn currentEvent(&self) -> Option<Id<NSEvent>>;

        #[method(postEvent:atStart:)]
        pub fn postEvent_atStart(&self, event: &NSEvent, front_of_queue: bool);

        #[method(presentationOptions)]
        pub fn presentationOptions(&self) -> NSApplicationPresentationOptions;

        #[method_id(windows)]
        pub fn windows(&self) -> Id<NSArray<NSWindow>>;

        #[method_id(keyWindow)]
        pub fn keyWindow(&self) -> Option<Id<NSWindow>>;

        // TODO: NSApplicationDelegate
        #[method(setDelegate:)]
        pub fn setDelegate(&self, delegate: &AnyObject);

        #[method(setPresentationOptions:)]
        pub fn setPresentationOptions(&self, options: NSApplicationPresentationOptions);

        #[method(hide:)]
        pub fn hide(&self, sender: Option<&AnyObject>);

        #[method(orderFrontCharacterPalette:)]
        #[allow(dead_code)]
        pub fn orderFrontCharacterPalette(&self, sender: Option<&AnyObject>);

        #[method(hideOtherApplications:)]
        pub fn hideOtherApplications(&self, sender: Option<&AnyObject>);

        #[method(stop:)]
        pub fn stop(&self, sender: Option<&AnyObject>);

        #[method(activateIgnoringOtherApps:)]
        pub fn activateIgnoringOtherApps(&self, ignore: bool);

        #[method(requestUserAttention:)]
        pub fn requestUserAttention(&self, type_: NSRequestUserAttentionType) -> NSInteger;

        #[method(setActivationPolicy:)]
        pub fn setActivationPolicy(&self, policy: NSApplicationActivationPolicy) -> bool;

        #[method(setMainMenu:)]
        pub fn setMainMenu(&self, menu: &NSMenu);

        #[method_id(effectiveAppearance)]
        pub fn effectiveAppearance(&self) -> Id<NSAppearance>;

        #[method(setAppearance:)]
        pub fn setAppearance(&self, appearance: Option<&NSAppearance>);

        #[method(run)]
        pub unsafe fn run(&self);
    }
);

#[allow(dead_code)]
#[repr(isize)] // NSInteger
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NSApplicationActivationPolicy {
    NSApplicationActivationPolicyRegular = 0,
    NSApplicationActivationPolicyAccessory = 1,
    NSApplicationActivationPolicyProhibited = 2,
    NSApplicationActivationPolicyERROR = -1,
}

unsafe impl Encode for NSApplicationActivationPolicy {
    const ENCODING: Encoding = NSInteger::ENCODING;
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct NSApplicationPresentationOptions: NSUInteger {
        const NSApplicationPresentationDefault = 0;
        const NSApplicationPresentationAutoHideDock = 1 << 0;
        const NSApplicationPresentationHideDock = 1 << 1;
        const NSApplicationPresentationAutoHideMenuBar = 1 << 2;
        const NSApplicationPresentationHideMenuBar = 1 << 3;
        const NSApplicationPresentationDisableAppleMenu = 1 << 4;
        const NSApplicationPresentationDisableProcessSwitching = 1 << 5;
        const NSApplicationPresentationDisableForceQuit = 1 << 6;
        const NSApplicationPresentationDisableSessionTermination = 1 << 7;
        const NSApplicationPresentationDisableHideApplication = 1 << 8;
        const NSApplicationPresentationDisableMenuBarTransparency = 1 << 9;
        const NSApplicationPresentationFullScreen = 1 << 10;
        const NSApplicationPresentationAutoHideToolbar = 1 << 11;
    }
}

unsafe impl Encode for NSApplicationPresentationOptions {
    const ENCODING: Encoding = NSUInteger::ENCODING;
}

#[repr(usize)] // NSUInteger
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NSRequestUserAttentionType {
    NSCriticalRequest = 0,
    NSInformationalRequest = 10,
}

unsafe impl Encode for NSRequestUserAttentionType {
    const ENCODING: Encoding = NSUInteger::ENCODING;
}
