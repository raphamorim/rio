// original copyright belongs to Makepad
// Copy-pasted from https://github.com/makepad/makepad/blob/live/platform/src/platform/apple/apple_utils.rs
// and slightly modified

#![allow(dead_code)]

use crate::{
    event::{KeyCode, KeyMods},
    native::apple::frameworks::*,
    CursorIcon,
};

pub fn nsstring_to_string(string: ObjcId) -> String {
    unsafe {
        let utf8_string: *const std::os::raw::c_uchar = msg_send![string, UTF8String];
        let utf8_len: usize = msg_send![string, lengthOfBytesUsingEncoding: UTF8_ENCODING];
        let slice = std::slice::from_raw_parts(utf8_string, utf8_len);
        std::str::from_utf8_unchecked(slice).to_owned()
    }
}

pub fn str_to_nsstring(str: &str) -> ObjcId {
    unsafe {
        let ns_string: ObjcId = msg_send![class!(NSString), alloc];
        let ns_string: ObjcId = msg_send![
            ns_string,
            initWithBytes: str.as_ptr()
            length: str.len()
            encoding: UTF8_ENCODING as ObjcId
        ];
        let _: () = msg_send![ns_string, autorelease];
        ns_string
    }
}

pub fn load_native_cursor(cursor_name: &str) -> ObjcId {
    let sel = Sel::register(cursor_name);
    let id: ObjcId = unsafe { msg_send![class!(NSCursor), performSelector: sel] };
    id
}

pub fn load_undocumented_cursor(cursor_name: &str) -> ObjcId {
    unsafe {
        let class = class!(NSCursor);
        let sel = Sel::register(cursor_name);
        let sel: ObjcId = msg_send![class, respondsToSelector: sel];
        let id: ObjcId = msg_send![class, performSelector: sel];
        id
    }
}

pub unsafe fn ccfstr_from_str(inp: &str) -> CFStringRef {
    let null = format!("{}\0", inp);
    __CFStringMakeConstantString(null.as_ptr() as *const ::std::os::raw::c_char)
}

pub unsafe fn cfstring_ref_to_string(cfstring: CFStringRef) -> String {
    let length = CFStringGetLength(cfstring);
    let range = CFRange {
        location: 0,
        length,
    };
    let mut num_bytes = 0u64;
    let converted = CFStringGetBytes(
        cfstring,
        range,
        kCFStringEncodingUTF8,
        0,
        false,
        0 as *mut u8,
        0,
        &mut num_bytes,
    );
    if converted == 0 || num_bytes == 0 {
        return String::new();
    }
    let mut buffer = Vec::new();
    buffer.resize(num_bytes as usize, 0u8);
    CFStringGetBytes(
        cfstring,
        range,
        kCFStringEncodingUTF8,
        0,
        false,
        buffer.as_mut_ptr() as *mut u8,
        num_bytes,
        0 as *mut u64,
    );
    if let Ok(val) = String::from_utf8(buffer) {
        val
    } else {
        String::new()
    }
}

pub fn load_webkit_cursor(cursor_name_str: &str) -> ObjcId {
    unsafe {
        static CURSOR_ROOT: &'static str = "/System/Library/Frameworks/ApplicationServices.framework/Versions/A/Frameworks/HIServices.framework/Versions/A/Resources/cursors";
        let cursor_root = str_to_nsstring(CURSOR_ROOT);
        let cursor_name = str_to_nsstring(cursor_name_str);
        let cursor_pdf = str_to_nsstring("cursor.pdf");
        let cursor_plist = str_to_nsstring("info.plist");
        let key_x = str_to_nsstring("hotx");
        let key_y = str_to_nsstring("hoty");

        let cursor_path: ObjcId =
            msg_send![cursor_root, stringByAppendingPathComponent: cursor_name];
        let pdf_path: ObjcId = msg_send![cursor_path, stringByAppendingPathComponent: cursor_pdf];
        let info_path: ObjcId =
            msg_send![cursor_path, stringByAppendingPathComponent: cursor_plist];

        let ns_image: ObjcId = msg_send![class!(NSImage), alloc];
        let () = msg_send![ns_image, initByReferencingFile: pdf_path];
        let info: ObjcId = msg_send![
            class!(NSDictionary),
            dictionaryWithContentsOfFile: info_path
        ];
        //let image = NSImage::alloc(nil).initByReferencingFile_(pdf_path);
        // let info = NSDictionary::dictionaryWithContentsOfFile_(nil, info_path);

        let x: ObjcId = msg_send![info, valueForKey: key_x]; //info.valueForKey_(key_x);
        let y: ObjcId = msg_send![info, valueForKey: key_y]; //info.valueForKey_(key_y);
        let point = NSPoint {
            x: msg_send![x, doubleValue],
            y: msg_send![y, doubleValue],
        };
        let cursor: ObjcId = msg_send![class!(NSCursor), alloc];
        msg_send![cursor, initWithImage: ns_image hotSpot: point]
    }
}

