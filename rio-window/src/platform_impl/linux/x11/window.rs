use std::ffi::CString;
use std::mem::replace;
use std::os::raw::*;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::{cmp, env};

use tracing::{debug, info, warn};
use x11rb::connection::Connection;
use x11rb::properties::{WmHints, WmSizeHints, WmSizeHintsSpecification};
use x11rb::protocol::shape::SK;
use x11rb::protocol::xfixes::{ConnectionExt, RegionWrapper};
use x11rb::protocol::xproto::{self, ConnectionExt as _, Rectangle};
use x11rb::protocol::{randr, xinput};

use crate::cursor::{Cursor, CustomCursor as RootCustomCursor};
use crate::dpi::{PhysicalPosition, PhysicalSize, Position, Size};
use crate::error::{ExternalError, NotSupportedError, OsError as RootOsError};
use crate::event::{Event, InnerSizeWriter, WindowEvent};
use crate::event_loop::AsyncRequestSerial;
use crate::platform::x11::WindowType;
use crate::platform_impl::x11::atoms::*;
use crate::platform_impl::x11::{
    xinput_fp1616_to_float, MonitorHandle as X11MonitorHandle, WakeSender, X11Error,
};
use crate::platform_impl::{
    Fullscreen, MonitorHandle as PlatformMonitorHandle, OsError, PlatformCustomCursor,
    PlatformIcon, VideoModeHandle as PlatformVideoModeHandle,
};
use crate::window::{
    CursorGrabMode, ImePurpose, ResizeDirection, Theme, UserAttentionType,
    WindowAttributes, WindowButtons, WindowLevel,
};

use super::util::{self, SelectedCursor};
use super::{
    ffi, ActiveEventLoop, CookieResultExt, ImeRequest, ImeSender, VoidCookie, WindowId,
    XConnection,
};

