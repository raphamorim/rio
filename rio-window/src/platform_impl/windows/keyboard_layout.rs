use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::sync::Mutex;

use crate::utils::Lazy;
use smol_str::SmolStr;
use windows_sys::Win32::System::SystemServices::{LANG_JAPANESE, LANG_KOREAN};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, GetKeyboardLayout, MapVirtualKeyExW, ToUnicodeEx, HKL,
    MAPVK_VK_TO_VSC_EX, VIRTUAL_KEY, VK_ACCEPT, VK_ADD, VK_APPS, VK_ATTN, VK_BACK,
    VK_BROWSER_BACK, VK_BROWSER_FAVORITES, VK_BROWSER_FORWARD, VK_BROWSER_HOME,
    VK_BROWSER_REFRESH, VK_BROWSER_SEARCH, VK_BROWSER_STOP, VK_CANCEL, VK_CAPITAL,
    VK_CLEAR, VK_CONTROL, VK_CONVERT, VK_CRSEL, VK_DECIMAL, VK_DELETE, VK_DIVIDE,
    VK_DOWN, VK_END, VK_EREOF, VK_ESCAPE, VK_EXECUTE, VK_EXSEL, VK_F1, VK_F10, VK_F11,
    VK_F12, VK_F13, VK_F14, VK_F15, VK_F16, VK_F17, VK_F18, VK_F19, VK_F2, VK_F20,
    VK_F21, VK_F22, VK_F23, VK_F24, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9,
    VK_FINAL, VK_GAMEPAD_A, VK_GAMEPAD_B, VK_GAMEPAD_DPAD_DOWN, VK_GAMEPAD_DPAD_LEFT,
    VK_GAMEPAD_DPAD_RIGHT, VK_GAMEPAD_DPAD_UP, VK_GAMEPAD_LEFT_SHOULDER,
    VK_GAMEPAD_LEFT_THUMBSTICK_BUTTON, VK_GAMEPAD_LEFT_THUMBSTICK_DOWN,
    VK_GAMEPAD_LEFT_THUMBSTICK_LEFT, VK_GAMEPAD_LEFT_THUMBSTICK_RIGHT,
    VK_GAMEPAD_LEFT_THUMBSTICK_UP, VK_GAMEPAD_LEFT_TRIGGER, VK_GAMEPAD_MENU,
    VK_GAMEPAD_RIGHT_SHOULDER, VK_GAMEPAD_RIGHT_THUMBSTICK_BUTTON,
    VK_GAMEPAD_RIGHT_THUMBSTICK_DOWN, VK_GAMEPAD_RIGHT_THUMBSTICK_LEFT,
    VK_GAMEPAD_RIGHT_THUMBSTICK_RIGHT, VK_GAMEPAD_RIGHT_THUMBSTICK_UP,
    VK_GAMEPAD_RIGHT_TRIGGER, VK_GAMEPAD_VIEW, VK_GAMEPAD_X, VK_GAMEPAD_Y, VK_HANGUL,
    VK_HANJA, VK_HELP, VK_HOME, VK_ICO_00, VK_ICO_CLEAR, VK_ICO_HELP, VK_INSERT,
    VK_JUNJA, VK_KANA, VK_KANJI, VK_LAUNCH_APP1, VK_LAUNCH_APP2, VK_LAUNCH_MAIL,
    VK_LAUNCH_MEDIA_SELECT, VK_LBUTTON, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT,
    VK_LWIN, VK_MBUTTON, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK,
    VK_MEDIA_STOP, VK_MENU, VK_MODECHANGE, VK_MULTIPLY, VK_NAVIGATION_ACCEPT,
    VK_NAVIGATION_CANCEL, VK_NAVIGATION_DOWN, VK_NAVIGATION_LEFT, VK_NAVIGATION_MENU,
    VK_NAVIGATION_RIGHT, VK_NAVIGATION_UP, VK_NAVIGATION_VIEW, VK_NEXT, VK_NONAME,
    VK_NONCONVERT, VK_NUMLOCK, VK_NUMPAD0, VK_NUMPAD1, VK_NUMPAD2, VK_NUMPAD3,
    VK_NUMPAD4, VK_NUMPAD5, VK_NUMPAD6, VK_NUMPAD7, VK_NUMPAD8, VK_NUMPAD9, VK_OEM_1,
    VK_OEM_102, VK_OEM_2, VK_OEM_3, VK_OEM_4, VK_OEM_5, VK_OEM_6, VK_OEM_7, VK_OEM_8,
    VK_OEM_ATTN, VK_OEM_AUTO, VK_OEM_AX, VK_OEM_BACKTAB, VK_OEM_CLEAR, VK_OEM_COMMA,
    VK_OEM_COPY, VK_OEM_CUSEL, VK_OEM_ENLW, VK_OEM_FINISH, VK_OEM_FJ_LOYA,
    VK_OEM_FJ_MASSHOU, VK_OEM_FJ_ROYA, VK_OEM_FJ_TOUROKU, VK_OEM_JUMP, VK_OEM_MINUS,
    VK_OEM_NEC_EQUAL, VK_OEM_PA1, VK_OEM_PA2, VK_OEM_PA3, VK_OEM_PERIOD, VK_OEM_PLUS,
    VK_OEM_RESET, VK_OEM_WSCTRL, VK_PA1, VK_PACKET, VK_PAUSE, VK_PLAY, VK_PRINT,
    VK_PRIOR, VK_PROCESSKEY, VK_RBUTTON, VK_RCONTROL, VK_RETURN, VK_RIGHT, VK_RMENU,
    VK_RSHIFT, VK_RWIN, VK_SCROLL, VK_SELECT, VK_SEPARATOR, VK_SHIFT, VK_SLEEP,
    VK_SNAPSHOT, VK_SPACE, VK_SUBTRACT, VK_TAB, VK_UP, VK_VOLUME_DOWN, VK_VOLUME_MUTE,
    VK_VOLUME_UP, VK_XBUTTON1, VK_XBUTTON2, VK_ZOOM,
};

use crate::keyboard::{Key, KeyCode, ModifiersState, NamedKey, NativeKey, PhysicalKey};
use crate::platform_impl::{loword, primarylangid, scancode_to_physicalkey};

pub(crate) static LAYOUT_CACHE: Lazy<Mutex<LayoutCache>> =
    Lazy::new(|| Mutex::new(LayoutCache::default()));

fn key_pressed(vkey: VIRTUAL_KEY) -> bool {
    unsafe { (GetKeyState(vkey as i32) & (1 << 15)) == (1 << 15) }
}

