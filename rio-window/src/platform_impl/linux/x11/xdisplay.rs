use std::collections::HashMap;
use std::error::Error;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard};
use std::{fmt, ptr};

use crate::window::CursorIcon;

use super::atoms::Atoms;
use super::ffi;
use super::monitor::MonitorHandle;
use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::render;
use x11rb::protocol::xproto::{self, ConnectionExt};
use x11rb::resource_manager;
use x11rb::xcb_ffi::XCBConnection;

/// A connection to an X server.
pub struct XConnection {
    pub xlib: ffi::Xlib,

    // TODO(notgull): I'd like to remove this, but apparently Xlib and Xinput2 are tied together
    // for some reason.
    pub xinput2: ffi::XInput2,

    pub display: *mut ffi::Display,

    /// The manager for the XCB connection.
    ///
    /// The `Option` ensures that we can drop it before we close the `Display`.
    xcb: Option<XCBConnection>,

    /// The atoms used by `winit`.
    ///
    /// This is a large structure, so I've elected to Box it to make accessing the fields of
    /// this struct easier. Feel free to unbox it if you like kicking puppies.
    atoms: Box<Atoms>,

    /// The index of the default screen.
    default_screen: usize,

    /// The last timestamp received by this connection.
    timestamp: AtomicU32,

    /// List of monitor handles.
    pub monitor_handles: Mutex<Option<Vec<MonitorHandle>>>,

    /// The resource database.
    database: RwLock<resource_manager::Database>,

    /// RandR version.
    randr_version: (u32, u32),

    /// Atom for the XSettings screen.
    xsettings_screen: Option<xproto::Atom>,

    /// XRender format information.
    render_formats: render::QueryPictFormatsReply,

    pub latest_error: Mutex<Option<XError>>,
    pub cursor_cache: Mutex<HashMap<Option<CursorIcon>, xproto::Cursor>>,
}

unsafe impl Send for XConnection {}
unsafe impl Sync for XConnection {}

pub type XErrorHandler = Option<
    unsafe extern "C" fn(*mut ffi::Display, *mut ffi::XErrorEvent) -> std::os::raw::c_int,
>;

impl XConnection {
    pub fn new(error_handler: XErrorHandler) -> Result<XConnection, XNotSupported> {
        // opening the libraries
        let xlib = ffi::Xlib::open()?;
        let xlib_xcb = ffi::Xlib_xcb::open()?;
        let xinput2 = ffi::XInput2::open()?;

        unsafe { (xlib.XInitThreads)() };
        unsafe { (xlib.XSetErrorHandler)(error_handler) };

        // calling XOpenDisplay
        let display = unsafe {
            let display = (xlib.XOpenDisplay)(ptr::null());
            if display.is_null() {
                return Err(XNotSupported::XOpenDisplayFailed);
            }
            display
        };

        // Open the x11rb XCB connection.
        let xcb = {
            // Get a pointer to the underlying XCB connection
            let xcb_connection =
                unsafe { (xlib_xcb.XGetXCBConnection)(display as *mut ffi::Display) };
            assert!(!xcb_connection.is_null());

            // Wrap the XCB connection in an x11rb XCB connection
            let conn = unsafe {
                XCBConnection::from_raw_xcb_connection(xcb_connection.cast(), false)
            };

            conn.map_err(|e| {
                XNotSupported::XcbConversionError(Arc::new(WrapConnectError(e)))
            })?
        };

        // Get the default screen.
        let default_screen = unsafe { (xlib.XDefaultScreen)(display) } as usize;

        // Load the database.
        let database = resource_manager::new_from_default(&xcb)
            .map_err(|e| XNotSupported::XcbConversionError(Arc::new(e)))?;

        // Load the RandR version.
        let randr_version = xcb
            .randr_query_version(1, 3)
            .expect("failed to request XRandR version")
            .reply()
            .expect("failed to query XRandR version");

        let xsettings_screen = Self::new_xsettings_screen(&xcb, default_screen);
        if xsettings_screen.is_none() {
            tracing::warn!(
                "error setting XSETTINGS; Xft options won't reload automatically"
            )
        }

        // Start getting the XRender formats.
        let formats_cookie = render::query_pict_formats(&xcb)
            .map_err(|e| XNotSupported::XcbConversionError(Arc::new(e)))?;

        // Fetch atoms.
        let atoms = Atoms::new(&xcb)
            .map_err(|e| XNotSupported::XcbConversionError(Arc::new(e)))?
            .reply()
            .map_err(|e| XNotSupported::XcbConversionError(Arc::new(e)))?;

        // Finish getting everything else.
        let formats = formats_cookie
            .reply()
            .map_err(|e| XNotSupported::XcbConversionError(Arc::new(e)))?;

        Ok(XConnection {
            xlib,
            xinput2,
            display,
            xcb: Some(xcb),
            atoms: Box::new(atoms),
            default_screen,
            timestamp: AtomicU32::new(0),
            latest_error: Mutex::new(None),
            monitor_handles: Mutex::new(None),
            database: RwLock::new(database),
            cursor_cache: Default::default(),
            randr_version: (randr_version.major_version, randr_version.minor_version),
            render_formats: formats,
            xsettings_screen,
        })
    }