#[derive(Debug)]
pub struct SharedState {
    pub cursor_pos: Option<(f64, f64)>,
    pub size: Option<(u32, u32)>,
    pub position: Option<(i32, i32)>,
    pub inner_position: Option<(i32, i32)>,
    pub inner_position_rel_parent: Option<(i32, i32)>,
    pub is_resizable: bool,
    pub is_decorated: bool,
    pub last_monitor: X11MonitorHandle,
    pub dpi_adjusted: Option<(u32, u32)>,
    pub(crate) fullscreen: Option<Fullscreen>,
    // Set when application calls `set_fullscreen` when window is not visible
    pub(crate) desired_fullscreen: Option<Option<Fullscreen>>,
    // Used to restore position after exiting fullscreen
    pub restore_position: Option<(i32, i32)>,
    // Used to restore video mode after exiting fullscreen
    pub desktop_video_mode: Option<(randr::Crtc, randr::Mode)>,
    pub frame_extents: Option<util::FrameExtentsHeuristic>,
    pub min_inner_size: Option<Size>,
    pub max_inner_size: Option<Size>,
    pub resize_increments: Option<Size>,
    pub base_size: Option<Size>,
    pub visibility: Visibility,
    pub has_focus: bool,
    // Use `Option` to not apply hittest logic when it was never requested.
    pub cursor_hittest: Option<bool>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Visibility {
    No,
    Yes,
    // Waiting for VisibilityNotify
    YesWait,
}

impl SharedState {
    fn new(
        last_monitor: X11MonitorHandle,
        window_attributes: &WindowAttributes,
    ) -> Mutex<Self> {
        let visibility = if window_attributes.visible {
            Visibility::YesWait
        } else {
            Visibility::No
        };

        Mutex::new(SharedState {
            last_monitor,
            visibility,

            is_resizable: window_attributes.resizable,
            is_decorated: window_attributes.decorations,
            cursor_pos: None,
            size: None,
            position: None,
            inner_position: None,
            inner_position_rel_parent: None,
            dpi_adjusted: None,
            fullscreen: None,
            desired_fullscreen: None,
            restore_position: None,
            desktop_video_mode: None,
            frame_extents: None,
            min_inner_size: None,
            max_inner_size: None,
            resize_increments: None,
            base_size: None,
            has_focus: false,
            cursor_hittest: None,
        })
    }
}

unsafe impl Send for UnownedWindow {}
unsafe impl Sync for UnownedWindow {}

pub struct UnownedWindow {
    pub(crate) xconn: Arc<XConnection>, // never changes
    xwindow: xproto::Window,            // never changes
    #[allow(dead_code)]
    visual: u32, // never changes
    root: xproto::Window,               // never changes
    #[allow(dead_code)]
    screen_id: i32, // never changes
    selected_cursor: Mutex<SelectedCursor>,
    cursor_grabbed_mode: Mutex<CursorGrabMode>,
    #[allow(clippy::mutex_atomic)]
    cursor_visible: Mutex<bool>,
    ime_sender: Mutex<ImeSender>,
    pub shared_state: Mutex<SharedState>,
    redraw_sender: WakeSender<WindowId>,
    activation_sender: WakeSender<super::ActivationToken>,
}

macro_rules! leap {
    ($e:expr) => {
        match $e {
            Ok(x) => x,
            Err(err) => {
                return Err(os_error!(OsError::XError(X11Error::from(err).into())))
            }
        }
    };
}

impl UnownedWindow {
    #[allow(clippy::unnecessary_cast)]
    pub(crate) fn new(
        event_loop: &ActiveEventLoop,
        window_attrs: WindowAttributes,
    ) -> Result<UnownedWindow, RootOsError> {
        let xconn = &event_loop.xconn;
        let atoms = xconn.atoms();
        let root = match window_attrs.parent_window.as_ref().map(|handle| handle.0) {
            Some(raw_window_handle::RawWindowHandle::Xlib(handle)) => {
                handle.window as xproto::Window
            }
            Some(raw_window_handle::RawWindowHandle::Xcb(handle)) => handle.window.get(),
            Some(raw) => unreachable!("Invalid raw window handle {raw:?} on X11"),
            None => event_loop.root,
        };

        let mut monitors = leap!(xconn.available_monitors());
        let guessed_monitor = if monitors.is_empty() {
            X11MonitorHandle::dummy()
        } else {
            xconn
                .query_pointer(root, util::VIRTUAL_CORE_POINTER)
                .ok()
                .and_then(|pointer_state| {
                    let (x, y) =
                        (pointer_state.root_x as i64, pointer_state.root_y as i64);

                    for i in 0..monitors.len() {
                        if monitors[i].rect.contains_point(x, y) {
                            return Some(monitors.swap_remove(i));
                        }
                    }

                    None
                })
                .unwrap_or_else(|| monitors.swap_remove(0))
        };
        let scale_factor = guessed_monitor.scale_factor();

        info!("Guessed window scale factor: {}", scale_factor);

        let max_inner_size: Option<(u32, u32)> = window_attrs
            .max_inner_size
            .map(|size| size.to_physical::<u32>(scale_factor).into());
        let min_inner_size: Option<(u32, u32)> = window_attrs
            .min_inner_size
            .map(|size| size.to_physical::<u32>(scale_factor).into());

        let position = window_attrs
            .position
            .map(|position| position.to_physical::<i32>(scale_factor));

        let dimensions = {
            // x11 only applies constraints when the window is actively resized
            // by the user, so we have to manually apply the initial constraints
            let mut dimensions: (u32, u32) = window_attrs
                .inner_size
                .map(|size| size.to_physical::<u32>(scale_factor))
                .or_else(|| Some((800, 600).into()))
                .map(Into::into)
                .unwrap();
            if let Some(max) = max_inner_size {
                dimensions.0 = cmp::min(dimensions.0, max.0);
                dimensions.1 = cmp::min(dimensions.1, max.1);
            }
            if let Some(min) = min_inner_size {
                dimensions.0 = cmp::max(dimensions.0, min.0);
                dimensions.1 = cmp::max(dimensions.1, min.1);
            }
            debug!(
                "Calculated physical dimensions: {}x{}",
                dimensions.0, dimensions.1
            );
            dimensions
        };

        let screen_id = match window_attrs.platform_specific.x11.screen_id {
            Some(id) => id,
            None => xconn.default_screen_index() as c_int,
        };

        // An iterator over all of the visuals combined with their depths.
        let mut all_visuals = xconn
            .xcb_connection()
            .setup()
            .roots
            .iter()
            .flat_map(|root| &root.allowed_depths)
            .flat_map(|depth| {
                depth
                    .visuals
                    .iter()
                    .map(move |visual| (visual, depth.depth))
            });

        // creating
        let (visualtype, depth, require_colormap) = match window_attrs
            .platform_specific
            .x11
            .visual_id
        {
            Some(vi) => {
                // Find this specific visual.
                let (visualtype, depth) = all_visuals
                    .find(|(visual, _)| visual.visual_id == vi)
                    .ok_or_else(|| {
                        os_error!(OsError::XError(X11Error::NoSuchVisual(vi).into()))
                    })?;

                (Some(visualtype), depth, true)
            }
            None if window_attrs.transparent => {
                // Find a suitable visual, true color with 32 bits of depth.
                all_visuals
                        .find_map(|(visual, depth)| {
                            (depth == 32 && visual.class == xproto::VisualClass::TRUE_COLOR)
                                .then_some((Some(visual), depth, true))
                        })
                        .unwrap_or_else(|| {
                            debug!(
                                "Could not set transparency, because XMatchVisualInfo returned \
                                 zero for the required parameters"
                            );
                            (None as _, x11rb::COPY_FROM_PARENT as _, false)
                        })
            }
            _ => (None, x11rb::COPY_FROM_PARENT as _, false),
        };
        let mut visual = visualtype.map_or(x11rb::COPY_FROM_PARENT, |v| v.visual_id);

        let window_attributes = {
            use xproto::EventMask;

            let mut aux = xproto::CreateWindowAux::new();
            let event_mask = EventMask::EXPOSURE
                | EventMask::STRUCTURE_NOTIFY
                | EventMask::VISIBILITY_CHANGE
                | EventMask::KEY_PRESS
                | EventMask::KEY_RELEASE
                | EventMask::KEYMAP_STATE
                | EventMask::BUTTON_PRESS
                | EventMask::BUTTON_RELEASE
                | EventMask::POINTER_MOTION
                | EventMask::PROPERTY_CHANGE;

            aux = aux.event_mask(event_mask).border_pixel(0);

            if window_attrs.platform_specific.x11.override_redirect {
                aux = aux.override_redirect(true as u32);
            }

            // Add a colormap if needed.
            let colormap_visual = match window_attrs.platform_specific.x11.visual_id {
                Some(vi) => Some(vi),
                None if require_colormap => Some(visual),
                _ => None,
            };

            if let Some(visual) = colormap_visual {
                let colormap = leap!(xconn.xcb_connection().generate_id());
                leap!(xconn.xcb_connection().create_colormap(
                    xproto::ColormapAlloc::NONE,
                    colormap,
                    root,
                    visual,
                ));
                aux = aux.colormap(colormap);
            } else {
                aux = aux.colormap(0);
            }

            aux
        };

        // Figure out the window's parent.
        let parent = window_attrs
            .platform_specific
            .x11
            .embed_window
            .unwrap_or(root);

        // finally creating the window
        let xwindow = {
            let (x, y) = position.map_or((0, 0), Into::into);
            let wid = leap!(xconn.xcb_connection().generate_id());
            let result = xconn.xcb_connection().create_window(
                depth,
                wid,
                parent,
                x,
                y,
                dimensions.0.try_into().unwrap(),
                dimensions.1.try_into().unwrap(),
                0,
                xproto::WindowClass::INPUT_OUTPUT,
                visual,
                &window_attributes,
            );
            leap!(leap!(result).check());

            wid
        };

        // The COPY_FROM_PARENT is a special value for the visual used to copy
        // the visual from the parent window, thus we have to query the visual
        // we've got when we built the window above.
        if visual == x11rb::COPY_FROM_PARENT {
            visual = leap!(leap!(xconn
                .xcb_connection()
                .get_window_attributes(xwindow as xproto::Window))
            .reply())
            .visual;
        }

        #[allow(clippy::mutex_atomic)]
        let mut window = UnownedWindow {
            xconn: Arc::clone(xconn),
            xwindow: xwindow as xproto::Window,
            visual,
            root,
            screen_id,
            selected_cursor: Default::default(),
            cursor_grabbed_mode: Mutex::new(CursorGrabMode::None),
            cursor_visible: Mutex::new(true),
            ime_sender: Mutex::new(event_loop.ime_sender.clone()),
            shared_state: SharedState::new(guessed_monitor, &window_attrs),
            redraw_sender: event_loop.redraw_sender.clone(),
            activation_sender: event_loop.activation_sender.clone(),
        };

        // Title must be set before mapping. Some tiling window managers (i.e. i3) use the window
        // title to determine placement/etc., so doing this after mapping would cause the WM to
        // act on the wrong title state.
        leap!(window.set_title_inner(&window_attrs.title)).ignore_error();
        leap!(window.set_decorations_inner(window_attrs.decorations)).ignore_error();

        if let Some(theme) = window_attrs.preferred_theme {
            leap!(window.set_theme_inner(Some(theme))).ignore_error();
        }

        // Embed the window if needed.
        if window_attrs.platform_specific.x11.embed_window.is_some() {
            window.embed_window()?;
        }

        {
            // Enable drag and drop (TODO: extend API to make this toggleable)
            {
                let dnd_aware_atom = atoms[XdndAware];
                let version = &[5u32]; // Latest version; hasn't changed since 2002
                leap!(xconn.change_property(
                    window.xwindow,
                    dnd_aware_atom,
                    u32::from(xproto::AtomEnum::ATOM),
                    xproto::PropMode::REPLACE,
                    version,
                ))
                .ignore_error();
            }

            // WM_CLASS must be set *before* mapping the window, as per ICCCM!
            {
                let (instance, class) =
                    if let Some(name) = window_attrs.platform_specific.name {
                        (name.instance, name.general)
                    } else {
                        let class = env::args_os()
                            .next()
                            .as_ref()
                            // Default to the name of the binary (via argv[0])
                            .and_then(|path| Path::new(path).file_name())
                            .and_then(|bin_name| bin_name.to_str())
                            .map(|bin_name| bin_name.to_owned())
                            .unwrap_or_else(|| window_attrs.title.clone());
                        // This environment variable is extraordinarily unlikely to actually be used...
                        let instance = env::var("RESOURCE_NAME")
                            .ok()
                            .unwrap_or_else(|| class.clone());
                        (instance, class)
                    };

                let class = format!("{instance}\0{class}\0");
                leap!(xconn.change_property(
                    window.xwindow,
                    xproto::Atom::from(xproto::AtomEnum::WM_CLASS),
                    xproto::Atom::from(xproto::AtomEnum::STRING),
                    xproto::PropMode::REPLACE,
                    class.as_bytes(),
                ))
                .ignore_error();
            }

            if let Some(flusher) = leap!(window.set_pid()) {
                flusher.ignore_error()
            }

            leap!(window
                .set_window_types(window_attrs.platform_specific.x11.x11_window_types))
            .ignore_error();

            // Set size hints.
            let mut min_inner_size = window_attrs
                .min_inner_size
                .map(|size| size.to_physical::<u32>(scale_factor));
            let mut max_inner_size = window_attrs
                .max_inner_size
                .map(|size| size.to_physical::<u32>(scale_factor));

            if !window_attrs.resizable {
                if util::wm_name_is_one_of(&["Xfwm4"]) {
                    warn!("To avoid a WM bug, disabling resizing has no effect on Xfwm4");
                } else {
                    max_inner_size = Some(dimensions.into());
                    min_inner_size = Some(dimensions.into());
                }
            }

            let shared_state = window.shared_state.get_mut().unwrap();
            shared_state.min_inner_size = min_inner_size.map(Into::into);
            shared_state.max_inner_size = max_inner_size.map(Into::into);
            shared_state.resize_increments = window_attrs.resize_increments;
            shared_state.base_size = window_attrs.platform_specific.x11.base_size;

            let normal_hints = WmSizeHints {
                position: position.map(|PhysicalPosition { x, y }| {
                    (WmSizeHintsSpecification::UserSpecified, x, y)
                }),
                size: Some((
                    WmSizeHintsSpecification::UserSpecified,
                    cast_dimension_to_hint(dimensions.0),
                    cast_dimension_to_hint(dimensions.1),
                )),
                max_size: max_inner_size.map(cast_physical_size_to_hint),
                min_size: min_inner_size.map(cast_physical_size_to_hint),
                size_increment: window_attrs
                    .resize_increments
                    .map(|size| cast_size_to_hint(size, scale_factor)),
                base_size: window_attrs
                    .platform_specific
                    .x11
                    .base_size
                    .map(|size| cast_size_to_hint(size, scale_factor)),
                aspect: None,
                win_gravity: None,
            };
            leap!(leap!(normal_hints.set(
                xconn.xcb_connection(),
                window.xwindow as xproto::Window,
                xproto::AtomEnum::WM_NORMAL_HINTS,
            ))
            .check());

            // Set window icons
            if let Some(icon) = window_attrs.window_icon {
                leap!(window.set_icon_inner(icon.inner)).ignore_error();
            }

            // Opt into handling window close
            let result = xconn.xcb_connection().change_property(
                xproto::PropMode::REPLACE,
                window.xwindow,
                atoms[WM_PROTOCOLS],
                xproto::AtomEnum::ATOM,
                32,
                2,
                bytemuck::cast_slice::<xproto::Atom, u8>(&[
                    atoms[WM_DELETE_WINDOW],
                    atoms[_NET_WM_PING],
                ]),
            );
            leap!(result).ignore_error();

            // Set visibility (map window)
            if window_attrs.visible {
                leap!(xconn.xcb_connection().map_window(window.xwindow)).ignore_error();
                leap!(xconn.xcb_connection().configure_window(
                    xwindow,
                    &xproto::ConfigureWindowAux::new()
                        .stack_mode(xproto::StackMode::ABOVE)
                ))
                .ignore_error();
            }

            // Attempt to make keyboard input repeat detectable
            unsafe {
                let mut supported_ptr = ffi::False;
                (xconn.xlib.XkbSetDetectableAutoRepeat)(
                    xconn.display,
                    ffi::True,
                    &mut supported_ptr,
                );
                if supported_ptr == ffi::False {
                    return Err(os_error!(OsError::Misc(
                        "`XkbSetDetectableAutoRepeat` failed"
                    )));
                }
            }

            // Select XInput2 events
            let mask = xinput::XIEventMask::MOTION
                | xinput::XIEventMask::BUTTON_PRESS
                | xinput::XIEventMask::BUTTON_RELEASE
                | xinput::XIEventMask::ENTER
                | xinput::XIEventMask::LEAVE
                | xinput::XIEventMask::FOCUS_IN
                | xinput::XIEventMask::FOCUS_OUT
                | xinput::XIEventMask::TOUCH_BEGIN
                | xinput::XIEventMask::TOUCH_UPDATE
                | xinput::XIEventMask::TOUCH_END;
            leap!(xconn.select_xinput_events(
                window.xwindow,
                super::ALL_MASTER_DEVICES,
                mask
            ))
            .ignore_error();

            // Try to create input context for the window.
            if let Some(ime) = event_loop.ime.as_ref() {
                let result = ime
                    .borrow_mut()
                    .create_context(window.xwindow as ffi::Window, false);
                leap!(result);
            }

            // These properties must be set after mapping
            if window_attrs.maximized {
                leap!(window.set_maximized_inner(window_attrs.maximized)).ignore_error();
            }
            if window_attrs.fullscreen.is_some() {
                if let Some(flusher) = leap!(window.set_fullscreen_inner(
                    window_attrs.fullscreen.clone().map(Into::into)
                )) {
                    flusher.ignore_error()
                }

                if let Some(PhysicalPosition { x, y }) = position {
                    let shared_state = window.shared_state.get_mut().unwrap();

                    shared_state.restore_position = Some((x, y));
                }
            }

            leap!(window.set_window_level_inner(window_attrs.window_level))
                .ignore_error();
        }

        window.set_cursor(window_attrs.cursor);

        // Remove the startup notification if we have one.
        if let Some(startup) = window_attrs.platform_specific.activation_token.as_ref() {
            leap!(xconn.remove_activation_token(xwindow, &startup._token));
        }

        // We never want to give the user a broken window, since by then, it's too late to handle.
        let window = leap!(xconn.sync_with_server().map(|_| window));

        Ok(window)
    }

