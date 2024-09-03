#![allow(clippy::unnecessary_cast)]

use std::collections::{BTreeSet, VecDeque};
use std::{fmt, hash, ptr};

use objc2::mutability::IsRetainable;
use objc2::rc::Retained;
use objc2::Message;
use objc2_foundation::{run_on_main, MainThreadBound, MainThreadMarker, NSInteger};
use objc2_ui_kit::{UIScreen, UIScreenMode};

use crate::dpi::{PhysicalPosition, PhysicalSize};
use crate::monitor::VideoModeHandle as RootVideoModeHandle;
use crate::platform_impl::platform::app_state;

// Workaround for `MainThreadBound` implementing almost no traits
#[derive(Debug)]
struct MainThreadBoundDelegateImpls<T>(MainThreadBound<Retained<T>>);

impl<T: IsRetainable + Message> Clone for MainThreadBoundDelegateImpls<T> {
    fn clone(&self) -> Self {
        Self(run_on_main(|mtm| {
            MainThreadBound::new(Retained::clone(self.0.get(mtm)), mtm)
        }))
    }
}

impl<T: IsRetainable + Message> hash::Hash for MainThreadBoundDelegateImpls<T> {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        // SAFETY: Marker only used to get the pointer
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        Retained::as_ptr(self.0.get(mtm)).hash(state);
    }
}

impl<T: IsRetainable + Message> PartialEq for MainThreadBoundDelegateImpls<T> {
    fn eq(&self, other: &Self) -> bool {
        // SAFETY: Marker only used to get the pointer
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        Retained::as_ptr(self.0.get(mtm)) == Retained::as_ptr(other.0.get(mtm))
    }
}

impl<T: IsRetainable + Message> Eq for MainThreadBoundDelegateImpls<T> {}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct VideoModeHandle {
    pub(crate) size: (u32, u32),
    pub(crate) bit_depth: u16,
    pub(crate) refresh_rate_millihertz: u32,
    screen_mode: MainThreadBoundDelegateImpls<UIScreenMode>,
    pub(crate) monitor: MonitorHandle,
}

impl VideoModeHandle {
    fn new(
        uiscreen: Retained<UIScreen>,
        screen_mode: Retained<UIScreenMode>,
        mtm: MainThreadMarker,
    ) -> VideoModeHandle {
        let refresh_rate_millihertz = refresh_rate_millihertz(&uiscreen);
        let size = screen_mode.size();
        VideoModeHandle {
            size: (size.width as u32, size.height as u32),
            bit_depth: 32,
            refresh_rate_millihertz,
            screen_mode: MainThreadBoundDelegateImpls(MainThreadBound::new(
                screen_mode,
                mtm,
            )),
            monitor: MonitorHandle::new(uiscreen),
        }
    }

    pub fn size(&self) -> PhysicalSize<u32> {
        self.size.into()
    }

    pub fn bit_depth(&self) -> u16 {
        self.bit_depth
    }

    pub fn refresh_rate_millihertz(&self) -> u32 {
        self.refresh_rate_millihertz
    }

    pub fn monitor(&self) -> MonitorHandle {
        self.monitor.clone()
    }

    pub(super) fn screen_mode(&self, mtm: MainThreadMarker) -> &Retained<UIScreenMode> {
        self.screen_mode.0.get(mtm)
    }
}

pub struct MonitorHandle {
    ui_screen: MainThreadBound<Retained<UIScreen>>,
}

impl Clone for MonitorHandle {
    fn clone(&self) -> Self {
        run_on_main(|mtm| Self {
            ui_screen: MainThreadBound::new(self.ui_screen.get(mtm).clone(), mtm),
        })
    }
}

impl hash::Hash for MonitorHandle {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        (self as *const Self).hash(state);
    }
}

impl PartialEq for MonitorHandle {
    fn eq(&self, other: &Self) -> bool {
        ptr::eq(self, other)
    }
}

impl Eq for MonitorHandle {}

impl PartialOrd for MonitorHandle {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MonitorHandle {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // TODO: Make a better ordering
        (self as *const Self).cmp(&(other as *const Self))
    }
}

impl fmt::Debug for MonitorHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MonitorHandle")
            .field("name", &self.name())
            .field("size", &self.size())
            .field("position", &self.position())
            .field("scale_factor", &self.scale_factor())
            .field("refresh_rate_millihertz", &self.refresh_rate_millihertz())
            .finish_non_exhaustive()
    }
}

