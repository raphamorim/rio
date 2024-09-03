use std::ffi::c_int;
use std::sync::Arc;

use x11_dl::xlib::{self, XEvent, XGenericEventCookie};

use crate::platform_impl::x11::XConnection;

/// XEvents of type GenericEvent store their actual data in an XGenericEventCookie data structure.
/// This is a wrapper to extract the cookie from a GenericEvent XEvent and release the cookie data
/// once it has been processed
pub struct GenericEventCookie {
    cookie: XGenericEventCookie,
    xconn: Arc<XConnection>,
}

impl GenericEventCookie {
    pub fn from_event(
        xconn: Arc<XConnection>,
        event: XEvent,
    ) -> Option<GenericEventCookie> {
        unsafe {
            let mut cookie: XGenericEventCookie = From::from(event);
            if (xconn.xlib.XGetEventData)(xconn.display, &mut cookie) == xlib::True {
                Some(GenericEventCookie { cookie, xconn })
            } else {
                None
            }
        }
    }

    #[inline]
    pub fn extension(&self) -> u8 {
        self.cookie.extension as u8
    }

    #[inline]
    pub fn evtype(&self) -> c_int {
        self.cookie.evtype
    }

    /// Borrow inner event data as `&T`.
    ///
    /// ## SAFETY
    ///
    /// The caller must ensure that the event has the `T` inside of it.
    #[inline]
    pub unsafe fn as_event<T>(&self) -> &T {
        unsafe { &*(self.cookie.data as *const _) }
    }
}

impl Drop for GenericEventCookie {
    fn drop(&mut self) {
        unsafe {
            (self.xconn.xlib.XFreeEventData)(self.xconn.display, &mut self.cookie);
        }
    }
}