    fn new_xsettings_screen(
        xcb: &XCBConnection,
        default_screen: usize,
    ) -> Option<xproto::Atom> {
        // Fetch the _XSETTINGS_S[screen number] atom.
        let xsettings_screen = xcb
            .intern_atom(false, format!("_XSETTINGS_S{default_screen}").as_bytes())
            .ok()?
            .reply()
            .ok()?
            .atom;

        // Get PropertyNotify events from the XSETTINGS window.
        // TODO: The XSETTINGS window here can change. In the future, listen for DestroyNotify on
        // this window in order to accommodate for a changed window here.
        let selector_window = xcb
            .get_selection_owner(xsettings_screen)
            .ok()?
            .reply()
            .ok()?
            .owner;

        xcb.change_window_attributes(
            selector_window,
            &xproto::ChangeWindowAttributesAux::new()
                .event_mask(xproto::EventMask::PROPERTY_CHANGE),
        )
        .ok()?
        .check()
        .ok()?;

        Some(xsettings_screen)
    }

    /// Checks whether an error has been triggered by the previous function calls.
    #[inline]
    pub fn check_errors(&self) -> Result<(), XError> {
        let error = self.latest_error.lock().unwrap().take();
        if let Some(error) = error {
            Err(error)
        } else {
            Ok(())
        }
    }

    #[inline]
    pub fn randr_version(&self) -> (u32, u32) {
        self.randr_version
    }

    /// Get the underlying XCB connection.
    #[inline]
    pub fn xcb_connection(&self) -> &XCBConnection {
        self.xcb
            .as_ref()
            .expect("xcb_connection somehow called after drop?")
    }

    /// Get the list of atoms.
    #[inline]
    pub fn atoms(&self) -> &Atoms {
        &self.atoms
    }

    /// Get the index of the default screen.
    #[inline]
    pub fn default_screen_index(&self) -> usize {
        self.default_screen
    }

    /// Get the default screen.
    #[inline]
    pub fn default_root(&self) -> &xproto::Screen {
        &self.xcb_connection().setup().roots[self.default_screen]
    }

    /// Get the resource database.
    #[inline]
    pub fn database(&self) -> RwLockReadGuard<'_, resource_manager::Database> {
        self.database.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Reload the resource database.
    #[inline]
    pub fn reload_database(&self) -> Result<(), super::X11Error> {
        let database = resource_manager::new_from_default(self.xcb_connection())?;
        *self.database.write().unwrap_or_else(|e| e.into_inner()) = database;
        Ok(())
    }

    /// Get the latest timestamp.
    #[inline]
    pub fn timestamp(&self) -> u32 {
        self.timestamp.load(Ordering::Relaxed)
    }

