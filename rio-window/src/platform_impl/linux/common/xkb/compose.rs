//! XKB compose handling.

use std::env;
use std::ffi::CString;
use std::ops::Deref;
use std::os::unix::ffi::OsStringExt;
use std::ptr::NonNull;

use super::{XkbContext, XKBCH};
use smol_str::SmolStr;
use xkbcommon_dl::{
    xkb_compose_compile_flags, xkb_compose_feed_result, xkb_compose_state,
    xkb_compose_state_flags, xkb_compose_status, xkb_compose_table, xkb_keysym_t,
};

#[derive(Debug)]
pub struct XkbComposeTable {
    table: NonNull<xkb_compose_table>,
}

impl XkbComposeTable {
    pub fn new(context: &XkbContext) -> Option<Self> {
        let locale = env::var_os("LC_ALL")
            .and_then(|v| if v.is_empty() { None } else { Some(v) })
            .or_else(|| env::var_os("LC_CTYPE"))
            .and_then(|v| if v.is_empty() { None } else { Some(v) })
            .or_else(|| env::var_os("LANG"))
            .and_then(|v| if v.is_empty() { None } else { Some(v) })
            .unwrap_or_else(|| "C".into());
        let locale = CString::new(locale.into_vec()).unwrap();

        let table = unsafe {
            (XKBCH.xkb_compose_table_new_from_locale)(
                context.as_ptr(),
                locale.as_ptr(),
                xkb_compose_compile_flags::XKB_COMPOSE_COMPILE_NO_FLAGS,
            )
        };

        let table = NonNull::new(table)?;
        Some(Self { table })
    }

    /// Create new state with the given compose table.
    pub fn new_state(&self) -> Option<XkbComposeState> {
        let state = unsafe {
            (XKBCH.xkb_compose_state_new)(
                self.table.as_ptr(),
                xkb_compose_state_flags::XKB_COMPOSE_STATE_NO_FLAGS,
            )
        };

        let state = NonNull::new(state)?;
        Some(XkbComposeState { state })
    }
}

impl Deref for XkbComposeTable {
    type Target = NonNull<xkb_compose_table>;

    fn deref(&self) -> &Self::Target {
        &self.table
    }
}

impl Drop for XkbComposeTable {
    fn drop(&mut self) {
        unsafe {
            (XKBCH.xkb_compose_table_unref)(self.table.as_ptr());
        }
    }
}

#[derive(Debug)]
pub struct XkbComposeState {
    state: NonNull<xkb_compose_state>,
}

impl XkbComposeState {
    pub fn get_string(&mut self, scratch_buffer: &mut Vec<u8>) -> Option<SmolStr> {
        super::make_string_with(scratch_buffer, |ptr, len| unsafe {
            (XKBCH.xkb_compose_state_get_utf8)(self.state.as_ptr(), ptr, len)
        })
    }

    #[inline]
    pub fn feed(&mut self, keysym: xkb_keysym_t) -> ComposeStatus {
        let feed_result =
            unsafe { (XKBCH.xkb_compose_state_feed)(self.state.as_ptr(), keysym) };
        match feed_result {
            xkb_compose_feed_result::XKB_COMPOSE_FEED_IGNORED => ComposeStatus::Ignored,
            xkb_compose_feed_result::XKB_COMPOSE_FEED_ACCEPTED => {
                ComposeStatus::Accepted(self.status())
            }
        }
    }

    #[inline]
    pub fn reset(&mut self) {
        unsafe {
            (XKBCH.xkb_compose_state_reset)(self.state.as_ptr());
        }
    }

    #[inline]
    pub fn status(&mut self) -> xkb_compose_status {
        unsafe { (XKBCH.xkb_compose_state_get_status)(self.state.as_ptr()) }
    }
}

impl Drop for XkbComposeState {
    fn drop(&mut self) {
        unsafe {
            (XKBCH.xkb_compose_state_unref)(self.state.as_ptr());
        };
    }
}

#[derive(Copy, Clone, Debug)]
pub enum ComposeStatus {
    Accepted(xkb_compose_status),
    Ignored,
    None,
}