pub fn get_event_char(event: ObjcId) -> Option<char> {
    unsafe {
        let characters: ObjcId = msg_send![event, characters];
        if characters == nil {
            return None;
        }
        let chars = nsstring_to_string(characters);

        if chars.len() == 0 {
            return None;
        }
        Some(chars.chars().next().unwrap())
    }
}

pub fn get_event_key_modifier(event: ObjcId) -> KeyMods {
    let flags: u64 = unsafe { msg_send![event, modifierFlags] };
    KeyMods {
        shift: flags & NSEventModifierFlags::NSShiftKeyMask as u64 != 0,
        ctrl: flags & NSEventModifierFlags::NSControlKeyMask as u64 != 0,
        alt: flags & NSEventModifierFlags::NSAlternateKeyMask as u64 != 0,
        logo: flags & NSEventModifierFlags::NSCommandKeyMask as u64 != 0,
    }
}

pub fn get_event_keycode(event: ObjcId) -> Option<KeyCode> {
    let scan_code: std::os::raw::c_ushort = unsafe { msg_send![event, keyCode] };

    Some(match scan_code {
        0x00 => KeyCode::A,
        0x01 => KeyCode::S,
        0x02 => KeyCode::D,
        0x03 => KeyCode::F,
        0x04 => KeyCode::H,
        0x05 => KeyCode::G,
        0x06 => KeyCode::Z,
        0x07 => KeyCode::X,
        0x08 => KeyCode::C,
        0x09 => KeyCode::V,
        //0x0a => World 1,
        0x0b => KeyCode::B,
        0x0c => KeyCode::Q,
        0x0d => KeyCode::W,
        0x0e => KeyCode::E,
        0x0f => KeyCode::R,
        0x10 => KeyCode::Y,
        0x11 => KeyCode::T,
        0x12 => KeyCode::Key1,
        0x13 => KeyCode::Key2,
        0x14 => KeyCode::Key3,
        0x15 => KeyCode::Key4,
        0x16 => KeyCode::Key6,
        0x17 => KeyCode::Key5,
        0x18 => KeyCode::Equal,
        0x19 => KeyCode::Key9,
        0x1a => KeyCode::Key7,
        0x1b => KeyCode::Minus,
        0x1c => KeyCode::Key8,
        0x1d => KeyCode::Key0,
        0x1e => KeyCode::RightBracket,
        0x1f => KeyCode::O,
        0x20 => KeyCode::U,
        0x21 => KeyCode::LeftBracket,
        0x22 => KeyCode::I,
        0x23 => KeyCode::P,
        0x24 => KeyCode::Enter,
        0x25 => KeyCode::L,
        0x26 => KeyCode::J,
        0x27 => KeyCode::Apostrophe,
        0x28 => KeyCode::K,
        0x29 => KeyCode::Semicolon,
        0x2a => KeyCode::Apostrophe,
        0x2b => KeyCode::Comma,
        0x2c => KeyCode::Slash,
        0x2d => KeyCode::N,
        0x2e => KeyCode::M,
        0x2f => KeyCode::Period,
        0x30 => KeyCode::Tab,
        0x31 => KeyCode::Space,
        0x32 => KeyCode::Backslash,
        0x33 => KeyCode::Backspace,
        //0x34 => unkown,
        0x35 => KeyCode::Escape,
        //0x36 => KeyCode::RLogo,
        //0x37 => KeyCode::LLogo,
        //0x38 => KeyCode::LShift,
        0x39 => KeyCode::CapsLock,
        //0x3a => KeyCode::LAlt,
        //0x3b => KeyCode::LControl,
        //0x3c => KeyCode::RShift,
        //0x3d => KeyCode::RAlt,
        //0x3e => KeyCode::RControl,
        //0x3f => Fn key,
        //0x40 => KeyCode::F17,
        0x41 => KeyCode::KpDecimal,
        //0x42 -> unkown,
        0x43 => KeyCode::KpMultiply,
        //0x44 => unkown,
        0x45 => KeyCode::KpAdd,
        //0x46 => unkown,
        0x47 => KeyCode::NumLock,
        //0x48 => KeypadClear,
        //0x49 => KeyCode::VolumeUp,
        //0x4a => KeyCode::VolumeDown,
        0x4b => KeyCode::KpDivide,
        0x4c => KeyCode::KpEnter,
        0x4e => KeyCode::KpSubtract,
        //0x4d => unkown,
        //0x4e => KeyCode::Subtract,
        //0x4f => KeyCode::F18,
        //0x50 => KeyCode::F19,
        0x51 => KeyCode::KpEqual,
        0x52 => KeyCode::Kp0,
        0x53 => KeyCode::Kp1,
        0x54 => KeyCode::Kp2,
        0x55 => KeyCode::Kp3,
        0x56 => KeyCode::Kp4,
        0x57 => KeyCode::Kp5,
        0x58 => KeyCode::Kp6,
        0x59 => KeyCode::Kp7,
        //0x5a => KeyCode::F20,
        0x5b => KeyCode::Kp8,
        0x5c => KeyCode::Kp9,
        //0x5d => KeyCode::Yen,
        //0x5e => JIS Ro,
        //0x5f => unkown,
        0x60 => KeyCode::F5,
        0x61 => KeyCode::F6,
        0x62 => KeyCode::F7,
        0x63 => KeyCode::F3,
        0x64 => KeyCode::F8,
        0x65 => KeyCode::F9,
        //0x66 => JIS Eisuu (macOS),
        0x67 => KeyCode::F11,
        //0x68 => JIS Kana (macOS),
        0x69 => KeyCode::PrintScreen,
        //0x6a => KeyCode::F16,
        //0x6b => KeyCode::F14,
        //0x6c => unkown,
        0x6d => KeyCode::F10,
        //0x6e => unkown,
        0x6f => KeyCode::F12,
        //0x70 => unkown,
        //0x71 => KeyCode::F15,
        0x72 => KeyCode::Insert,
        0x73 => KeyCode::Home,
        0x74 => KeyCode::PageUp,
        0x75 => KeyCode::Delete,
        0x76 => KeyCode::F4,
        0x77 => KeyCode::End,
        0x78 => KeyCode::F2,
        0x79 => KeyCode::PageDown,
        0x7a => KeyCode::F1,
        0x7b => KeyCode::Left,
        0x7c => KeyCode::Right,
        0x7d => KeyCode::Down,
        0x7e => KeyCode::Up,
        //0x7f =>  unkown,
        //0xa => KeyCode::Caret,
        _ => return None,
    })
}

