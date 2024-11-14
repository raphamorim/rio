use std::collections::HashMap;
use std::mem;
use std::sync::Arc;

use super::{ffi, XConnection, XError};

use super::context::ImeContext;
use super::input_method::{InputMethod, PotentialInputMethods};
use crate::platform_impl::platform::x11::ime::ImeEventSender;

pub(crate) unsafe fn close_im(
    xconn: &Arc<XConnection>,
    im: ffi::XIM,
) -> Result<(), XError> {
    unsafe { (xconn.xlib.XCloseIM)(im) };
    xconn.check_errors()
}

pub(crate) unsafe fn destroy_ic(
    xconn: &Arc<XConnection>,
    ic: ffi::XIC,
) -> Result<(), XError> {
    unsafe { (xconn.xlib.XDestroyIC)(ic) };
    xconn.check_errors()
}

pub(crate) struct ImeInner {
    pub xconn: Arc<XConnection>,
    pub im: Option<InputMethod>,
    pub potential_input_methods: PotentialInputMethods,
    pub contexts: HashMap<ffi::Window, Option<ImeContext>>,
    // WARNING: this is initially zeroed!
    pub destroy_callback: ffi::XIMCallback,
    pub event_sender: ImeEventSender,
    // Indicates whether or not the input method was destroyed on the server end
    // (i.e. if ibus/fcitx/etc. was terminated/restarted)
    pub is_destroyed: bool,
    pub is_fallback: bool,
}

impl ImeInner {
    pub(crate) fn new(
        xconn: Arc<XConnection>,
        potential_input_methods: PotentialInputMethods,
        event_sender: ImeEventSender,
    ) -> Self {
        ImeInner {
            xconn,
            im: None,
            potential_input_methods,
            contexts: HashMap::new(),
            destroy_callback: unsafe { mem::zeroed() },
            event_sender,
            is_destroyed: false,
            is_fallback: false,
        }
    }

    pub unsafe fn close_im_if_necessary(&self) -> Result<bool, XError> {
        if !self.is_destroyed && self.im.is_some() {
            unsafe { close_im(&self.xconn, self.im.as_ref().unwrap().im) }.map(|_| true)
        } else {
            Ok(false)
        }
    }

    pub unsafe fn destroy_ic_if_necessary(&self, ic: ffi::XIC) -> Result<bool, XError> {
        if !self.is_destroyed {
            unsafe { destroy_ic(&self.xconn, ic) }.map(|_| true)
        } else {
            Ok(false)
        }
    }

    pub unsafe fn destroy_all_contexts_if_necessary(&self) -> Result<bool, XError> {
        for context in self.contexts.values().flatten() {
            unsafe { self.destroy_ic_if_necessary(context.ic)? };
        }
        Ok(!self.is_destroyed)
    }
}
