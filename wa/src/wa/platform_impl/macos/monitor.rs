#![allow(clippy::unnecessary_cast)]

use std::{collections::VecDeque, fmt};

use core_foundation::{
    array::{CFArrayGetCount, CFArrayGetValueAtIndex},
    base::{CFRelease, TCFType},
    string::CFString,
};
use core_graphics::display::{CGDirectDisplayID, CGDisplay, CGDisplayBounds};
use objc2::rc::Id;

use super::appkit::NSScreen;
use super::ffi;
use crate::dpi::{PhysicalPosition, PhysicalSize};

#[derive(Clone)]
pub struct VideoMode {
    pub(crate) size: (u32, u32),
    pub(crate) bit_depth: u16,
    pub(crate) refresh_rate_millihertz: u32,
    pub(crate) monitor: MonitorHandle,
    pub(crate) native_mode: NativeDisplayMode,
}

impl PartialEq for VideoMode {
    fn eq(&self, other: &Self) -> bool {
        self.size == other.size
            && self.bit_depth == other.bit_depth
            && self.refresh_rate_millihertz == other.refresh_rate_millihertz
            && self.monitor == other.monitor
    }
}

impl Eq for VideoMode {}

impl std::hash::Hash for VideoMode {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.size.hash(state);
        self.bit_depth.hash(state);
        self.refresh_rate_millihertz.hash(state);
        self.monitor.hash(state);
    }
}

impl std::fmt::Debug for VideoMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoMode")
            .field("size", &self.size)
            .field("bit_depth", &self.bit_depth)
            .field("refresh_rate_millihertz", &self.refresh_rate_millihertz)
            .field("monitor", &self.monitor)
            .finish()
    }
}

pub struct NativeDisplayMode(pub ffi::CGDisplayModeRef);

unsafe impl Send for NativeDisplayMode {}

impl Drop for NativeDisplayMode {
    fn drop(&mut self) {
        unsafe {
            ffi::CGDisplayModeRelease(self.0);
        }
    }
}

impl Clone for NativeDisplayMode {
    fn clone(&self) -> Self {
        unsafe {
            ffi::CGDisplayModeRetain(self.0);
        }
        NativeDisplayMode(self.0)
    }
}

impl VideoMode {
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
}

#[derive(Clone)]
pub struct MonitorHandle(CGDirectDisplayID);

// `CGDirectDisplayID` changes on video mode change, so we cannot rely on that
// for comparisons, but we can use `CGDisplayCreateUUIDFromDisplayID` to get an
// unique identifier that persists even across system reboots
impl PartialEq for MonitorHandle {
    fn eq(&self, other: &Self) -> bool {
        unsafe {
            ffi::CGDisplayCreateUUIDFromDisplayID(self.0)
                == ffi::CGDisplayCreateUUIDFromDisplayID(other.0)
        }
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
        unsafe {
            ffi::CGDisplayCreateUUIDFromDisplayID(self.0)
                .cmp(&ffi::CGDisplayCreateUUIDFromDisplayID(other.0))
        }
    }
}

impl std::hash::Hash for MonitorHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        unsafe {
            ffi::CGDisplayCreateUUIDFromDisplayID(self.0).hash(state);
        }
    }
}

pub fn available_monitors() -> VecDeque<MonitorHandle> {
    if let Ok(displays) = CGDisplay::active_displays() {
        let mut monitors = VecDeque::with_capacity(displays.len());
        for display in displays {
            monitors.push_back(MonitorHandle(display));
        }
        monitors
    } else {
        VecDeque::with_capacity(0)
    }
}

pub fn primary_monitor() -> MonitorHandle {
    MonitorHandle(CGDisplay::main().id)
}

impl fmt::Debug for MonitorHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO: Do this using the proper fmt API
        #[derive(Debug)]
        #[allow(dead_code)]
        struct MonitorHandle {
            name: Option<String>,
            native_identifier: u32,
            size: PhysicalSize<u32>,
            position: PhysicalPosition<i32>,
            scale_factor: f64,
        }

        let monitor_id_proxy = MonitorHandle {
            name: self.name(),
            native_identifier: self.native_identifier(),
            size: self.size(),
            position: self.position(),
            scale_factor: self.scale_factor(),
        };

        monitor_id_proxy.fmt(f)
    }
}

impl MonitorHandle {
    pub fn new(id: CGDirectDisplayID) -> Self {
        MonitorHandle(id)
    }

    pub fn name(&self) -> Option<String> {
        let MonitorHandle(display_id) = *self;
        let screen_num = CGDisplay::new(display_id).model_number();
        Some(format!("Monitor #{screen_num}"))
    }

