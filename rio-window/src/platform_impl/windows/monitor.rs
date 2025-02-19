use std::collections::{BTreeSet, VecDeque};
use std::hash::Hash;
use std::{io, mem, ptr};

use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, EnumDisplaySettingsExW, GetMonitorInfoW, MonitorFromPoint,
    MonitorFromWindow, DEVMODEW, DM_BITSPERPEL, DM_DISPLAYFREQUENCY, DM_PELSHEIGHT,
    DM_PELSWIDTH, ENUM_CURRENT_SETTINGS, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
    MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
};

use super::util::decode_wide;
use crate::dpi::{PhysicalPosition, PhysicalSize};
use crate::monitor::VideoModeHandle as RootVideoModeHandle;
use crate::platform_impl::platform::dpi::{dpi_to_scale_factor, get_monitor_dpi};
use crate::platform_impl::platform::util::has_flag;
use crate::platform_impl::platform::window::Window;

#[derive(Clone)]
pub struct VideoModeHandle {
    pub(crate) size: (u32, u32),
    pub(crate) bit_depth: u16,
    pub(crate) refresh_rate_millihertz: u32,
    pub(crate) monitor: MonitorHandle,
    // DEVMODEW is huge so we box it to avoid blowing up the size of rio_window::window::Fullscreen
    pub(crate) native_video_mode: Box<DEVMODEW>,
}

impl PartialEq for VideoModeHandle {
    fn eq(&self, other: &Self) -> bool {
        self.size == other.size
            && self.bit_depth == other.bit_depth
            && self.refresh_rate_millihertz == other.refresh_rate_millihertz
            && self.monitor == other.monitor
    }
}

impl Eq for VideoModeHandle {}

impl std::hash::Hash for VideoModeHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.size.hash(state);
        self.bit_depth.hash(state);
        self.refresh_rate_millihertz.hash(state);
        self.monitor.hash(state);
    }
}

impl std::fmt::Debug for VideoModeHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoModeHandle")
            .field("size", &self.size)
            .field("bit_depth", &self.bit_depth)
            .field("refresh_rate_millihertz", &self.refresh_rate_millihertz)
            .field("monitor", &self.monitor)
            .finish()
    }
}

impl VideoModeHandle {
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

#[derive(Debug, Clone, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct MonitorHandle(HMONITOR);

// Send is not implemented for HMONITOR, we have to wrap it and implement it manually.

unsafe impl Send for MonitorHandle {}
unsafe impl Sync for MonitorHandle {}

unsafe extern "system" fn monitor_enum_proc(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _place: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let monitors = data as *mut VecDeque<MonitorHandle>;
    unsafe { (*monitors).push_back(MonitorHandle::new(hmonitor)) };
    true.into() // continue enumeration
}

pub fn available_monitors() -> VecDeque<MonitorHandle> {
    let mut monitors: VecDeque<MonitorHandle> = VecDeque::new();
    unsafe {
        EnumDisplayMonitors(
            ptr::null_mut(),
            ptr::null(),
            Some(monitor_enum_proc),
            &mut monitors as *mut _ as LPARAM,
        );
    }
    monitors
}

pub fn primary_monitor() -> MonitorHandle {
    const ORIGIN: POINT = POINT { x: 0, y: 0 };
    let hmonitor = unsafe { MonitorFromPoint(ORIGIN, MONITOR_DEFAULTTOPRIMARY) };
    MonitorHandle::new(hmonitor)
}

pub fn current_monitor(hwnd: HWND) -> MonitorHandle {
    let hmonitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    MonitorHandle::new(hmonitor)
}

impl Window {
    pub fn available_monitors(&self) -> VecDeque<MonitorHandle> {
        available_monitors()
    }

    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        let monitor = primary_monitor();
        Some(monitor)
    }
}

pub(crate) fn get_monitor_info(hmonitor: HMONITOR) -> Result<MONITORINFOEXW, io::Error> {
    let mut monitor_info: MONITORINFOEXW = unsafe { mem::zeroed() };
    monitor_info.monitorInfo.cbSize = mem::size_of::<MONITORINFOEXW>() as u32;
    let status = unsafe {
        GetMonitorInfoW(
            hmonitor,
            &mut monitor_info as *mut MONITORINFOEXW as *mut MONITORINFO,
        )
    };
    if status == false.into() {
        Err(io::Error::last_os_error())
    } else {
        Ok(monitor_info)
    }
}