impl MonitorHandle {
    pub(crate) fn new(ui_screen: Retained<UIScreen>) -> Self {
        // Holding `Retained<UIScreen>` implies we're on the main thread.
        let mtm = MainThreadMarker::new().unwrap();
        Self {
            ui_screen: MainThreadBound::new(ui_screen, mtm),
        }
    }

    pub fn name(&self) -> Option<String> {
        run_on_main(|mtm| {
            #[allow(deprecated)]
            let main = UIScreen::mainScreen(mtm);
            if *self.ui_screen(mtm) == main {
                Some("Primary".to_string())
            } else if Some(self.ui_screen(mtm)) == main.mirroredScreen().as_ref() {
                Some("Mirrored".to_string())
            } else {
                #[allow(deprecated)]
                UIScreen::screens(mtm)
                    .iter()
                    .position(|rhs| rhs == &**self.ui_screen(mtm))
                    .map(|idx| idx.to_string())
            }
        })
    }

    pub fn size(&self) -> PhysicalSize<u32> {
        let bounds = self
            .ui_screen
            .get_on_main(|ui_screen| ui_screen.nativeBounds());
        PhysicalSize::new(bounds.size.width as u32, bounds.size.height as u32)
    }

    pub fn position(&self) -> PhysicalPosition<i32> {
        let bounds = self
            .ui_screen
            .get_on_main(|ui_screen| ui_screen.nativeBounds());
        (bounds.origin.x as f64, bounds.origin.y as f64).into()
    }

    pub fn scale_factor(&self) -> f64 {
        self.ui_screen
            .get_on_main(|ui_screen| ui_screen.nativeScale()) as f64
    }

    pub fn refresh_rate_millihertz(&self) -> Option<u32> {
        Some(
            self.ui_screen
                .get_on_main(|ui_screen| refresh_rate_millihertz(ui_screen)),
        )
    }

    pub fn video_modes(&self) -> impl Iterator<Item = VideoModeHandle> {
        run_on_main(|mtm| {
            let ui_screen = self.ui_screen(mtm);
            // Use Ord impl of RootVideoModeHandle

            let modes: BTreeSet<_> = ui_screen
                .availableModes()
                .into_iter()
                .map(|mode| RootVideoModeHandle {
                    video_mode: VideoModeHandle::new(ui_screen.clone(), mode, mtm),
                })
                .collect();

            modes.into_iter().map(|mode| mode.video_mode)
        })
    }

    pub(crate) fn ui_screen(&self, mtm: MainThreadMarker) -> &Retained<UIScreen> {
        self.ui_screen.get(mtm)
    }

    pub fn preferred_video_mode(&self) -> VideoModeHandle {
        run_on_main(|mtm| {
            VideoModeHandle::new(
                self.ui_screen(mtm).clone(),
                self.ui_screen(mtm).preferredMode().unwrap(),
                mtm,
            )
        })
    }
}

fn refresh_rate_millihertz(uiscreen: &UIScreen) -> u32 {
    let refresh_rate_millihertz: NSInteger = {
        let os_capabilities = app_state::os_capabilities();
        if os_capabilities.maximum_frames_per_second {
            uiscreen.maximumFramesPerSecond()
        } else {
            // https://developer.apple.com/library/archive/technotes/tn2460/_index.html
            // https://en.wikipedia.org/wiki/IPad_Pro#Model_comparison
            //
            // All iOS devices support 60 fps, and on devices where `maximumFramesPerSecond` is not
            // supported, they are all guaranteed to have 60hz refresh rates. This does not
            // correctly handle external displays. ProMotion displays support 120fps, but they were
            // introduced at the same time as the `maximumFramesPerSecond` API.
            //
            // FIXME: earlier OSs could calculate the refresh rate using
            // `-[CADisplayLink duration]`.
            os_capabilities.maximum_frames_per_second_err_msg("defaulting to 60 fps");
            60
        }
    };

    refresh_rate_millihertz as u32 * 1000
}

pub fn uiscreens(mtm: MainThreadMarker) -> VecDeque<MonitorHandle> {
    #[allow(deprecated)]
    UIScreen::screens(mtm)
        .into_iter()
        .map(MonitorHandle::new)
        .collect()
}