const NUMPAD_VKEYS: [VIRTUAL_KEY; 16] = [
    VK_NUMPAD0,
    VK_NUMPAD1,
    VK_NUMPAD2,
    VK_NUMPAD3,
    VK_NUMPAD4,
    VK_NUMPAD5,
    VK_NUMPAD6,
    VK_NUMPAD7,
    VK_NUMPAD8,
    VK_NUMPAD9,
    VK_MULTIPLY,
    VK_ADD,
    VK_SEPARATOR,
    VK_SUBTRACT,
    VK_DECIMAL,
    VK_DIVIDE,
];

static NUMPAD_KEYCODES: Lazy<HashSet<KeyCode>> = Lazy::new(|| {
    let mut keycodes = HashSet::new();
    keycodes.insert(KeyCode::Numpad0);
    keycodes.insert(KeyCode::Numpad1);
    keycodes.insert(KeyCode::Numpad2);
    keycodes.insert(KeyCode::Numpad3);
    keycodes.insert(KeyCode::Numpad4);
    keycodes.insert(KeyCode::Numpad5);
    keycodes.insert(KeyCode::Numpad6);
    keycodes.insert(KeyCode::Numpad7);
    keycodes.insert(KeyCode::Numpad8);
    keycodes.insert(KeyCode::Numpad9);
    keycodes.insert(KeyCode::NumpadMultiply);
    keycodes.insert(KeyCode::NumpadAdd);
    keycodes.insert(KeyCode::NumpadComma);
    keycodes.insert(KeyCode::NumpadSubtract);
    keycodes.insert(KeyCode::NumpadDecimal);
    keycodes.insert(KeyCode::NumpadDivide);
    keycodes
});

bitflags::bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    pub struct WindowsModifiers : u8 {
        const SHIFT = 1 << 0;
        const CONTROL = 1 << 1;
        const ALT = 1 << 2;
        const CAPS_LOCK = 1 << 3;
        const FLAGS_END = 1 << 4;
    }
}

impl WindowsModifiers {
    pub fn active_modifiers(key_state: &[u8; 256]) -> WindowsModifiers {
        let shift = key_state[VK_SHIFT as usize] & 0x80 != 0;
        let lshift = key_state[VK_LSHIFT as usize] & 0x80 != 0;
        let rshift = key_state[VK_RSHIFT as usize] & 0x80 != 0;

        let control = key_state[VK_CONTROL as usize] & 0x80 != 0;
        let lcontrol = key_state[VK_LCONTROL as usize] & 0x80 != 0;
        let rcontrol = key_state[VK_RCONTROL as usize] & 0x80 != 0;

        let alt = key_state[VK_MENU as usize] & 0x80 != 0;
        let lalt = key_state[VK_LMENU as usize] & 0x80 != 0;
        let ralt = key_state[VK_RMENU as usize] & 0x80 != 0;

        let caps = key_state[VK_CAPITAL as usize] & 0x01 != 0;

        let mut result = WindowsModifiers::empty();
        if shift || lshift || rshift {
            result.insert(WindowsModifiers::SHIFT);
        }
        if control || lcontrol || rcontrol {
            result.insert(WindowsModifiers::CONTROL);
        }
        if alt || lalt || ralt {
            result.insert(WindowsModifiers::ALT);
        }
        if caps {
            result.insert(WindowsModifiers::CAPS_LOCK);
        }

        result
    }

    pub fn apply_to_kbd_state(self, key_state: &mut [u8; 256]) {
        if self.intersects(Self::SHIFT) {
            key_state[VK_SHIFT as usize] |= 0x80;
        } else {
            key_state[VK_SHIFT as usize] &= !0x80;
            key_state[VK_LSHIFT as usize] &= !0x80;
            key_state[VK_RSHIFT as usize] &= !0x80;
        }
        if self.intersects(Self::CONTROL) {
            key_state[VK_CONTROL as usize] |= 0x80;
        } else {
            key_state[VK_CONTROL as usize] &= !0x80;
            key_state[VK_LCONTROL as usize] &= !0x80;
            key_state[VK_RCONTROL as usize] &= !0x80;
        }
        if self.intersects(Self::ALT) {
            key_state[VK_MENU as usize] |= 0x80;
        } else {
            key_state[VK_MENU as usize] &= !0x80;
            key_state[VK_LMENU as usize] &= !0x80;
            key_state[VK_RMENU as usize] &= !0x80;
        }
        if self.intersects(Self::CAPS_LOCK) {
            key_state[VK_CAPITAL as usize] |= 0x01;
        } else {
            key_state[VK_CAPITAL as usize] &= !0x01;
        }
    }

    /// Removes the control modifier if the alt modifier is not present.
    /// This is useful because on Windows: (Control + Alt) == AltGr
    /// but we don't want to interfere with the AltGr state.
    pub fn remove_only_ctrl(mut self) -> WindowsModifiers {
        if !self.contains(WindowsModifiers::ALT) {
            self.remove(WindowsModifiers::CONTROL);
        }
        self
    }
}

pub(crate) struct Layout {
    pub hkl: u64,

    /// Maps numpad keys from Windows virtual key to a `Key`.
    ///
    /// This is useful because some numpad keys generate different characters based on the locale.
    /// For example `VK_DECIMAL` is sometimes "." and sometimes ",". Note: numpad-specific virtual
    /// keys are only produced by Windows when the NumLock is active.
    ///
    /// Making this field separate from the `keys` field saves having to add NumLock as a modifier
    /// to `WindowsModifiers`, which would double the number of items in keys.
    pub numlock_on_keys: HashMap<VIRTUAL_KEY, Key>,
    /// Like `numlock_on_keys` but this will map to the key that would be produced if numlock was
    /// off. The keys of this map are identical to the keys of `numlock_on_keys`.
    pub numlock_off_keys: HashMap<VIRTUAL_KEY, Key>,

    /// Maps a modifier state to group of key strings
    /// We're not using `ModifiersState` here because that object cannot express caps lock,
    /// but we need to handle caps lock too.
    ///
    /// This map shouldn't need to exist.
    /// However currently this seems to be the only good way
    /// of getting the label for the pressed key. Note that calling `ToUnicode`
    /// just when the key is pressed/released would be enough if `ToUnicode` wouldn't
    /// change the keyboard state (it clears the dead key). There is a flag to prevent
    /// changing the state, but that flag requires Windows 10, version 1607 or newer)
    pub keys: HashMap<WindowsModifiers, HashMap<KeyCode, Key>>,
    pub has_alt_graph: bool,
}