pub fn keycode_to_menu_key(keycode: KeyCode, shift: bool) -> &'static str {
    if !shift {
        match keycode {
            KeyCode::Apostrophe => "`",
            KeyCode::Key0 => "0",
            KeyCode::Key1 => "1",
            KeyCode::Key2 => "2",
            KeyCode::Key3 => "3",
            KeyCode::Key4 => "4",
            KeyCode::Key5 => "5",
            KeyCode::Key6 => "6",
            KeyCode::Key7 => "7",
            KeyCode::Key8 => "8",
            KeyCode::Key9 => "9",
            KeyCode::Minus => "-",
            KeyCode::Equal => "=",

            KeyCode::Q => "q",
            KeyCode::W => "w",
            KeyCode::E => "e",
            KeyCode::R => "r",
            KeyCode::T => "t",
            KeyCode::Y => "y",
            KeyCode::U => "u",
            KeyCode::I => "i",
            KeyCode::O => "o",
            KeyCode::P => "p",
            KeyCode::LeftBracket => "[",
            KeyCode::RightBracket => "]",

            KeyCode::A => "a",
            KeyCode::S => "s",
            KeyCode::D => "d",
            KeyCode::F => "f",
            KeyCode::G => "g",
            KeyCode::H => "h",
            KeyCode::J => "j",
            KeyCode::K => "l",
            KeyCode::L => "l",
            KeyCode::Semicolon => ";",
            KeyCode::Backslash => "\\",

            KeyCode::Z => "z",
            KeyCode::X => "x",
            KeyCode::C => "c",
            KeyCode::V => "v",
            KeyCode::B => "b",
            KeyCode::N => "n",
            KeyCode::M => "m",
            KeyCode::Comma => ",",
            KeyCode::Period => ".",
            KeyCode::Slash => "/",
            _ => "",
        }
    } else {
        match keycode {
            //KeyCode::Backtick => "~",
            KeyCode::Key0 => "!",
            KeyCode::Key1 => "@",
            KeyCode::Key2 => "#",
            KeyCode::Key3 => "$",
            KeyCode::Key4 => "%",
            KeyCode::Key5 => "^",
            KeyCode::Key6 => "&",
            KeyCode::Key7 => "*",
            KeyCode::Key8 => "(",
            KeyCode::Key9 => ")",
            KeyCode::Minus => "_",
            KeyCode::Equal => "=",

            KeyCode::Q => "Q",
            KeyCode::W => "W",
            KeyCode::E => "E",
            KeyCode::R => "R",
            KeyCode::T => "T",
            KeyCode::Y => "Y",
            KeyCode::U => "U",
            KeyCode::I => "I",
            KeyCode::O => "O",
            KeyCode::P => "P",
            KeyCode::LeftBracket => "{",
            KeyCode::RightBracket => "}",

            KeyCode::A => "A",
            KeyCode::S => "S",
            KeyCode::D => "D",
            KeyCode::F => "F",
            KeyCode::G => "G",
            KeyCode::H => "H",
            KeyCode::J => "J",
            KeyCode::K => "K",
            KeyCode::L => "L",
            KeyCode::Semicolon => ":",
            KeyCode::Slash => "\"",
            KeyCode::Backslash => "|",

            KeyCode::Z => "Z",
            KeyCode::X => "X",
            KeyCode::C => "C",
            KeyCode::V => "V",
            KeyCode::B => "B",
            KeyCode::N => "N",
            KeyCode::M => "M",
            KeyCode::Comma => ",",
            KeyCode::Period => ".k",
            _ => "",
        }
    }
}

