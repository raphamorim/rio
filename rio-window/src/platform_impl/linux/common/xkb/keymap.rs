//! XKB keymap.

use std::ffi::c_char;
use std::ops::Deref;
use std::ptr::{self, NonNull};

#[cfg(x11_platform)]
use x11_dl::xlib_xcb::xcb_connection_t;
#[cfg(wayland_platform)]
use {memmap2::MmapOptions, std::os::unix::io::OwnedFd};

use xkb::XKB_MOD_INVALID;
use xkbcommon_dl::{
    self as xkb, xkb_keycode_t, xkb_keymap, xkb_keymap_compile_flags, xkb_keysym_t,
    xkb_layout_index_t, xkb_mod_index_t,
};

use crate::keyboard::{
    Key, KeyCode, KeyLocation, NamedKey, NativeKey, NativeKeyCode, PhysicalKey,
};
#[cfg(x11_platform)]
use crate::platform_impl::common::xkb::XKBXH;
use crate::platform_impl::common::xkb::{XkbContext, XKBH};

/// Map the raw X11-style keycode to the `KeyCode` enum.
///
/// X11-style keycodes are offset by 8 from the keycodes the Linux kernel uses.
pub fn raw_keycode_to_physicalkey(keycode: u32) -> PhysicalKey {
    scancode_to_physicalkey(keycode.saturating_sub(8))
}

