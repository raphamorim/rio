use std::ffi::CStr;
use std::os::raw::c_short;
use std::sync::Arc;
use std::{mem, ptr};

use x11_dl::xlib::{
    XIMCallback, XIMPreeditCaretCallbackStruct, XIMPreeditDrawCallbackStruct,
};

use crate::platform_impl::platform::x11::ime::input_method::{Style, XIMStyle};
use crate::platform_impl::platform::x11::ime::{ImeEvent, ImeEventSender};

use super::{ffi, util, XConnection, XError};

/// IME creation error.
#[derive(Debug)]
pub enum ImeContextCreationError {
    /// Got the error from Xlib.
    XError(XError),

    /// Got null pointer from Xlib but without exact reason.
    Null,
}

/// The callback used by XIM preedit functions.
type XIMProcNonnull = unsafe extern "C" fn(ffi::XIM, ffi::XPointer, ffi::XPointer);

/// Wrapper for creating XIM callbacks.
#[inline]
fn create_xim_callback(
    client_data: ffi::XPointer,
    callback: XIMProcNonnull,
) -> ffi::XIMCallback {
    XIMCallback {
        client_data,
        callback: Some(callback),
    }
}

/// The server started preedit.
extern "C" fn preedit_start_callback(
    _xim: ffi::XIM,
    client_data: ffi::XPointer,
    _call_data: ffi::XPointer,
) -> i32 {
    let client_data = unsafe { &mut *(client_data as *mut ImeContextClientData) };

    client_data.text.clear();
    client_data.cursor_pos = 0;
    client_data
        .event_sender
        .send((client_data.window, ImeEvent::Start))
        .expect("failed to send preedit start event");
    -1
}

/// Done callback is used when the preedit should be hidden.
extern "C" fn preedit_done_callback(
    _xim: ffi::XIM,
    client_data: ffi::XPointer,
    _call_data: ffi::XPointer,
) {
    let client_data = unsafe { &mut *(client_data as *mut ImeContextClientData) };

    // Drop text buffer and reset cursor position on done.
    client_data.text = Vec::new();
    client_data.cursor_pos = 0;

    client_data
        .event_sender
        .send((client_data.window, ImeEvent::End))
        .expect("failed to send preedit end event");
}

fn calc_byte_position(text: &[char], pos: usize) -> usize {
    text.iter()
        .take(pos)
        .fold(0, |byte_pos, text| byte_pos + text.len_utf8())
}

/// Preedit text information to be drawn inline by the client.
extern "C" fn preedit_draw_callback(
    _xim: ffi::XIM,
    client_data: ffi::XPointer,
    call_data: ffi::XPointer,
) {
    let client_data = unsafe { &mut *(client_data as *mut ImeContextClientData) };
    let call_data = unsafe { &mut *(call_data as *mut XIMPreeditDrawCallbackStruct) };
    client_data.cursor_pos = call_data.caret as usize;

    let chg_range = call_data.chg_first as usize
        ..(call_data.chg_first + call_data.chg_length) as usize;
    if chg_range.start > client_data.text.len() || chg_range.end > client_data.text.len()
    {
        tracing::warn!(
            "invalid chg range: buffer length={}, but chg_first={} chg_lengthg={}",
            client_data.text.len(),
            call_data.chg_first,
            call_data.chg_length
        );
        return;
    }

    // NULL indicate text deletion
    let mut new_chars = if call_data.text.is_null() {
        Vec::new()
    } else {
        let xim_text = unsafe { &mut *(call_data.text) };
        if xim_text.encoding_is_wchar > 0 {
            return;
        }

        let new_text = unsafe { xim_text.string.multi_byte };

        if new_text.is_null() {
            return;
        }

        let new_text = unsafe { CStr::from_ptr(new_text) };

        String::from(new_text.to_str().expect("Invalid UTF-8 String from IME"))
            .chars()
            .collect()
    };
    let mut old_text_tail = client_data.text.split_off(chg_range.end);
    client_data.text.truncate(chg_range.start);
    client_data.text.append(&mut new_chars);
    client_data.text.append(&mut old_text_tail);
    let cursor_byte_pos = calc_byte_position(&client_data.text, client_data.cursor_pos);

    client_data
        .event_sender
        .send((
            client_data.window,
            ImeEvent::Update(client_data.text.iter().collect(), cursor_byte_pos),
        ))
        .expect("failed to send preedit update event");
}