pub unsafe fn superclass<'a>(this: &'a Object) -> &'a Class {
    let superclass: ObjcId = msg_send![this, superclass];
    &*(superclass as *const _)
}

pub fn bottom_left_to_top_left(rect: NSRect) -> f64 {
    let height = unsafe { CGDisplayPixelsHigh(CGMainDisplayID()) };
    height as f64 - (rect.origin.y + rect.size.height)
}

pub fn load_mouse_cursor(cursor: CursorIcon) -> ObjcId {
    match cursor {
        CursorIcon::Default => load_native_cursor("arrowCursor"),
        CursorIcon::Pointer => load_native_cursor("pointingHandCursor"),
        CursorIcon::Text => load_native_cursor("IBeamCursor"),
        CursorIcon::NotAllowed /*| CursorIcon::NoDrop*/ => load_native_cursor("operationNotAllowedCursor"),
        CursorIcon::Crosshair => load_native_cursor("crosshairCursor"),
        /*
        CursorIcon::Grabbing | CursorIcon::Grab => load_native_cursor("closedHandCursor"),
        CursorIcon::VerticalText => load_native_cursor("IBeamCursorForVerticalLayout"),
        CursorIcon::Copy => load_native_cursor("dragCopyCursor"),
        CursorIcon::Alias => load_native_cursor("dragLinkCursor"),
        CursorIcon::ContextMenu => load_native_cursor("contextualMenuCursor"),
        */
        //CursorIcon::EResize => load_native_cursor("resizeRightCursor"),
        //CursorIcon::NResize => load_native_cursor("resizeUpCursor"),
        //CursorIcon::WResize => load_native_cursor("resizeLeftCursor"),
        //CursorIcon::SResize => load_native_cursor("resizeDownCursor"),
        CursorIcon::EWResize => load_native_cursor("resizeLeftRightCursor"),
        CursorIcon::NSResize => load_native_cursor("resizeUpDownCursor"),

        // Undocumented cursors: https://stackoverflow.com/a/46635398/5435443
        // Unfortunately undocumented cursors requires NSTracking areas that
        // we do not use yet.
        _ => load_native_cursor("arrowCursor"),
        // CursorIcon::Help => load_undocumented_cursor("_helpCursor"),
        // //CursorIcon::ZoomIn => load_undocumented_cursor("_zoomInCursor"),
        // //CursorIcon::ZoomOut => load_undocumented_cursor("_zoomOutCursor"),

        // CursorIcon::NESWResize => load_undocumented_cursor("_windowResizeNorthEastSouthWestCursor"),
        // CursorIcon::NWSEResize => load_undocumented_cursor("_windowResizeNorthWestSouthEastCursor"),

        // // While these are available, the former just loads a white arrow,
        // // and the latter loads an ugly deflated beachball!
        // // CursorIcon::Move => Cursor::Undocumented("_moveCursor"),
        // // CursorIcon::Wait => Cursor::Undocumented("_waitCursor"),
        // // An even more undocumented cursor...
        // // https://bugs.eclipse.org/bugs/show_bug.cgi?id=522349
        // // This is the wrong semantics for `Wait`, but it's the same as
        // // what's used in Safari and Chrome.
        // CursorIcon::Wait/* | CursorIcon::Progress*/ => load_undocumented_cursor("busyButClickableCursor"),

        // // For the rest, we can just snatch the cursors from WebKit...
        // // They fit the style of the native cursors, and will seem
        // // completely standard to macOS users.
        // // https://stackoverflow.com/a/21786835/5435443
        // CursorIcon::Move /*| CursorIcon::AllScroll*/ => load_webkit_cursor("move"),
        // CursorIcon::Cell => load_webkit_cursor("cell"),
    }
}