impl Layout {
    pub fn get_key(
        &self,
        mods: WindowsModifiers,
        num_lock_on: bool,
        vkey: VIRTUAL_KEY,
        physical_key: &PhysicalKey,
    ) -> Key {
        let native_code = NativeKey::Windows(vkey);

        let unknown_alt = vkey == VK_MENU;
        if !unknown_alt {
            // Here we try using the virtual key directly but if the virtual key doesn't distinguish
            // between left and right alt, we can't report AltGr. Therefore, we only do this if the
            // key is not the "unknown alt" key.
            //
            // The reason for using the virtual key directly is that `MapVirtualKeyExW` (used when
            // building the keys map) sometimes maps virtual keys to odd scancodes that don't match
            // the scancode coming from the KEYDOWN message for the same key. For example: `VK_LEFT`
            // is mapped to `0x004B`, but the scancode for the left arrow is `0xE04B`.
            let key_from_vkey = vkey_to_non_char_key(
                vkey,
                native_code.clone(),
                self.hkl,
                self.has_alt_graph,
            );

            if !matches!(key_from_vkey, Key::Unidentified(_)) {
                return key_from_vkey;
            }
        }
        if num_lock_on {
            if let Some(key) = self.numlock_on_keys.get(&vkey) {
                return key.clone();
            }
        } else if let Some(key) = self.numlock_off_keys.get(&vkey) {
            return key.clone();
        }
        if let PhysicalKey::Code(code) = physical_key {
            if let Some(keys) = self.keys.get(&mods) {
                if let Some(key) = keys.get(code) {
                    return key.clone();
                }
            }
        }
        Key::Unidentified(native_code)
    }
}

#[derive(Default)]
pub(crate) struct LayoutCache {
    /// Maps locale identifiers (HKL) to layouts
    pub layouts: HashMap<u64, Layout>,
}

impl LayoutCache {
    /// Checks whether the current layout is already known and
    /// prepares the layout if it isn't known.
    /// The current layout is then returned.
    pub fn get_current_layout(&mut self) -> (u64, &Layout) {
        let locale_id = unsafe { GetKeyboardLayout(0) } as u64;
        match self.layouts.entry(locale_id) {
            Entry::Occupied(entry) => (locale_id, entry.into_mut()),
            Entry::Vacant(entry) => {
                let layout = Self::prepare_layout(locale_id);
                (locale_id, entry.insert(layout))
            }
        }
    }

    pub fn get_agnostic_mods(&mut self) -> ModifiersState {
        let (_, layout) = self.get_current_layout();
        let filter_out_altgr = layout.has_alt_graph && key_pressed(VK_RMENU);
        let mut mods = ModifiersState::empty();
        mods.set(ModifiersState::SHIFT, key_pressed(VK_SHIFT));
        mods.set(
            ModifiersState::CONTROL,
            key_pressed(VK_CONTROL) && !filter_out_altgr,
        );
        mods.set(
            ModifiersState::ALT,
            key_pressed(VK_MENU) && !filter_out_altgr,
        );
        mods.set(
            ModifiersState::SUPER,
            key_pressed(VK_LWIN) || key_pressed(VK_RWIN),
        );
        mods
    }

