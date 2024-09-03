//! # iOS / UIKit
//!
//! Winit has an OS requirement of iOS 8 or higher, and is regularly tested on
//! iOS 9.3.
//!
//! iOS's main `UIApplicationMain` does some init work that's required by all
//! UI-related code (see issue [#1705]). It is best to create your windows
//! inside `Event::Resumed`.
//!
//! [#1705]: https://github.com/rust-windowing/winit/issues/1705
//!
//! ## Building app
//!
//! To build ios app you will need rustc built for this targets:
//!
//!  - armv7-apple-ios
//!  - armv7s-apple-ios
//!  - i386-apple-ios
//!  - aarch64-apple-ios
//!  - x86_64-apple-ios
//!
//! Then
//!
//! ```
//! cargo build --target=...
//! ```
//! The simplest way to integrate your app into xcode environment is to build it
//! as a static library. Wrap your main function and export it.
//!
//! ```rust, ignore
//! #[no_mangle]
//! pub extern fn start_winit_app() {
//!     start_inner()
//! }
//!
//! fn start_inner() {
//!    ...
//! }
//! ```
//!
//! Compile project and then drag resulting .a into Xcode project. Add winit.h to xcode.
//!
//! ```ignore
//! void start_winit_app();
//! ```
//!
//! Use start_winit_app inside your xcode's main function.
//!
//!
//! ## App lifecycle and events
//!
//! iOS environment is very different from other platforms and you must be very
//! careful with it's events. Familiarize yourself with
//! [app lifecycle](https://developer.apple.com/library/ios/documentation/UIKit/Reference/UIApplicationDelegate_Protocol/).
//!
//! This is how those event are represented in winit:
//!
//!  - applicationDidBecomeActive is Resumed
//!  - applicationWillResignActive is Suspended
//!  - applicationWillTerminate is LoopExiting
//!
//! Keep in mind that after LoopExiting event is received every attempt to draw with
//! opengl will result in segfault.
//!
//! Also note that app may not receive the LoopExiting event if suspended; it might be SIGKILL'ed.

use std::os::raw::c_void;

use crate::event_loop::EventLoop;
use crate::monitor::{MonitorHandle, VideoModeHandle};
use crate::window::{Window, WindowAttributes};

/// Additional methods on [`EventLoop`] that are specific to iOS.
pub trait EventLoopExtIOS {
    /// Returns the [`Idiom`] (phone/tablet/tv/etc) for the current device.
    fn idiom(&self) -> Idiom;
}

impl<T: 'static> EventLoopExtIOS for EventLoop<T> {
    fn idiom(&self) -> Idiom {
        self.event_loop.idiom()
    }
}

/// Additional methods on [`Window`] that are specific to iOS.
pub trait WindowExtIOS {
    /// Sets the [`contentScaleFactor`] of the underlying [`UIWindow`] to `scale_factor`.
    ///
    /// The default value is device dependent, and it's recommended GLES or Metal applications set
    /// this to [`MonitorHandle::scale_factor()`].
    ///
    /// [`UIWindow`]: https://developer.apple.com/documentation/uikit/uiwindow?language=objc
    /// [`contentScaleFactor`]: https://developer.apple.com/documentation/uikit/uiview/1622657-contentscalefactor?language=objc
    fn set_scale_factor(&self, scale_factor: f64);

    /// Sets the valid orientations for the [`Window`].
    ///
    /// The default value is [`ValidOrientations::LandscapeAndPortrait`].
    ///
    /// This changes the value returned by
    /// [`-[UIViewController supportedInterfaceOrientations]`](https://developer.apple.com/documentation/uikit/uiviewcontroller/1621435-supportedinterfaceorientations?language=objc),
    /// and then calls
    /// [`-[UIViewController attemptRotationToDeviceOrientation]`](https://developer.apple.com/documentation/uikit/uiviewcontroller/1621400-attemptrotationtodeviceorientati?language=objc).
    fn set_valid_orientations(&self, valid_orientations: ValidOrientations);

    /// Sets whether the [`Window`] prefers the home indicator hidden.
    ///
    /// The default is to prefer showing the home indicator.
    ///
    /// This changes the value returned by
    /// [`-[UIViewController prefersHomeIndicatorAutoHidden]`](https://developer.apple.com/documentation/uikit/uiviewcontroller/2887510-prefershomeindicatorautohidden?language=objc),
    /// and then calls
    /// [`-[UIViewController setNeedsUpdateOfHomeIndicatorAutoHidden]`](https://developer.apple.com/documentation/uikit/uiviewcontroller/2887509-setneedsupdateofhomeindicatoraut?language=objc).
    ///
    /// This only has an effect on iOS 11.0+.
    fn set_prefers_home_indicator_hidden(&self, hidden: bool);