    /// Embed this window into a parent window.
    pub(super) fn embed_window(&self) -> Result<(), RootOsError> {
        let atoms = self.xconn.atoms();
        leap!(leap!(self.xconn.change_property(
            self.xwindow,
            atoms[_XEMBED],
            atoms[_XEMBED],
            xproto::PropMode::REPLACE,
            &[0u32, 1u32],
        ))
        .check());

        Ok(())
    }

    pub(super) fn shared_state_lock(&self) -> MutexGuard<'_, SharedState> {
        self.shared_state.lock().unwrap()
    }

    fn set_pid(&self) -> Result<Option<VoidCookie<'_>>, X11Error> {
        let atoms = self.xconn.atoms();
        let pid_atom = atoms[_NET_WM_PID];
        let client_machine_atom = atoms[WM_CLIENT_MACHINE];

        // Get the hostname and the PID.
        let uname = rustix::system::uname();
        let pid = rustix::process::getpid();

        self.xconn
            .change_property(
                self.xwindow,
                pid_atom,
                xproto::Atom::from(xproto::AtomEnum::CARDINAL),
                xproto::PropMode::REPLACE,
                &[pid.as_raw_nonzero().get() as util::Cardinal],
            )?
            .ignore_error();
        let flusher = self.xconn.change_property(
            self.xwindow,
            client_machine_atom,
            xproto::Atom::from(xproto::AtomEnum::STRING),
            xproto::PropMode::REPLACE,
            uname.nodename().to_bytes(),
        );
        flusher.map(Some)
    }

    fn set_window_types(
        &self,
        window_types: Vec<WindowType>,
    ) -> Result<VoidCookie<'_>, X11Error> {
        let atoms = self.xconn.atoms();
        let hint_atom = atoms[_NET_WM_WINDOW_TYPE];
        let atoms: Vec<_> = window_types
            .iter()
            .map(|t| t.as_atom(&self.xconn))
            .collect();

        self.xconn.change_property(
            self.xwindow,
            hint_atom,
            xproto::Atom::from(xproto::AtomEnum::ATOM),
            xproto::PropMode::REPLACE,
            &atoms,
        )
    }

    pub fn set_theme_inner(
        &self,
        theme: Option<Theme>,
    ) -> Result<VoidCookie<'_>, X11Error> {
        let atoms = self.xconn.atoms();
        let hint_atom = atoms[_GTK_THEME_VARIANT];
        let utf8_atom = atoms[UTF8_STRING];
        let variant = match theme {
            Some(Theme::Dark) => "dark",
            Some(Theme::Light) => "light",
            None => "dark",
        };
        let variant =
            CString::new(variant).expect("`_GTK_THEME_VARIANT` contained null byte");
        self.xconn.change_property(
            self.xwindow,
            hint_atom,
            utf8_atom,
            xproto::PropMode::REPLACE,
            variant.as_bytes(),
        )
    }

    #[inline]
    pub fn set_theme(&self, theme: Option<Theme>) {
        self.set_theme_inner(theme)
            .expect("Failed to change window theme")
            .ignore_error();

        self.xconn
            .flush_requests()
            .expect("Failed to change window theme");
    }

    fn set_netwm(
        &self,
        operation: util::StateOperation,
        properties: (u32, u32, u32, u32),
    ) -> Result<VoidCookie<'_>, X11Error> {
        let atoms = self.xconn.atoms();
        let state_atom = atoms[_NET_WM_STATE];
        self.xconn.send_client_msg(
            self.xwindow,
            self.root,
            state_atom,
            Some(
                xproto::EventMask::SUBSTRUCTURE_REDIRECT
                    | xproto::EventMask::SUBSTRUCTURE_NOTIFY,
            ),
            [
                operation as u32,
                properties.0,
                properties.1,
                properties.2,
                properties.3,
            ],
        )
    }

    fn set_fullscreen_hint(&self, fullscreen: bool) -> Result<VoidCookie<'_>, X11Error> {
        let atoms = self.xconn.atoms();
        let fullscreen_atom = atoms[_NET_WM_STATE_FULLSCREEN];
        let flusher = self.set_netwm(fullscreen.into(), (fullscreen_atom, 0, 0, 0));

        if fullscreen {
            // Ensure that the fullscreen window receives input focus to prevent
            // locking up the user's display.
            self.xconn
                .xcb_connection()
                .set_input_focus(
                    xproto::InputFocus::PARENT,
                    self.xwindow,
                    x11rb::CURRENT_TIME,
                )?
                .ignore_error();
        }

        flusher
    }

    fn set_fullscreen_inner(
        &self,
        fullscreen: Option<Fullscreen>,
    ) -> Result<Option<VoidCookie<'_>>, X11Error> {
        let mut shared_state_lock = self.shared_state_lock();

        match shared_state_lock.visibility {
            // Setting fullscreen on a window that is not visible will generate an error.
            Visibility::No | Visibility::YesWait => {
                shared_state_lock.desired_fullscreen = Some(fullscreen);
                return Ok(None);
            }
            Visibility::Yes => (),
        }

        let old_fullscreen = shared_state_lock.fullscreen.clone();
        if old_fullscreen == fullscreen {
            return Ok(None);
        }
        shared_state_lock.fullscreen.clone_from(&fullscreen);

        match (&old_fullscreen, &fullscreen) {
            // Store the desktop video mode before entering exclusive
            // fullscreen, so we can restore it upon exit, as XRandR does not
            // provide a mechanism to set this per app-session or restore this
            // to the desktop video mode as macOS and Windows do
            (
                &None,
                &Some(Fullscreen::Exclusive(PlatformVideoModeHandle::X(ref video_mode))),
            )
            | (
                &Some(Fullscreen::Borderless(_)),
                &Some(Fullscreen::Exclusive(PlatformVideoModeHandle::X(ref video_mode))),
            ) => {
                let monitor = video_mode.monitor.as_ref().unwrap();
                shared_state_lock.desktop_video_mode = Some((
                    monitor.id,
                    self.xconn
                        .get_crtc_mode(monitor.id)
                        .expect("Failed to get desktop video mode"),
                ));
            }
            // Restore desktop video mode upon exiting exclusive fullscreen
            (&Some(Fullscreen::Exclusive(_)), &None)
            | (&Some(Fullscreen::Exclusive(_)), &Some(Fullscreen::Borderless(_))) => {
                let (monitor_id, mode_id) =
                    shared_state_lock.desktop_video_mode.take().unwrap();
                self.xconn
                    .set_crtc_config(monitor_id, mode_id)
                    .expect("failed to restore desktop video mode");
            }
            _ => (),
        }

        drop(shared_state_lock);

        match fullscreen {
            None => {
                let flusher = self.set_fullscreen_hint(false);
                let mut shared_state_lock = self.shared_state_lock();
                if let Some(position) = shared_state_lock.restore_position.take() {
                    drop(shared_state_lock);
                    self.set_position_inner(position.0, position.1)
                        .expect_then_ignore_error("Failed to restore window position");
                }
                flusher.map(Some)
            }
            Some(fullscreen) => {
                let (video_mode, monitor) = match fullscreen {
                    Fullscreen::Exclusive(PlatformVideoModeHandle::X(ref video_mode)) => {
                        (Some(video_mode), video_mode.monitor.clone().unwrap())
                    }
                    Fullscreen::Borderless(Some(PlatformMonitorHandle::X(monitor))) => {
                        (None, monitor)
                    }
                    Fullscreen::Borderless(None) => {
                        (None, self.shared_state_lock().last_monitor.clone())
                    }
                    #[cfg(wayland_platform)]
                    _ => unreachable!(),
                };

                // Don't set fullscreen on an invalid dummy monitor handle
                if monitor.is_dummy() {
                    return Ok(None);
                }

                if let Some(video_mode) = video_mode {
                    // FIXME: this is actually not correct if we're setting the
                    // video mode to a resolution higher than the current
                    // desktop resolution, because XRandR does not automatically
                    // reposition the monitors to the right and below this
                    // monitor.
                    //
                    // What ends up happening is we will get the fullscreen
                    // window showing up on those monitors as well, because
                    // their virtual position now overlaps with the monitor that
                    // we just made larger..
                    //
                    // It'd be quite a bit of work to handle this correctly (and
                    // nobody else seems to bother doing this correctly either),
                    // so we're just leaving this broken. Fixing this would
                    // involve storing all CRTCs upon entering fullscreen,
                    // restoring them upon exit, and after entering fullscreen,
                    // repositioning displays to the right and below this
                    // display. I think there would still be edge cases that are
                    // difficult or impossible to handle correctly, e.g. what if
                    // a new monitor was plugged in while in fullscreen?
                    //
                    // I think we might just want to disallow setting the video
                    // mode higher than the current desktop video mode (I'm sure
                    // this will make someone unhappy, but it's very unusual for
                    // games to want to do this anyway).
                    self.xconn
                        .set_crtc_config(monitor.id, video_mode.native_mode)
                        .expect("failed to set video mode");
                }

                let window_position = self.outer_position_physical();
                self.shared_state_lock().restore_position = Some(window_position);
                let monitor_origin: (i32, i32) = monitor.position().into();
                self.set_position_inner(monitor_origin.0, monitor_origin.1)
                    .expect_then_ignore_error("Failed to set window position");
                self.set_fullscreen_hint(true).map(Some)
            }
        }
    }

    #[inline]
    pub(crate) fn fullscreen(&self) -> Option<Fullscreen> {
        let shared_state = self.shared_state_lock();

        shared_state
            .desired_fullscreen
            .clone()
            .unwrap_or_else(|| shared_state.fullscreen.clone())
    }

    #[inline]
    pub(crate) fn set_fullscreen(&self, fullscreen: Option<Fullscreen>) {
        if let Some(flusher) = self
            .set_fullscreen_inner(fullscreen)
            .expect("Failed to change window fullscreen state")
        {
            flusher
                .check()
                .expect("Failed to change window fullscreen state");
            self.invalidate_cached_frame_extents();
        }
    }

    // Called by EventProcessor when a VisibilityNotify event is received
    pub(crate) fn visibility_notify(&self) {
        let mut shared_state = self.shared_state_lock();

        match shared_state.visibility {
            Visibility::No => self
                .xconn
                .xcb_connection()
                .unmap_window(self.xwindow)
                .expect_then_ignore_error("Failed to unmap window"),
            Visibility::Yes => (),
            Visibility::YesWait => {
                shared_state.visibility = Visibility::Yes;

                if let Some(fullscreen) = shared_state.desired_fullscreen.take() {
                    drop(shared_state);
                    self.set_fullscreen(fullscreen);
                }
            }
        }
    }

    pub fn current_monitor(&self) -> Option<X11MonitorHandle> {
        Some(self.shared_state_lock().last_monitor.clone())
    }

    pub fn available_monitors(&self) -> Vec<X11MonitorHandle> {
        self.xconn
            .available_monitors()
            .expect("Failed to get available monitors")
    }

    pub fn primary_monitor(&self) -> Option<X11MonitorHandle> {
        Some(
            self.xconn
                .primary_monitor()
                .expect("Failed to get primary monitor"),
        )
    }

    #[inline]
    pub fn is_minimized(&self) -> Option<bool> {
        let atoms = self.xconn.atoms();
        let state_atom = atoms[_NET_WM_STATE];
        let state = self.xconn.get_property(
            self.xwindow,
            state_atom,
            xproto::Atom::from(xproto::AtomEnum::ATOM),
        );
        let hidden_atom = atoms[_NET_WM_STATE_HIDDEN];

        Some(match state {
            Ok(atoms) => atoms
                .iter()
                .any(|atom: &xproto::Atom| *atom as xproto::Atom == hidden_atom),
            _ => false,
        })
    }

    /// Refresh the API for the given monitor.
    #[inline]
    pub(super) fn refresh_dpi_for_monitor<T: 'static>(
        &self,
        new_monitor: &X11MonitorHandle,
        maybe_prev_scale_factor: Option<f64>,
        mut callback: impl FnMut(Event<T>),
    ) {
        // Check if the self is on this monitor
        let monitor = self.shared_state_lock().last_monitor.clone();
        if monitor.name == new_monitor.name {
            let (width, height) = self.inner_size_physical();
            let (new_width, new_height) = self.adjust_for_dpi(
                // If we couldn't determine the previous scale
                // factor (e.g., because all monitors were closed
                // before), just pick whatever the current monitor
                // has set as a baseline.
                maybe_prev_scale_factor.unwrap_or(monitor.scale_factor),
                new_monitor.scale_factor,
                width,
                height,
                &self.shared_state_lock(),
            );

            let window_id = crate::window::WindowId(self.id());
            let old_inner_size = PhysicalSize::new(width, height);
            let inner_size =
                Arc::new(Mutex::new(PhysicalSize::new(new_width, new_height)));
            callback(Event::WindowEvent {
                window_id,
                event: WindowEvent::ScaleFactorChanged {
                    scale_factor: new_monitor.scale_factor,
                    inner_size_writer: InnerSizeWriter::new(Arc::downgrade(&inner_size)),
                },
            });

            let new_inner_size = *inner_size.lock().unwrap();
            drop(inner_size);

            if new_inner_size != old_inner_size {
                let (new_width, new_height) = new_inner_size.into();
                self.request_inner_size_physical(new_width, new_height);
            }
        }
    }

    fn set_minimized_inner(&self, minimized: bool) -> Result<VoidCookie<'_>, X11Error> {
        let atoms = self.xconn.atoms();

        if minimized {
            let root_window = self.xconn.default_root().root;

            self.xconn.send_client_msg(
                self.xwindow,
                root_window,
                atoms[WM_CHANGE_STATE],
                Some(
                    xproto::EventMask::SUBSTRUCTURE_REDIRECT
                        | xproto::EventMask::SUBSTRUCTURE_NOTIFY,
                ),
                [3u32, 0, 0, 0, 0],
            )
        } else {
            self.xconn.send_client_msg(
                self.xwindow,
                self.root,
                atoms[_NET_ACTIVE_WINDOW],
                Some(
                    xproto::EventMask::SUBSTRUCTURE_REDIRECT
                        | xproto::EventMask::SUBSTRUCTURE_NOTIFY,
                ),
                [1, x11rb::CURRENT_TIME, 0, 0, 0],
            )
        }
    }

    #[inline]
    pub fn set_minimized(&self, minimized: bool) {
        self.set_minimized_inner(minimized)
            .expect_then_ignore_error("Failed to change window minimization");

        self.xconn
            .flush_requests()
            .expect("Failed to change window minimization");
    }

    #[inline]
    pub fn is_maximized(&self) -> bool {
        let atoms = self.xconn.atoms();
        let state_atom = atoms[_NET_WM_STATE];
        let state = self.xconn.get_property(
            self.xwindow,
            state_atom,
            xproto::Atom::from(xproto::AtomEnum::ATOM),
        );
        let horz_atom = atoms[_NET_WM_STATE_MAXIMIZED_HORZ];
        let vert_atom = atoms[_NET_WM_STATE_MAXIMIZED_VERT];
        match state {
            Ok(atoms) => {
                let horz_maximized = atoms.contains(&horz_atom);
                let vert_maximized = atoms.contains(&vert_atom);
                horz_maximized && vert_maximized
            }
            _ => false,
        }
    }

    fn set_maximized_inner(&self, maximized: bool) -> Result<VoidCookie<'_>, X11Error> {
        let atoms = self.xconn.atoms();
        let horz_atom = atoms[_NET_WM_STATE_MAXIMIZED_HORZ];
        let vert_atom = atoms[_NET_WM_STATE_MAXIMIZED_VERT];

        self.set_netwm(maximized.into(), (horz_atom, vert_atom, 0, 0))
    }

    #[inline]
    pub fn set_maximized(&self, maximized: bool) {
        self.set_maximized_inner(maximized)
            .expect_then_ignore_error("Failed to change window maximization");
        self.xconn
            .flush_requests()
            .expect("Failed to change window maximization");
        self.invalidate_cached_frame_extents();
    }

    fn set_title_inner(&self, title: &str) -> Result<VoidCookie<'_>, X11Error> {
        let atoms = self.xconn.atoms();

        let title = CString::new(title).expect("Window title contained null byte");
        self.xconn
            .change_property(
                self.xwindow,
                xproto::Atom::from(xproto::AtomEnum::WM_NAME),
                xproto::Atom::from(xproto::AtomEnum::STRING),
                xproto::PropMode::REPLACE,
                title.as_bytes(),
            )?
            .ignore_error();
        self.xconn.change_property(
            self.xwindow,
            atoms[_NET_WM_NAME],
            atoms[UTF8_STRING],
            xproto::PropMode::REPLACE,
            title.as_bytes(),
        )
    }

    #[inline]
    pub fn set_title(&self, title: &str) {
        self.set_title_inner(title)
            .expect_then_ignore_error("Failed to set window title");

        self.xconn
            .flush_requests()
            .expect("Failed to set window title");
    }

    #[inline]
    pub fn set_transparent(&self, _transparent: bool) {}

    #[inline]
    pub fn set_blur(&self, _blur: bool) {}

    fn set_decorations_inner(
        &self,
        decorations: bool,
    ) -> Result<VoidCookie<'_>, X11Error> {
        self.shared_state_lock().is_decorated = decorations;
        let mut hints = self.xconn.get_motif_hints(self.xwindow);

        hints.set_decorations(decorations);

        self.xconn.set_motif_hints(self.xwindow, &hints)
    }

    #[inline]
    pub fn set_decorations(&self, decorations: bool) {
        self.set_decorations_inner(decorations)
            .expect_then_ignore_error("Failed to set decoration state");
        self.xconn
            .flush_requests()
            .expect("Failed to set decoration state");
        self.invalidate_cached_frame_extents();
    }

    #[inline]
    pub fn is_decorated(&self) -> bool {
        self.shared_state_lock().is_decorated
    }

    fn set_maximizable_inner(
        &self,
        maximizable: bool,
    ) -> Result<VoidCookie<'_>, X11Error> {
        let mut hints = self.xconn.get_motif_hints(self.xwindow);

        hints.set_maximizable(maximizable);

        self.xconn.set_motif_hints(self.xwindow, &hints)
    }

    fn toggle_atom(
        &self,
        atom_name: AtomName,
        enable: bool,
    ) -> Result<VoidCookie<'_>, X11Error> {
        let atoms = self.xconn.atoms();
        let atom = atoms[atom_name];
        self.set_netwm(enable.into(), (atom, 0, 0, 0))
    }

    fn set_window_level_inner(
        &self,
        level: WindowLevel,
    ) -> Result<VoidCookie<'_>, X11Error> {
        self.toggle_atom(_NET_WM_STATE_ABOVE, level == WindowLevel::AlwaysOnTop)?
            .ignore_error();
        self.toggle_atom(_NET_WM_STATE_BELOW, level == WindowLevel::AlwaysOnBottom)
    }

    #[inline]
    pub fn set_window_level(&self, level: WindowLevel) {
        self.set_window_level_inner(level)
            .expect_then_ignore_error("Failed to set window-level state");
        self.xconn
            .flush_requests()
            .expect("Failed to set window-level state");
    }

    fn set_icon_inner(&self, icon: PlatformIcon) -> Result<VoidCookie<'_>, X11Error> {
        let atoms = self.xconn.atoms();
        let icon_atom = atoms[_NET_WM_ICON];
        let data = icon.to_cardinals();
        self.xconn.change_property(
            self.xwindow,
            icon_atom,
            xproto::Atom::from(xproto::AtomEnum::CARDINAL),
            xproto::PropMode::REPLACE,
            data.as_slice(),
        )
    }

    fn unset_icon_inner(&self) -> Result<VoidCookie<'_>, X11Error> {
        let atoms = self.xconn.atoms();
        let icon_atom = atoms[_NET_WM_ICON];
        let empty_data: [util::Cardinal; 0] = [];
        self.xconn.change_property(
            self.xwindow,
            icon_atom,
            xproto::Atom::from(xproto::AtomEnum::CARDINAL),
            xproto::PropMode::REPLACE,
            &empty_data,
        )
    }

    #[inline]
    pub(crate) fn set_window_icon(&self, icon: Option<PlatformIcon>) {
        match icon {
            Some(icon) => self.set_icon_inner(icon),
            None => self.unset_icon_inner(),
        }
        .expect_then_ignore_error("Failed to set icons");

        self.xconn.flush_requests().expect("Failed to set icons");
    }

    #[inline]
    pub fn set_visible(&self, visible: bool) {
        let mut shared_state = self.shared_state_lock();

        match (visible, shared_state.visibility) {
            (true, Visibility::Yes)
            | (true, Visibility::YesWait)
            | (false, Visibility::No) => return,
            _ => (),
        }

        if visible {
            self.xconn
                .xcb_connection()
                .map_window(self.xwindow)
                .expect_then_ignore_error("Failed to call `xcb_map_window`");
            self.xconn
                .xcb_connection()
                .configure_window(
                    self.xwindow,
                    &xproto::ConfigureWindowAux::new()
                        .stack_mode(xproto::StackMode::ABOVE),
                )
                .expect_then_ignore_error("Failed to call `xcb_configure_window`");
            self.xconn
                .flush_requests()
                .expect("Failed to call XMapRaised");
            shared_state.visibility = Visibility::YesWait;
        } else {
            self.xconn
                .xcb_connection()
                .unmap_window(self.xwindow)
                .expect_then_ignore_error("Failed to call `xcb_unmap_window`");
            self.xconn
                .flush_requests()
                .expect("Failed to call XUnmapWindow");
            shared_state.visibility = Visibility::No;
        }
    }

    #[inline]
    pub fn is_visible(&self) -> Option<bool> {
        Some(self.shared_state_lock().visibility == Visibility::Yes)
    }

    fn update_cached_frame_extents(&self) {
        let extents = self
            .xconn
            .get_frame_extents_heuristic(self.xwindow, self.root);
        self.shared_state_lock().frame_extents = Some(extents);
    }

    pub(crate) fn invalidate_cached_frame_extents(&self) {
        self.shared_state_lock().frame_extents.take();
    }

    pub(crate) fn outer_position_physical(&self) -> (i32, i32) {
        let extents = self.shared_state_lock().frame_extents.clone();
        if let Some(extents) = extents {
            let (x, y) = self.inner_position_physical();
            extents.inner_pos_to_outer(x, y)
        } else {
            self.update_cached_frame_extents();
            self.outer_position_physical()
        }
    }

    #[inline]
    pub fn outer_position(&self) -> Result<PhysicalPosition<i32>, NotSupportedError> {
        let extents = self.shared_state_lock().frame_extents.clone();
        if let Some(extents) = extents {
            let (x, y) = self.inner_position_physical();
            Ok(extents.inner_pos_to_outer(x, y).into())
        } else {
            self.update_cached_frame_extents();
            self.outer_position()
        }
    }

    pub(crate) fn inner_position_physical(&self) -> (i32, i32) {
        // This should be okay to unwrap since the only error XTranslateCoordinates can return
        // is BadWindow, and if the window handle is bad we have bigger problems.
        self.xconn
            .translate_coords(self.xwindow, self.root)
            .map(|coords| (coords.dst_x.into(), coords.dst_y.into()))
            .unwrap()
    }

    #[inline]
    pub fn inner_position(&self) -> Result<PhysicalPosition<i32>, NotSupportedError> {
        Ok(self.inner_position_physical().into())
    }

    pub(crate) fn set_position_inner(
        &self,
        mut x: i32,
        mut y: i32,
    ) -> Result<VoidCookie<'_>, X11Error> {
        // There are a few WMs that set client area position rather than window position, so
        // we'll translate for consistency.
        if util::wm_name_is_one_of(&["Enlightenment", "FVWM"]) {
            let extents = self.shared_state_lock().frame_extents.clone();
            if let Some(extents) = extents {
                x += cast_dimension_to_hint(extents.frame_extents.left);
                y += cast_dimension_to_hint(extents.frame_extents.top);
            } else {
                self.update_cached_frame_extents();
                return self.set_position_inner(x, y);
            }
        }

        self.xconn
            .xcb_connection()
            .configure_window(self.xwindow, &xproto::ConfigureWindowAux::new().x(x).y(y))
            .map_err(Into::into)
    }

    pub(crate) fn set_position_physical(&self, x: i32, y: i32) {
        self.set_position_inner(x, y)
            .expect_then_ignore_error("Failed to call `XMoveWindow`");
    }

    #[inline]
    pub fn set_outer_position(&self, position: Position) {
        let (x, y) = position.to_physical::<i32>(self.scale_factor()).into();
        self.set_position_physical(x, y);
    }

    pub(crate) fn inner_size_physical(&self) -> (u32, u32) {
        // This should be okay to unwrap since the only error XGetGeometry can return
        // is BadWindow, and if the window handle is bad we have bigger problems.
        self.xconn
            .get_geometry(self.xwindow)
            .map(|geo| (geo.width.into(), geo.height.into()))
            .unwrap()
    }

    #[inline]
    pub fn inner_size(&self) -> PhysicalSize<u32> {
        self.inner_size_physical().into()
    }

    #[inline]
    pub fn outer_size(&self) -> PhysicalSize<u32> {
        let extents = self.shared_state_lock().frame_extents.clone();
        if let Some(extents) = extents {
            let (width, height) = self.inner_size_physical();
            extents.inner_size_to_outer(width, height).into()
        } else {
            self.update_cached_frame_extents();
            self.outer_size()
        }
    }

    pub(crate) fn request_inner_size_physical(&self, width: u32, height: u32) {
        self.xconn
            .xcb_connection()
            .configure_window(
                self.xwindow,
                &xproto::ConfigureWindowAux::new()
                    .width(width)
                    .height(height),
            )
            .expect_then_ignore_error("Failed to call `xcb_configure_window`");
        self.xconn
            .flush_requests()
            .expect("Failed to call XResizeWindow");
        // cursor_hittest needs to be reapplied after each window resize.
        if self.shared_state_lock().cursor_hittest.unwrap_or(false) {
            let _ = self.set_cursor_hittest(true);
        }
    }

    #[inline]
    pub fn request_inner_size(&self, size: Size) -> Option<PhysicalSize<u32>> {
        let scale_factor = self.scale_factor();
        let size = size.to_physical::<u32>(scale_factor).into();
        if !self.shared_state_lock().is_resizable {
            self.update_normal_hints(|normal_hints| {
                normal_hints.min_size = Some(size);
                normal_hints.max_size = Some(size);
            })
            .expect("Failed to call `XSetWMNormalHints`");
        }
        self.request_inner_size_physical(size.0 as u32, size.1 as u32);

        None
    }

    fn update_normal_hints<F>(&self, callback: F) -> Result<(), X11Error>
    where
        F: FnOnce(&mut WmSizeHints),
    {
        let mut normal_hints = WmSizeHints::get(
            self.xconn.xcb_connection(),
            self.xwindow as xproto::Window,
            xproto::AtomEnum::WM_NORMAL_HINTS,
        )?
        .reply()?
        .unwrap_or_default();
        callback(&mut normal_hints);
        normal_hints
            .set(
                self.xconn.xcb_connection(),
                self.xwindow as xproto::Window,
                xproto::AtomEnum::WM_NORMAL_HINTS,
            )?
            .ignore_error();
        Ok(())
    }

    pub(crate) fn set_min_inner_size_physical(&self, dimensions: Option<(u32, u32)>) {
        self.update_normal_hints(|normal_hints| {
            normal_hints.min_size = dimensions
                .map(|(w, h)| (cast_dimension_to_hint(w), cast_dimension_to_hint(h)))
        })
        .expect("Failed to call `XSetWMNormalHints`");
    }

    #[inline]
    pub fn set_min_inner_size(&self, dimensions: Option<Size>) {
        self.shared_state_lock().min_inner_size = dimensions;
        let physical_dimensions = dimensions
            .map(|dimensions| dimensions.to_physical::<u32>(self.scale_factor()).into());
        self.set_min_inner_size_physical(physical_dimensions);
    }

    pub(crate) fn set_max_inner_size_physical(&self, dimensions: Option<(u32, u32)>) {
        self.update_normal_hints(|normal_hints| {
            normal_hints.max_size = dimensions
                .map(|(w, h)| (cast_dimension_to_hint(w), cast_dimension_to_hint(h)))
        })
        .expect("Failed to call `XSetWMNormalHints`");
    }

    #[inline]
    pub fn set_max_inner_size(&self, dimensions: Option<Size>) {
        self.shared_state_lock().max_inner_size = dimensions;
        let physical_dimensions = dimensions
            .map(|dimensions| dimensions.to_physical::<u32>(self.scale_factor()).into());
        self.set_max_inner_size_physical(physical_dimensions);
    }

    #[inline]
    pub fn resize_increments(&self) -> Option<PhysicalSize<u32>> {
        WmSizeHints::get(
            self.xconn.xcb_connection(),
            self.xwindow as xproto::Window,
            xproto::AtomEnum::WM_NORMAL_HINTS,
        )
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .flatten()
        .and_then(|hints| hints.size_increment)
        .map(|(width, height)| (width as u32, height as u32).into())
    }

    #[inline]
    pub fn set_resize_increments(&self, increments: Option<Size>) {
        self.shared_state_lock().resize_increments = increments;
        let physical_increments = increments
            .map(|increments| cast_size_to_hint(increments, self.scale_factor()));
        self.update_normal_hints(|hints| hints.size_increment = physical_increments)
            .expect("Failed to call `XSetWMNormalHints`");
    }

    pub(crate) fn adjust_for_dpi(
        &self,
        old_scale_factor: f64,
        new_scale_factor: f64,
        width: u32,
        height: u32,
        shared_state: &SharedState,
    ) -> (u32, u32) {
        let scale_factor = new_scale_factor / old_scale_factor;
        self.update_normal_hints(|normal_hints| {
            let dpi_adjuster =
                |size: Size| -> (i32, i32) { cast_size_to_hint(size, scale_factor) };
            let max_size = shared_state.max_inner_size.map(dpi_adjuster);
            let min_size = shared_state.min_inner_size.map(dpi_adjuster);
            let resize_increments = shared_state.resize_increments.map(dpi_adjuster);
            let base_size = shared_state.base_size.map(dpi_adjuster);

            normal_hints.max_size = max_size;
            normal_hints.min_size = min_size;
            normal_hints.size_increment = resize_increments;
            normal_hints.base_size = base_size;
        })
        .expect("Failed to update normal hints");

        let new_width = (width as f64 * scale_factor).round() as u32;
        let new_height = (height as f64 * scale_factor).round() as u32;

        (new_width, new_height)
    }

    pub fn set_resizable(&self, resizable: bool) {
        if util::wm_name_is_one_of(&["Xfwm4"]) {
            // Making the window unresizable on Xfwm prevents further changes to `WM_NORMAL_HINTS`
            // from being detected. This makes it impossible for resizing to be
            // re-enabled, and also breaks DPI scaling. As such, we choose the lesser of
            // two evils and do nothing.
            warn!("To avoid a WM bug, disabling resizing has no effect on Xfwm4");
            return;
        }

        let (min_size, max_size) = if resizable {
            let shared_state_lock = self.shared_state_lock();
            (
                shared_state_lock.min_inner_size,
                shared_state_lock.max_inner_size,
            )
        } else {
            let window_size = Some(Size::from(self.inner_size()));
            (window_size, window_size)
        };
        self.shared_state_lock().is_resizable = resizable;

        self.set_maximizable_inner(resizable)
            .expect_then_ignore_error("Failed to call `XSetWMNormalHints`");

        let scale_factor = self.scale_factor();
        let min_inner_size = min_size.map(|size| cast_size_to_hint(size, scale_factor));
        let max_inner_size = max_size.map(|size| cast_size_to_hint(size, scale_factor));
        self.update_normal_hints(|normal_hints| {
            normal_hints.min_size = min_inner_size;
            normal_hints.max_size = max_inner_size;
        })
        .expect("Failed to call `XSetWMNormalHints`");
    }

    #[inline]
    pub fn is_resizable(&self) -> bool {
        self.shared_state_lock().is_resizable
    }

    #[inline]
    pub fn set_enabled_buttons(&self, _buttons: WindowButtons) {}

    #[inline]
    pub fn enabled_buttons(&self) -> WindowButtons {
        WindowButtons::all()
    }

    #[allow(dead_code)]
    #[inline]
    pub fn xlib_display(&self) -> *mut c_void {
        self.xconn.display as _
    }

    #[allow(dead_code)]
    #[inline]
    pub fn xlib_window(&self) -> c_ulong {
        self.xwindow as ffi::Window
    }

    #[inline]
    pub fn set_cursor(&self, cursor: Cursor) {
        match cursor {
            Cursor::Icon(icon) => {
                let old_cursor = replace(
                    &mut *self.selected_cursor.lock().unwrap(),
                    SelectedCursor::Named(icon),
                );

                #[allow(clippy::mutex_atomic)]
                if SelectedCursor::Named(icon) != old_cursor
                    && *self.cursor_visible.lock().unwrap()
                {
                    if let Err(err) = self.xconn.set_cursor_icon(self.xwindow, Some(icon))
                    {
                        tracing::error!("failed to set cursor icon: {err}");
                    }
                }
            }
            Cursor::Custom(RootCustomCursor {
                inner: PlatformCustomCursor::X(cursor),
            }) => {
                #[allow(clippy::mutex_atomic)]
                if *self.cursor_visible.lock().unwrap() {
                    if let Err(err) = self.xconn.set_custom_cursor(self.xwindow, &cursor)
                    {
                        tracing::error!("failed to set window icon: {err}");
                    }
                }

                *self.selected_cursor.lock().unwrap() = SelectedCursor::Custom(cursor);
            }
            #[cfg(wayland_platform)]
            Cursor::Custom(RootCustomCursor {
                inner: PlatformCustomCursor::Wayland(_),
            }) => {
                tracing::error!("passed a Wayland cursor to X11 backend")
            }
        }
    }

    #[inline]
    pub fn set_cursor_grab(&self, mode: CursorGrabMode) -> Result<(), ExternalError> {
        let mut grabbed_lock = self.cursor_grabbed_mode.lock().unwrap();
        if mode == *grabbed_lock {
            return Ok(());
        }

        // We ungrab before grabbing to prevent passive grabs from causing `AlreadyGrabbed`.
        // Therefore, this is common to both codepaths.
        self.xconn
            .xcb_connection()
            .ungrab_pointer(x11rb::CURRENT_TIME)
            .expect_then_ignore_error("Failed to call `xcb_ungrab_pointer`");

        let result = match mode {
            CursorGrabMode::None => self.xconn.flush_requests().map_err(|err| {
                ExternalError::Os(os_error!(OsError::XError(X11Error::Xlib(err).into())))
            }),
            CursorGrabMode::Confined => {
                let result = {
                    self.xconn
                        .xcb_connection()
                        .grab_pointer(
                            true as _,
                            self.xwindow,
                            xproto::EventMask::BUTTON_PRESS
                                | xproto::EventMask::BUTTON_RELEASE
                                | xproto::EventMask::ENTER_WINDOW
                                | xproto::EventMask::LEAVE_WINDOW
                                | xproto::EventMask::POINTER_MOTION
                                | xproto::EventMask::POINTER_MOTION_HINT
                                | xproto::EventMask::BUTTON1_MOTION
                                | xproto::EventMask::BUTTON2_MOTION
                                | xproto::EventMask::BUTTON3_MOTION
                                | xproto::EventMask::BUTTON4_MOTION
                                | xproto::EventMask::BUTTON5_MOTION
                                | xproto::EventMask::KEYMAP_STATE,
                            xproto::GrabMode::ASYNC,
                            xproto::GrabMode::ASYNC,
                            self.xwindow,
                            0u32,
                            x11rb::CURRENT_TIME,
                        )
                        .expect("Failed to call `grab_pointer`")
                        .reply()
                        .expect("Failed to receive reply from `grab_pointer`")
                };

                match result.status {
                    xproto::GrabStatus::SUCCESS => Ok(()),
                    xproto::GrabStatus::ALREADY_GRABBED => {
                        Err("Cursor could not be confined: already confined by another client")
                    },
                    xproto::GrabStatus::INVALID_TIME => {
                        Err("Cursor could not be confined: invalid time")
                    },
                    xproto::GrabStatus::NOT_VIEWABLE => {
                        Err("Cursor could not be confined: confine location not viewable")
                    },
                    xproto::GrabStatus::FROZEN => {
                        Err("Cursor could not be confined: frozen by another client")
                    },
                    _ => unreachable!(),
                }
                .map_err(|err| ExternalError::Os(os_error!(OsError::Misc(err))))
            }
            CursorGrabMode::Locked => {
                return Err(ExternalError::NotSupported(NotSupportedError::new()));
            }
        };

        if result.is_ok() {
            *grabbed_lock = mode;
        }

        result
    }

    #[inline]
    pub fn set_cursor_visible(&self, visible: bool) {
        #[allow(clippy::mutex_atomic)]
        let mut visible_lock = self.cursor_visible.lock().unwrap();
        if visible == *visible_lock {
            return;
        }
        let cursor = if visible {
            Some((*self.selected_cursor.lock().unwrap()).clone())
        } else {
            None
        };
        *visible_lock = visible;
        drop(visible_lock);
        let result = match cursor {
            Some(SelectedCursor::Custom(cursor)) => {
                self.xconn.set_custom_cursor(self.xwindow, &cursor)
            }
            Some(SelectedCursor::Named(cursor)) => {
                self.xconn.set_cursor_icon(self.xwindow, Some(cursor))
            }
            None => self.xconn.set_cursor_icon(self.xwindow, None),
        };

        if let Err(err) = result {
            tracing::error!("failed to set cursor icon: {err}");
        }
    }

    #[inline]
    pub fn scale_factor(&self) -> f64 {
        self.shared_state_lock().last_monitor.scale_factor
    }

    pub fn set_cursor_position_physical(
        &self,
        x: i32,
        y: i32,
    ) -> Result<(), ExternalError> {
        {
            self.xconn
                .xcb_connection()
                .warp_pointer(x11rb::NONE, self.xwindow, 0, 0, 0, 0, x as _, y as _)
                .map_err(|e| {
                    ExternalError::Os(os_error!(OsError::XError(
                        X11Error::from(e).into()
                    )))
                })?;
            self.xconn.flush_requests().map_err(|e| {
                ExternalError::Os(os_error!(OsError::XError(X11Error::Xlib(e).into())))
            })
        }
    }

    #[inline]
    pub fn set_cursor_position(&self, position: Position) -> Result<(), ExternalError> {
        let (x, y) = position.to_physical::<i32>(self.scale_factor()).into();
        self.set_cursor_position_physical(x, y)
    }

    #[inline]
    pub fn set_cursor_hittest(&self, hittest: bool) -> Result<(), ExternalError> {
        let mut rectangles: Vec<Rectangle> = Vec::new();
        if hittest {
            let size = self.inner_size();
            rectangles.push(Rectangle {
                x: 0,
                y: 0,
                width: size.width as u16,
                height: size.height as u16,
            })
        }
        let region =
            RegionWrapper::create_region(self.xconn.xcb_connection(), &rectangles)
                .map_err(|_e| ExternalError::Ignored)?;
        self.xconn
            .xcb_connection()
            .xfixes_set_window_shape_region(
                self.xwindow,
                SK::INPUT,
                0,
                0,
                region.region(),
            )
            .map_err(|_e| ExternalError::Ignored)?;
        self.shared_state_lock().cursor_hittest = Some(hittest);
        Ok(())
    }

    /// Moves the window while it is being dragged.
    pub fn drag_window(&self) -> Result<(), ExternalError> {
        self.drag_initiate(util::MOVERESIZE_MOVE)
    }

    #[inline]
    pub fn show_window_menu(&self, _position: Position) {}

    /// Resizes the window while it is being dragged.
    pub fn drag_resize_window(
        &self,
        direction: ResizeDirection,
    ) -> Result<(), ExternalError> {
        self.drag_initiate(match direction {
            ResizeDirection::East => util::MOVERESIZE_RIGHT,
            ResizeDirection::North => util::MOVERESIZE_TOP,
            ResizeDirection::NorthEast => util::MOVERESIZE_TOPRIGHT,
            ResizeDirection::NorthWest => util::MOVERESIZE_TOPLEFT,
            ResizeDirection::South => util::MOVERESIZE_BOTTOM,
            ResizeDirection::SouthEast => util::MOVERESIZE_BOTTOMRIGHT,
            ResizeDirection::SouthWest => util::MOVERESIZE_BOTTOMLEFT,
            ResizeDirection::West => util::MOVERESIZE_LEFT,
        })
    }

    /// Initiates a drag operation while the left mouse button is pressed.
    fn drag_initiate(&self, action: isize) -> Result<(), ExternalError> {
        let pointer = self
            .xconn
            .query_pointer(self.xwindow, util::VIRTUAL_CORE_POINTER)
            .map_err(|err| ExternalError::Os(os_error!(OsError::XError(err.into()))))?;

        let window = self.inner_position().map_err(ExternalError::NotSupported)?;

        let atoms = self.xconn.atoms();
        let message = atoms[_NET_WM_MOVERESIZE];

        // we can't use `set_cursor_grab(false)` here because it doesn't run `XUngrabPointer`
        // if the cursor isn't currently grabbed
        let mut grabbed_lock = self.cursor_grabbed_mode.lock().unwrap();
        self.xconn
            .xcb_connection()
            .ungrab_pointer(x11rb::CURRENT_TIME)
            .map_err(|err| {
                ExternalError::Os(os_error!(OsError::XError(X11Error::from(err).into())))
            })?
            .ignore_error();
        self.xconn.flush_requests().map_err(|err| {
            ExternalError::Os(os_error!(OsError::XError(X11Error::Xlib(err).into())))
        })?;
        *grabbed_lock = CursorGrabMode::None;

        // we keep the lock until we are done
        self.xconn
            .send_client_msg(
                self.xwindow,
                self.root,
                message,
                Some(
                    xproto::EventMask::SUBSTRUCTURE_REDIRECT
                        | xproto::EventMask::SUBSTRUCTURE_NOTIFY,
                ),
                [
                    (window.x + xinput_fp1616_to_float(pointer.win_x) as i32) as u32,
                    (window.y + xinput_fp1616_to_float(pointer.win_y) as i32) as u32,
                    action.try_into().unwrap(),
                    1, // Button 1
                    1,
                ],
            )
            .map_err(|err| ExternalError::Os(os_error!(OsError::XError(err.into()))))?;

        self.xconn.flush_requests().map_err(|err| {
            ExternalError::Os(os_error!(OsError::XError(X11Error::Xlib(err).into())))
        })
    }

    #[inline]
    pub fn set_ime_cursor_area(&self, spot: Position, _size: Size) {
        let (x, y) = spot.to_physical::<i32>(self.scale_factor()).into();
        let _ = self.ime_sender.lock().unwrap().send(ImeRequest::Position(
            self.xwindow as ffi::Window,
            x,
            y,
        ));
    }

    #[inline]
    pub fn set_ime_allowed(&self, allowed: bool) {
        let _ = self
            .ime_sender
            .lock()
            .unwrap()
            .send(ImeRequest::Allow(self.xwindow as ffi::Window, allowed));
    }

    #[inline]
    pub fn set_ime_purpose(&self, _purpose: ImePurpose) {}

    #[inline]
    pub fn focus_window(&self) {
        let atoms = self.xconn.atoms();
        let state_atom = atoms[WM_STATE];
        let state_type_atom = atoms[CARD32];
        let is_minimized = if let Ok(state) =
            self.xconn
                .get_property::<u32>(self.xwindow, state_atom, state_type_atom)
        {
            state.contains(&super::ICONIC_STATE)
        } else {
            false
        };
        let is_visible = match self.shared_state_lock().visibility {
            Visibility::Yes => true,
            Visibility::YesWait | Visibility::No => false,
        };

        if is_visible && !is_minimized {
            self.xconn
                .send_client_msg(
                    self.xwindow,
                    self.root,
                    atoms[_NET_ACTIVE_WINDOW],
                    Some(
                        xproto::EventMask::SUBSTRUCTURE_REDIRECT
                            | xproto::EventMask::SUBSTRUCTURE_NOTIFY,
                    ),
                    [1, x11rb::CURRENT_TIME, 0, 0, 0],
                )
                .expect_then_ignore_error("Failed to send client message");
            if let Err(e) = self.xconn.flush_requests() {
                tracing::error!(
                    "`flush` returned an error when focusing the window. Error was: {}",
                    e
                );
            }
        }
    }

    #[inline]
    pub fn request_user_attention(&self, request_type: Option<UserAttentionType>) {
        let mut wm_hints =
            WmHints::get(self.xconn.xcb_connection(), self.xwindow as xproto::Window)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .flatten()
                .unwrap_or_default();

        wm_hints.urgent = request_type.is_some();
        wm_hints
            .set(self.xconn.xcb_connection(), self.xwindow as xproto::Window)
            .expect_then_ignore_error("Failed to set WM hints");
    }

    #[inline]
    pub(crate) fn generate_activation_token(&self) -> Result<String, X11Error> {
        // Get the title from the WM_NAME property.
        let atoms = self.xconn.atoms();
        let title = {
            let title_bytes = self
                .xconn
                .get_property(self.xwindow, atoms[_NET_WM_NAME], atoms[UTF8_STRING])
                .expect("Failed to get title");

            String::from_utf8(title_bytes).expect("Bad title")
        };

        // Get the activation token and then put it in the event queue.
        let token = self.xconn.request_activation_token(&title)?;

        Ok(token)
    }

    #[inline]
    pub fn request_activation_token(
        &self,
    ) -> Result<AsyncRequestSerial, NotSupportedError> {
        let serial = AsyncRequestSerial::get();
        self.activation_sender
            .send((self.id(), serial))
            .expect("activation token channel should never be closed");
        Ok(serial)
    }

    #[inline]
    pub fn id(&self) -> WindowId {
        WindowId(self.xwindow as _)
    }

    #[inline]
    pub fn request_redraw(&self) {
        self.redraw_sender
            .send(WindowId(self.xwindow as _))
            .unwrap();
    }

    #[inline]
    pub fn pre_present_notify(&self) {
        // TODO timer
    }

    #[inline]
    pub fn raw_window_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawWindowHandle, raw_window_handle::HandleError> {
        let mut window_handle =
            raw_window_handle::XlibWindowHandle::new(self.xlib_window());
        window_handle.visual_id = self.visual as c_ulong;
        Ok(window_handle.into())
    }

    #[inline]
    pub fn raw_display_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawDisplayHandle, raw_window_handle::HandleError> {
        Ok(raw_window_handle::XlibDisplayHandle::new(
            Some(
                std::ptr::NonNull::new(self.xlib_display())
                    .expect("display pointer should never be null"),
            ),
            self.screen_id,
        )
        .into())
    }

    #[inline]
    pub fn theme(&self) -> Option<Theme> {
        None
    }

    pub fn set_content_protected(&self, _protected: bool) {}

    #[inline]
    pub fn has_focus(&self) -> bool {
        self.shared_state_lock().has_focus
    }

    pub fn title(&self) -> String {
        String::new()
    }
}

/// Cast a dimension value into a hinted dimension for `WmSizeHints`, clamping if too large.
fn cast_dimension_to_hint(val: u32) -> i32 {
    val.try_into().unwrap_or(i32::MAX)
}

/// Use the above strategy to cast a physical size into a hinted size.
fn cast_physical_size_to_hint(size: PhysicalSize<u32>) -> (i32, i32) {
    let PhysicalSize { width, height } = size;
    (
        cast_dimension_to_hint(width),
        cast_dimension_to_hint(height),
    )
}

/// Use the above strategy to cast a size into a hinted size.
fn cast_size_to_hint(size: Size, scale_factor: f64) -> (i32, i32) {
    match size {
        Size::Physical(size) => cast_physical_size_to_hint(size),
        Size::Logical(size) => size.to_physical::<i32>(scale_factor).into(),
    }
}