    fn prepare_layout(locale_id: u64) -> Layout {
        let mut layout = Layout {
            hkl: locale_id,
            numlock_on_keys: Default::default(),
            numlock_off_keys: Default::default(),
            keys: Default::default(),
            has_alt_graph: false,
        };

        // We initialize the keyboard state with all zeros to
        // simulate a scenario when no modifier is active.
        let mut key_state = [0u8; 256];

        // `MapVirtualKeyExW` maps (non-numpad-specific) virtual keys to scancodes as if numlock
        // was off. We rely on this behavior to find all virtual keys which are not numpad-specific
        // but map to the numpad.
        //
        // src_vkey: VK  ==>  scancode: u16 (on the numpad)
        //
        // Then we convert the source virtual key into a `Key` and the scancode into a virtual key
        // to get the reverse mapping.
        //
        // src_vkey: VK  ==>  scancode: u16 (on the numpad)
        //    ||                    ||
        //    \/                    \/
        // map_value: Key  <-  map_vkey: VK
        layout.numlock_off_keys.reserve(NUMPAD_KEYCODES.len());
        for vk in 0..256 {
            let scancode =
                unsafe { MapVirtualKeyExW(vk, MAPVK_VK_TO_VSC_EX, locale_id as HKL) };
            if scancode == 0 {
                continue;
            }
            let keycode = match scancode_to_physicalkey(scancode) {
                PhysicalKey::Code(code) => code,
                // TODO: validate that we can skip on unidentified keys (probably never occurs?)
                _ => continue,
            };
            if !is_numpad_specific(vk as VIRTUAL_KEY)
                && NUMPAD_KEYCODES.contains(&keycode)
            {
                let native_code = NativeKey::Windows(vk as VIRTUAL_KEY);
                let map_vkey = keycode_to_vkey(keycode, locale_id);
                if map_vkey == 0 {
                    continue;
                }
                let map_value = vkey_to_non_char_key(
                    vk as VIRTUAL_KEY,
                    native_code,
                    locale_id,
                    false,
                );
                if matches!(map_value, Key::Unidentified(_)) {
                    continue;
                }
                layout.numlock_off_keys.insert(map_vkey, map_value);
            }
        }

        layout.numlock_on_keys.reserve(NUMPAD_VKEYS.len());
        for vk in NUMPAD_VKEYS.iter() {
            let vk = (*vk) as u32;
            let scancode =
                unsafe { MapVirtualKeyExW(vk, MAPVK_VK_TO_VSC_EX, locale_id as HKL) };
            let unicode = Self::to_unicode_string(&key_state, vk, scancode, locale_id);
            if let ToUnicodeResult::Str(s) = unicode {
                layout
                    .numlock_on_keys
                    .insert(vk as VIRTUAL_KEY, Key::Character(SmolStr::new(s)));
            }
        }

        // Iterate through every combination of modifiers
        let mods_end = WindowsModifiers::FLAGS_END.bits();
        for mod_state in 0..mods_end {
            let mut keys_for_this_mod = HashMap::with_capacity(256);

            let mod_state = WindowsModifiers::from_bits_retain(mod_state);
            mod_state.apply_to_kbd_state(&mut key_state);

            // Virtual key values are in the domain [0, 255].
            // This is reinforced by the fact that the keyboard state array has 256
            // elements. This array is allowed to be indexed by virtual key values
            // giving the key state for the virtual key used for indexing.
            for vk in 0..256 {
                let scancode =
                    unsafe { MapVirtualKeyExW(vk, MAPVK_VK_TO_VSC_EX, locale_id as HKL) };
                if scancode == 0 {
                    continue;
                }

                let native_code = NativeKey::Windows(vk as VIRTUAL_KEY);
                let key_code = match scancode_to_physicalkey(scancode) {
                    PhysicalKey::Code(code) => code,
                    // TODO: validate that we can skip on unidentified keys (probably never occurs?)
                    _ => continue,
                };
                // Let's try to get the key from just the scancode and vk
                // We don't necessarily know yet if AltGraph is present on this layout so we'll
                // assume it isn't. Then we'll do a second pass where we set the "AltRight" keys to
                // "AltGr" in case we find out that there's an AltGraph.
                let preliminary_key = vkey_to_non_char_key(
                    vk as VIRTUAL_KEY,
                    native_code,
                    locale_id,
                    false,
                );
                match preliminary_key {
                    Key::Unidentified(_) => (),
                    _ => {
                        keys_for_this_mod.insert(key_code, preliminary_key);
                        continue;
                    }
                }

                let unicode =
                    Self::to_unicode_string(&key_state, vk, scancode, locale_id);
                let key = match unicode {
                    ToUnicodeResult::Str(str) => Key::Character(SmolStr::new(str)),
                    ToUnicodeResult::Dead(dead_char) => {
                        // println!("{:?} - {:?} produced dead {:?}", key_code, mod_state,
                        // dead_char);
                        Key::Dead(dead_char)
                    }
                    ToUnicodeResult::None => {
                        let has_alt = mod_state.contains(WindowsModifiers::ALT);
                        let has_ctrl = mod_state.contains(WindowsModifiers::CONTROL);
                        // HACK: `ToUnicodeEx` seems to fail getting the string for the numpad
                        // divide key, so we handle that explicitly here
                        if !has_alt && !has_ctrl && key_code == KeyCode::NumpadDivide {
                            Key::Character(SmolStr::new("/"))
                        } else {
                            // Just use the unidentified key, we got earlier
                            preliminary_key
                        }
                    }
                };

                // Check for alt graph.
                // The logic is that if a key pressed with no modifier produces
                // a different `Character` from when it's pressed with CTRL+ALT then the layout
                // has AltGr.
                let ctrl_alt: WindowsModifiers =
                    WindowsModifiers::CONTROL | WindowsModifiers::ALT;
                let is_in_ctrl_alt = mod_state == ctrl_alt;
                if !layout.has_alt_graph && is_in_ctrl_alt {
                    // Unwrapping here because if we are in the ctrl+alt modifier state
                    // then the alt modifier state must have come before.
                    let simple_keys =
                        layout.keys.get(&WindowsModifiers::empty()).unwrap();
                    if let Some(Key::Character(key_no_altgr)) = simple_keys.get(&key_code)
                    {
                        if let Key::Character(key) = &key {
                            layout.has_alt_graph = key != key_no_altgr;
                        }
                    }
                }

                keys_for_this_mod.insert(key_code, key);
            }
            layout.keys.insert(mod_state, keys_for_this_mod);
        }

        // Second pass: replace right alt keys with AltGr if the layout has alt graph
        if layout.has_alt_graph {
            for mod_state in 0..mods_end {
                let mod_state = WindowsModifiers::from_bits_retain(mod_state);
                if let Some(keys) = layout.keys.get_mut(&mod_state) {
                    if let Some(key) = keys.get_mut(&KeyCode::AltRight) {
                        *key = Key::Named(NamedKey::AltGraph);
                    }
                }
            }
        }

        layout
    }

