//! XKB state.

use std::os::raw::c_char;
use std::ptr::NonNull;

use smol_str::SmolStr;
#[cfg(x11_platform)]
use x11_dl::xlib_xcb::xcb_connection_t;
use xkbcommon_dl::{
    self as xkb, xkb_keycode_t, xkb_keysym_t, xkb_layout_index_t, xkb_state,
    xkb_state_component,
};

use crate::platform_impl::common::xkb::keymap::XkbKeymap;
#[cfg(x11_platform)]
use crate::platform_impl::common::xkb::XKBXH;
use crate::platform_impl::common::xkb::{make_string_with, XKBH};

#[derive(Debug)]
pub struct XkbState {
    state: NonNull<xkb_state>,
    modifiers: ModifiersState,
}

impl XkbState {
    #[cfg(wayland_platform)]
    pub fn new_wayland(keymap: &XkbKeymap) -> Option<Self> {
        let state = NonNull::new(unsafe { (XKBH.xkb_state_new)(keymap.as_ptr()) })?;
        Some(Self::new_inner(state))
    }

    #[cfg(x11_platform)]
    pub fn new_x11(xcb: *mut xcb_connection_t, keymap: &XkbKeymap) -> Option<Self> {
        let state = unsafe {
            (XKBXH.xkb_x11_state_new_from_device)(
                keymap.as_ptr(),
                xcb,
                keymap._core_keyboard_id,
            )
        };
        let state = NonNull::new(state)?;
        Some(Self::new_inner(state))
    }

    fn new_inner(state: NonNull<xkb_state>) -> Self {
        let modifiers = ModifiersState::default();
        let mut this = Self { state, modifiers };
        this.reload_modifiers();
        this
    }

    pub fn get_one_sym_raw(&mut self, keycode: xkb_keycode_t) -> xkb_keysym_t {
        unsafe { (XKBH.xkb_state_key_get_one_sym)(self.state.as_ptr(), keycode) }
    }

    pub fn layout(&mut self, key: xkb_keycode_t) -> xkb_layout_index_t {
        unsafe { (XKBH.xkb_state_key_get_layout)(self.state.as_ptr(), key) }
    }

    #[cfg(x11_platform)]
    pub fn depressed_modifiers(&mut self) -> xkb::xkb_mod_mask_t {
        unsafe {
            (XKBH.xkb_state_serialize_mods)(
                self.state.as_ptr(),
                xkb_state_component::XKB_STATE_MODS_DEPRESSED,
            )
        }
    }

    #[cfg(x11_platform)]
    pub fn latched_modifiers(&mut self) -> xkb::xkb_mod_mask_t {
        unsafe {
            (XKBH.xkb_state_serialize_mods)(
                self.state.as_ptr(),
                xkb_state_component::XKB_STATE_MODS_LATCHED,
            )
        }
    }

    #[cfg(x11_platform)]
    pub fn locked_modifiers(&mut self) -> xkb::xkb_mod_mask_t {
        unsafe {
            (XKBH.xkb_state_serialize_mods)(
                self.state.as_ptr(),
                xkb_state_component::XKB_STATE_MODS_LOCKED,
            )
        }
    }

    pub fn get_utf8_raw(
        &mut self,
        keycode: xkb_keycode_t,
        scratch_buffer: &mut Vec<u8>,
    ) -> Option<SmolStr> {
        make_string_with(scratch_buffer, |ptr, len| unsafe {
            (XKBH.xkb_state_key_get_utf8)(self.state.as_ptr(), keycode, ptr, len)
        })
    }

    pub fn modifiers(&self) -> ModifiersState {
        self.modifiers
    }

    pub fn update_modifiers(
        &mut self,
        mods_depressed: u32,
        mods_latched: u32,
        mods_locked: u32,
        depressed_group: u32,
        latched_group: u32,
        locked_group: u32,
    ) {
        let mask = unsafe {
            (XKBH.xkb_state_update_mask)(
                self.state.as_ptr(),
                mods_depressed,
                mods_latched,
                mods_locked,
                depressed_group,
                latched_group,
                locked_group,
            )
        };

        if mask.contains(xkb_state_component::XKB_STATE_MODS_EFFECTIVE) {
            // Effective value of mods have changed, we need to update our state.
            self.reload_modifiers();
        }
    }

    /// Reload the modifiers.
    fn reload_modifiers(&mut self) {
        self.modifiers.ctrl = self.mod_name_is_active(xkb::XKB_MOD_NAME_CTRL);
        self.modifiers.alt = self.mod_name_is_active(xkb::XKB_MOD_NAME_ALT);
        self.modifiers.shift = self.mod_name_is_active(xkb::XKB_MOD_NAME_SHIFT);
        self.modifiers.caps_lock = self.mod_name_is_active(xkb::XKB_MOD_NAME_CAPS);
        self.modifiers.logo = self.mod_name_is_active(xkb::XKB_MOD_NAME_LOGO);
        self.modifiers.num_lock = self.mod_name_is_active(xkb::XKB_MOD_NAME_NUM);
    }

    /// Check if the modifier is active within xkb.
    fn mod_name_is_active(&mut self, name: &[u8]) -> bool {
        unsafe {
            (XKBH.xkb_state_mod_name_is_active)(
                self.state.as_ptr(),
                name.as_ptr() as *const c_char,
                xkb_state_component::XKB_STATE_MODS_EFFECTIVE,
            ) > 0
        }
    }
}

impl Drop for XkbState {
    fn drop(&mut self) {
        unsafe {
            (XKBH.xkb_state_unref)(self.state.as_ptr());
        }
    }
}

/// Represents the current state of the keyboard modifiers
///
/// Each field of this struct represents a modifier and is `true` if this modifier is active.
///
/// For some modifiers, this means that the key is currently pressed, others are toggled
/// (like caps lock).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct ModifiersState {
    /// The "control" key
    pub ctrl: bool,
    /// The "alt" key
    pub alt: bool,
    /// The "shift" key
    pub shift: bool,
    /// The "Caps lock" key
    pub caps_lock: bool,
    /// The "logo" key
    ///
    /// Also known as the "windows" key on most keyboards
    pub logo: bool,
    /// The "Num lock" key
    pub num_lock: bool,
}

impl From<ModifiersState> for crate::keyboard::ModifiersState {
    fn from(mods: ModifiersState) -> crate::keyboard::ModifiersState {
        let mut to_mods = crate::keyboard::ModifiersState::empty();
        to_mods.set(crate::keyboard::ModifiersState::SHIFT, mods.shift);
        to_mods.set(crate::keyboard::ModifiersState::CONTROL, mods.ctrl);
        to_mods.set(crate::keyboard::ModifiersState::ALT, mods.alt);
        to_mods.set(crate::keyboard::ModifiersState::SUPER, mods.logo);
        to_mods
    }
}