/// Map the linux scancode to Keycode.
///
/// Both X11 and Wayland use keys with `+ 8` offset to linux scancode.
pub fn scancode_to_physicalkey(scancode: u32) -> PhysicalKey {
    // The keycode values are taken from linux/include/uapi/linux/input-event-codes.h, as
    // libxkbcommon's documentation seems to suggest that the keycode values we're interested in
    // are defined by the Linux kernel. If Winit programs end up being run on other Unix-likes,
    // I can only hope they agree on what the keycodes mean.
    //
    // The mapping here is heavily influenced by Firefox' source:
    // https://searchfox.org/mozilla-central/rev/c597e9c789ad36af84a0370d395be066b7dc94f4/widget/NativeKeyToDOMCodeName.h
    //
    // Some of the keycodes are likely superfluous for our purposes, and some are ones which are
    // difficult to test the correctness of, or discover the purpose of. Because of this, they've
    // either been commented out here, or not included at all.
    PhysicalKey::Code(match scancode {
        0 => return PhysicalKey::Unidentified(NativeKeyCode::Xkb(0)),
        1 => KeyCode::Escape,
        2 => KeyCode::Digit1,
        3 => KeyCode::Digit2,
        4 => KeyCode::Digit3,
        5 => KeyCode::Digit4,
        6 => KeyCode::Digit5,
        7 => KeyCode::Digit6,
        8 => KeyCode::Digit7,
        9 => KeyCode::Digit8,
        10 => KeyCode::Digit9,
        11 => KeyCode::Digit0,
        12 => KeyCode::Minus,
        13 => KeyCode::Equal,
        14 => KeyCode::Backspace,
        15 => KeyCode::Tab,
        16 => KeyCode::KeyQ,
        17 => KeyCode::KeyW,
        18 => KeyCode::KeyE,
        19 => KeyCode::KeyR,
        20 => KeyCode::KeyT,
        21 => KeyCode::KeyY,
        22 => KeyCode::KeyU,
        23 => KeyCode::KeyI,
        24 => KeyCode::KeyO,
        25 => KeyCode::KeyP,
        26 => KeyCode::BracketLeft,
        27 => KeyCode::BracketRight,
        28 => KeyCode::Enter,
        29 => KeyCode::ControlLeft,
        30 => KeyCode::KeyA,
        31 => KeyCode::KeyS,
        32 => KeyCode::KeyD,
        33 => KeyCode::KeyF,
        34 => KeyCode::KeyG,
        35 => KeyCode::KeyH,
        36 => KeyCode::KeyJ,
        37 => KeyCode::KeyK,
        38 => KeyCode::KeyL,
        39 => KeyCode::Semicolon,
        40 => KeyCode::Quote,
        41 => KeyCode::Backquote,
        42 => KeyCode::ShiftLeft,
        43 => KeyCode::Backslash,
        44 => KeyCode::KeyZ,
        45 => KeyCode::KeyX,
        46 => KeyCode::KeyC,
        47 => KeyCode::KeyV,
        48 => KeyCode::KeyB,
        49 => KeyCode::KeyN,
        50 => KeyCode::KeyM,
        51 => KeyCode::Comma,
        52 => KeyCode::Period,
        53 => KeyCode::Slash,
        54 => KeyCode::ShiftRight,
        55 => KeyCode::NumpadMultiply,
        56 => KeyCode::AltLeft,
        57 => KeyCode::Space,
        58 => KeyCode::CapsLock,
        59 => KeyCode::F1,
        60 => KeyCode::F2,
        61 => KeyCode::F3,
        62 => KeyCode::F4,
        63 => KeyCode::F5,
        64 => KeyCode::F6,
        65 => KeyCode::F7,
        66 => KeyCode::F8,
        67 => KeyCode::F9,
        68 => KeyCode::F10,
        69 => KeyCode::NumLock,
        70 => KeyCode::ScrollLock,
        71 => KeyCode::Numpad7,
        72 => KeyCode::Numpad8,
        73 => KeyCode::Numpad9,
        74 => KeyCode::NumpadSubtract,
        75 => KeyCode::Numpad4,
        76 => KeyCode::Numpad5,
        77 => KeyCode::Numpad6,
        78 => KeyCode::NumpadAdd,
        79 => KeyCode::Numpad1,
        80 => KeyCode::Numpad2,
        81 => KeyCode::Numpad3,
        82 => KeyCode::Numpad0,
        83 => KeyCode::NumpadDecimal,
        85 => KeyCode::Lang5,
        86 => KeyCode::IntlBackslash,
        87 => KeyCode::F11,
        88 => KeyCode::F12,
        89 => KeyCode::IntlRo,
        90 => KeyCode::Lang3,
        91 => KeyCode::Lang4,
        92 => KeyCode::Convert,
        93 => KeyCode::KanaMode,
        94 => KeyCode::NonConvert,
        // 95 => KeyCode::KPJPCOMMA,
        96 => KeyCode::NumpadEnter,
        97 => KeyCode::ControlRight,
        98 => KeyCode::NumpadDivide,
        99 => KeyCode::PrintScreen,
        100 => KeyCode::AltRight,
        // 101 => KeyCode::LINEFEED,
        102 => KeyCode::Home,
        103 => KeyCode::ArrowUp,
        104 => KeyCode::PageUp,
        105 => KeyCode::ArrowLeft,
        106 => KeyCode::ArrowRight,
        107 => KeyCode::End,
        108 => KeyCode::ArrowDown,
        109 => KeyCode::PageDown,
        110 => KeyCode::Insert,
        111 => KeyCode::Delete,
        // 112 => KeyCode::MACRO,
        113 => KeyCode::AudioVolumeMute,
        114 => KeyCode::AudioVolumeDown,
        115 => KeyCode::AudioVolumeUp,
        // 116 => KeyCode::POWER,
        117 => KeyCode::NumpadEqual,
        // 118 => KeyCode::KPPLUSMINUS,
        119 => KeyCode::Pause,
        // 120 => KeyCode::SCALE,
        121 => KeyCode::NumpadComma,
        122 => KeyCode::Lang1,
        123 => KeyCode::Lang2,
        124 => KeyCode::IntlYen,
        125 => KeyCode::SuperLeft,
        126 => KeyCode::SuperRight,
        127 => KeyCode::ContextMenu,
        128 => KeyCode::BrowserStop,
        129 => KeyCode::Again,
        130 => KeyCode::Props,
        131 => KeyCode::Undo,
        132 => KeyCode::Select, // FRONT
        133 => KeyCode::Copy,
        134 => KeyCode::Open,
        135 => KeyCode::Paste,
        136 => KeyCode::Find,
        137 => KeyCode::Cut,
        138 => KeyCode::Help,
        // 139 => KeyCode::MENU,
        140 => KeyCode::LaunchApp2, // CALC
        // 141 => KeyCode::SETUP,
        // 142 => KeyCode::SLEEP,
        143 => KeyCode::WakeUp,
        144 => KeyCode::LaunchApp1, // FILE
        // 145 => KeyCode::SENDFILE,
        // 146 => KeyCode::DELETEFILE,
        // 147 => KeyCode::XFER,
        // 148 => KeyCode::PROG1,
        // 149 => KeyCode::PROG2,
        // 150 => KeyCode::WWW,
        // 151 => KeyCode::MSDOS,
        // 152 => KeyCode::COFFEE,
        // 153 => KeyCode::ROTATE_DISPLAY,
        // 154 => KeyCode::CYCLEWINDOWS,
        // 155 => KeyCode::MAIL,
        // 156 => KeyCode::BOOKMARKS,
        // 157 => KeyCode::COMPUTER,
        158 => KeyCode::BrowserBack,
        159 => KeyCode::BrowserForward,
        // 160 => KeyCode::CLOSECD,
        161 => KeyCode::Eject, // EJECTCD
        // 162 => KeyCode::EJECTCLOSECD,
        163 => KeyCode::MediaTrackNext,
        164 => KeyCode::MediaPlayPause,
        165 => KeyCode::MediaTrackPrevious,
        166 => KeyCode::MediaStop,
        // 167 => KeyCode::RECORD,
        // 168 => KeyCode::REWIND,
        // 169 => KeyCode::PHONE,
        // 170 => KeyCode::ISO,
        171 => KeyCode::MediaSelect, // CONFIG
        172 => KeyCode::BrowserHome,
        173 => KeyCode::BrowserRefresh,
        // 174 => KeyCode::EXIT,
        // 175 => KeyCode::MOVE,
        // 176 => KeyCode::EDIT,
        // 177 => KeyCode::SCROLLUP,
        // 178 => KeyCode::SCROLLDOWN,
        // 179 => KeyCode::KPLEFTPAREN,
        // 180 => KeyCode::KPRIGHTPAREN,
        // 181 => KeyCode::NEW,
        // 182 => KeyCode::REDO,
        183 => KeyCode::F13,
        184 => KeyCode::F14,
        185 => KeyCode::F15,
        186 => KeyCode::F16,
        187 => KeyCode::F17,
        188 => KeyCode::F18,
        189 => KeyCode::F19,
        190 => KeyCode::F20,
        191 => KeyCode::F21,
        192 => KeyCode::F22,
        193 => KeyCode::F23,
        194 => KeyCode::F24,
        // 200 => KeyCode::PLAYCD,
        // 201 => KeyCode::PAUSECD,
        // 202 => KeyCode::PROG3,
        // 203 => KeyCode::PROG4,
        // 204 => KeyCode::DASHBOARD,
        // 205 => KeyCode::SUSPEND,
        // 206 => KeyCode::CLOSE,
        // 207 => KeyCode::PLAY,
        // 208 => KeyCode::FASTFORWARD,
        // 209 => KeyCode::BASSBOOST,
        // 210 => KeyCode::PRINT,
        // 211 => KeyCode::HP,
        // 212 => KeyCode::CAMERA,
        // 213 => KeyCode::SOUND,
        // 214 => KeyCode::QUESTION,
        // 215 => KeyCode::EMAIL,
        // 216 => KeyCode::CHAT,
        217 => KeyCode::BrowserSearch,
        // 218 => KeyCode::CONNECT,
        // 219 => KeyCode::FINANCE,
        // 220 => KeyCode::SPORT,
        // 221 => KeyCode::SHOP,
        // 222 => KeyCode::ALTERASE,
        // 223 => KeyCode::CANCEL,
        // 224 => KeyCode::BRIGHTNESSDOW,
        // 225 => KeyCode::BRIGHTNESSU,
        // 226 => KeyCode::MEDIA,
        // 227 => KeyCode::SWITCHVIDEOMODE,
        // 228 => KeyCode::KBDILLUMTOGGLE,
        // 229 => KeyCode::KBDILLUMDOWN,
        // 230 => KeyCode::KBDILLUMUP,
        // 231 => KeyCode::SEND,
        // 232 => KeyCode::REPLY,
        // 233 => KeyCode::FORWARDMAIL,
        // 234 => KeyCode::SAVE,
        // 235 => KeyCode::DOCUMENTS,
        // 236 => KeyCode::BATTERY,
        // 237 => KeyCode::BLUETOOTH,
        // 238 => KeyCode::WLAN,
        // 239 => KeyCode::UWB,
        240 => return PhysicalKey::Unidentified(NativeKeyCode::Unidentified),
        // 241 => KeyCode::VIDEO_NEXT,
        // 242 => KeyCode::VIDEO_PREV,
        // 243 => KeyCode::BRIGHTNESS_CYCLE,
        // 244 => KeyCode::BRIGHTNESS_AUTO,
        // 245 => KeyCode::DISPLAY_OFF,
        // 246 => KeyCode::WWAN,
        // 247 => KeyCode::RFKILL,
        // 248 => KeyCode::KEY_MICMUTE,
        _ => return PhysicalKey::Unidentified(NativeKeyCode::Xkb(scancode)),
    })
}