/// Handling of cursor movements in preedit text.
extern "C" fn preedit_caret_callback(
    _xim: ffi::XIM,
    client_data: ffi::XPointer,
    call_data: ffi::XPointer,
) {
    let client_data = unsafe { &mut *(client_data as *mut ImeContextClientData) };
    let call_data = unsafe { &mut *(call_data as *mut XIMPreeditCaretCallbackStruct) };

    if call_data.direction == ffi::XIMCaretDirection::XIMAbsolutePosition {
        client_data.cursor_pos = call_data.position as usize;
        let cursor_byte_pos =
            calc_byte_position(&client_data.text, client_data.cursor_pos);

        client_data
            .event_sender
            .send((
                client_data.window,
                ImeEvent::Update(client_data.text.iter().collect(), cursor_byte_pos),
            ))
            .expect("failed to send preedit update event");
    }
}

/// Struct to simplify callback creation and latter passing into Xlib XIM.
struct PreeditCallbacks {
    start_callback: ffi::XIMCallback,
    done_callback: ffi::XIMCallback,
    draw_callback: ffi::XIMCallback,
    caret_callback: ffi::XIMCallback,
}

impl PreeditCallbacks {
    pub fn new(client_data: ffi::XPointer) -> PreeditCallbacks {
        let start_callback = create_xim_callback(client_data, unsafe {
            mem::transmute::<
                usize,
                unsafe extern "C" fn(ffi::XIM, ffi::XPointer, ffi::XPointer),
            >(preedit_start_callback as usize)
        });
        let done_callback = create_xim_callback(client_data, preedit_done_callback);
        let caret_callback = create_xim_callback(client_data, preedit_caret_callback);
        let draw_callback = create_xim_callback(client_data, preedit_draw_callback);

        PreeditCallbacks {
            start_callback,
            done_callback,
            caret_callback,
            draw_callback,
        }
    }
}

struct ImeContextClientData {
    window: ffi::Window,
    event_sender: ImeEventSender,
    text: Vec<char>,
    cursor_pos: usize,
}

// XXX: this struct doesn't destroy its XIC resource when dropped.
// This is intentional, as it doesn't have enough information to know whether or not the context
// still exists on the server. Since `ImeInner` has that awareness, destruction must be handled
// through `ImeInner`.
pub struct ImeContext {
    pub(crate) ic: ffi::XIC,
    pub(crate) ic_spot: ffi::XPoint,
    pub(crate) style: Style,
    // Since the data is passed shared between X11 XIM callbacks, but couldn't be directly free
    // from there we keep the pointer to automatically deallocate it.
    _client_data: Box<ImeContextClientData>,
}

impl ImeContext {
    pub(crate) unsafe fn new(
        xconn: &Arc<XConnection>,
        im: ffi::XIM,
        style: Style,
        window: ffi::Window,
        ic_spot: Option<ffi::XPoint>,
        event_sender: ImeEventSender,
    ) -> Result<Self, ImeContextCreationError> {
        let client_data = Box::into_raw(Box::new(ImeContextClientData {
            window,
            event_sender,
            text: Vec::new(),
            cursor_pos: 0,
        }));

        let ic = match style as _ {
            Style::Preedit(style) => unsafe {
                ImeContext::create_preedit_ic(
                    xconn,
                    im,
                    style,
                    window,
                    client_data as ffi::XPointer,
                )
            },
            Style::Nothing(style) => unsafe {
                ImeContext::create_nothing_ic(xconn, im, style, window)
            },
            Style::None(style) => unsafe {
                ImeContext::create_none_ic(xconn, im, style, window)
            },
        }
        .ok_or(ImeContextCreationError::Null)?;

        xconn
            .check_errors()
            .map_err(ImeContextCreationError::XError)?;

        let mut context = ImeContext {
            ic,
            ic_spot: ffi::XPoint { x: 0, y: 0 },
            style,
            _client_data: unsafe { Box::from_raw(client_data) },
        };

        // Set the spot location, if it's present.
        if let Some(ic_spot) = ic_spot {
            context.set_spot(xconn, ic_spot.x, ic_spot.y)
        }

        Ok(context)
    }