    #[inline]
    pub fn native_identifier(&self) -> u32 {
        self.0
    }

    pub fn size(&self) -> PhysicalSize<u32> {
        let MonitorHandle(display_id) = *self;
        let display = CGDisplay::new(display_id);
        let height = display.pixels_high();
        let width = display.pixels_wide();
        PhysicalSize::from_logical::<_, f64>((width as f64, height as f64), self.scale_factor())
    }

    #[inline]
    pub fn position(&self) -> PhysicalPosition<i32> {
        let bounds = unsafe { CGDisplayBounds(self.native_identifier()) };
        PhysicalPosition::from_logical::<_, f64>(
            (bounds.origin.x as f64, bounds.origin.y as f64),
            self.scale_factor(),
        )
    }

    pub fn scale_factor(&self) -> f64 {
        match self.ns_screen() {
            Some(screen) => screen.backingScaleFactor() as f64,
            None => 1.0, // default to 1.0 when we can't find the screen
        }
    }

    pub fn refresh_rate_millihertz(&self) -> Option<u32> {
        unsafe {
            let mut display_link = std::ptr::null_mut();
            if ffi::CVDisplayLinkCreateWithCGDisplay(self.0, &mut display_link)
                != ffi::kCVReturnSuccess
            {
                return None;
            }
            let time = ffi::CVDisplayLinkGetNominalOutputVideoRefreshPeriod(display_link);
            ffi::CVDisplayLinkRelease(display_link);

            // This value is indefinite if an invalid display link was specified
            if time.flags & ffi::kCVTimeIsIndefinite != 0 {
                return None;
            }

            (time.time_scale as i64)
                .checked_div(time.time_value)
                .map(|v| (v * 1000) as u32)
        }
    }

    pub fn video_modes(&self) -> impl Iterator<Item = VideoMode> {
        let refresh_rate_millihertz = self.refresh_rate_millihertz().unwrap_or(0);
        let monitor = self.clone();

        unsafe {
            let modes = {
                let array = ffi::CGDisplayCopyAllDisplayModes(self.0, std::ptr::null());
                assert!(!array.is_null(), "failed to get list of display modes");
                let array_count = CFArrayGetCount(array);
                let modes: Vec<_> = (0..array_count)
                    .map(move |i| {
                        let mode = CFArrayGetValueAtIndex(array, i) as *mut _;
                        ffi::CGDisplayModeRetain(mode);
                        mode
                    })
                    .collect();
                CFRelease(array as *const _);
                modes
            };

            modes.into_iter().map(move |mode| {
                let cg_refresh_rate_millihertz =
                    ffi::CGDisplayModeGetRefreshRate(mode).round() as i64;

                // CGDisplayModeGetRefreshRate returns 0.0 for any display that
                // isn't a CRT
                let refresh_rate_millihertz = if cg_refresh_rate_millihertz > 0 {
                    (cg_refresh_rate_millihertz * 1000) as u32
                } else {
                    refresh_rate_millihertz
                };

                let pixel_encoding =
                    CFString::wrap_under_create_rule(ffi::CGDisplayModeCopyPixelEncoding(mode))
                        .to_string();
                let bit_depth = if pixel_encoding.eq_ignore_ascii_case(ffi::IO32BitDirectPixels) {
                    32
                } else if pixel_encoding.eq_ignore_ascii_case(ffi::IO16BitDirectPixels) {
                    16
                } else if pixel_encoding.eq_ignore_ascii_case(ffi::kIO30BitDirectPixels) {
                    30
                } else {
                    unimplemented!()
                };

                VideoMode {
                    size: (
                        ffi::CGDisplayModeGetPixelWidth(mode) as u32,
                        ffi::CGDisplayModeGetPixelHeight(mode) as u32,
                    ),
                    refresh_rate_millihertz,
                    bit_depth,
                    monitor: monitor.clone(),
                    native_mode: NativeDisplayMode(mode),
                }
            })
        }
    }

    pub(crate) fn ns_screen(&self) -> Option<Id<NSScreen>> {
        let uuid = unsafe { ffi::CGDisplayCreateUUIDFromDisplayID(self.0) };
        NSScreen::screens().into_iter().find(|screen| {
            let other_native_id = screen.display_id();
            let other_uuid = unsafe {
                ffi::CGDisplayCreateUUIDFromDisplayID(other_native_id as CGDirectDisplayID)
            };
            uuid == other_uuid
        })
    }
}