pub fn physicalkey_to_scancode(key: PhysicalKey) -> Option<u32> {
    let code = match key {
        PhysicalKey::Code(code) => code,
        PhysicalKey::Unidentified(code) => {
            return match code {
                NativeKeyCode::Unidentified => Some(240),
                NativeKeyCode::Xkb(raw) => Some(raw),
                _ => None,
            };
        }
    };

    match code {
        KeyCode::Escape => Some(1),
        KeyCode::Digit1 => Some(2),
        KeyCode::Digit2 => Some(3),
        KeyCode::Digit3 => Some(4),
        KeyCode::Digit4 => Some(5),
        KeyCode::Digit5 => Some(6),
        KeyCode::Digit6 => Some(7),
        KeyCode::Digit7 => Some(8),
        KeyCode::Digit8 => Some(9),
        KeyCode::Digit9 => Some(10),
        KeyCode::Digit0 => Some(11),
        KeyCode::Minus => Some(12),
        KeyCode::Equal => Some(13),
        KeyCode::Backspace => Some(14),
        KeyCode::Tab => Some(15),
        KeyCode::KeyQ => Some(16),
        KeyCode::KeyW => Some(17),
        KeyCode::KeyE => Some(18),
        KeyCode::KeyR => Some(19),
        KeyCode::KeyT => Some(20),
        KeyCode::KeyY => Some(21),
        KeyCode::KeyU => Some(22),
        KeyCode::KeyI => Some(23),
        KeyCode::KeyO => Some(24),
        KeyCode::KeyP => Some(25),
        KeyCode::BracketLeft => Some(26),
        KeyCode::BracketRight => Some(27),
        KeyCode::Enter => Some(28),
        KeyCode::ControlLeft => Some(29),
        KeyCode::KeyA => Some(30),
        KeyCode::KeyS => Some(31),
        KeyCode::KeyD => Some(32),
        KeyCode::KeyF => Some(33),
        KeyCode::KeyG => Some(34),
        KeyCode::KeyH => Some(35),
        KeyCode::KeyJ => Some(36),
        KeyCode::KeyK => Some(37),
        KeyCode::KeyL => Some(38),
        KeyCode::Semicolon => Some(39),
        KeyCode::Quote => Some(40),
        KeyCode::Backquote => Some(41),
        KeyCode::ShiftLeft => Some(42),
        KeyCode::Backslash => Some(43),
        KeyCode::KeyZ => Some(44),
        KeyCode::KeyX => Some(45),
        KeyCode::KeyC => Some(46),
        KeyCode::KeyV => Some(47),
        KeyCode::KeyB => Some(48),
        KeyCode::KeyN => Some(49),
        KeyCode::KeyM => Some(50),
        KeyCode::Comma => Some(51),
        KeyCode::Period => Some(52),
        KeyCode::Slash => Some(53),
        KeyCode::ShiftRight => Some(54),
        KeyCode::NumpadMultiply => Some(55),
        KeyCode::AltLeft => Some(56),
        KeyCode::Space => Some(57),
        KeyCode::CapsLock => Some(58),
        KeyCode::F1 => Some(59),
        KeyCode::F2 => Some(60),
        KeyCode::F3 => Some(61),
        KeyCode::F4 => Some(62),
        KeyCode::F5 => Some(63),
        KeyCode::F6 => Some(64),
        KeyCode::F7 => Some(65),
        KeyCode::F8 => Some(66),
        KeyCode::F9 => Some(67),
        KeyCode::F10 => Some(68),
        KeyCode::NumLock => Some(69),
        KeyCode::ScrollLock => Some(70),
        KeyCode::Numpad7 => Some(71),
        KeyCode::Numpad8 => Some(72),
        KeyCode::Numpad9 => Some(73),
        KeyCode::NumpadSubtract => Some(74),
        KeyCode::Numpad4 => Some(75),
        KeyCode::Numpad5 => Some(76),
        KeyCode::Numpad6 => Some(77),
        KeyCode::NumpadAdd => Some(78),
        KeyCode::Numpad1 => Some(79),
        KeyCode::Numpad2 => Some(80),
        KeyCode::Numpad3 => Some(81),
        KeyCode::Numpad0 => Some(82),
        KeyCode::NumpadDecimal => Some(83),
        KeyCode::Lang5 => Some(85),
        KeyCode::IntlBackslash => Some(86),
        KeyCode::F11 => Some(87),
        KeyCode::F12 => Some(88),
        KeyCode::IntlRo => Some(89),
        KeyCode::Lang3 => Some(90),
        KeyCode::Lang4 => Some(91),
        KeyCode::Convert => Some(92),
        KeyCode::KanaMode => Some(93),
        KeyCode::NonConvert => Some(94),
        KeyCode::NumpadEnter => Some(96),
        KeyCode::ControlRight => Some(97),
        KeyCode::NumpadDivide => Some(98),
        KeyCode::PrintScreen => Some(99),
        KeyCode::AltRight => Some(100),
        KeyCode::Home => Some(102),
        KeyCode::ArrowUp => Some(103),
        KeyCode::PageUp => Some(104),
        KeyCode::ArrowLeft => Some(105),
        KeyCode::ArrowRight => Some(106),
        KeyCode::End => Some(107),
        KeyCode::ArrowDown => Some(108),
        KeyCode::PageDown => Some(109),
        KeyCode::Insert => Some(110),
        KeyCode::Delete => Some(111),
        KeyCode::AudioVolumeMute => Some(113),
        KeyCode::AudioVolumeDown => Some(114),
        KeyCode::AudioVolumeUp => Some(115),
        KeyCode::NumpadEqual => Some(117),
        KeyCode::Pause => Some(119),
        KeyCode::NumpadComma => Some(121),
        KeyCode::Lang1 => Some(122),
        KeyCode::Lang2 => Some(123),
        KeyCode::IntlYen => Some(124),
        KeyCode::SuperLeft => Some(125),
        KeyCode::SuperRight => Some(126),
        KeyCode::ContextMenu => Some(127),
        KeyCode::BrowserStop => Some(128),
        KeyCode::Again => Some(129),
        KeyCode::Props => Some(130),
        KeyCode::Undo => Some(131),
        KeyCode::Select => Some(132),
        KeyCode::Copy => Some(133),
        KeyCode::Open => Some(134),
        KeyCode::Paste => Some(135),
        KeyCode::Find => Some(136),
        KeyCode::Cut => Some(137),
        KeyCode::Help => Some(138),
        KeyCode::LaunchApp2 => Some(140),
        KeyCode::WakeUp => Some(143),
        KeyCode::LaunchApp1 => Some(144),
        KeyCode::LaunchMail => Some(155),
        KeyCode::BrowserFavorites => Some(156),
        KeyCode::BrowserBack => Some(158),
        KeyCode::BrowserForward => Some(159),
        KeyCode::Eject => Some(161),
        KeyCode::MediaTrackNext => Some(163),
        KeyCode::MediaPlayPause => Some(164),
        KeyCode::MediaTrackPrevious => Some(165),
        KeyCode::MediaStop => Some(166),
        KeyCode::MediaSelect => Some(171),
        KeyCode::BrowserHome => Some(172),
        KeyCode::BrowserRefresh => Some(173),
        KeyCode::F13 => Some(183),
        KeyCode::F14 => Some(184),
        KeyCode::F15 => Some(185),
        KeyCode::F16 => Some(186),
        KeyCode::F17 => Some(187),
        KeyCode::F18 => Some(188),
        KeyCode::F19 => Some(189),
        KeyCode::F20 => Some(190),
        KeyCode::F21 => Some(191),
        KeyCode::F22 => Some(192),
        KeyCode::F23 => Some(193),
        KeyCode::F24 => Some(194),
        KeyCode::BrowserSearch => Some(217),
        _ => None,
    }
}

