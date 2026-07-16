#![allow(clippy::missing_safety_doc)]

use crate::{Renderer, Theme};
use librio_vt::RenderState;
use std::ffi::{c_char, c_void, CStr};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::NonNull;

#[no_mangle]
pub unsafe extern "C" fn rio_renderer_new(
    ns_view: *mut c_void,
    width: f32,
    height: f32,
    scale: f32,
    font_size: f32,
) -> *mut Renderer {
    catch_unwind(AssertUnwindSafe(|| {
        let Some(view) = NonNull::new(ns_view) else {
            return std::ptr::null_mut();
        };
        let handle = raw_window_handle::AppKitWindowHandle::new(view);
        let window = sugarloaf::SugarloafWindow {
            handle: raw_window_handle::RawWindowHandle::AppKit(handle),
            display: raw_window_handle::RawDisplayHandle::AppKit(
                raw_window_handle::AppKitDisplayHandle::new(),
            ),
            scale,
            size: sugarloaf::SugarloafWindowSize { width, height },
        };
        match Renderer::new(window, font_size, Theme::default()) {
            Ok(renderer) => Box::into_raw(Box::new(renderer)),
            Err(err) => {
                tracing_error(&err);
                std::ptr::null_mut()
            }
        }
    }))
    .unwrap_or(std::ptr::null_mut())
}

fn tracing_error(err: &str) {
    eprintln!("rio_renderer_new failed: {err}");
}

#[no_mangle]
pub unsafe extern "C" fn rio_renderer_free(renderer: *mut Renderer) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !renderer.is_null() {
            drop(unsafe { Box::from_raw(renderer) });
        }
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_renderer_draw(
    renderer: *mut Renderer,
    state: *mut RenderState,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if renderer.is_null() || state.is_null() {
            return;
        }
        let renderer = unsafe { &mut *renderer };
        let state = unsafe { &*state };
        renderer.draw(state);
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_renderer_resize(
    renderer: *mut Renderer,
    pixel_width: u32,
    pixel_height: u32,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !renderer.is_null() {
            unsafe { &mut *renderer }.resize(pixel_width, pixel_height);
        }
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_renderer_rescale(renderer: *mut Renderer, scale: f32) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !renderer.is_null() {
            unsafe { &mut *renderer }.rescale(scale);
        }
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_renderer_cell_size(
    renderer: *const Renderer,
    out_width: *mut f32,
    out_height: *mut f32,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if renderer.is_null() || out_width.is_null() || out_height.is_null() {
            return;
        }
        let (width, height) = unsafe { &*renderer }.cell_size();
        unsafe {
            *out_width = width;
            *out_height = height;
        }
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_renderer_set_font_size(renderer: *mut Renderer, size: f32) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !renderer.is_null() {
            unsafe { &mut *renderer }.set_font_size(size);
        }
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_renderer_font_size(renderer: *const Renderer) -> f32 {
    catch_unwind(AssertUnwindSafe(|| {
        if renderer.is_null() {
            return 0.0;
        }
        unsafe { &*renderer }.font_size()
    }))
    .unwrap_or(0.0)
}

#[no_mangle]
pub unsafe extern "C" fn rio_renderer_set_preedit(
    renderer: *mut Renderer,
    preedit: *const c_char,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if renderer.is_null() {
            return;
        }
        let value = if preedit.is_null() {
            None
        } else {
            let text = unsafe { CStr::from_ptr(preedit) }
                .to_string_lossy()
                .into_owned();
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        };
        unsafe { &mut *renderer }.set_preedit(value);
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rio_renderer_padding(renderer: *const Renderer) -> f32 {
    catch_unwind(AssertUnwindSafe(|| {
        if renderer.is_null() {
            return 0.0;
        }
        unsafe { &*renderer }.padding()
    }))
    .unwrap_or(0.0)
}