    /// Set the last witnessed timestamp.
    #[inline]
    pub fn set_timestamp(&self, timestamp: u32) {
        // Store the timestamp in the slot if it's greater than the last one.
        let mut last_timestamp = self.timestamp.load(Ordering::Relaxed);
        loop {
            let wrapping_sub =
                |a: xproto::Timestamp, b: xproto::Timestamp| (a as i32) - (b as i32);

            if wrapping_sub(timestamp, last_timestamp) <= 0 {
                break;
            }

            match self.timestamp.compare_exchange(
                last_timestamp,
                timestamp,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => last_timestamp = x,
            }
        }
    }

    /// Get the atom for Xsettings.
    #[inline]
    pub fn xsettings_screen(&self) -> Option<xproto::Atom> {
        self.xsettings_screen
    }

    /// Get the data containing our rendering formats.
    #[inline]
    pub fn render_formats(&self) -> &render::QueryPictFormatsReply {
        &self.render_formats
    }

    /// Do we need to do an endian swap?
    #[inline]
    pub fn needs_endian_swap(&self) -> bool {
        #[cfg(target_endian = "big")]
        let endian = xproto::ImageOrder::MSB_FIRST;
        #[cfg(not(target_endian = "big"))]
        let endian = xproto::ImageOrder::LSB_FIRST;

        self.xcb_connection().setup().image_byte_order != endian
    }
}

impl fmt::Debug for XConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.display.fmt(f)
    }
}

impl Drop for XConnection {
    #[inline]
    fn drop(&mut self) {
        self.xcb = None;
        unsafe { (self.xlib.XCloseDisplay)(self.display) };
    }
}

/// Error triggered by xlib.
#[derive(Debug, Clone)]
pub struct XError {
    pub description: String,
    pub error_code: u8,
    pub request_code: u8,
    pub minor_code: u8,
}

impl Error for XError {}

impl fmt::Display for XError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            formatter,
            "X error: {} (code: {}, request code: {}, minor code: {})",
            self.description, self.error_code, self.request_code, self.minor_code
        )
    }
}

/// Error returned if this system doesn't have XLib or can't create an X connection.
#[derive(Clone, Debug)]
pub enum XNotSupported {
    /// Failed to load one or several shared libraries.
    LibraryOpenError(ffi::OpenError),

    /// Connecting to the X server with `XOpenDisplay` failed.
    XOpenDisplayFailed, // TODO: add better message.

    /// We encountered an error while converting the connection to XCB.
    XcbConversionError(Arc<dyn Error + Send + Sync + 'static>),
}

impl From<ffi::OpenError> for XNotSupported {
    #[inline]
    fn from(err: ffi::OpenError) -> XNotSupported {
        XNotSupported::LibraryOpenError(err)
    }
}

impl XNotSupported {
    fn description(&self) -> &'static str {
        match self {
            XNotSupported::LibraryOpenError(_) => {
                "Failed to load one of xlib's shared libraries"
            }
            XNotSupported::XOpenDisplayFailed => "Failed to open connection to X server",
            XNotSupported::XcbConversionError(_) => {
                "Failed to convert Xlib connection to XCB"
            }
        }
    }
}

impl Error for XNotSupported {
    #[inline]
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match *self {
            XNotSupported::LibraryOpenError(ref err) => Some(err),
            XNotSupported::XcbConversionError(ref err) => Some(&**err),
            _ => None,
        }
    }
}

impl fmt::Display for XNotSupported {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        formatter.write_str(self.description())
    }
}

/// A newtype wrapper around a `ConnectError` that can't be accessed by downstream libraries.
///
/// Without this, `x11rb` would become a public dependency.
#[derive(Debug)]
struct WrapConnectError(x11rb::rust_connection::ConnectError);

impl fmt::Display for WrapConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl Error for WrapConnectError {
    // We can't implement `source()` here or otherwise risk exposing `x11rb`.
}