pub fn keysym_to_key(keysym: u32) -> Key {
    use xkbcommon_dl::keysyms;
    Key::Named(match keysym {
        // TTY function keys
        keysyms::BackSpace => NamedKey::Backspace,
        keysyms::Tab => NamedKey::Tab,
        // keysyms::Linefeed => NamedKey::Linefeed,
        keysyms::Clear => NamedKey::Clear,
        keysyms::Return => NamedKey::Enter,
        keysyms::Pause => NamedKey::Pause,
        keysyms::Scroll_Lock => NamedKey::ScrollLock,
        keysyms::Sys_Req => NamedKey::PrintScreen,
        keysyms::Escape => NamedKey::Escape,
        keysyms::Delete => NamedKey::Delete,

        // IME keys
        keysyms::Multi_key => NamedKey::Compose,
        keysyms::Codeinput => NamedKey::CodeInput,
        keysyms::SingleCandidate => NamedKey::SingleCandidate,
        keysyms::MultipleCandidate => NamedKey::AllCandidates,
        keysyms::PreviousCandidate => NamedKey::PreviousCandidate,

        // Japanese keys
        keysyms::Kanji => NamedKey::KanjiMode,
        keysyms::Muhenkan => NamedKey::NonConvert,
        keysyms::Henkan_Mode => NamedKey::Convert,
        keysyms::Romaji => NamedKey::Romaji,
        keysyms::Hiragana => NamedKey::Hiragana,
        keysyms::Hiragana_Katakana => NamedKey::HiraganaKatakana,
        keysyms::Zenkaku => NamedKey::Zenkaku,
        keysyms::Hankaku => NamedKey::Hankaku,
        keysyms::Zenkaku_Hankaku => NamedKey::ZenkakuHankaku,
        // keysyms::Touroku => NamedKey::Touroku,
        // keysyms::Massyo => NamedKey::Massyo,
        keysyms::Kana_Lock => NamedKey::KanaMode,
        keysyms::Kana_Shift => NamedKey::KanaMode,
        keysyms::Eisu_Shift => NamedKey::Alphanumeric,
        keysyms::Eisu_toggle => NamedKey::Alphanumeric,
        // NOTE: The next three items are aliases for values we've already mapped.
        // keysyms::Kanji_Bangou => NamedKey::CodeInput,
        // keysyms::Zen_Koho => NamedKey::AllCandidates,
        // keysyms::Mae_Koho => NamedKey::PreviousCandidate,

        // Cursor control & motion
        keysyms::Home => NamedKey::Home,
        keysyms::Left => NamedKey::ArrowLeft,
        keysyms::Up => NamedKey::ArrowUp,
        keysyms::Right => NamedKey::ArrowRight,
        keysyms::Down => NamedKey::ArrowDown,
        // keysyms::Prior => NamedKey::PageUp,
        keysyms::Page_Up => NamedKey::PageUp,
        // keysyms::Next => NamedKey::PageDown,
        keysyms::Page_Down => NamedKey::PageDown,
        keysyms::End => NamedKey::End,
        // keysyms::Begin => NamedKey::Begin,

        // Misc. functions
        keysyms::Select => NamedKey::Select,
        keysyms::Print => NamedKey::PrintScreen,
        keysyms::Execute => NamedKey::Execute,
        keysyms::Insert => NamedKey::Insert,
        keysyms::Undo => NamedKey::Undo,
        keysyms::Redo => NamedKey::Redo,
        keysyms::Menu => NamedKey::ContextMenu,
        keysyms::Find => NamedKey::Find,
        keysyms::Cancel => NamedKey::Cancel,
        keysyms::Help => NamedKey::Help,
        keysyms::Break => NamedKey::Pause,
        keysyms::Mode_switch => NamedKey::ModeChange,
        // keysyms::script_switch => NamedKey::ModeChange,
        keysyms::Num_Lock => NamedKey::NumLock,

        // Keypad keys
        // keysyms::KP_Space => return Key::Character(" "),
        keysyms::KP_Tab => NamedKey::Tab,
        keysyms::KP_Enter => NamedKey::Enter,
        keysyms::KP_F1 => NamedKey::F1,
        keysyms::KP_F2 => NamedKey::F2,
        keysyms::KP_F3 => NamedKey::F3,
        keysyms::KP_F4 => NamedKey::F4,
        keysyms::KP_Home => NamedKey::Home,
        keysyms::KP_Left => NamedKey::ArrowLeft,
        keysyms::KP_Up => NamedKey::ArrowUp,
        keysyms::KP_Right => NamedKey::ArrowRight,
        keysyms::KP_Down => NamedKey::ArrowDown,
        // keysyms::KP_Prior => NamedKey::PageUp,
        keysyms::KP_Page_Up => NamedKey::PageUp,
        // keysyms::KP_Next => NamedKey::PageDown,
        keysyms::KP_Page_Down => NamedKey::PageDown,
        keysyms::KP_End => NamedKey::End,
        // This is the key labeled "5" on the numpad when NumLock is off.
        // keysyms::KP_Begin => NamedKey::Begin,
        keysyms::KP_Insert => NamedKey::Insert,
        keysyms::KP_Delete => NamedKey::Delete,
        // keysyms::KP_Equal => NamedKey::Equal,
        // keysyms::KP_Multiply => NamedKey::Multiply,
        // keysyms::KP_Add => NamedKey::Add,
        // keysyms::KP_Separator => NamedKey::Separator,
        // keysyms::KP_Subtract => NamedKey::Subtract,
        // keysyms::KP_Decimal => NamedKey::Decimal,
        // keysyms::KP_Divide => NamedKey::Divide,

        // keysyms::KP_0 => return Key::Character("0"),
        // keysyms::KP_1 => return Key::Character("1"),
        // keysyms::KP_2 => return Key::Character("2"),
        // keysyms::KP_3 => return Key::Character("3"),
        // keysyms::KP_4 => return Key::Character("4"),
        // keysyms::KP_5 => return Key::Character("5"),
        // keysyms::KP_6 => return Key::Character("6"),
        // keysyms::KP_7 => return Key::Character("7"),
        // keysyms::KP_8 => return Key::Character("8"),
        // keysyms::KP_9 => return Key::Character("9"),

        // Function keys
        keysyms::F1 => NamedKey::F1,
        keysyms::F2 => NamedKey::F2,
        keysyms::F3 => NamedKey::F3,
        keysyms::F4 => NamedKey::F4,
        keysyms::F5 => NamedKey::F5,
        keysyms::F6 => NamedKey::F6,
        keysyms::F7 => NamedKey::F7,
        keysyms::F8 => NamedKey::F8,
        keysyms::F9 => NamedKey::F9,
        keysyms::F10 => NamedKey::F10,
        keysyms::F11 => NamedKey::F11,
        keysyms::F12 => NamedKey::F12,
        keysyms::F13 => NamedKey::F13,
        keysyms::F14 => NamedKey::F14,
        keysyms::F15 => NamedKey::F15,
        keysyms::F16 => NamedKey::F16,
        keysyms::F17 => NamedKey::F17,
        keysyms::F18 => NamedKey::F18,
        keysyms::F19 => NamedKey::F19,
        keysyms::F20 => NamedKey::F20,
        keysyms::F21 => NamedKey::F21,
        keysyms::F22 => NamedKey::F22,
        keysyms::F23 => NamedKey::F23,
        keysyms::F24 => NamedKey::F24,
        keysyms::F25 => NamedKey::F25,
        keysyms::F26 => NamedKey::F26,
        keysyms::F27 => NamedKey::F27,
        keysyms::F28 => NamedKey::F28,
        keysyms::F29 => NamedKey::F29,
        keysyms::F30 => NamedKey::F30,
        keysyms::F31 => NamedKey::F31,
        keysyms::F32 => NamedKey::F32,
        keysyms::F33 => NamedKey::F33,
        keysyms::F34 => NamedKey::F34,
        keysyms::F35 => NamedKey::F35,

        // Modifiers
        keysyms::Shift_L => NamedKey::Shift,
        keysyms::Shift_R => NamedKey::Shift,
        keysyms::Control_L => NamedKey::Control,
        keysyms::Control_R => NamedKey::Control,
        keysyms::Caps_Lock => NamedKey::CapsLock,
        // keysyms::Shift_Lock => NamedKey::ShiftLock,

        // keysyms::Meta_L => NamedKey::Meta,
        // keysyms::Meta_R => NamedKey::Meta,
        keysyms::Alt_L => NamedKey::Alt,
        keysyms::Alt_R => NamedKey::Alt,
        keysyms::Super_L => NamedKey::Super,
        keysyms::Super_R => NamedKey::Super,
        keysyms::Hyper_L => NamedKey::Hyper,
        keysyms::Hyper_R => NamedKey::Hyper,

        // XKB function and modifier keys
        // keysyms::ISO_Lock => NamedKey::IsoLock,
        // keysyms::ISO_Level2_Latch => NamedKey::IsoLevel2Latch,
        keysyms::ISO_Level3_Shift => NamedKey::AltGraph,
        keysyms::ISO_Level3_Latch => NamedKey::AltGraph,
        keysyms::ISO_Level3_Lock => NamedKey::AltGraph,
        // keysyms::ISO_Level5_Shift => NamedKey::IsoLevel5Shift,
        // keysyms::ISO_Level5_Latch => NamedKey::IsoLevel5Latch,
        // keysyms::ISO_Level5_Lock => NamedKey::IsoLevel5Lock,
        // keysyms::ISO_Group_Shift => NamedKey::IsoGroupShift,
        // keysyms::ISO_Group_Latch => NamedKey::IsoGroupLatch,
        // keysyms::ISO_Group_Lock => NamedKey::IsoGroupLock,
        keysyms::ISO_Next_Group => NamedKey::GroupNext,
        // keysyms::ISO_Next_Group_Lock => NamedKey::GroupNextLock,
        keysyms::ISO_Prev_Group => NamedKey::GroupPrevious,
        // keysyms::ISO_Prev_Group_Lock => NamedKey::GroupPreviousLock,
        keysyms::ISO_First_Group => NamedKey::GroupFirst,
        // keysyms::ISO_First_Group_Lock => NamedKey::GroupFirstLock,
        keysyms::ISO_Last_Group => NamedKey::GroupLast,
        // keysyms::ISO_Last_Group_Lock => NamedKey::GroupLastLock,
        keysyms::ISO_Left_Tab => NamedKey::Tab,
        // keysyms::ISO_Move_Line_Up => NamedKey::IsoMoveLineUp,
        // keysyms::ISO_Move_Line_Down => NamedKey::IsoMoveLineDown,
        // keysyms::ISO_Partial_Line_Up => NamedKey::IsoPartialLineUp,
        // keysyms::ISO_Partial_Line_Down => NamedKey::IsoPartialLineDown,
        // keysyms::ISO_Partial_Space_Left => NamedKey::IsoPartialSpaceLeft,
        // keysyms::ISO_Partial_Space_Right => NamedKey::IsoPartialSpaceRight,
        // keysyms::ISO_Set_Margin_Left => NamedKey::IsoSetMarginLeft,
        // keysyms::ISO_Set_Margin_Right => NamedKey::IsoSetMarginRight,
        // keysyms::ISO_Release_Margin_Left => NamedKey::IsoReleaseMarginLeft,
        // keysyms::ISO_Release_Margin_Right => NamedKey::IsoReleaseMarginRight,
        // keysyms::ISO_Release_Both_Margins => NamedKey::IsoReleaseBothMargins,
        // keysyms::ISO_Fast_Cursor_Left => NamedKey::IsoFastCursorLeft,
        // keysyms::ISO_Fast_Cursor_Right => NamedKey::IsoFastCursorRight,
        // keysyms::ISO_Fast_Cursor_Up => NamedKey::IsoFastCursorUp,
        // keysyms::ISO_Fast_Cursor_Down => NamedKey::IsoFastCursorDown,
        // keysyms::ISO_Continuous_Underline => NamedKey::IsoContinuousUnderline,
        // keysyms::ISO_Discontinuous_Underline => NamedKey::IsoDiscontinuousUnderline,
        // keysyms::ISO_Emphasize => NamedKey::IsoEmphasize,
        // keysyms::ISO_Center_Object => NamedKey::IsoCenterObject,
        keysyms::ISO_Enter => NamedKey::Enter,

        // dead_grave..dead_currency

        // dead_lowline..dead_longsolidusoverlay

        // dead_a..dead_capital_schwa

        // dead_greek

        // First_Virtual_Screen..Terminate_Server

        // AccessX_Enable..AudibleBell_Enable

        // Pointer_Left..Pointer_Drag5

        // Pointer_EnableKeys..Pointer_DfltBtnPrev

        // ch..C_H

        // 3270 terminal keys
        // keysyms::3270_Duplicate => NamedKey::Duplicate,
        // keysyms::3270_FieldMark => NamedKey::FieldMark,
        // keysyms::3270_Right2 => NamedKey::Right2,
        // keysyms::3270_Left2 => NamedKey::Left2,
        // keysyms::3270_BackTab => NamedKey::BackTab,
        keysyms::_3270_EraseEOF => NamedKey::EraseEof,
        // keysyms::3270_EraseInput => NamedKey::EraseInput,
        // keysyms::3270_Reset => NamedKey::Reset,
        // keysyms::3270_Quit => NamedKey::Quit,
        // keysyms::3270_PA1 => NamedKey::Pa1,
        // keysyms::3270_PA2 => NamedKey::Pa2,
        // keysyms::3270_PA3 => NamedKey::Pa3,
        // keysyms::3270_Test => NamedKey::Test,
        keysyms::_3270_Attn => NamedKey::Attn,
        // keysyms::3270_CursorBlink => NamedKey::CursorBlink,
        // keysyms::3270_AltCursor => NamedKey::AltCursor,
        // keysyms::3270_KeyClick => NamedKey::KeyClick,
        // keysyms::3270_Jump => NamedKey::Jump,
        // keysyms::3270_Ident => NamedKey::Ident,
        // keysyms::3270_Rule => NamedKey::Rule,
        // keysyms::3270_Copy => NamedKey::Copy,
        keysyms::_3270_Play => NamedKey::Play,
        // keysyms::3270_Setup => NamedKey::Setup,
        // keysyms::3270_Record => NamedKey::Record,
        // keysyms::3270_ChangeScreen => NamedKey::ChangeScreen,
        // keysyms::3270_DeleteWord => NamedKey::DeleteWord,
        keysyms::_3270_ExSelect => NamedKey::ExSel,
        keysyms::_3270_CursorSelect => NamedKey::CrSel,
        keysyms::_3270_PrintScreen => NamedKey::PrintScreen,
        keysyms::_3270_Enter => NamedKey::Enter,

        keysyms::space => NamedKey::Space,
        // exclam..Sinh_kunddaliya

        // XFree86
        // keysyms::XF86_ModeLock => NamedKey::ModeLock,

        // XFree86 - Backlight controls
        keysyms::XF86_MonBrightnessUp => NamedKey::BrightnessUp,
        keysyms::XF86_MonBrightnessDown => NamedKey::BrightnessDown,
        // keysyms::XF86_KbdLightOnOff => NamedKey::LightOnOff,
        // keysyms::XF86_KbdBrightnessUp => NamedKey::KeyboardBrightnessUp,
        // keysyms::XF86_KbdBrightnessDown => NamedKey::KeyboardBrightnessDown,

        // XFree86 - "Internet"
        keysyms::XF86_Standby => NamedKey::Standby,
        keysyms::XF86_AudioLowerVolume => NamedKey::AudioVolumeDown,
        keysyms::XF86_AudioRaiseVolume => NamedKey::AudioVolumeUp,
        keysyms::XF86_AudioPlay => NamedKey::MediaPlay,
        keysyms::XF86_AudioStop => NamedKey::MediaStop,
        keysyms::XF86_AudioPrev => NamedKey::MediaTrackPrevious,
        keysyms::XF86_AudioNext => NamedKey::MediaTrackNext,
        keysyms::XF86_HomePage => NamedKey::BrowserHome,
        keysyms::XF86_Mail => NamedKey::LaunchMail,
        // keysyms::XF86_Start => NamedKey::Start,
        keysyms::XF86_Search => NamedKey::BrowserSearch,
        keysyms::XF86_AudioRecord => NamedKey::MediaRecord,

        // XFree86 - PDA
        keysyms::XF86_Calculator => NamedKey::LaunchApplication2,
        // keysyms::XF86_Memo => NamedKey::Memo,
        // keysyms::XF86_ToDoList => NamedKey::ToDoList,
        keysyms::XF86_Calendar => NamedKey::LaunchCalendar,
        keysyms::XF86_PowerDown => NamedKey::Power,
        // keysyms::XF86_ContrastAdjust => NamedKey::AdjustContrast,
        // keysyms::XF86_RockerUp => NamedKey::RockerUp,
        // keysyms::XF86_RockerDown => NamedKey::RockerDown,
        // keysyms::XF86_RockerEnter => NamedKey::RockerEnter,

        // XFree86 - More "Internet"
        keysyms::XF86_Back => NamedKey::BrowserBack,
        keysyms::XF86_Forward => NamedKey::BrowserForward,
        // keysyms::XF86_Stop => NamedKey::Stop,
        keysyms::XF86_Refresh => NamedKey::BrowserRefresh,
        keysyms::XF86_PowerOff => NamedKey::Power,
        keysyms::XF86_WakeUp => NamedKey::WakeUp,
        keysyms::XF86_Eject => NamedKey::Eject,
        keysyms::XF86_ScreenSaver => NamedKey::LaunchScreenSaver,
        keysyms::XF86_WWW => NamedKey::LaunchWebBrowser,
        keysyms::XF86_Sleep => NamedKey::Standby,
        keysyms::XF86_Favorites => NamedKey::BrowserFavorites,
        keysyms::XF86_AudioPause => NamedKey::MediaPause,
        // keysyms::XF86_AudioMedia => NamedKey::AudioMedia,
        keysyms::XF86_MyComputer => NamedKey::LaunchApplication1,
        // keysyms::XF86_VendorHome => NamedKey::VendorHome,
        // keysyms::XF86_LightBulb => NamedKey::LightBulb,
        // keysyms::XF86_Shop => NamedKey::BrowserShop,
        // keysyms::XF86_History => NamedKey::BrowserHistory,
        // keysyms::XF86_OpenURL => NamedKey::OpenUrl,
        // keysyms::XF86_AddFavorite => NamedKey::AddFavorite,
        // keysyms::XF86_HotLinks => NamedKey::HotLinks,
        // keysyms::XF86_BrightnessAdjust => NamedKey::BrightnessAdjust,
        // keysyms::XF86_Finance => NamedKey::BrowserFinance,
        // keysyms::XF86_Community => NamedKey::BrowserCommunity,
        keysyms::XF86_AudioRewind => NamedKey::MediaRewind,
        // keysyms::XF86_BackForward => Key::???,
        // XF86_Launch0..XF86_LaunchF

        // XF86_ApplicationLeft..XF86_CD
        keysyms::XF86_Calculater => NamedKey::LaunchApplication2, // Nice typo, libxkbcommon :)
        // XF86_Clear
        keysyms::XF86_Close => NamedKey::Close,
        keysyms::XF86_Copy => NamedKey::Copy,
        keysyms::XF86_Cut => NamedKey::Cut,
        // XF86_Display..XF86_Documents
        keysyms::XF86_Excel => NamedKey::LaunchSpreadsheet,
        // XF86_Explorer..XF86iTouch
        keysyms::XF86_LogOff => NamedKey::LogOff,
        // XF86_Market..XF86_MenuPB
        keysyms::XF86_MySites => NamedKey::BrowserFavorites,
        keysyms::XF86_New => NamedKey::New,
        // XF86_News..XF86_OfficeHome
        keysyms::XF86_Open => NamedKey::Open,
        // XF86_Option
        keysyms::XF86_Paste => NamedKey::Paste,
        keysyms::XF86_Phone => NamedKey::LaunchPhone,
        // XF86_Q
        keysyms::XF86_Reply => NamedKey::MailReply,
        keysyms::XF86_Reload => NamedKey::BrowserRefresh,
        // XF86_RotateWindows..XF86_RotationKB
        keysyms::XF86_Save => NamedKey::Save,
        // XF86_ScrollUp..XF86_ScrollClick
        keysyms::XF86_Send => NamedKey::MailSend,
        keysyms::XF86_Spell => NamedKey::SpellCheck,
        keysyms::XF86_SplitScreen => NamedKey::SplitScreenToggle,
        // XF86_Support..XF86_User2KB
        keysyms::XF86_Video => NamedKey::LaunchMediaPlayer,
        // XF86_WheelButton
        keysyms::XF86_Word => NamedKey::LaunchWordProcessor,
        // XF86_Xfer
        keysyms::XF86_ZoomIn => NamedKey::ZoomIn,
        keysyms::XF86_ZoomOut => NamedKey::ZoomOut,

        // XF86_Away..XF86_Messenger
        keysyms::XF86_WebCam => NamedKey::LaunchWebCam,
        keysyms::XF86_MailForward => NamedKey::MailForward,
        // XF86_Pictures
        keysyms::XF86_Music => NamedKey::LaunchMusicPlayer,

        // XF86_Battery..XF86_UWB
        keysyms::XF86_AudioForward => NamedKey::MediaFastForward,
        // XF86_AudioRepeat
        keysyms::XF86_AudioRandomPlay => NamedKey::RandomToggle,
        keysyms::XF86_Subtitle => NamedKey::Subtitle,
        keysyms::XF86_AudioCycleTrack => NamedKey::MediaAudioTrack,
        // XF86_CycleAngle..XF86_Blue
        keysyms::XF86_Suspend => NamedKey::Standby,
        keysyms::XF86_Hibernate => NamedKey::Hibernate,
        // XF86_TouchpadToggle..XF86_TouchpadOff
        keysyms::XF86_AudioMute => NamedKey::AudioVolumeMute,

        // XF86_Switch_VT_1..XF86_Switch_VT_12

        // XF86_Ungrab..XF86_ClearGrab
        keysyms::XF86_Next_VMode => NamedKey::VideoModeNext,
        // keysyms::XF86_Prev_VMode => NamedKey::VideoModePrevious,
        // XF86_LogWindowTree..XF86_LogGrabInfo

        // SunFA_Grave..SunFA_Cedilla

        // keysyms::SunF36 => NamedKey::F36 | NamedKey::F11,
        // keysyms::SunF37 => NamedKey::F37 | NamedKey::F12,

        // keysyms::SunSys_Req => NamedKey::PrintScreen,
        // The next couple of xkb (until SunStop) are already handled.
        // SunPrint_Screen..SunPageDown

        // SunUndo..SunFront
        keysyms::SUN_Copy => NamedKey::Copy,
        keysyms::SUN_Open => NamedKey::Open,
        keysyms::SUN_Paste => NamedKey::Paste,
        keysyms::SUN_Cut => NamedKey::Cut,

        // SunPowerSwitch
        keysyms::SUN_AudioLowerVolume => NamedKey::AudioVolumeDown,
        keysyms::SUN_AudioMute => NamedKey::AudioVolumeMute,
        keysyms::SUN_AudioRaiseVolume => NamedKey::AudioVolumeUp,
        // SUN_VideoDegauss
        keysyms::SUN_VideoLowerBrightness => NamedKey::BrightnessDown,
        keysyms::SUN_VideoRaiseBrightness => NamedKey::BrightnessUp,
        // SunPowerSwitchShift
        0 => return Key::Unidentified(NativeKey::Unidentified),
        _ => return Key::Unidentified(NativeKey::Xkb(keysym)),
    })
}