// macro_rules!objc_block {
//     (move | $ ( $ arg_ident: ident: $ arg_ty: ty), * | $ (: $ return_ty: ty) ? $ body: block) => {
//         {
//             #[repr(C)]
//             struct BlockDescriptor {
//                 reserved: std::os::raw::c_ulong,
//                 size: std::os::raw::c_ulong,
//                 copy_helper: extern "C" fn(*mut std::os::raw::c_void, *const std::os::raw::c_void),
//                 dispose_helper: extern "C" fn(*mut std::os::raw::c_void),
//             }

//             static DESCRIPTOR: BlockDescriptor = BlockDescriptor {
//                 reserved: 0,
//                 size: mem::size_of::<BlockLiteral>() as std::os::raw::c_ulong,
//                 copy_helper,
//                 dispose_helper,
//             };

//             #[allow(unused_unsafe)]
//             extern "C" fn copy_helper(dst: *mut std::os::raw::c_void, src: *const std::os::raw::c_void) {
//                 unsafe {
//                     ptr::write(
//                         &mut (*(dst as *mut BlockLiteral)).inner as *mut _,
//                         (&*(src as *const BlockLiteral)).inner.clone()
//                     );
//                 }
//             }

//             #[allow(unused_unsafe)]
//             extern "C" fn dispose_helper(src: *mut std::os::raw::c_void) {
//                 unsafe {
//                     ptr::drop_in_place(src as *mut BlockLiteral);
//                 }
//             }

//             #[allow(unused_unsafe)]
//             extern "C" fn invoke(literal: *mut BlockLiteral, $ ( $ arg_ident: $ arg_ty), *) $ ( -> $ return_ty) ? {
//                 let literal = unsafe {&mut *literal};
//                 literal.inner.lock().unwrap()( $ ( $ arg_ident), *)
//             }

//             #[repr(C)]
//             struct BlockLiteral {
//                 isa: *const std::os::raw::c_void,
//                 flags: std::os::raw::c_int,
//                 reserved: std::os::raw::c_int,
//                 invoke: extern "C" fn(*mut BlockLiteral, $ ( $ arg_ty), *) $ ( -> $ return_ty) ?,
//                 descriptor: *const BlockDescriptor,
//                 inner: ::std::sync::Arc<::std::sync::Mutex<dyn Fn( $ ( $ arg_ty), *) $ ( -> $ return_ty) ? >>,
//             }

//             #[allow(unused_unsafe)]
//             BlockLiteral {
//                 isa: unsafe {_NSConcreteStackBlock.as_ptr() as *const std::os::raw::c_void},
//                 flags: 1 << 25,
//                 reserved: 0,
//                 invoke,
//                 descriptor: &DESCRIPTOR,
//                 inner: ::std::sync::Arc::new(::std::sync::Mutex::new(move | $ ( $ arg_ident: $ arg_ty), * | {
//                     $ body
//                 }))
//             }
//         }
//     }
// }

// macro_rules!objc_block_invoke {
//     ( $ inp: expr, invoke ( $ ( ($ arg_ident: expr): $ arg_ty: ty), *) $ ( -> $ return_ty: ty) ?) => {
//         {
//             #[repr(C)]
//             struct BlockLiteral {
//                 isa: *const std::os::raw::c_void,
//                 flags: std::os::raw::c_int,
//                 reserved: std::os::raw::c_int,
//                 invoke: extern "C" fn(*mut BlockLiteral, $ ( $ arg_ty), *) $ ( -> $ return_ty) ?,
//             }

//             let block: &mut BlockLiteral = &mut *( $ inp as *mut _);
//             (block.invoke)(block, $ ( $ arg_ident), *)
//         }
//     }
// }

macro_rules! msg_send_ {
    ($obj:expr, $name:ident) => ({
        let res: ObjcId = msg_send!($obj, $name);
        res
    });
    ($obj:expr, $($name:ident : $arg:expr)+) => ({
        let res: ObjcId = msg_send!($obj, $($name: $arg)*);
        res
    });
}
pub(crate) use msg_send_;

pub extern "C" fn yes(_: &Object, _: Sel) -> BOOL {
    YES
}

pub extern "C" fn yes1(_: &Object, _: Sel, _: ObjcId) -> BOOL {
    YES
}