    unsafe fn create_none_ic(
        xconn: &Arc<XConnection>,
        im: ffi::XIM,
        style: XIMStyle,
        window: ffi::Window,
    ) -> Option<ffi::XIC> {
        let ic = unsafe {
            (xconn.xlib.XCreateIC)(
                im,
                ffi::XNInputStyle_0.as_ptr() as *const _,
                style,
                ffi::XNClientWindow_0.as_ptr() as *const _,
                window,
                ptr::null_mut::<()>(),
            )
        };

        (!ic.is_null()).then_some(ic)
    }

    unsafe fn create_preedit_ic(
        xconn: &Arc<XConnection>,
        im: ffi::XIM,
        style: XIMStyle,
        window: ffi::Window,
        client_data: ffi::XPointer,
    ) -> Option<ffi::XIC> {
        let preedit_callbacks = PreeditCallbacks::new(client_data);
        let preedit_attr = util::memory::XSmartPointer::new(xconn, unsafe {
            (xconn.xlib.XVaCreateNestedList)(
                0,
                ffi::XNPreeditStartCallback_0.as_ptr() as *const _,
                &(preedit_callbacks.start_callback) as *const _,
                ffi::XNPreeditDoneCallback_0.as_ptr() as *const _,
                &(preedit_callbacks.done_callback) as *const _,
                ffi::XNPreeditCaretCallback_0.as_ptr() as *const _,
                &(preedit_callbacks.caret_callback) as *const _,
                ffi::XNPreeditDrawCallback_0.as_ptr() as *const _,
                &(preedit_callbacks.draw_callback) as *const _,
                ptr::null_mut::<()>(),
            )
        })
        .expect("XVaCreateNestedList returned NULL");

        let ic = unsafe {
            (xconn.xlib.XCreateIC)(
                im,
                ffi::XNInputStyle_0.as_ptr() as *const _,
                style,
                ffi::XNClientWindow_0.as_ptr() as *const _,
                window,
                ffi::XNPreeditAttributes_0.as_ptr() as *const _,
                preedit_attr.ptr,
                ptr::null_mut::<()>(),
            )
        };

        (!ic.is_null()).then_some(ic)
    }

    unsafe fn create_nothing_ic(
        xconn: &Arc<XConnection>,
        im: ffi::XIM,
        style: XIMStyle,
        window: ffi::Window,
    ) -> Option<ffi::XIC> {
        let ic = unsafe {
            (xconn.xlib.XCreateIC)(
                im,
                ffi::XNInputStyle_0.as_ptr() as *const _,
                style,
                ffi::XNClientWindow_0.as_ptr() as *const _,
                window,
                ptr::null_mut::<()>(),
            )
        };

        (!ic.is_null()).then_some(ic)
    }

    pub(crate) fn focus(&self, xconn: &Arc<XConnection>) -> Result<(), XError> {
        unsafe {
            (xconn.xlib.XSetICFocus)(self.ic);
        }
        xconn.check_errors()
    }

    pub(crate) fn unfocus(&self, xconn: &Arc<XConnection>) -> Result<(), XError> {
        unsafe {
            (xconn.xlib.XUnsetICFocus)(self.ic);
        }
        xconn.check_errors()
    }

    pub fn is_allowed(&self) -> bool {
        !matches!(self.style, Style::None(_))
    }

    // Set the spot for preedit text. Setting spot isn't working with libX11 when preedit callbacks
    // are being used. Certain IMEs do show selection window, but it's placed in bottom left of the
    // window and couldn't be changed.
    //
    // For me see: https://bugs.freedesktop.org/show_bug.cgi?id=1580.
    pub(crate) fn set_spot(&mut self, xconn: &Arc<XConnection>, x: c_short, y: c_short) {
        if !self.is_allowed() || self.ic_spot.x == x && self.ic_spot.y == y {
            return;
        }

        self.ic_spot = ffi::XPoint { x, y };

        unsafe {
            let preedit_attr = util::memory::XSmartPointer::new(
                xconn,
                (xconn.xlib.XVaCreateNestedList)(
                    0,
                    ffi::XNSpotLocation_0.as_ptr(),
                    &self.ic_spot,
                    ptr::null_mut::<()>(),
                ),
            )
            .expect("XVaCreateNestedList returned NULL");

            (xconn.xlib.XSetICValues)(
                self.ic,
                ffi::XNPreeditAttributes_0.as_ptr() as *const _,
                preedit_attr.ptr,
                ptr::null_mut::<()>(),
            );
        }
    }
}