pub fn keysym_location(keysym: u32) -> KeyLocation {
    use xkbcommon_dl::keysyms;
    match keysym {
        keysyms::Shift_L
        | keysyms::Control_L
        | keysyms::Meta_L
        | keysyms::Alt_L
        | keysyms::Super_L
        | keysyms::Hyper_L => KeyLocation::Left,
        keysyms::Shift_R
        | keysyms::Control_R
        | keysyms::Meta_R
        | keysyms::Alt_R
        | keysyms::Super_R
        | keysyms::Hyper_R => KeyLocation::Right,
        keysyms::KP_0
        | keysyms::KP_1
        | keysyms::KP_2
        | keysyms::KP_3
        | keysyms::KP_4
        | keysyms::KP_5
        | keysyms::KP_6
        | keysyms::KP_7
        | keysyms::KP_8
        | keysyms::KP_9
        | keysyms::KP_Space
        | keysyms::KP_Tab
        | keysyms::KP_Enter
        | keysyms::KP_F1
        | keysyms::KP_F2
        | keysyms::KP_F3
        | keysyms::KP_F4
        | keysyms::KP_Home
        | keysyms::KP_Left
        | keysyms::KP_Up
        | keysyms::KP_Right
        | keysyms::KP_Down
        | keysyms::KP_Page_Up
        | keysyms::KP_Page_Down
        | keysyms::KP_End
        | keysyms::KP_Begin
        | keysyms::KP_Insert
        | keysyms::KP_Delete
        | keysyms::KP_Equal
        | keysyms::KP_Multiply
        | keysyms::KP_Add
        | keysyms::KP_Separator
        | keysyms::KP_Subtract
        | keysyms::KP_Decimal
        | keysyms::KP_Divide => KeyLocation::Numpad,
        _ => KeyLocation::Standard,
    }
}

