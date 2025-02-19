use std::ffi::{c_void, OsString};
use std::os::windows::prelude::OsStringExt;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::UI::Input::Ime::{
    ImmAssociateContextEx, ImmGetCompositionStringW, ImmGetContext, ImmReleaseContext,
    ImmSetCandidateWindow, ImmSetCompositionWindow, ATTR_TARGET_CONVERTED,
    ATTR_TARGET_NOTCONVERTED, CANDIDATEFORM, CFS_EXCLUDE, CFS_POINT, COMPOSITIONFORM,
    GCS_COMPATTR, GCS_COMPSTR, GCS_CURSORPOS, GCS_RESULTSTR, HIMC, IACE_CHILDREN,
    IACE_DEFAULT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_IMMENABLED};

use crate::dpi::{Position, Size};
use crate::platform::windows::HWND;

pub struct ImeContext {
    hwnd: HWND,
    himc: HIMC,
}

impl ImeContext {
    pub unsafe fn current(hwnd: HWND) -> Self {
        let himc = unsafe { ImmGetContext(hwnd) };
        ImeContext { hwnd, himc }
    }

    pub unsafe fn get_composing_text_and_cursor(
        &self,
    ) -> Option<(String, Option<usize>, Option<usize>)> {
        let text = unsafe { self.get_composition_string(GCS_COMPSTR) }?;
        let attrs =
            unsafe { self.get_composition_data(GCS_COMPATTR) }.unwrap_or_default();

        let mut first = None;
        let mut last = None;
        let mut boundary_before_char = 0;

        for (attr, chr) in attrs.into_iter().zip(text.chars()) {
            let char_is_targeted = attr as u32 == ATTR_TARGET_CONVERTED
                || attr as u32 == ATTR_TARGET_NOTCONVERTED;

            if first.is_none() && char_is_targeted {
                first = Some(boundary_before_char);
            } else if first.is_some() && last.is_none() && !char_is_targeted {
                last = Some(boundary_before_char);
            }

            boundary_before_char += chr.len_utf8();
        }

        if first.is_some() && last.is_none() {
            last = Some(text.len());
        } else if first.is_none() {
            // IME haven't split words and select any clause yet, so trying to retrieve normal
            // cursor.
            let cursor = unsafe { self.get_composition_cursor(&text) };
            first = cursor;
            last = cursor;
        }

        Some((text, first, last))
    }

    pub unsafe fn get_composed_text(&self) -> Option<String> {
        unsafe { self.get_composition_string(GCS_RESULTSTR) }
    }

    unsafe fn get_composition_cursor(&self, text: &str) -> Option<usize> {
        let cursor =
            unsafe { ImmGetCompositionStringW(self.himc, GCS_CURSORPOS, null_mut(), 0) };
        (cursor >= 0).then(|| text.chars().take(cursor as _).map(|c| c.len_utf8()).sum())
    }

    unsafe fn get_composition_string(&self, gcs_mode: u32) -> Option<String> {
        let data = unsafe { self.get_composition_data(gcs_mode) }?;
        let (prefix, shorts, suffix) = unsafe { data.align_to::<u16>() };
        if prefix.is_empty() && suffix.is_empty() {
            OsString::from_wide(shorts).into_string().ok()
        } else {
            None
        }
    }

    unsafe fn get_composition_data(&self, gcs_mode: u32) -> Option<Vec<u8>> {
        let size =
            match unsafe { ImmGetCompositionStringW(self.himc, gcs_mode, null_mut(), 0) }
            {
                0 => return Some(Vec::new()),
                size if size < 0 => return None,
                size => size,
            };

        let mut buf = Vec::<u8>::with_capacity(size as _);
        let size = unsafe {
            ImmGetCompositionStringW(
                self.himc,
                gcs_mode,
                buf.as_mut_ptr() as *mut c_void,
                size as _,
            )
        };

        if size < 0 {
            None
        } else {
            unsafe { buf.set_len(size as _) };
            Some(buf)
        }
    }

    pub unsafe fn set_ime_cursor_area(
        &self,
        spot: Position,
        size: Size,
        scale_factor: f64,
    ) {
        if !unsafe { ImeContext::system_has_ime() } {
            return;
        }

        let (x, y) = spot.to_physical::<i32>(scale_factor).into();
        let (width, height): (i32, i32) = size.to_physical::<i32>(scale_factor).into();
        let rc_area = RECT {
            left: x,
            top: y,
            right: x + width,
            bottom: y + height,
        };
        let candidate_form = CANDIDATEFORM {
            dwIndex: 0,
            dwStyle: CFS_EXCLUDE,
            ptCurrentPos: POINT { x, y },
            rcArea: rc_area,
        };
        let composition_form = COMPOSITIONFORM {
            dwStyle: CFS_POINT,
            ptCurrentPos: POINT { x, y: y + height },
            rcArea: rc_area,
        };

        unsafe {
            ImmSetCompositionWindow(self.himc, &composition_form);
            ImmSetCandidateWindow(self.himc, &candidate_form);
        }
    }

    pub unsafe fn set_ime_allowed(hwnd: HWND, allowed: bool) {
        if !unsafe { ImeContext::system_has_ime() } {
            return;
        }

        if allowed {
            unsafe { ImmAssociateContextEx(hwnd, null_mut(), IACE_DEFAULT) };
        } else {
            unsafe { ImmAssociateContextEx(hwnd, null_mut(), IACE_CHILDREN) };
        }
    }

    unsafe fn system_has_ime() -> bool {
        unsafe { GetSystemMetrics(SM_IMMENABLED) != 0 }
    }
}

impl Drop for ImeContext {
    fn drop(&mut self) {
        unsafe { ImmReleaseContext(self.hwnd, self.himc) };
    }
}
