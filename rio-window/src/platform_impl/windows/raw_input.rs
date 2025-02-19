use std::mem::{self, size_of};
use std::ptr;

use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    HID_USAGE_GENERIC_KEYBOARD, HID_USAGE_GENERIC_MOUSE, HID_USAGE_PAGE_GENERIC,
};
use windows_sys::Win32::Foundation::{HANDLE, HWND};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, MAPVK_VK_TO_VSC_EX, VK_NUMLOCK, VK_SHIFT,
};
use windows_sys::Win32::UI::Input::{
    GetRawInputData, GetRawInputDeviceInfoW, GetRawInputDeviceList,
    RegisterRawInputDevices, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTDEVICELIST,
    RAWINPUTHEADER, RAWKEYBOARD, RIDEV_DEVNOTIFY, RIDEV_INPUTSINK, RIDEV_REMOVE,
    RIDI_DEVICEINFO, RIDI_DEVICENAME, RID_DEVICE_INFO, RID_DEVICE_INFO_HID,
    RID_DEVICE_INFO_KEYBOARD, RID_DEVICE_INFO_MOUSE, RID_INPUT, RIM_TYPEHID,
    RIM_TYPEKEYBOARD, RIM_TYPEMOUSE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    RI_KEY_E0, RI_KEY_E1, RI_MOUSE_BUTTON_1_DOWN, RI_MOUSE_BUTTON_1_UP,
    RI_MOUSE_BUTTON_2_DOWN, RI_MOUSE_BUTTON_2_UP, RI_MOUSE_BUTTON_3_DOWN,
    RI_MOUSE_BUTTON_3_UP, RI_MOUSE_BUTTON_4_DOWN, RI_MOUSE_BUTTON_4_UP,
    RI_MOUSE_BUTTON_5_DOWN, RI_MOUSE_BUTTON_5_UP,
};

use super::scancode_to_physicalkey;
use crate::event::ElementState;
use crate::event_loop::DeviceEvents;
use crate::keyboard::{KeyCode, PhysicalKey};
use crate::platform_impl::platform::util;

#[allow(dead_code)]
pub fn get_raw_input_device_list() -> Option<Vec<RAWINPUTDEVICELIST>> {
    let list_size = size_of::<RAWINPUTDEVICELIST>() as u32;

    let mut num_devices = 0;
    let status =
        unsafe { GetRawInputDeviceList(ptr::null_mut(), &mut num_devices, list_size) };

    if status == u32::MAX {
        return None;
    }

    let mut buffer = Vec::with_capacity(num_devices as _);

    let num_stored = unsafe {
        GetRawInputDeviceList(buffer.as_mut_ptr(), &mut num_devices, list_size)
    };

    if num_stored == u32::MAX {
        return None;
    }

    debug_assert_eq!(num_devices, num_stored);

    unsafe { buffer.set_len(num_devices as _) };

    Some(buffer)
}

#[allow(dead_code)]
pub enum RawDeviceInfo {
    Mouse(RID_DEVICE_INFO_MOUSE),
    Keyboard(RID_DEVICE_INFO_KEYBOARD),
    Hid(RID_DEVICE_INFO_HID),
}

impl From<RID_DEVICE_INFO> for RawDeviceInfo {
    fn from(info: RID_DEVICE_INFO) -> Self {
        unsafe {
            match info.dwType {
                RIM_TYPEMOUSE => RawDeviceInfo::Mouse(info.Anonymous.mouse),
                RIM_TYPEKEYBOARD => RawDeviceInfo::Keyboard(info.Anonymous.keyboard),
                RIM_TYPEHID => RawDeviceInfo::Hid(info.Anonymous.hid),
                _ => unreachable!(),
            }
        }
    }
}

#[allow(dead_code)]
pub fn get_raw_input_device_info(handle: HANDLE) -> Option<RawDeviceInfo> {
    let mut info: RID_DEVICE_INFO = unsafe { mem::zeroed() };
    let info_size = size_of::<RID_DEVICE_INFO>() as u32;

    info.cbSize = info_size;

    let mut minimum_size = 0;
    let status = unsafe {
        GetRawInputDeviceInfoW(
            handle,
            RIDI_DEVICEINFO,
            &mut info as *mut _ as _,
            &mut minimum_size,
        )
    };

    if status == u32::MAX || status == 0 {
        return None;
    }

    debug_assert_eq!(info_size, status);

    Some(info.into())
}