#[derive(Debug)]
pub struct XkbKeymap {
    keymap: NonNull<xkb_keymap>,
    _mods_indices: ModsIndices,
    pub _core_keyboard_id: i32,
}

impl XkbKeymap {
    #[cfg(wayland_platform)]
    pub fn from_fd(context: &XkbContext, fd: OwnedFd, size: usize) -> Option<Self> {
        let map = unsafe { MmapOptions::new().len(size).map_copy_read_only(&fd).ok()? };

        let keymap = unsafe {
            let keymap = (XKBH.xkb_keymap_new_from_string)(
                (*context).as_ptr(),
                map.as_ptr() as *const _,
                xkb::xkb_keymap_format::XKB_KEYMAP_FORMAT_TEXT_V1,
                xkb_keymap_compile_flags::XKB_KEYMAP_COMPILE_NO_FLAGS,
            );
            NonNull::new(keymap)?
        };

        Some(Self::new_inner(keymap, 0))
    }

    #[cfg(x11_platform)]
    pub fn from_x11_keymap(
        context: &XkbContext,
        xcb: *mut xcb_connection_t,
        core_keyboard_id: i32,
    ) -> Option<Self> {
        let keymap = unsafe {
            (XKBXH.xkb_x11_keymap_new_from_device)(
                context.as_ptr(),
                xcb,
                core_keyboard_id,
                xkb_keymap_compile_flags::XKB_KEYMAP_COMPILE_NO_FLAGS,
            )
        };
        let keymap = NonNull::new(keymap)?;
        Some(Self::new_inner(keymap, core_keyboard_id))
    }