    /// Sets the screen edges for which the system gestures will take a lower priority than the
    /// application's touch handling.
    ///
    /// This changes the value returned by
    /// [`-[UIViewController preferredScreenEdgesDeferringSystemGestures]`](https://developer.apple.com/documentation/uikit/uiviewcontroller/2887512-preferredscreenedgesdeferringsys?language=objc),
    /// and then calls
    /// [`-[UIViewController setNeedsUpdateOfScreenEdgesDeferringSystemGestures]`](https://developer.apple.com/documentation/uikit/uiviewcontroller/2887507-setneedsupdateofscreenedgesdefer?language=objc).
    ///
    /// This only has an effect on iOS 11.0+.
    fn set_preferred_screen_edges_deferring_system_gestures(&self, edges: ScreenEdge);

    /// Sets whether the [`Window`] prefers the status bar hidden.
    ///
    /// The default is to prefer showing the status bar.
    ///
    /// This sets the value of the
    /// [`prefersStatusBarHidden`](https://developer.apple.com/documentation/uikit/uiviewcontroller/1621440-prefersstatusbarhidden?language=objc)
    /// property.
    ///
    /// [`setNeedsStatusBarAppearanceUpdate()`](https://developer.apple.com/documentation/uikit/uiviewcontroller/1621354-setneedsstatusbarappearanceupdat?language=objc)
    /// is also called for you.
    fn set_prefers_status_bar_hidden(&self, hidden: bool);

    /// Sets the preferred status bar style for the [`Window`].
    ///
    /// The default is system-defined.
    ///
    /// This sets the value of the
    /// [`preferredStatusBarStyle`](https://developer.apple.com/documentation/uikit/uiviewcontroller/1621416-preferredstatusbarstyle?language=objc)
    /// property.
    ///
    /// [`setNeedsStatusBarAppearanceUpdate()`](https://developer.apple.com/documentation/uikit/uiviewcontroller/1621354-setneedsstatusbarappearanceupdat?language=objc)
    /// is also called for you.
    fn set_preferred_status_bar_style(&self, status_bar_style: StatusBarStyle);

    /// Sets whether the [`Window`] should recognize pinch gestures.
    ///
    /// The default is to not recognize gestures.
    fn recognize_pinch_gesture(&self, should_recognize: bool);

    /// Sets whether the [`Window`] should recognize pan gestures.
    ///
    /// The default is to not recognize gestures.
    /// Installs [`UIPanGestureRecognizer`](https://developer.apple.com/documentation/uikit/uipangesturerecognizer) onto view
    ///
    /// Set the minimum number of touches required: [`minimumNumberOfTouches`](https://developer.apple.com/documentation/uikit/uipangesturerecognizer/1621208-minimumnumberoftouches)
    ///
    /// Set the maximum number of touches recognized: [`maximumNumberOfTouches`](https://developer.apple.com/documentation/uikit/uipangesturerecognizer/1621208-maximumnumberoftouches)
    fn recognize_pan_gesture(
        &self,
        should_recognize: bool,
        minimum_number_of_touches: u8,
        maximum_number_of_touches: u8,
    );

    /// Sets whether the [`Window`] should recognize double tap gestures.
    ///
    /// The default is to not recognize gestures.
    fn recognize_doubletap_gesture(&self, should_recognize: bool);

    /// Sets whether the [`Window`] should recognize rotation gestures.
    ///
    /// The default is to not recognize gestures.
    fn recognize_rotation_gesture(&self, should_recognize: bool);
}

impl WindowExtIOS for Window {
    #[inline]
    fn set_scale_factor(&self, scale_factor: f64) {
        self.window
            .maybe_queue_on_main(move |w| w.set_scale_factor(scale_factor))
    }

    #[inline]
    fn set_valid_orientations(&self, valid_orientations: ValidOrientations) {
        self.window
            .maybe_queue_on_main(move |w| w.set_valid_orientations(valid_orientations))
    }

    #[inline]
    fn set_prefers_home_indicator_hidden(&self, hidden: bool) {
        self.window
            .maybe_queue_on_main(move |w| w.set_prefers_home_indicator_hidden(hidden))
    }