    fn to_unicode_string(
        key_state: &[u8; 256],
        vkey: u32,
        scancode: u32,
        locale_id: u64,
    ) -> ToUnicodeResult {
        unsafe {
            let mut label_wide = [0u16; 8];
            let mut wide_len = ToUnicodeEx(
                vkey,
                scancode,
                (&key_state[0]) as *const _,
                (&mut label_wide[0]) as *mut _,
                label_wide.len() as i32,
                0,
                locale_id as HKL,
            );
            if wide_len < 0 {
                // If it's dead, we run `ToUnicode` again to consume the dead-key
                wide_len = ToUnicodeEx(
                    vkey,
                    scancode,
                    (&key_state[0]) as *const _,
                    (&mut label_wide[0]) as *mut _,
                    label_wide.len() as i32,
                    0,
                    locale_id as HKL,
                );
                if wide_len > 0 {
                    let os_string =
                        OsString::from_wide(&label_wide[0..wide_len as usize]);
                    if let Ok(label_str) = os_string.into_string() {
                        if let Some(ch) = label_str.chars().next() {
                            return ToUnicodeResult::Dead(Some(ch));
                        }
                    }
                }
                return ToUnicodeResult::Dead(None);
            }
            if wide_len > 0 {
                let os_string = OsString::from_wide(&label_wide[0..wide_len as usize]);
                if let Ok(label_str) = os_string.into_string() {
                    return ToUnicodeResult::Str(label_str);
                }
            }
        }
        ToUnicodeResult::None
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum ToUnicodeResult {
    Str(String),
    Dead(Option<char>),
    None,
}

fn is_numpad_specific(vk: VIRTUAL_KEY) -> bool {
    matches!(
        vk,
        VK_NUMPAD0
            | VK_NUMPAD1
            | VK_NUMPAD2
            | VK_NUMPAD3
            | VK_NUMPAD4
            | VK_NUMPAD5
            | VK_NUMPAD6
            | VK_NUMPAD7
            | VK_NUMPAD8
            | VK_NUMPAD9
            | VK_ADD
            | VK_SUBTRACT
            | VK_DIVIDE
            | VK_DECIMAL
            | VK_SEPARATOR
    )
}

fn keycode_to_vkey(keycode: KeyCode, hkl: u64) -> VIRTUAL_KEY {
    let primary_lang_id = primarylangid(loword(hkl as u32));
    let is_korean = primary_lang_id as u32 == LANG_KOREAN;
    let is_japanese = primary_lang_id as u32 == LANG_JAPANESE;

    match keycode {
        KeyCode::Backquote => 0,
        KeyCode::Backslash => 0,
        KeyCode::BracketLeft => 0,
        KeyCode::BracketRight => 0,
        KeyCode::Comma => 0,
        KeyCode::Digit0 => 0,
        KeyCode::Digit1 => 0,
        KeyCode::Digit2 => 0,
        KeyCode::Digit3 => 0,
        KeyCode::Digit4 => 0,
        KeyCode::Digit5 => 0,
        KeyCode::Digit6 => 0,
        KeyCode::Digit7 => 0,
        KeyCode::Digit8 => 0,
        KeyCode::Digit9 => 0,
        KeyCode::Equal => 0,
        KeyCode::IntlBackslash => 0,
        KeyCode::IntlRo => 0,
        KeyCode::IntlYen => 0,
        KeyCode::KeyA => 0,
        KeyCode::KeyB => 0,
        KeyCode::KeyC => 0,
        KeyCode::KeyD => 0,
        KeyCode::KeyE => 0,
        KeyCode::KeyF => 0,
        KeyCode::KeyG => 0,
        KeyCode::KeyH => 0,
        KeyCode::KeyI => 0,
        KeyCode::KeyJ => 0,
        KeyCode::KeyK => 0,
        KeyCode::KeyL => 0,
        KeyCode::KeyM => 0,
        KeyCode::KeyN => 0,
        KeyCode::KeyO => 0,
        KeyCode::KeyP => 0,
        KeyCode::KeyQ => 0,
        KeyCode::KeyR => 0,
        KeyCode::KeyS => 0,
        KeyCode::KeyT => 0,
        KeyCode::KeyU => 0,
        KeyCode::KeyV => 0,
        KeyCode::KeyW => 0,
        KeyCode::KeyX => 0,
        KeyCode::KeyY => 0,
        KeyCode::KeyZ => 0,
        KeyCode::Minus => 0,
        KeyCode::Period => 0,
        KeyCode::Quote => 0,
        KeyCode::Semicolon => 0,
        KeyCode::Slash => 0,
        KeyCode::AltLeft => VK_LMENU,
        KeyCode::AltRight => VK_RMENU,
        KeyCode::Backspace => VK_BACK,
        KeyCode::CapsLock => VK_CAPITAL,
        KeyCode::ContextMenu => VK_APPS,
        KeyCode::ControlLeft => VK_LCONTROL,
        KeyCode::ControlRight => VK_RCONTROL,
        KeyCode::Enter => VK_RETURN,
        KeyCode::SuperLeft => VK_LWIN,
        KeyCode::SuperRight => VK_RWIN,
        KeyCode::ShiftLeft => VK_RSHIFT,
        KeyCode::ShiftRight => VK_LSHIFT,
        KeyCode::Space => VK_SPACE,
        KeyCode::Tab => VK_TAB,
        KeyCode::Convert => VK_CONVERT,
        KeyCode::KanaMode => VK_KANA,
        KeyCode::Lang1 if is_korean => VK_HANGUL,
        KeyCode::Lang1 if is_japanese => VK_KANA,
        KeyCode::Lang2 if is_korean => VK_HANJA,
        KeyCode::Lang2 if is_japanese => 0,
        KeyCode::Lang3 if is_japanese => VK_OEM_FINISH,
        KeyCode::Lang4 if is_japanese => 0,
        KeyCode::Lang5 if is_japanese => 0,
        KeyCode::NonConvert => VK_NONCONVERT,
        KeyCode::Delete => VK_DELETE,
        KeyCode::End => VK_END,
        KeyCode::Help => VK_HELP,
        KeyCode::Home => VK_HOME,
        KeyCode::Insert => VK_INSERT,
        KeyCode::PageDown => VK_NEXT,
        KeyCode::PageUp => VK_PRIOR,
        KeyCode::ArrowDown => VK_DOWN,
        KeyCode::ArrowLeft => VK_LEFT,
        KeyCode::ArrowRight => VK_RIGHT,
        KeyCode::ArrowUp => VK_UP,
        KeyCode::NumLock => VK_NUMLOCK,
        KeyCode::Numpad0 => VK_NUMPAD0,
        KeyCode::Numpad1 => VK_NUMPAD1,
        KeyCode::Numpad2 => VK_NUMPAD2,
        KeyCode::Numpad3 => VK_NUMPAD3,
        KeyCode::Numpad4 => VK_NUMPAD4,
        KeyCode::Numpad5 => VK_NUMPAD5,
        KeyCode::Numpad6 => VK_NUMPAD6,
        KeyCode::Numpad7 => VK_NUMPAD7,
        KeyCode::Numpad8 => VK_NUMPAD8,
        KeyCode::Numpad9 => VK_NUMPAD9,
        KeyCode::NumpadAdd => VK_ADD,
        KeyCode::NumpadBackspace => VK_BACK,
        KeyCode::NumpadClear => VK_CLEAR,
        KeyCode::NumpadClearEntry => 0,
        KeyCode::NumpadComma => VK_SEPARATOR,
        KeyCode::NumpadDecimal => VK_DECIMAL,
        KeyCode::NumpadDivide => VK_DIVIDE,
        KeyCode::NumpadEnter => VK_RETURN,
        KeyCode::NumpadEqual => 0,
        KeyCode::NumpadHash => 0,
        KeyCode::NumpadMemoryAdd => 0,
        KeyCode::NumpadMemoryClear => 0,
        KeyCode::NumpadMemoryRecall => 0,
        KeyCode::NumpadMemoryStore => 0,
        KeyCode::NumpadMemorySubtract => 0,
        KeyCode::NumpadMultiply => VK_MULTIPLY,
        KeyCode::NumpadParenLeft => 0,
        KeyCode::NumpadParenRight => 0,
        KeyCode::NumpadStar => 0,
        KeyCode::NumpadSubtract => VK_SUBTRACT,
        KeyCode::Escape => VK_ESCAPE,
        KeyCode::Fn => 0,
        KeyCode::FnLock => 0,
        KeyCode::PrintScreen => VK_SNAPSHOT,
        KeyCode::ScrollLock => VK_SCROLL,
        KeyCode::Pause => VK_PAUSE,
        KeyCode::BrowserBack => VK_BROWSER_BACK,
        KeyCode::BrowserFavorites => VK_BROWSER_FAVORITES,
        KeyCode::BrowserForward => VK_BROWSER_FORWARD,
        KeyCode::BrowserHome => VK_BROWSER_HOME,
        KeyCode::BrowserRefresh => VK_BROWSER_REFRESH,
        KeyCode::BrowserSearch => VK_BROWSER_SEARCH,
        KeyCode::BrowserStop => VK_BROWSER_STOP,
        KeyCode::Eject => 0,
        KeyCode::LaunchApp1 => VK_LAUNCH_APP1,
        KeyCode::LaunchApp2 => VK_LAUNCH_APP2,
        KeyCode::LaunchMail => VK_LAUNCH_MAIL,
        KeyCode::MediaPlayPause => VK_MEDIA_PLAY_PAUSE,
        KeyCode::MediaSelect => VK_LAUNCH_MEDIA_SELECT,
        KeyCode::MediaStop => VK_MEDIA_STOP,
        KeyCode::MediaTrackNext => VK_MEDIA_NEXT_TRACK,
        KeyCode::MediaTrackPrevious => VK_MEDIA_PREV_TRACK,
        KeyCode::Power => 0,
        KeyCode::Sleep => 0,
        KeyCode::AudioVolumeDown => VK_VOLUME_DOWN,
        KeyCode::AudioVolumeMute => VK_VOLUME_MUTE,
        KeyCode::AudioVolumeUp => VK_VOLUME_UP,
        KeyCode::WakeUp => 0,
        KeyCode::Hyper => 0,
        KeyCode::Turbo => 0,
        KeyCode::Abort => 0,
        KeyCode::Resume => 0,
        KeyCode::Suspend => 0,
        KeyCode::Again => 0,
        KeyCode::Copy => 0,
        KeyCode::Cut => 0,
        KeyCode::Find => 0,
        KeyCode::Open => 0,
        KeyCode::Paste => 0,
        KeyCode::Props => 0,
        KeyCode::Select => VK_SELECT,
        KeyCode::Undo => 0,
        KeyCode::Hiragana => 0,
        KeyCode::Katakana => 0,
        KeyCode::F1 => VK_F1,
        KeyCode::F2 => VK_F2,
        KeyCode::F3 => VK_F3,
        KeyCode::F4 => VK_F4,
        KeyCode::F5 => VK_F5,
        KeyCode::F6 => VK_F6,
        KeyCode::F7 => VK_F7,
        KeyCode::F8 => VK_F8,
        KeyCode::F9 => VK_F9,
        KeyCode::F10 => VK_F10,
        KeyCode::F11 => VK_F11,
        KeyCode::F12 => VK_F12,
        KeyCode::F13 => VK_F13,
        KeyCode::F14 => VK_F14,
        KeyCode::F15 => VK_F15,
        KeyCode::F16 => VK_F16,
        KeyCode::F17 => VK_F17,
        KeyCode::F18 => VK_F18,
        KeyCode::F19 => VK_F19,
        KeyCode::F20 => VK_F20,
        KeyCode::F21 => VK_F21,
        KeyCode::F22 => VK_F22,
        KeyCode::F23 => VK_F23,
        KeyCode::F24 => VK_F24,
        KeyCode::F25 => 0,
        KeyCode::F26 => 0,
        KeyCode::F27 => 0,
        KeyCode::F28 => 0,
        KeyCode::F29 => 0,
        KeyCode::F30 => 0,
        KeyCode::F31 => 0,
        KeyCode::F32 => 0,
        KeyCode::F33 => 0,
        KeyCode::F34 => 0,
        KeyCode::F35 => 0,
        _ => 0,
    }
}

/// This converts virtual keys to `Key`s. Only virtual keys which can be unambiguously converted to
/// a `Key`, with only the information passed in as arguments, are converted.
///
/// In other words: this function does not need to "prepare" the current layout in order to do
/// the conversion, but as such it cannot convert certain keys, like language-specific character
/// keys.
///
/// The result includes all non-character keys defined within `Key` plus characters from numpad
/// keys. For example, backspace and tab are included.
fn vkey_to_non_char_key(
    vkey: VIRTUAL_KEY,
    native_code: NativeKey,
    hkl: u64,
    has_alt_graph: bool,
) -> Key {
    // List of the Web key names and their corresponding platform-native key names:
    // https://developer.mozilla.org/en-US/docs/Web/API/KeyboardEvent/key/Key_Values

    let primary_lang_id = primarylangid(loword(hkl as u32));
    let is_korean = primary_lang_id as u32 == LANG_KOREAN;
    let is_japanese = primary_lang_id as u32 == LANG_JAPANESE;

    match vkey {
        VK_LBUTTON => Key::Unidentified(NativeKey::Unidentified), // Mouse
        VK_RBUTTON => Key::Unidentified(NativeKey::Unidentified), // Mouse

        // I don't think this can be represented with a Key
        VK_CANCEL => Key::Unidentified(native_code),

        VK_MBUTTON => Key::Unidentified(NativeKey::Unidentified), // Mouse
        VK_XBUTTON1 => Key::Unidentified(NativeKey::Unidentified), // Mouse
        VK_XBUTTON2 => Key::Unidentified(NativeKey::Unidentified), // Mouse
        VK_BACK => Key::Named(NamedKey::Backspace),
        VK_TAB => Key::Named(NamedKey::Tab),
        VK_CLEAR => Key::Named(NamedKey::Clear),
        VK_RETURN => Key::Named(NamedKey::Enter),
        VK_SHIFT => Key::Named(NamedKey::Shift),
        VK_CONTROL => Key::Named(NamedKey::Control),
        VK_MENU => Key::Named(NamedKey::Alt),
        VK_PAUSE => Key::Named(NamedKey::Pause),
        VK_CAPITAL => Key::Named(NamedKey::CapsLock),

        // VK_HANGEUL => Key::Named(NamedKey::HangulMode), // Deprecated in favour of VK_HANGUL

        // VK_HANGUL and VK_KANA are defined as the same constant, therefore
        // we use appropriate conditions to differentiate between them
        VK_HANGUL if is_korean => Key::Named(NamedKey::HangulMode),
        VK_KANA if is_japanese => Key::Named(NamedKey::KanaMode),

        VK_JUNJA => Key::Named(NamedKey::JunjaMode),
        VK_FINAL => Key::Named(NamedKey::FinalMode),

        // VK_HANJA and VK_KANJI are defined as the same constant, therefore
        // we use appropriate conditions to differentiate between them
        VK_HANJA if is_korean => Key::Named(NamedKey::HanjaMode),
        VK_KANJI if is_japanese => Key::Named(NamedKey::KanjiMode),

        VK_ESCAPE => Key::Named(NamedKey::Escape),
        VK_CONVERT => Key::Named(NamedKey::Convert),
        VK_NONCONVERT => Key::Named(NamedKey::NonConvert),
        VK_ACCEPT => Key::Named(NamedKey::Accept),
        VK_MODECHANGE => Key::Named(NamedKey::ModeChange),
        VK_SPACE => Key::Named(NamedKey::Space),
        VK_PRIOR => Key::Named(NamedKey::PageUp),
        VK_NEXT => Key::Named(NamedKey::PageDown),
        VK_END => Key::Named(NamedKey::End),
        VK_HOME => Key::Named(NamedKey::Home),
        VK_LEFT => Key::Named(NamedKey::ArrowLeft),
        VK_UP => Key::Named(NamedKey::ArrowUp),
        VK_RIGHT => Key::Named(NamedKey::ArrowRight),
        VK_DOWN => Key::Named(NamedKey::ArrowDown),
        VK_SELECT => Key::Named(NamedKey::Select),
        VK_PRINT => Key::Named(NamedKey::Print),
        VK_EXECUTE => Key::Named(NamedKey::Execute),
        VK_SNAPSHOT => Key::Named(NamedKey::PrintScreen),
        VK_INSERT => Key::Named(NamedKey::Insert),
        VK_DELETE => Key::Named(NamedKey::Delete),
        VK_HELP => Key::Named(NamedKey::Help),
        VK_LWIN => Key::Named(NamedKey::Super),
        VK_RWIN => Key::Named(NamedKey::Super),
        VK_APPS => Key::Named(NamedKey::ContextMenu),
        VK_SLEEP => Key::Named(NamedKey::Standby),

        // Numpad keys produce characters
        VK_NUMPAD0 => Key::Unidentified(native_code),
        VK_NUMPAD1 => Key::Unidentified(native_code),
        VK_NUMPAD2 => Key::Unidentified(native_code),
        VK_NUMPAD3 => Key::Unidentified(native_code),
        VK_NUMPAD4 => Key::Unidentified(native_code),
        VK_NUMPAD5 => Key::Unidentified(native_code),
        VK_NUMPAD6 => Key::Unidentified(native_code),
        VK_NUMPAD7 => Key::Unidentified(native_code),
        VK_NUMPAD8 => Key::Unidentified(native_code),
        VK_NUMPAD9 => Key::Unidentified(native_code),
        VK_MULTIPLY => Key::Unidentified(native_code),
        VK_ADD => Key::Unidentified(native_code),
        VK_SEPARATOR => Key::Unidentified(native_code),
        VK_SUBTRACT => Key::Unidentified(native_code),
        VK_DECIMAL => Key::Unidentified(native_code),
        VK_DIVIDE => Key::Unidentified(native_code),

        VK_F1 => Key::Named(NamedKey::F1),
        VK_F2 => Key::Named(NamedKey::F2),
        VK_F3 => Key::Named(NamedKey::F3),
        VK_F4 => Key::Named(NamedKey::F4),
        VK_F5 => Key::Named(NamedKey::F5),
        VK_F6 => Key::Named(NamedKey::F6),
        VK_F7 => Key::Named(NamedKey::F7),
        VK_F8 => Key::Named(NamedKey::F8),
        VK_F9 => Key::Named(NamedKey::F9),
        VK_F10 => Key::Named(NamedKey::F10),
        VK_F11 => Key::Named(NamedKey::F11),
        VK_F12 => Key::Named(NamedKey::F12),
        VK_F13 => Key::Named(NamedKey::F13),
        VK_F14 => Key::Named(NamedKey::F14),
        VK_F15 => Key::Named(NamedKey::F15),
        VK_F16 => Key::Named(NamedKey::F16),
        VK_F17 => Key::Named(NamedKey::F17),
        VK_F18 => Key::Named(NamedKey::F18),
        VK_F19 => Key::Named(NamedKey::F19),
        VK_F20 => Key::Named(NamedKey::F20),
        VK_F21 => Key::Named(NamedKey::F21),
        VK_F22 => Key::Named(NamedKey::F22),
        VK_F23 => Key::Named(NamedKey::F23),
        VK_F24 => Key::Named(NamedKey::F24),
        VK_NAVIGATION_VIEW => Key::Unidentified(native_code),
        VK_NAVIGATION_MENU => Key::Unidentified(native_code),
        VK_NAVIGATION_UP => Key::Unidentified(native_code),
        VK_NAVIGATION_DOWN => Key::Unidentified(native_code),
        VK_NAVIGATION_LEFT => Key::Unidentified(native_code),
        VK_NAVIGATION_RIGHT => Key::Unidentified(native_code),
        VK_NAVIGATION_ACCEPT => Key::Unidentified(native_code),
        VK_NAVIGATION_CANCEL => Key::Unidentified(native_code),
        VK_NUMLOCK => Key::Named(NamedKey::NumLock),
        VK_SCROLL => Key::Named(NamedKey::ScrollLock),
        VK_OEM_NEC_EQUAL => Key::Unidentified(native_code),
        // VK_OEM_FJ_JISHO => Key::Unidentified(native_code), // Conflicts with `VK_OEM_NEC_EQUAL`
        VK_OEM_FJ_MASSHOU => Key::Unidentified(native_code),
        VK_OEM_FJ_TOUROKU => Key::Unidentified(native_code),
        VK_OEM_FJ_LOYA => Key::Unidentified(native_code),
        VK_OEM_FJ_ROYA => Key::Unidentified(native_code),
        VK_LSHIFT => Key::Named(NamedKey::Shift),
        VK_RSHIFT => Key::Named(NamedKey::Shift),
        VK_LCONTROL => Key::Named(NamedKey::Control),
        VK_RCONTROL => Key::Named(NamedKey::Control),
        VK_LMENU => Key::Named(NamedKey::Alt),
        VK_RMENU => {
            if has_alt_graph {
                Key::Named(NamedKey::AltGraph)
            } else {
                Key::Named(NamedKey::Alt)
            }
        }
        VK_BROWSER_BACK => Key::Named(NamedKey::BrowserBack),
        VK_BROWSER_FORWARD => Key::Named(NamedKey::BrowserForward),
        VK_BROWSER_REFRESH => Key::Named(NamedKey::BrowserRefresh),
        VK_BROWSER_STOP => Key::Named(NamedKey::BrowserStop),
        VK_BROWSER_SEARCH => Key::Named(NamedKey::BrowserSearch),
        VK_BROWSER_FAVORITES => Key::Named(NamedKey::BrowserFavorites),
        VK_BROWSER_HOME => Key::Named(NamedKey::BrowserHome),
        VK_VOLUME_MUTE => Key::Named(NamedKey::AudioVolumeMute),
        VK_VOLUME_DOWN => Key::Named(NamedKey::AudioVolumeDown),
        VK_VOLUME_UP => Key::Named(NamedKey::AudioVolumeUp),
        VK_MEDIA_NEXT_TRACK => Key::Named(NamedKey::MediaTrackNext),
        VK_MEDIA_PREV_TRACK => Key::Named(NamedKey::MediaTrackPrevious),
        VK_MEDIA_STOP => Key::Named(NamedKey::MediaStop),
        VK_MEDIA_PLAY_PAUSE => Key::Named(NamedKey::MediaPlayPause),
        VK_LAUNCH_MAIL => Key::Named(NamedKey::LaunchMail),
        VK_LAUNCH_MEDIA_SELECT => Key::Named(NamedKey::LaunchMediaPlayer),
        VK_LAUNCH_APP1 => Key::Named(NamedKey::LaunchApplication1),
        VK_LAUNCH_APP2 => Key::Named(NamedKey::LaunchApplication2),

        // This function only converts "non-printable"
        VK_OEM_1 => Key::Unidentified(native_code),
        VK_OEM_PLUS => Key::Unidentified(native_code),
        VK_OEM_COMMA => Key::Unidentified(native_code),
        VK_OEM_MINUS => Key::Unidentified(native_code),
        VK_OEM_PERIOD => Key::Unidentified(native_code),
        VK_OEM_2 => Key::Unidentified(native_code),
        VK_OEM_3 => Key::Unidentified(native_code),

        VK_GAMEPAD_A => Key::Unidentified(native_code),
        VK_GAMEPAD_B => Key::Unidentified(native_code),
        VK_GAMEPAD_X => Key::Unidentified(native_code),
        VK_GAMEPAD_Y => Key::Unidentified(native_code),
        VK_GAMEPAD_RIGHT_SHOULDER => Key::Unidentified(native_code),
        VK_GAMEPAD_LEFT_SHOULDER => Key::Unidentified(native_code),
        VK_GAMEPAD_LEFT_TRIGGER => Key::Unidentified(native_code),
        VK_GAMEPAD_RIGHT_TRIGGER => Key::Unidentified(native_code),
        VK_GAMEPAD_DPAD_UP => Key::Unidentified(native_code),
        VK_GAMEPAD_DPAD_DOWN => Key::Unidentified(native_code),
        VK_GAMEPAD_DPAD_LEFT => Key::Unidentified(native_code),
        VK_GAMEPAD_DPAD_RIGHT => Key::Unidentified(native_code),
        VK_GAMEPAD_MENU => Key::Unidentified(native_code),
        VK_GAMEPAD_VIEW => Key::Unidentified(native_code),
        VK_GAMEPAD_LEFT_THUMBSTICK_BUTTON => Key::Unidentified(native_code),
        VK_GAMEPAD_RIGHT_THUMBSTICK_BUTTON => Key::Unidentified(native_code),
        VK_GAMEPAD_LEFT_THUMBSTICK_UP => Key::Unidentified(native_code),
        VK_GAMEPAD_LEFT_THUMBSTICK_DOWN => Key::Unidentified(native_code),
        VK_GAMEPAD_LEFT_THUMBSTICK_RIGHT => Key::Unidentified(native_code),
        VK_GAMEPAD_LEFT_THUMBSTICK_LEFT => Key::Unidentified(native_code),
        VK_GAMEPAD_RIGHT_THUMBSTICK_UP => Key::Unidentified(native_code),
        VK_GAMEPAD_RIGHT_THUMBSTICK_DOWN => Key::Unidentified(native_code),
        VK_GAMEPAD_RIGHT_THUMBSTICK_RIGHT => Key::Unidentified(native_code),
        VK_GAMEPAD_RIGHT_THUMBSTICK_LEFT => Key::Unidentified(native_code),

        // This function only converts "non-printable"
        VK_OEM_4 => Key::Unidentified(native_code),
        VK_OEM_5 => Key::Unidentified(native_code),
        VK_OEM_6 => Key::Unidentified(native_code),
        VK_OEM_7 => Key::Unidentified(native_code),
        VK_OEM_8 => Key::Unidentified(native_code),
        VK_OEM_AX => Key::Unidentified(native_code),
        VK_OEM_102 => Key::Unidentified(native_code),

        VK_ICO_HELP => Key::Unidentified(native_code),
        VK_ICO_00 => Key::Unidentified(native_code),

        VK_PROCESSKEY => Key::Named(NamedKey::Process),

        VK_ICO_CLEAR => Key::Unidentified(native_code),
        VK_PACKET => Key::Unidentified(native_code),
        VK_OEM_RESET => Key::Unidentified(native_code),
        VK_OEM_JUMP => Key::Unidentified(native_code),
        VK_OEM_PA1 => Key::Unidentified(native_code),
        VK_OEM_PA2 => Key::Unidentified(native_code),
        VK_OEM_PA3 => Key::Unidentified(native_code),
        VK_OEM_WSCTRL => Key::Unidentified(native_code),
        VK_OEM_CUSEL => Key::Unidentified(native_code),

        VK_OEM_ATTN => Key::Named(NamedKey::Attn),
        VK_OEM_FINISH => {
            if is_japanese {
                Key::Named(NamedKey::Katakana)
            } else {
                // This matches IE and Firefox behaviour according to
                // https://developer.mozilla.org/en-US/docs/Web/API/KeyboardEvent/key/Key_Values
                // At the time of writing, there is no `NamedKey::Finish` variant as
                // Finish is not mentioned at https://w3c.github.io/uievents-key/
                // Also see: https://github.com/pyfisch/keyboard-types/issues/9
                Key::Unidentified(native_code)
            }
        }
        VK_OEM_COPY => Key::Named(NamedKey::Copy),
        VK_OEM_AUTO => Key::Named(NamedKey::Hankaku),
        VK_OEM_ENLW => Key::Named(NamedKey::Zenkaku),
        VK_OEM_BACKTAB => Key::Named(NamedKey::Romaji),
        VK_ATTN => Key::Named(NamedKey::KanaMode),
        VK_CRSEL => Key::Named(NamedKey::CrSel),
        VK_EXSEL => Key::Named(NamedKey::ExSel),
        VK_EREOF => Key::Named(NamedKey::EraseEof),
        VK_PLAY => Key::Named(NamedKey::Play),
        VK_ZOOM => Key::Named(NamedKey::ZoomToggle),
        VK_NONAME => Key::Unidentified(native_code),
        VK_PA1 => Key::Unidentified(native_code),
        VK_OEM_CLEAR => Key::Named(NamedKey::Clear),
        _ => Key::Unidentified(native_code),
    }
}