    fn new_inner(keymap: NonNull<xkb_keymap>, _core_keyboard_id: i32) -> Self {
        let mods_indices = ModsIndices {
            shift: mod_index_for_name(keymap, xkb::XKB_MOD_NAME_SHIFT),
            caps: mod_index_for_name(keymap, xkb::XKB_MOD_NAME_CAPS),
            ctrl: mod_index_for_name(keymap, xkb::XKB_MOD_NAME_CTRL),
            alt: mod_index_for_name(keymap, xkb::XKB_MOD_NAME_ALT),
            num: mod_index_for_name(keymap, xkb::XKB_MOD_NAME_NUM),
            mod3: mod_index_for_name(keymap, b"Mod3\0"),
            logo: mod_index_for_name(keymap, xkb::XKB_MOD_NAME_LOGO),
            mod5: mod_index_for_name(keymap, b"Mod5\0"),
        };

        Self {
            keymap,
            _mods_indices: mods_indices,
            _core_keyboard_id,
        }
    }

    #[cfg(x11_platform)]
    pub fn mods_indices(&self) -> ModsIndices {
        self._mods_indices
    }

    pub fn first_keysym_by_level(
        &mut self,
        layout: xkb_layout_index_t,
        keycode: xkb_keycode_t,
    ) -> xkb_keysym_t {
        unsafe {
            let mut keysyms = ptr::null();
            let count = (XKBH.xkb_keymap_key_get_syms_by_level)(
                self.keymap.as_ptr(),
                keycode,
                layout,
                // NOTE: The level should be zero to ignore modifiers.
                0,
                &mut keysyms,
            );

            if count == 1 {
                *keysyms
            } else {
                0
            }
        }
    }