    #[inline]
    fn set_preferred_screen_edges_deferring_system_gestures(&self, edges: ScreenEdge) {
        self.window.maybe_queue_on_main(move |w| {
            w.set_preferred_screen_edges_deferring_system_gestures(edges)
        })
    }

    #[inline]
    fn set_prefers_status_bar_hidden(&self, hidden: bool) {
        self.window
            .maybe_queue_on_main(move |w| w.set_prefers_status_bar_hidden(hidden))
    }

    #[inline]
    fn set_preferred_status_bar_style(&self, status_bar_style: StatusBarStyle) {
        self.window.maybe_queue_on_main(move |w| {
            w.set_preferred_status_bar_style(status_bar_style)
        })
    }

    #[inline]
    fn recognize_pinch_gesture(&self, should_recognize: bool) {
        self.window
            .maybe_queue_on_main(move |w| w.recognize_pinch_gesture(should_recognize));
    }

    #[inline]
    fn recognize_pan_gesture(
        &self,
        should_recognize: bool,
        minimum_number_of_touches: u8,
        maximum_number_of_touches: u8,
    ) {
        self.window.maybe_queue_on_main(move |w| {
            w.recognize_pan_gesture(
                should_recognize,
                minimum_number_of_touches,
                maximum_number_of_touches,
            )
        });
    }

    #[inline]
    fn recognize_doubletap_gesture(&self, should_recognize: bool) {
        self.window.maybe_queue_on_main(move |w| {
            w.recognize_doubletap_gesture(should_recognize)
        });
    }

    #[inline]
    fn recognize_rotation_gesture(&self, should_recognize: bool) {
        self.window
            .maybe_queue_on_main(move |w| w.recognize_rotation_gesture(should_recognize));
    }
}

/// Additional methods on [`WindowAttributes`] that are specific to iOS.
pub trait WindowAttributesExtIOS {
    /// Sets the [`contentScaleFactor`] of the underlying [`UIWindow`] to `scale_factor`.
    ///
    /// The default value is device dependent, and it's recommended GLES or Metal applications set
    /// this to [`MonitorHandle::scale_factor()`].
    ///
    /// [`UIWindow`]: https://developer.apple.com/documentation/uikit/uiwindow?language=objc
    /// [`contentScaleFactor`]: https://developer.apple.com/documentation/uikit/uiview/1622657-contentscalefactor?language=objc
    fn with_scale_factor(self, scale_factor: f64) -> Self;

    /// Sets the valid orientations for the [`Window`].
    ///
    /// The default value is [`ValidOrientations::LandscapeAndPortrait`].
    ///
    /// This sets the initial value returned by
    /// [`-[UIViewController supportedInterfaceOrientations]`](https://developer.apple.com/documentation/uikit/uiviewcontroller/1621435-supportedinterfaceorientations?language=objc).
    fn with_valid_orientations(self, valid_orientations: ValidOrientations) -> Self;

    /// Sets whether the [`Window`] prefers the home indicator hidden.
    ///
    /// The default is to prefer showing the home indicator.
    ///
    /// This sets the initial value returned by
    /// [`-[UIViewController prefersHomeIndicatorAutoHidden]`](https://developer.apple.com/documentation/uikit/uiviewcontroller/2887510-prefershomeindicatorautohidden?language=objc).
    ///
    /// This only has an effect on iOS 11.0+.
    fn with_prefers_home_indicator_hidden(self, hidden: bool) -> Self;

    /// Sets the screen edges for which the system gestures will take a lower priority than the
    /// application's touch handling.
    ///
    /// This sets the initial value returned by
    /// [`-[UIViewController preferredScreenEdgesDeferringSystemGestures]`](https://developer.apple.com/documentation/uikit/uiviewcontroller/2887512-preferredscreenedgesdeferringsys?language=objc).
    ///
    /// This only has an effect on iOS 11.0+.
    fn with_preferred_screen_edges_deferring_system_gestures(
        self,
        edges: ScreenEdge,
    ) -> Self;

    /// Sets whether the [`Window`] prefers the status bar hidden.
    ///
    /// The default is to prefer showing the status bar.
    ///
    /// This sets the initial value returned by
    /// [`-[UIViewController prefersStatusBarHidden]`](https://developer.apple.com/documentation/uikit/uiviewcontroller/1621440-prefersstatusbarhidden?language=objc).
    fn with_prefers_status_bar_hidden(self, hidden: bool) -> Self;

