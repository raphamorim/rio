use super::{util, X11Error, XConnection};
use crate::dpi::{PhysicalPosition, PhysicalSize};
use crate::platform_impl::VideoModeHandle as PlatformVideoModeHandle;
use x11rb::connection::RequestConnection;
use x11rb::protocol::randr::{self, ConnectionExt as _};
use x11rb::protocol::xproto;

// Used for testing. This should always be committed as false.
const DISABLE_MONITOR_LIST_CACHING: bool = false;

impl XConnection {
    pub fn invalidate_cached_monitor_list(&self) -> Option<Vec<MonitorHandle>> {
        // We update this lazily.
        self.monitor_handles.lock().unwrap().take()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VideoModeHandle {
    pub(crate) size: (u32, u32),
    pub(crate) bit_depth: u16,
    pub(crate) refresh_rate_millihertz: u32,
    pub(crate) native_mode: randr::Mode,
    pub(crate) monitor: Option<MonitorHandle>,
}

impl VideoModeHandle {
    #[inline]
    pub fn size(&self) -> PhysicalSize<u32> {
        self.size.into()
    }

    #[inline]
    pub fn bit_depth(&self) -> u16 {
        self.bit_depth
    }

    #[inline]
    pub fn refresh_rate_millihertz(&self) -> u32 {
        self.refresh_rate_millihertz
    }

    #[inline]
    pub fn monitor(&self) -> MonitorHandle {
        self.monitor.clone().unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct MonitorHandle {
    /// The actual id
    pub(crate) id: randr::Crtc,
    /// The name of the monitor
    pub(crate) name: String,
    /// The size of the monitor
    dimensions: (u32, u32),
    /// The position of the monitor in the X screen
    position: (i32, i32),
    /// If the monitor is the primary one
    primary: bool,
    /// The refresh rate used by monitor.
    refresh_rate_millihertz: Option<u32>,
    /// The DPI scale factor
    pub(crate) scale_factor: f64,
    /// Used to determine which windows are on this monitor
    pub(crate) rect: util::AaRect,
    /// Supported video modes on this monitor
    video_modes: Vec<VideoModeHandle>,
}

impl PartialEq for MonitorHandle {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
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
        self.id.cmp(&other.id)
    }
}

impl std::hash::Hash for MonitorHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

#[inline]
pub fn mode_refresh_rate_millihertz(mode: &randr::ModeInfo) -> Option<u32> {
    if mode.dot_clock > 0 && mode.htotal > 0 && mode.vtotal > 0 {
        #[allow(clippy::unnecessary_cast)]
        Some(
            (mode.dot_clock as u64 * 1000 / (mode.htotal as u64 * mode.vtotal as u64))
                as u32,
        )
    } else {
        None
    }
}

impl MonitorHandle {
    fn new(
        xconn: &XConnection,
        resources: &ScreenResources,
        id: randr::Crtc,
        crtc: &randr::GetCrtcInfoReply,
        primary: bool,
    ) -> Option<Self> {
        let (name, scale_factor, video_modes) = xconn.get_output_info(resources, crtc)?;
        let dimensions = (crtc.width as u32, crtc.height as u32);
        let position = (crtc.x as i32, crtc.y as i32);

        // Get the refresh rate of the current video mode.
        let current_mode = crtc.mode;
        let screen_modes = resources.modes();
        let refresh_rate_millihertz = screen_modes
            .iter()
            .find(|mode| mode.id == current_mode)
            .and_then(mode_refresh_rate_millihertz);

        let rect = util::AaRect::new(position, dimensions);

        Some(MonitorHandle {
            id,
            name,
            refresh_rate_millihertz,
            scale_factor,
            dimensions,
            position,
            primary,
            rect,
            video_modes,
        })
    }

    pub fn dummy() -> Self {
        MonitorHandle {
            id: 0,
            name: "<dummy monitor>".into(),
            scale_factor: 1.0,
            dimensions: (1, 1),
            position: (0, 0),
            refresh_rate_millihertz: None,
            primary: true,
            rect: util::AaRect::new((0, 0), (1, 1)),
            video_modes: Vec::new(),
        }
    }

    pub(crate) fn is_dummy(&self) -> bool {
        // Zero is an invalid XID value; no real monitor will have it
        self.id == 0
    }

    pub fn name(&self) -> Option<String> {
        Some(self.name.clone())
    }

    #[inline]
    pub fn native_identifier(&self) -> u32 {
        self.id as _
    }

    pub fn size(&self) -> PhysicalSize<u32> {
        self.dimensions.into()
    }

    pub fn position(&self) -> PhysicalPosition<i32> {
        self.position.into()
    }

    pub fn refresh_rate_millihertz(&self) -> Option<u32> {
        self.refresh_rate_millihertz
    }

    #[inline]
    pub fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    #[inline]
    pub fn video_modes(&self) -> impl Iterator<Item = PlatformVideoModeHandle> {
        let monitor = self.clone();
        self.video_modes.clone().into_iter().map(move |mut x| {
            x.monitor = Some(monitor.clone());
            PlatformVideoModeHandle::X(x)
        })
    }
}

impl XConnection {
    pub fn get_monitor_for_window(
        &self,
        window_rect: Option<util::AaRect>,
    ) -> Result<MonitorHandle, X11Error> {
        let monitors = self.available_monitors()?;

        if monitors.is_empty() {
            // Return a dummy monitor to avoid panicking
            return Ok(MonitorHandle::dummy());
        }

        let default = monitors.first().unwrap();

        let window_rect = match window_rect {
            Some(rect) => rect,
            None => return Ok(default.to_owned()),
        };

        let mut largest_overlap = 0;
        let mut matched_monitor = default;
        for monitor in &monitors {
            let overlapping_area = window_rect.get_overlapping_area(&monitor.rect);
            if overlapping_area > largest_overlap {
                largest_overlap = overlapping_area;
                matched_monitor = monitor;
            }
        }

        Ok(matched_monitor.to_owned())
    }

    fn query_monitor_list(&self) -> Result<Vec<MonitorHandle>, X11Error> {
        let root = self.default_root();
        let resources = ScreenResources::from_connection(
            self.xcb_connection(),
            root,
            self.randr_version(),
        )?;

        // Pipeline all of the get-crtc requests.
        let mut crtc_cookies = Vec::with_capacity(resources.crtcs().len());
        for &crtc in resources.crtcs() {
            crtc_cookies.push(
                self.xcb_connection()
                    .randr_get_crtc_info(crtc, x11rb::CURRENT_TIME)?,
            );
        }

        // Do this here so we do all of our requests in one shot.
        let primary = self
            .xcb_connection()
            .randr_get_output_primary(root.root)?
            .reply()?
            .output;

        let mut crtc_infos = Vec::with_capacity(crtc_cookies.len());
        for cookie in crtc_cookies {
            let reply = cookie.reply()?;
            crtc_infos.push(reply);
        }

        let mut has_primary = false;
        let mut available_monitors = Vec::with_capacity(resources.crtcs().len());
        for (crtc_id, crtc) in resources.crtcs().iter().zip(crtc_infos.iter()) {
            if crtc.width == 0 || crtc.height == 0 || crtc.outputs.is_empty() {
                continue;
            }

            let is_primary = crtc.outputs[0] == primary;
            has_primary |= is_primary;
            let monitor =
                MonitorHandle::new(self, &resources, *crtc_id, crtc, is_primary);
            available_monitors.extend(monitor);
        }

        // If we don't have a primary monitor, just pick one ourselves!
        if !has_primary {
            if let Some(ref mut fallback) = available_monitors.first_mut() {
                // Setting this here will come in handy if we ever add an `is_primary` method.
                fallback.primary = true;
            }
        }

        Ok(available_monitors)
    }

    pub fn available_monitors(&self) -> Result<Vec<MonitorHandle>, X11Error> {
        let mut monitors_lock = self.monitor_handles.lock().unwrap();
        match *monitors_lock {
            Some(ref monitors) => Ok(monitors.clone()),
            None => {
                let monitors = self.query_monitor_list()?;
                if !DISABLE_MONITOR_LIST_CACHING {
                    *monitors_lock = Some(monitors.clone());
                }
                Ok(monitors)
            }
        }
    }

    #[inline]
    pub fn primary_monitor(&self) -> Result<MonitorHandle, X11Error> {
        Ok(self
            .available_monitors()?
            .into_iter()
            .find(|monitor| monitor.primary)
            .unwrap_or_else(MonitorHandle::dummy))
    }

    pub fn select_xrandr_input(&self, root: xproto::Window) -> Result<u8, X11Error> {
        use randr::NotifyMask;

        // Get extension info.
        let info = self
            .xcb_connection()
            .extension_information(randr::X11_EXTENSION_NAME)?
            .ok_or(X11Error::MissingExtension(randr::X11_EXTENSION_NAME))?;

        // Select input data.
        let event_mask = NotifyMask::CRTC_CHANGE
            | NotifyMask::OUTPUT_PROPERTY
            | NotifyMask::SCREEN_CHANGE;
        self.xcb_connection().randr_select_input(root, event_mask)?;

        Ok(info.first_event)
    }
}

pub struct ScreenResources {
    /// List of attached modes.
    modes: Vec<randr::ModeInfo>,

    /// List of attached CRTCs.
    crtcs: Vec<randr::Crtc>,
}

impl ScreenResources {
    pub(crate) fn modes(&self) -> &[randr::ModeInfo] {
        &self.modes
    }

    pub(crate) fn crtcs(&self) -> &[randr::Crtc] {
        &self.crtcs
    }

    pub(crate) fn from_connection(
        conn: &impl x11rb::connection::Connection,
        root: &x11rb::protocol::xproto::Screen,
        (major_version, minor_version): (u32, u32),
    ) -> Result<Self, X11Error> {
        if (major_version == 1 && minor_version >= 3) || major_version > 1 {
            let reply = conn
                .randr_get_screen_resources_current(root.root)?
                .reply()?;
            Ok(Self::from_get_screen_resources_current_reply(reply))
        } else {
            let reply = conn.randr_get_screen_resources(root.root)?.reply()?;
            Ok(Self::from_get_screen_resources_reply(reply))
        }
    }

    pub(crate) fn from_get_screen_resources_reply(
        reply: randr::GetScreenResourcesReply,
    ) -> Self {
        Self {
            modes: reply.modes,
            crtcs: reply.crtcs,
        }
    }

    pub(crate) fn from_get_screen_resources_current_reply(
        reply: randr::GetScreenResourcesCurrentReply,
    ) -> Self {
        Self {
            modes: reply.modes,
            crtcs: reply.crtcs,
        }
    }
}