    /// Check whether the given key repeats.
    pub fn key_repeats(&mut self, keycode: xkb_keycode_t) -> bool {
        unsafe { (XKBH.xkb_keymap_key_repeats)(self.keymap.as_ptr(), keycode) == 1 }
    }
}

impl Drop for XkbKeymap {
    fn drop(&mut self) {
        unsafe {
            (XKBH.xkb_keymap_unref)(self.keymap.as_ptr());
        };
    }
}

impl Deref for XkbKeymap {
    type Target = NonNull<xkb_keymap>;

    fn deref(&self) -> &Self::Target {
        &self.keymap
    }
}

/// Modifier index in the keymap.
#[cfg_attr(not(x11_platform), allow(dead_code))]
#[derive(Default, Debug, Clone, Copy)]
pub struct ModsIndices {
    pub shift: Option<xkb_mod_index_t>,
    pub caps: Option<xkb_mod_index_t>,
    pub ctrl: Option<xkb_mod_index_t>,
    pub alt: Option<xkb_mod_index_t>,
    pub num: Option<xkb_mod_index_t>,
    pub mod3: Option<xkb_mod_index_t>,
    pub logo: Option<xkb_mod_index_t>,
    pub mod5: Option<xkb_mod_index_t>,
}

fn mod_index_for_name(
    keymap: NonNull<xkb_keymap>,
    name: &[u8],
) -> Option<xkb_mod_index_t> {
    unsafe {
        let mod_index = (XKBH.xkb_keymap_mod_get_index)(
            keymap.as_ptr(),
            name.as_ptr() as *const c_char,
        );
        if mod_index == XKB_MOD_INVALID {
            None
        } else {
            Some(mod_index)
        }
    }
}
