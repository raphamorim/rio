// ASCII and ANSI Characters

pub const A: u8 = b'a';
pub const B: u8 = b'b';
pub const C: u8 = b'c';
pub const D: u8 = b'd';
pub const E: u8 = b'e';
pub const F: u8 = b'f';
pub const G: u8 = b'g';
pub const H: u8 = b'h';
pub const I: u8 = b'i';
pub const J: u8 = b'j';
pub const K: u8 = b'k';
pub const L: u8 = b'l';
pub const M: u8 = b'm';
pub const N: u8 = b'n';
pub const O: u8 = b'o';
pub const P: u8 = b'p';
pub const Q: u8 = b'q';
pub const R: u8 = b'r';
pub const S: u8 = b's';
pub const T: u8 = b't';
pub const U: u8 = b'u';
pub const V: u8 = b'v';
pub const W: u8 = b'w';
pub const X: u8 = b'x';
pub const Y: u8 = b'y';
pub const Z: u8 = b'z';
pub const K0: u8 = b'0';
pub const K1: u8 = b'1';
pub const K2: u8 = b'2';
pub const K3: u8 = b'3';
pub const K4: u8 = b'4';
pub const K5: u8 = b'5';
pub const K6: u8 = b'6';
pub const K7: u8 = b'7';
pub const K8: u8 = b'8';
pub const K9: u8 = b'9';
#[allow(unused)]
pub const EQUAL: u8 = b'=';
#[allow(unused)]
pub const MINUS: u8 = b'-';
#[allow(unused)]
pub const RIGHT_BRACKET: u8 = 0x1E;
#[allow(unused)]
pub const LEFT_BRACKET: u8 = 0x21;
#[allow(unused)]
pub const QUOTE: u8 = 0x27;
#[allow(unused)]
pub const SEMICOLON: u8 = 0x29;
#[allow(unused)]
pub const BACKSLASH: u8 = 0x2A;
#[allow(unused)]
pub const COMMA: u8 = 0x2B;
#[allow(unused)]
pub const SLASH: u8 = 0x2C;
#[allow(unused)]
pub const PERIOD: u8 = b'.';
#[allow(unused)]
pub const GRAVE: u8 = 0x32;
#[allow(unused)]
pub const KEYPAD_DECIMAL: u8 = 0x41;
#[allow(unused)]
pub const KEYPAD_MULTIPLY: u8 = 0x43;
#[allow(unused)]
pub const KEYPAD_PLUS: u8 = 0x45;
#[allow(unused)]
pub const KEYPAD_CLEAR: u8 = 0x47;
#[allow(unused)]
pub const KEYPAD_DIVIDE: u8 = 0x4B;
#[allow(unused)]
pub const KEYPAD_ENTER: u8 = 0x4C;
#[allow(unused)]
pub const KEYPAD_MINUS: u8 = 0x4E;
#[allow(unused)]
pub const KEYPAD_EQUALS: u8 = 0x51;
#[allow(unused)]
pub const KEYPAD0: u8 = 48;
#[allow(unused)]
pub const KEYPAD1: u8 = 49;
#[allow(unused)]
pub const KEYPAD2: u8 = 50;
#[allow(unused)]
pub const KEYPAD3: u8 = 51;
#[allow(unused)]
pub const KEYPAD4: u8 = 52;
#[allow(unused)]
pub const KEYPAD5: u8 = 53;
#[allow(unused)]
pub const KEYPAD6: u8 = 54;
#[allow(unused)]
pub const KEYPAD7: u8 = 55;
#[allow(unused)]
pub const KEYPAD8: u8 = 56;
#[allow(unused)]
pub const KEYPAD9: u8 = 57;

pub const RETURN: u8 = 13;
pub const TAB: u8 = 9;
pub const SPACE: u8 = 32;
#[allow(unused)]
pub const DELETE: u8 = 0x7F;
pub const BACKSPACE: u8 = 8;
#[allow(unused)]
pub const COMMAND: u8 = 0x37;
#[allow(unused)]
pub const SHIFT_IN: u8 = 15;
#[allow(unused)]
pub const SHIFT_OUT: u8 = 16;
#[allow(unused)]
pub const CAPS_LOCK: u8 = 0x39;
#[allow(unused)]
pub const OPTION: u8 = 0x3A;
#[allow(unused)]
pub const CONTROL: u8 = 0x3B;
#[allow(unused)]
pub const RIGHT_COMMAND: u8 = 0x36;
#[allow(unused)]
pub const RIGHT_SHIFT: u8 = 0x3C;
#[allow(unused)]
pub const RIGHT_OPTION: u8 = 0x3D;
#[allow(unused)]
pub const RIGHT_CONTROL: u8 = 0x3E;
#[allow(unused)]
pub const FUNCTION: u8 = 0x3F;
#[allow(unused)]
pub const F17: u8 = 0x40;
#[allow(unused)]
pub const VOLUME_UP: u8 = 0x48;
#[allow(unused)]
pub const VOLUME_DOWN: u8 = 0x49;
#[allow(unused)]
pub const MUTE: u8 = 0x4A;
#[allow(unused)]
pub const F18: u8 = 0x4F;
#[allow(unused)]
pub const F19: u8 = 0x50;
#[allow(unused)]
pub const F20: u8 = 0x5A;
#[allow(unused)]
pub const F5: u8 = 0x60;
#[allow(unused)]
pub const F6: u8 = 0x61;
#[allow(unused)]
pub const F7: u8 = 0x62;
#[allow(unused)]
pub const F3: u8 = 0x63;
#[allow(unused)]
pub const F8: u8 = 0x64;
#[allow(unused)]
pub const F9: u8 = 0x65;
#[allow(unused)]
pub const F11: u8 = 0x67;
#[allow(unused)]
pub const F13: u8 = 0x69;
#[allow(unused)]
pub const F16: u8 = 0x6A;
#[allow(unused)]
pub const F14: u8 = 0x6B;
#[allow(unused)]
pub const F10: u8 = 0x6D;
#[allow(unused)]
pub const F12: u8 = 0x6F;
#[allow(unused)]
pub const F15: u8 = 0x71;
#[allow(unused)]
pub const HELP: u8 = 0x72;
#[allow(unused)]
pub const HOME: u8 = 0x73;
#[allow(unused)]
pub const PAGE_UP: u8 = 0x74;
#[allow(unused)]
pub const FORWARD_DELETE: u8 = 0x75;
#[allow(unused)]
pub const F4: u8 = 0x76;
#[allow(unused)]
pub const END: u8 = 0x77;
#[allow(unused)]
pub const F2: u8 = 0x78;
#[allow(unused)]
pub const PAGE_DOWN: u8 = 0x79;
#[allow(unused)]
pub const F1: u8 = 0x7A;
#[allow(unused)]
pub const LEFT_ARROW: u8 = 0x7B;
#[allow(unused)]
pub const RIGHT_ARROW: u8 = 0x7C;
#[allow(unused)]
pub const DOWN_ARROW: u8 = 0x7D;
#[allow(unused)]
pub const UP_ARROW: u8 = 0x7E;
