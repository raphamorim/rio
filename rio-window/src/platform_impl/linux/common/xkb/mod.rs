use std::ops::Deref;
use std::os::raw::c_char;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::utils::Lazy;
use smol_str::SmolStr;
#[cfg(wayland_platform)]
use std::os::unix::io::OwnedFd;
use tracing::warn;
use xkbcommon_dl::{
    self as xkb, xkb_compose_status, xkb_context, xkb_context_flags,
    xkbcommon_compose_handle, xkbcommon_handle, XkbCommon, XkbCommonCompose,
};
#[cfg(x11_platform)]
use {x11_dl::xlib_xcb::xcb_connection_t, xkbcommon_dl::x11::xkbcommon_x11_handle};

use crate::event::{ElementState, KeyEvent};
use crate::keyboard::{Key, KeyLocation};
use crate::platform_impl::KeyEventExtra;

mod compose;
mod keymap;
mod state;

use compose::{ComposeStatus, XkbComposeState, XkbComposeTable};
use keymap::XkbKeymap;

#[cfg(x11_platform)]
pub use keymap::raw_keycode_to_physicalkey;
pub use keymap::{physicalkey_to_scancode, scancode_to_physicalkey};
pub use state::XkbState;

// TODO: Wire this up without using a static `AtomicBool`.
static RESET_DEAD_KEYS: AtomicBool = AtomicBool::new(false);

static XKBH: Lazy<&'static XkbCommon> = Lazy::new(xkbcommon_handle);
static XKBCH: Lazy<&'static XkbCommonCompose> = Lazy::new(xkbcommon_compose_handle);
#[cfg(feature = "x11")]
static XKBXH: Lazy<&'static xkb::x11::XkbCommonX11> = Lazy::new(xkbcommon_x11_handle);

#[inline(always)]
pub fn reset_dead_keys() {
    RESET_DEAD_KEYS.store(true, Ordering::SeqCst);
}

#[derive(Debug)]
pub enum Error {
    /// libxkbcommon is not available
    XKBNotFound,
}

#[derive(Debug)]
pub struct Context {
    // NOTE: field order matters.
    #[cfg(x11_platform)]
    pub core_keyboard_id: i32,
    state: Option<XkbState>,
    keymap: Option<XkbKeymap>,
    compose_state1: Option<XkbComposeState>,
    compose_state2: Option<XkbComposeState>,
    _compose_table: Option<XkbComposeTable>,
    context: XkbContext,
    scratch_buffer: Vec<u8>,
}

impl Context {
    pub fn new() -> Result<Self, Error> {
        if xkb::xkbcommon_option().is_none() {
            return Err(Error::XKBNotFound);
        }

        let context = XkbContext::new()?;
        let mut compose_table = XkbComposeTable::new(&context);
        let mut compose_state1 =
            compose_table.as_ref().and_then(|table| table.new_state());
        let mut compose_state2 =
            compose_table.as_ref().and_then(|table| table.new_state());

        // Disable compose if anything compose related failed to initialize.
        if compose_table.is_none() || compose_state1.is_none() || compose_state2.is_none()
        {
            compose_state2 = None;
            compose_state1 = None;
            compose_table = None;
        }

        Ok(Self {
            state: None,
            keymap: None,
            compose_state1,
            compose_state2,
            #[cfg(x11_platform)]
            core_keyboard_id: 0,
            _compose_table: compose_table,
            context,
            scratch_buffer: Vec::with_capacity(8),
        })
    }

    #[cfg(feature = "x11")]
    pub fn from_x11_xkb(xcb: *mut xcb_connection_t) -> Result<Self, Error> {
        let result = unsafe {
            (XKBXH.xkb_x11_setup_xkb_extension)(
                xcb,
                1,
                2,
                xkbcommon_dl::x11::xkb_x11_setup_xkb_extension_flags::XKB_X11_SETUP_XKB_EXTENSION_NO_FLAGS,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };

        if result != 1 {
            return Err(Error::XKBNotFound);
        }

        let mut this = Self::new()?;
        this.core_keyboard_id =
            unsafe { (XKBXH.xkb_x11_get_core_keyboard_device_id)(xcb) };
        this.set_keymap_from_x11(xcb);
        Ok(this)
    }

    pub fn state_mut(&mut self) -> Option<&mut XkbState> {
        self.state.as_mut()
    }

    pub fn keymap_mut(&mut self) -> Option<&mut XkbKeymap> {
        self.keymap.as_mut()
    }

    #[cfg(wayland_platform)]
    pub fn set_keymap_from_fd(&mut self, fd: OwnedFd, size: usize) {
        let keymap = XkbKeymap::from_fd(&self.context, fd, size);
        let state = keymap.as_ref().and_then(XkbState::new_wayland);
        if keymap.is_none() || state.is_none() {
            warn!("failed to update xkb keymap");
        }
        self.state = state;
        self.keymap = keymap;
    }

    #[cfg(x11_platform)]
    pub fn set_keymap_from_x11(&mut self, xcb: *mut xcb_connection_t) {
        let keymap =
            XkbKeymap::from_x11_keymap(&self.context, xcb, self.core_keyboard_id);
        let state = keymap
            .as_ref()
            .and_then(|keymap| XkbState::new_x11(xcb, keymap));
        if keymap.is_none() || state.is_none() {
            warn!("failed to update xkb keymap");
        }
        self.state = state;
        self.keymap = keymap;
    }

    /// Key builder context with the user provided xkb state.
    pub fn key_context(&mut self) -> Option<KeyContext<'_>> {
        let state = self.state.as_mut()?;
        let keymap = self.keymap.as_mut()?;
        let compose_state1 = self.compose_state1.as_mut();
        let compose_state2 = self.compose_state2.as_mut();
        let scratch_buffer = &mut self.scratch_buffer;
        Some(KeyContext {
            state,
            keymap,
            compose_state1,
            compose_state2,
            scratch_buffer,
        })
    }