impl MonitorHandle {
    pub(crate) fn new(hmonitor: HMONITOR) -> Self {
        MonitorHandle(hmonitor)
    }

    #[inline]
    pub fn name(&self) -> Option<String> {
        let monitor_info = get_monitor_info(self.0).unwrap();
        Some(
            decode_wide(&monitor_info.szDevice)
                .to_string_lossy()
                .to_string(),
        )
    }

    #[inline]
    pub fn native_identifier(&self) -> String {
        self.name().unwrap()
    }

    #[inline]
    pub fn hmonitor(&self) -> HMONITOR {
        self.0
    }

    #[inline]
    pub fn size(&self) -> PhysicalSize<u32> {
        let rc_monitor = get_monitor_info(self.0).unwrap().monitorInfo.rcMonitor;
        PhysicalSize {
            width: (rc_monitor.right - rc_monitor.left) as u32,
            height: (rc_monitor.bottom - rc_monitor.top) as u32,
        }
    }

    #[inline]
    pub fn refresh_rate_millihertz(&self) -> Option<u32> {
        let monitor_info = get_monitor_info(self.0).ok()?;
        let device_name = monitor_info.szDevice.as_ptr();
        unsafe {
            let mut mode: DEVMODEW = mem::zeroed();
            mode.dmSize = mem::size_of_val(&mode) as u16;
            if EnumDisplaySettingsExW(device_name, ENUM_CURRENT_SETTINGS, &mut mode, 0)
                == false.into()
            {
                None
            } else {
                Some(mode.dmDisplayFrequency * 1000)
            }
        }
    }

    #[inline]
    pub fn position(&self) -> PhysicalPosition<i32> {
        get_monitor_info(self.0)
            .map(|info| {
                let rc_monitor = info.monitorInfo.rcMonitor;
                PhysicalPosition {
                    x: rc_monitor.left,
                    y: rc_monitor.top,
                }
            })
            .unwrap_or(PhysicalPosition { x: 0, y: 0 })
    }

    #[inline]
    pub fn scale_factor(&self) -> f64 {
        dpi_to_scale_factor(get_monitor_dpi(self.0).unwrap_or(96))
    }

    #[inline]
    pub fn video_modes(&self) -> impl Iterator<Item = VideoModeHandle> {
        // EnumDisplaySettingsExW can return duplicate values (or some of the
        // fields are probably changing, but we aren't looking at those fields
        // anyway), so we're using a BTreeSet deduplicate
        let mut modes = BTreeSet::<RootVideoModeHandle>::new();
        let mod_map = |mode: RootVideoModeHandle| mode.video_mode;

        let monitor_info = match get_monitor_info(self.0) {
            Ok(monitor_info) => monitor_info,
            Err(error) => {
                tracing::warn!("Error from get_monitor_info: {error}");
                return modes.into_iter().map(mod_map);
            }
        };

        let device_name = monitor_info.szDevice.as_ptr();

        let mut i = 0;
        loop {
            let mut mode: DEVMODEW = unsafe { mem::zeroed() };
            mode.dmSize = mem::size_of_val(&mode) as u16;
            if unsafe { EnumDisplaySettingsExW(device_name, i, &mut mode, 0) }
                == false.into()
            {
                break;
            }

            const REQUIRED_FIELDS: u32 =
                DM_BITSPERPEL | DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY;
            assert!(has_flag(mode.dmFields, REQUIRED_FIELDS));

            // Use Ord impl of RootVideoModeHandle
            modes.insert(RootVideoModeHandle {
                video_mode: VideoModeHandle {
                    size: (mode.dmPelsWidth, mode.dmPelsHeight),
                    bit_depth: mode.dmBitsPerPel as u16,
                    refresh_rate_millihertz: mode.dmDisplayFrequency * 1000,
                    monitor: self.clone(),
                    native_video_mode: Box::new(mode),
                },
            });

            i += 1;
        }

        modes.into_iter().map(mod_map)
    }
}