pub fn get_raw_input_device_name(handle: HANDLE) -> Option<String> {
    let mut minimum_size = 0;
    let status = unsafe {
        GetRawInputDeviceInfoW(
            handle,
            RIDI_DEVICENAME,
            ptr::null_mut(),
            &mut minimum_size,
        )
    };

    if status != 0 {
        return None;
    }

    let mut name: Vec<u16> = Vec::with_capacity(minimum_size as _);

    let status = unsafe {
        GetRawInputDeviceInfoW(
            handle,
            RIDI_DEVICENAME,
            name.as_ptr() as _,
            &mut minimum_size,
        )
    };

    if status == u32::MAX || status == 0 {
        return None;
    }

    debug_assert_eq!(minimum_size, status);

    unsafe { name.set_len(minimum_size as _) };

    util::decode_wide(&name).into_string().ok()
}

pub fn register_raw_input_devices(devices: &[RAWINPUTDEVICE]) -> bool {
    let device_size = size_of::<RAWINPUTDEVICE>() as u32;

    unsafe {
        RegisterRawInputDevices(devices.as_ptr(), devices.len() as u32, device_size)
            == true.into()
    }
}

pub fn register_all_mice_and_keyboards_for_raw_input(
    mut window_handle: HWND,
    filter: DeviceEvents,
) -> bool {
    // RIDEV_DEVNOTIFY: receive hotplug events
    // RIDEV_INPUTSINK: receive events even if we're not in the foreground
    // RIDEV_REMOVE: don't receive device events (requires NULL hwndTarget)
    let flags = match filter {
        DeviceEvents::Never => {
            window_handle = ptr::null_mut();
            RIDEV_REMOVE
        }
        DeviceEvents::WhenFocused => RIDEV_DEVNOTIFY,
        DeviceEvents::Always => RIDEV_DEVNOTIFY | RIDEV_INPUTSINK,
    };

    let devices: [RAWINPUTDEVICE; 2] = [
        RAWINPUTDEVICE {
            usUsagePage: HID_USAGE_PAGE_GENERIC,
            usUsage: HID_USAGE_GENERIC_MOUSE,
            dwFlags: flags,
            hwndTarget: window_handle,
        },
        RAWINPUTDEVICE {
            usUsagePage: HID_USAGE_PAGE_GENERIC,
            usUsage: HID_USAGE_GENERIC_KEYBOARD,
            dwFlags: flags,
            hwndTarget: window_handle,
        },
    ];

    register_raw_input_devices(&devices)
}

pub fn get_raw_input_data(handle: HRAWINPUT) -> Option<RAWINPUT> {
    let mut data: RAWINPUT = unsafe { mem::zeroed() };
    let mut data_size = size_of::<RAWINPUT>() as u32;
    let header_size = size_of::<RAWINPUTHEADER>() as u32;

    let status = unsafe {
        GetRawInputData(
            handle,
            RID_INPUT,
            &mut data as *mut _ as _,
            &mut data_size,
            header_size,
        )
    };

    if status == u32::MAX || status == 0 {
        return None;
    }

    Some(data)
}

fn button_flags_to_element_state(
    button_flags: u32,
    down_flag: u32,
    up_flag: u32,
) -> Option<ElementState> {
    // We assume the same button won't be simultaneously pressed and released.
    if util::has_flag(button_flags, down_flag) {
        Some(ElementState::Pressed)
    } else if util::has_flag(button_flags, up_flag) {
        Some(ElementState::Released)
    } else {
        None
    }
}

pub fn get_raw_mouse_button_state(button_flags: u32) -> [Option<ElementState>; 5] {
    [
        button_flags_to_element_state(
            button_flags,
            RI_MOUSE_BUTTON_1_DOWN,
            RI_MOUSE_BUTTON_1_UP,
        ),
        button_flags_to_element_state(
            button_flags,
            RI_MOUSE_BUTTON_2_DOWN,
            RI_MOUSE_BUTTON_2_UP,
        ),
        button_flags_to_element_state(
            button_flags,
            RI_MOUSE_BUTTON_3_DOWN,
            RI_MOUSE_BUTTON_3_UP,
        ),
        button_flags_to_element_state(
            button_flags,
            RI_MOUSE_BUTTON_4_DOWN,
            RI_MOUSE_BUTTON_4_UP,
        ),
        button_flags_to_element_state(
            button_flags,
            RI_MOUSE_BUTTON_5_DOWN,
            RI_MOUSE_BUTTON_5_UP,
        ),
    ]
}

