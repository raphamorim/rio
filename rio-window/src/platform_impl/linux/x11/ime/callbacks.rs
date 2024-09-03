use std::collections::HashMap;
use std::os::raw::c_char;
use std::ptr;
use std::sync::Arc;

use super::{ffi, XConnection, XError};

use super::context::{ImeContext, ImeContextCreationError};
use super::inner::{close_im, ImeInner};
use super::input_method::PotentialInputMethods;

pub(crate) unsafe fn xim_set_callback(
    xconn: &Arc<XConnection>,
    xim: ffi::XIM,
    field: *const c_char,
    callback: *mut ffi::XIMCallback,
) -> Result<(), XError> {
    // It's advisable to wrap variadic FFI functions in our own functions, as we want to minimize
    // access that isn't type-checked.
    unsafe { (xconn.xlib.XSetIMValues)(xim, field, callback, ptr::null_mut::<()>()) };
    xconn.check_errors()
}

// Set a callback for when an input method matching the current locale modifiers becomes
// available. Note that this has nothing to do with what input methods are open or able to be
// opened, and simply uses the modifiers that are set when the callback is set.
// * This is called per locale modifier, not per input method opened with that locale modifier.
// * Trying to set this for multiple locale modifiers causes problems, i.e. one of the rebuilt input
//   contexts would always silently fail to use the input method.
pub(crate) unsafe fn set_instantiate_callback(
    xconn: &Arc<XConnection>,
    client_data: ffi::XPointer,
) -> Result<(), XError> {
    unsafe {
        (xconn.xlib.XRegisterIMInstantiateCallback)(
            xconn.display,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            Some(xim_instantiate_callback),
            client_data,
        )
    };
    xconn.check_errors()
}

pub(crate) unsafe fn unset_instantiate_callback(
    xconn: &Arc<XConnection>,
    client_data: ffi::XPointer,
) -> Result<(), XError> {
    unsafe {
        (xconn.xlib.XUnregisterIMInstantiateCallback)(
            xconn.display,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            Some(xim_instantiate_callback),
            client_data,
        )
    };
    xconn.check_errors()
}

pub(crate) unsafe fn set_destroy_callback(
    xconn: &Arc<XConnection>,
    im: ffi::XIM,
    inner: &ImeInner,
) -> Result<(), XError> {
    unsafe {
        xim_set_callback(
            xconn,
            im,
            ffi::XNDestroyCallback_0.as_ptr() as *const _,
            &inner.destroy_callback as *const _ as *mut _,
        )
    }
}

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
enum ReplaceImError {
    // Boxed to prevent large error type
    MethodOpenFailed(#[allow(dead_code)] Box<PotentialInputMethods>),
    ContextCreationFailed(#[allow(dead_code)] ImeContextCreationError),
    SetDestroyCallbackFailed(#[allow(dead_code)] XError),
}

// Attempt to replace current IM (which may or may not be presently valid) with a new one. This
// includes replacing all existing input contexts and free'ing resources as necessary. This only
// modifies existing state if all operations succeed.
unsafe fn replace_im(inner: *mut ImeInner) -> Result<(), ReplaceImError> {
    let xconn = unsafe { &(*inner).xconn };

    let (new_im, is_fallback) = {
        let new_im = unsafe { (*inner).potential_input_methods.open_im(xconn, None) };
        let is_fallback = new_im.is_fallback();
        (
            new_im.ok().ok_or_else(|| {
                ReplaceImError::MethodOpenFailed(Box::new(unsafe {
                    (*inner).potential_input_methods.clone()
                }))
            })?,
            is_fallback,
        )
    };

    // It's important to always set a destroy callback, since there's otherwise potential for us
    // to try to use or free a resource that's already been destroyed on the server.
    {
        let result = unsafe { set_destroy_callback(xconn, new_im.im, &*inner) };
        if result.is_err() {
            let _ = unsafe { close_im(xconn, new_im.im) };
        }
        result
    }
    .map_err(ReplaceImError::SetDestroyCallbackFailed)?;

    let mut new_contexts = HashMap::new();
    for (window, old_context) in unsafe { (*inner).contexts.iter() } {
        let spot = old_context.as_ref().map(|old_context| old_context.ic_spot);

        // Check if the IME was allowed on that context.
        let is_allowed = old_context
            .as_ref()
            .map(|old_context| old_context.is_allowed())
            .unwrap_or_default();

        // We can't use the style from the old context here, since it may change on reload, so
        // pick style from the new XIM based on the old state.
        let style = if is_allowed {
            new_im.preedit_style
        } else {
            new_im.none_style
        };

        let new_context = {
            let result = unsafe {
                ImeContext::new(
                    xconn,
                    new_im.im,
                    style,
                    *window,
                    spot,
                    (*inner).event_sender.clone(),
                )
            };
            if result.is_err() {
                let _ = unsafe { close_im(xconn, new_im.im) };
            }
            result.map_err(ReplaceImError::ContextCreationFailed)?
        };
        new_contexts.insert(*window, Some(new_context));
    }

    // If we've made it this far, everything succeeded.
    unsafe {
        let _ = (*inner).destroy_all_contexts_if_necessary();
        let _ = (*inner).close_im_if_necessary();
        (*inner).im = Some(new_im);
        (*inner).contexts = new_contexts;
        (*inner).is_destroyed = false;
        (*inner).is_fallback = is_fallback;
    }
    Ok(())
}

pub unsafe extern "C" fn xim_instantiate_callback(
    _display: *mut ffi::Display,
    client_data: ffi::XPointer,
    // This field is unsupplied.
    _call_data: ffi::XPointer,
) {
    let inner: *mut ImeInner = client_data as _;
    if !inner.is_null() {
        let xconn = unsafe { &(*inner).xconn };
        match unsafe { replace_im(inner) } {
            Ok(()) => unsafe {
                let _ = unset_instantiate_callback(xconn, client_data);
                (*inner).is_fallback = false;
            },
            Err(err) => unsafe {
                if (*inner).is_destroyed {
                    // We have no usable input methods!
                    panic!("Failed to reopen input method: {err:?}");
                }
            },
        }
    }
}

// This callback is triggered when the input method is closed on the server end. When this
// happens, XCloseIM/XDestroyIC doesn't need to be called, as the resources have already been
// free'd (attempting to do so causes our connection to freeze).
pub unsafe extern "C" fn xim_destroy_callback(
    _xim: ffi::XIM,
    client_data: ffi::XPointer,
    // This field is unsupplied.
    _call_data: ffi::XPointer,
) {
    let inner: *mut ImeInner = client_data as _;
    if !inner.is_null() {
        unsafe { (*inner).is_destroyed = true };
        let xconn = unsafe { &(*inner).xconn };
        if unsafe { !(*inner).is_fallback } {
            let _ = unsafe { set_instantiate_callback(xconn, client_data) };
            // Attempt to open fallback input method.
            match unsafe { replace_im(inner) } {
                Ok(()) => unsafe { (*inner).is_fallback = true },
                Err(err) => {
                    // We have no usable input methods!
                    panic!("Failed to open fallback input method: {err:?}");
                }
            }
        }
    }
}
