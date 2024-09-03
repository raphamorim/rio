use std::{slice, str};
use x11rb::protocol::xinput::{self, ConnectionExt as _};
use x11rb::protocol::xkb;

use super::*;

pub const VIRTUAL_CORE_POINTER: u16 = 2;
pub const VIRTUAL_CORE_KEYBOARD: u16 = 3;

// A base buffer size of 1kB uses a negligible amount of RAM while preventing us from having to
// re-allocate (and make another round-trip) in the *vast* majority of cases.
// To test if `lookup_utf8` works correctly, set this to 1.
const TEXT_BUFFER_SIZE: usize = 1024;

impl XConnection {
    pub fn select_xinput_events(
        &self,
        window: xproto::Window,
        device_id: u16,
        mask: xinput::XIEventMask,
    ) -> Result<VoidCookie<'_>, X11Error> {
        self.xcb_connection()
            .xinput_xi_select_events(
                window,
                &[xinput::EventMask {
                    deviceid: device_id,
                    mask: vec![mask],
                }],
            )
            .map_err(Into::into)
    }

    pub fn select_xkb_events(
        &self,
        device_id: xkb::DeviceSpec,
        mask: xkb::EventType,
    ) -> Result<bool, X11Error> {
        let mask = u16::from(mask) as _;
        let status = unsafe {
            (self.xlib.XkbSelectEvents)(self.display, device_id as _, mask, mask)
        };

        if status == ffi::True {
            self.flush_requests()?;
            Ok(true)
        } else {
            tracing::error!(
                "Could not select XKB events: The XKB extension is not initialized!"
            );
            Ok(false)
        }
    }

    pub fn query_pointer(
        &self,
        window: xproto::Window,
        device_id: u16,
    ) -> Result<xinput::XIQueryPointerReply, X11Error> {
        self.xcb_connection()
            .xinput_xi_query_pointer(window, device_id)?
            .reply()
            .map_err(Into::into)
    }

    fn lookup_utf8_inner(
        &self,
        ic: ffi::XIC,
        key_event: &mut ffi::XKeyEvent,
        buffer: *mut u8,
        size: usize,
    ) -> (ffi::KeySym, ffi::Status, c_int) {
        let mut keysym: ffi::KeySym = 0;
        let mut status: ffi::Status = 0;
        let count = unsafe {
            (self.xlib.Xutf8LookupString)(
                ic,
                key_event,
                buffer as *mut c_char,
                size as c_int,
                &mut keysym,
                &mut status,
            )
        };
        (keysym, status, count)
    }

    pub fn lookup_utf8(&self, ic: ffi::XIC, key_event: &mut ffi::XKeyEvent) -> String {
        // `assume_init` is safe here because the array consists of `MaybeUninit` values,
        // which do not require initialization.
        let mut buffer: [MaybeUninit<u8>; TEXT_BUFFER_SIZE] =
            unsafe { MaybeUninit::uninit().assume_init() };
        // If the buffer overflows, we'll make a new one on the heap.
        let mut vec;

        let (_, status, count) = self.lookup_utf8_inner(
            ic,
            key_event,
            buffer.as_mut_ptr() as *mut u8,
            buffer.len(),
        );

        let bytes = if status == ffi::XBufferOverflow {
            vec = Vec::with_capacity(count as usize);
            let (_, _, new_count) =
                self.lookup_utf8_inner(ic, key_event, vec.as_mut_ptr(), vec.capacity());
            debug_assert_eq!(count, new_count);

            unsafe { vec.set_len(count as usize) };
            &vec[..count as usize]
        } else {
            unsafe { slice::from_raw_parts(buffer.as_ptr() as *const u8, count as usize) }
        };

        str::from_utf8(bytes).unwrap_or("").to_string()
    }
}