pub fn get_keyboard_physical_key(keyboard: RAWKEYBOARD) -> Option<PhysicalKey> {
    let extension = {
        if util::has_flag(keyboard.Flags, RI_KEY_E0 as _) {
            0xe000
        } else if util::has_flag(keyboard.Flags, RI_KEY_E1 as _) {
            0xe100
        } else {
            0x0000
        }
    };
    let scancode = if keyboard.MakeCode == 0 {
        // In some cases (often with media keys) the device reports a scancode of 0 but a
        // valid virtual key. In these cases we obtain the scancode from the virtual key.
        unsafe { MapVirtualKeyW(keyboard.VKey as u32, MAPVK_VK_TO_VSC_EX) as u16 }
    } else {
        keyboard.MakeCode | extension
    };
    if scancode == 0xe11d || scancode == 0xe02a {
        // At the hardware (or driver?) level, pressing the Pause key is equivalent to pressing
        // Ctrl+NumLock.
        // This equvalence means that if the user presses Pause, the keyboard will emit two
        // subsequent keypresses:
        // 1, 0xE11D - Which is a left Ctrl (0x1D) with an extension flag (0xE100)
        // 2, 0x0045 - Which on its own can be interpreted as Pause
        //
        // There's another combination which isn't quite an equivalence:
        // PrtSc used to be Shift+Asterisk. This means that on some keyboards, presssing
        // PrtSc (print screen) produces the following sequence:
        // 1, 0xE02A - Which is a left shift (0x2A) with an extension flag (0xE000)
        // 2, 0xE037 - Which is a numpad multiply (0x37) with an exteion flag (0xE000). This on
        //             its own it can be interpreted as PrtSc
        //
        // For this reason, if we encounter the first keypress, we simply ignore it, trusting
        // that there's going to be another event coming, from which we can extract the
        // appropriate key.
        // For more on this, read the article by Raymond Chen, titled:
        // "Why does Ctrl+ScrollLock cancel dialogs?"
        // https://devblogs.microsoft.com/oldnewthing/20080211-00/?p=23503
        return None;
    }
    let physical_key = if keyboard.VKey == VK_NUMLOCK {
        // Historically, the NumLock and the Pause key were one and the same physical key.
        // The user could trigger Pause by pressing Ctrl+NumLock.
        // Now these are often physically separate and the two keys can be differentiated by
        // checking the extension flag of the scancode. NumLock is 0xE045, Pause is 0x0045.
        //
        // However in this event, both keys are reported as 0x0045 even on modern hardware.
        // Therefore we use the virtual key instead to determine whether it's a NumLock and
        // set the KeyCode accordingly.
        //
        // For more on this, read the article by Raymond Chen, titled:
        // "Why does Ctrl+ScrollLock cancel dialogs?"
        // https://devblogs.microsoft.com/oldnewthing/20080211-00/?p=23503
        PhysicalKey::Code(KeyCode::NumLock)
    } else {
        scancode_to_physicalkey(scancode as u32)
    };
    if keyboard.VKey == VK_SHIFT {
        #[allow(clippy::collapsible_match)]
        if let PhysicalKey::Code(code) = physical_key {
            match code {
                KeyCode::NumpadDecimal
                | KeyCode::Numpad0
                | KeyCode::Numpad1
                | KeyCode::Numpad2
                | KeyCode::Numpad3
                | KeyCode::Numpad4
                | KeyCode::Numpad5
                | KeyCode::Numpad6
                | KeyCode::Numpad7
                | KeyCode::Numpad8
                | KeyCode::Numpad9 => {
                    // On Windows, holding the Shift key makes numpad keys behave as if NumLock
                    // wasn't active. The way this is exposed to applications by the system is that
                    // the application receives a fake key release event for the shift key at the
                    // moment when the numpad key is pressed, just before receiving the numpad key
                    // as well.
                    //
                    // The issue is that in the raw device event (here), the fake shift release
                    // event reports the numpad key as the scancode. Unfortunately, the event
                    // doesn't have any information to tell whether it's the
                    // left shift or the right shift that needs to get the fake
                    // release (or press) event so we don't forward this
                    // event to the application at all.
                    //
                    // For more on this, read the article by Raymond Chen, titled:
                    // "The shift key overrides NumLock"
                    // https://devblogs.microsoft.com/oldnewthing/20040906-00/?p=37953
                    return None;
                }
                _ => (),
            }
        }
    }

    Some(physical_key)
}