    /// Key builder context with the user provided xkb state.
    ///
    /// Should be used when the original context must not be altered.
    #[cfg(x11_platform)]
    pub fn key_context_with_state<'a>(
        &'a mut self,
        state: &'a mut XkbState,
    ) -> Option<KeyContext<'a>> {
        let keymap = self.keymap.as_mut()?;
        let compose_state1 = self.compose_state1.as_mut();
        let compose_state2 = self.compose_state2.as_mut();
        let scratch_buffer = &mut self.scratch_buffer;
        Some(KeyContext {
            state,
            keymap,
            compose_state1,
            compose_state2,
            scratch_buffer,
        })
    }
}

pub struct KeyContext<'a> {
    pub state: &'a mut XkbState,
    pub keymap: &'a mut XkbKeymap,
    compose_state1: Option<&'a mut XkbComposeState>,
    compose_state2: Option<&'a mut XkbComposeState>,
    scratch_buffer: &'a mut Vec<u8>,
}

impl<'a> KeyContext<'a> {
    pub fn process_key_event(
        &mut self,
        keycode: u32,
        state: ElementState,
        repeat: bool,
    ) -> KeyEvent {
        let mut event = KeyEventResults::new(
            self,
            keycode,
            !repeat && state == ElementState::Pressed,
        );
        let physical_key = keymap::raw_keycode_to_physicalkey(keycode);
        let (logical_key, location) = event.key();
        let text = event.text();
        let (key_without_modifiers, _) = event.key_without_modifiers();
        let text_with_all_modifiers = event.text_with_all_modifiers();

        let platform_specific = KeyEventExtra {
            text_with_all_modifiers,
            key_without_modifiers,
        };

        KeyEvent {
            physical_key,
            logical_key,
            text,
            location,
            state,
            repeat,
            platform_specific,
        }
    }

    fn keysym_to_utf8_raw(&mut self, keysym: u32) -> Option<SmolStr> {
        self.scratch_buffer.clear();
        self.scratch_buffer.reserve(8);
        loop {
            let bytes_written = unsafe {
                (XKBH.xkb_keysym_to_utf8)(
                    keysym,
                    self.scratch_buffer.as_mut_ptr().cast(),
                    self.scratch_buffer.capacity(),
                )
            };
            if bytes_written == 0 {
                return None;
            } else if bytes_written == -1 {
                self.scratch_buffer.reserve(8);
            } else {
                unsafe {
                    self.scratch_buffer
                        .set_len(bytes_written.try_into().unwrap())
                };
                break;
            }
        }

        // Remove the null-terminator
        self.scratch_buffer.pop();
        byte_slice_to_smol_str(self.scratch_buffer)
    }
}

struct KeyEventResults<'a, 'b> {
    context: &'a mut KeyContext<'b>,
    keycode: u32,
    keysym: u32,
    compose: ComposeStatus,
}

impl<'a, 'b> KeyEventResults<'a, 'b> {
    fn new(context: &'a mut KeyContext<'b>, keycode: u32, compose: bool) -> Self {
        let keysym = context.state.get_one_sym_raw(keycode);

        let compose =
            if let Some(state) = context.compose_state1.as_mut().filter(|_| compose) {
                if RESET_DEAD_KEYS.swap(false, Ordering::SeqCst) {
                    state.reset();
                    context.compose_state2.as_mut().unwrap().reset();
                }
                state.feed(keysym)
            } else {
                ComposeStatus::None
            };

        KeyEventResults {
            context,
            keycode,
            keysym,
            compose,
        }
    }

    pub fn key(&mut self) -> (Key, KeyLocation) {
        let (key, location) = match self.keysym_to_key(self.keysym) {
            Ok(known) => return known,
            Err(undefined) => undefined,
        };

        if let ComposeStatus::Accepted(xkb_compose_status::XKB_COMPOSE_COMPOSING) =
            self.compose
        {
            let compose_state = self.context.compose_state2.as_mut().unwrap();
            // When pressing a dead key twice, the non-combining variant of that character will
            // be produced. Since this function only concerns itself with a single keypress, we
            // simulate this double press here by feeding the keysym to the compose state
            // twice.

            compose_state.feed(self.keysym);
            if matches!(compose_state.feed(self.keysym), ComposeStatus::Accepted(_)) {
                // Extracting only a single `char` here *should* be fine, assuming that no
                // dead key's non-combining variant ever occupies more than one `char`.
                let text = compose_state.get_string(self.context.scratch_buffer);
                let key = Key::Dead(text.and_then(|s| s.chars().next()));
                (key, location)
            } else {
                (key, location)
            }
        } else {
            let key = self
                .composed_text()
                .unwrap_or_else(|_| self.context.keysym_to_utf8_raw(self.keysym))
                .map(Key::Character)
                .unwrap_or(key);
            (key, location)
        }
    }