    /// Sets the style of the [`Window`]'s status bar.
    ///
    /// The default is system-defined.
    ///
    /// This sets the initial value returned by
    /// [`-[UIViewController preferredStatusBarStyle]`](https://developer.apple.com/documentation/uikit/uiviewcontroller/1621416-preferredstatusbarstyle?language=objc),
    fn with_preferred_status_bar_style(self, status_bar_style: StatusBarStyle) -> Self;
}

impl WindowAttributesExtIOS for WindowAttributes {
    #[inline]
    fn with_scale_factor(mut self, scale_factor: f64) -> Self {
        self.platform_specific.scale_factor = Some(scale_factor);
        self
    }

    #[inline]
    fn with_valid_orientations(mut self, valid_orientations: ValidOrientations) -> Self {
        self.platform_specific.valid_orientations = valid_orientations;
        self
    }

    #[inline]
    fn with_prefers_home_indicator_hidden(mut self, hidden: bool) -> Self {
        self.platform_specific.prefers_home_indicator_hidden = hidden;
        self
    }

    #[inline]
    fn with_preferred_screen_edges_deferring_system_gestures(
        mut self,
        edges: ScreenEdge,
    ) -> Self {
        self.platform_specific
            .preferred_screen_edges_deferring_system_gestures = edges;
        self
    }

    #[inline]
    fn with_prefers_status_bar_hidden(mut self, hidden: bool) -> Self {
        self.platform_specific.prefers_status_bar_hidden = hidden;
        self
    }

    #[inline]
    fn with_preferred_status_bar_style(
        mut self,
        status_bar_style: StatusBarStyle,
    ) -> Self {
        self.platform_specific.preferred_status_bar_style = status_bar_style;
        self
    }
}

/// Additional methods on [`MonitorHandle`] that are specific to iOS.
pub trait MonitorHandleExtIOS {
    /// Returns a pointer to the [`UIScreen`] that is used by this monitor.
    ///
    /// [`UIScreen`]: https://developer.apple.com/documentation/uikit/uiscreen?language=objc
    fn ui_screen(&self) -> *mut c_void;

    /// Returns the preferred [`VideoModeHandle`] for this monitor.
    ///
    /// This translates to a call to [`-[UIScreen preferredMode]`](https://developer.apple.com/documentation/uikit/uiscreen/1617823-preferredmode?language=objc).
    fn preferred_video_mode(&self) -> VideoModeHandle;
}

impl MonitorHandleExtIOS for MonitorHandle {
    #[inline]
    fn ui_screen(&self) -> *mut c_void {
        // SAFETY: The marker is only used to get the pointer of the screen
        let mtm = unsafe { objc2_foundation::MainThreadMarker::new_unchecked() };
        objc2::rc::Retained::as_ptr(self.inner.ui_screen(mtm)) as *mut c_void
    }

    #[inline]
    fn preferred_video_mode(&self) -> VideoModeHandle {
        VideoModeHandle {
            video_mode: self.inner.preferred_video_mode(),
        }
    }
}

/// Valid orientations for a particular [`Window`].
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValidOrientations {
    /// Excludes `PortraitUpsideDown` on iphone
    #[default]
    LandscapeAndPortrait,

    Landscape,

    /// Excludes `PortraitUpsideDown` on iphone
    Portrait,
}

/// The device [idiom].
///
/// [idiom]: https://developer.apple.com/documentation/uikit/uidevice/1620037-userinterfaceidiom?language=objc
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Idiom {
    Unspecified,

    /// iPhone and iPod touch.
    Phone,

    /// iPad.
    Pad,

    /// tvOS and Apple TV.
    TV,
    CarPlay,
}

bitflags::bitflags! {
    /// The [edges] of a screen.
    ///
    /// [edges]: https://developer.apple.com/documentation/uikit/uirectedge?language=objc
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ScreenEdge: u8 {
        const NONE   = 0;
        const TOP    = 1 << 0;
        const LEFT   = 1 << 1;
        const BOTTOM = 1 << 2;
        const RIGHT  = 1 << 3;
        const ALL = ScreenEdge::TOP.bits() | ScreenEdge::LEFT.bits()
            | ScreenEdge::BOTTOM.bits() | ScreenEdge::RIGHT.bits();
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatusBarStyle {
    #[default]
    Default,
    LightContent,
    DarkContent,
}