    pub fn key_without_modifiers(&mut self) -> (Key, KeyLocation) {
        // This will become a pointer to an array which libxkbcommon owns, so we don't need to
        // deallocate it.
        let layout = self.context.state.layout(self.keycode);
        let keysym = self
            .context
            .keymap
            .first_keysym_by_level(layout, self.keycode);

        match self.keysym_to_key(keysym) {
            Ok((key, location)) => (key, location),
            Err((key, location)) => {
                let key = self
                    .context
                    .keysym_to_utf8_raw(keysym)
                    .map(Key::Character)
                    .unwrap_or(key);
                (key, location)
            }
        }
    }

    fn keysym_to_key(
        &self,
        keysym: u32,
    ) -> Result<(Key, KeyLocation), (Key, KeyLocation)> {
        let location = keymap::keysym_location(keysym);
        let key = keymap::keysym_to_key(keysym);
        if matches!(key, Key::Unidentified(_)) {
            Err((key, location))
        } else {
            Ok((key, location))
        }
    }

    pub fn text(&mut self) -> Option<SmolStr> {
        self.composed_text()
            .unwrap_or_else(|_| self.context.keysym_to_utf8_raw(self.keysym))
    }

    // The current behaviour makes it so composing a character overrides attempts to input a
    // control character with the `Ctrl` key. We can potentially add a configuration option
    // if someone specifically wants the oppsite behaviour.
    pub fn text_with_all_modifiers(&mut self) -> Option<SmolStr> {
        match self.composed_text() {
            Ok(text) => text,
            Err(_) => self
                .context
                .state
                .get_utf8_raw(self.keycode, self.context.scratch_buffer),
        }
    }

    fn composed_text(&mut self) -> Result<Option<SmolStr>, ()> {
        match self.compose {
            ComposeStatus::Accepted(status) => match status {
                xkb_compose_status::XKB_COMPOSE_COMPOSED => {
                    let state = self.context.compose_state1.as_mut().unwrap();
                    Ok(state.get_string(self.context.scratch_buffer))
                }
                xkb_compose_status::XKB_COMPOSE_COMPOSING
                | xkb_compose_status::XKB_COMPOSE_CANCELLED => Ok(None),
                xkb_compose_status::XKB_COMPOSE_NOTHING => Err(()),
            },
            _ => Err(()),
        }
    }
}

#[derive(Debug)]
pub struct XkbContext {
    context: NonNull<xkb_context>,
}

impl XkbContext {
    pub fn new() -> Result<Self, Error> {
        let context =
            unsafe { (XKBH.xkb_context_new)(xkb_context_flags::XKB_CONTEXT_NO_FLAGS) };

        let context = match NonNull::new(context) {
            Some(context) => context,
            None => return Err(Error::XKBNotFound),
        };

        Ok(Self { context })
    }
}

impl Drop for XkbContext {
    fn drop(&mut self) {
        unsafe {
            (XKBH.xkb_context_unref)(self.context.as_ptr());
        }
    }
}

impl Deref for XkbContext {
    type Target = NonNull<xkb_context>;

    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

/// Shared logic for constructing a string with `xkb_compose_state_get_utf8` and
/// `xkb_state_key_get_utf8`.
fn make_string_with<F>(scratch_buffer: &mut Vec<u8>, mut f: F) -> Option<SmolStr>
where
    F: FnMut(*mut c_char, usize) -> i32,
{
    let size = f(ptr::null_mut(), 0);
    if size == 0 {
        return None;
    }
    let size = usize::try_from(size).unwrap();
    scratch_buffer.clear();
    // The allocated buffer must include space for the null-terminator.
    scratch_buffer.reserve(size + 1);
    unsafe {
        let written = f(
            scratch_buffer.as_mut_ptr().cast(),
            scratch_buffer.capacity(),
        );
        if usize::try_from(written).unwrap() != size {
            // This will likely never happen.
            return None;
        }
        scratch_buffer.set_len(size);
    };

    byte_slice_to_smol_str(scratch_buffer)
}

// NOTE: This is track_caller so we can have more informative line numbers when logging
#[track_caller]
fn byte_slice_to_smol_str(bytes: &[u8]) -> Option<SmolStr> {
    std::str::from_utf8(bytes)
        .map(SmolStr::new)
        .map_err(|e| {
            tracing::warn!(
                "UTF-8 received from libxkbcommon ({:?}) was invalid: {e}",
                bytes
            )
        })
        .ok()
}
