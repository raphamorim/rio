// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Originally retired from https://github.com/not-fl3/macroquad licensed under MIT
// https://github.com/not-fl3/macroquad/blob/master/LICENSE-MIT
// The code has suffered several changes like support to multiple windows, extension of windows
// properties, menu support, IME support, and etc.

use bitflags::bitflags;
use smol_str::SmolStr;

#[derive(Debug, Copy, Clone, PartialEq, Hash, Eq)]
pub enum MouseButton {
    Right,
    Left,
    Middle,
    Unknown,
}

#[derive(Clone, Debug, PartialEq)]
pub enum KeyAssignment {
    SpawnWindow,
    SpawnTab,
    Copy(String),
}

#[derive(Debug, Copy, Clone)]
pub struct Touch {
    pub id: u32,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Copy, Clone, PartialEq, Hash, Eq)]
pub enum KeyCode {
    Space,
    Apostrophe,
    Comma,
    Minus,
    Period,
    Slash,
    Key0,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    Semicolon,
    Equal,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    LeftBracket,
    Backslash,
    RightBracket,
    GraveAccent,
    World1,
    World2,
    Escape,
    Enter,
    Tab,
    Backspace,
    Insert,
    Delete,
    Right,
    Left,
    Down,
    Up,
    PageUp,
    PageDown,
    Home,
    End,
    CapsLock,
    ScrollLock,
    NumLock,
    PrintScreen,
    Pause,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    F25,
    Kp0,
    Kp1,
    Kp2,
    Kp3,
    Kp4,
    Kp5,
    Kp6,
    Kp7,
    Kp8,
    Kp9,
    KpDecimal,
    KpDivide,
    KpMultiply,
    KpSubtract,
    KpAdd,
    KpEnter,
    KpEqual,
    LeftShift,
    LeftControl,
    LeftAlt,
    LeftSuper,
    RightShift,
    RightControl,
    RightAlt,
    RightSuper,
    Menu,
    Unknown,
}

impl TryFrom<&str> for KeyCode {
    type Error = Box<dyn std::error::Error>;

    fn try_from(k: &str) -> Result<KeyCode, Self::Error> {
        let key = match k {
            "`" => KeyCode::Apostrophe,
            "0" => KeyCode::Key0,
            "1" => KeyCode::Key1,
            "2" => KeyCode::Key2,
            "3" => KeyCode::Key3,
            "4" => KeyCode::Key4,
            "5" => KeyCode::Key5,
            "6" => KeyCode::Key6,
            "7" => KeyCode::Key7,
            "8" => KeyCode::Key8,
            "9" => KeyCode::Key9,
            "-" => KeyCode::Minus,
            "=" => KeyCode::Equal,
            // "+" => KeyCode::Plus,
            "q" => KeyCode::Q,
            "w" => KeyCode::W,
            "e" => KeyCode::E,
            "r" => KeyCode::R,
            "t" => KeyCode::T,
            "y" => KeyCode::Y,
            "u" => KeyCode::U,
            "i" => KeyCode::I,
            "o" => KeyCode::O,
            "p" => KeyCode::P,
            "[" => KeyCode::LeftBracket,
            "]" => KeyCode::RightBracket,

            "a" => KeyCode::A,
            "s" => KeyCode::S,
            "d" => KeyCode::D,
            "f" => KeyCode::F,
            "g" => KeyCode::G,
            "h" => KeyCode::H,
            "j" => KeyCode::J,
            "k" => KeyCode::K,
            "l" => KeyCode::L,
            ";" => KeyCode::Semicolon,
            "\\" => KeyCode::Backslash,

            "z" => KeyCode::Z,
            "x" => KeyCode::X,
            "c" => KeyCode::C,
            "v" => KeyCode::V,
            "b" => KeyCode::B,
            "n" => KeyCode::N,
            "m" => KeyCode::M,
            "," => KeyCode::Comma,
            "." => KeyCode::Period,
            "/" => KeyCode::Slash,
            " " => KeyCode::Space,
            _ => return Err("Could not convert str to KeyCode".into()),
        };

        Ok(key)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub struct ModifiersState {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub logo: bool,
}

bitflags! {
    /// Represents the current state of the keyboard modifiers
    ///
    /// Each flag represents a modifier and is set if this modifier is active.
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Modifiers: u32 {
        /// The "shift" key.
        const SHIFT = 0b100;
        /// The "control" key.
        const CONTROL = 0b100 << 3;
        /// The "alt" key.
        const ALT = 0b100 << 6;
        /// This is the "windows" key on PC and "command" key on Mac.
        const SUPER = 0b100 << 9;
    }
}

impl From<ModifiersState> for Modifiers {
    fn from(mods: ModifiersState) -> Modifiers {
        let mut to_mods = Modifiers::empty();
        to_mods.set(Modifiers::SHIFT, mods.shift);
        to_mods.set(Modifiers::CONTROL, mods.control);
        to_mods.set(Modifiers::ALT, mods.alt);
        to_mods.set(Modifiers::SUPER, mods.logo);
        to_mods
    }
}

impl ModifiersState {
    pub fn is_empty(&self) -> bool {
        !self.shift && !self.control && !self.alt && !self.logo
    }

    pub fn empty() -> ModifiersState {
        ModifiersState::default()
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum TouchPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

pub enum EventHandlerAction {
    Init,
    Noop,
    Quit,
}

// -> Press "`" key
// Ime::Preedit("`", Some((0, 0)))
// -> Press "E" key
// Ime::Preedit("", None) // Synthetic event generated to clear preedit
// Ime::Commit("è")
pub enum ImeState {
    Preedit(String, Option<(usize, usize)>),
    Commit(String),
    Disabled,
    Enabled,
}

pub enum DragState {
    Entered,
    Exited,
}

pub enum Appearance {
    Dark,
    Light,
    LightHighContrast,
    DarkHighContrast,
}

#[derive(PartialEq, Debug)]
pub enum EventHandlerControl {
    Wait,
    WaitUntil(std::time::Instant),
    Running,
}

/// A trait defining event callbacks.
pub trait EventHandler {
    fn create_window(&mut self);
    fn create_tab(&mut self, _urls_to_load: Option<&str>);
    fn start(&mut self);
    fn process(&mut self) -> EventHandlerControl;
    // #[allow(clippy::too_many_arguments)]
    // fn init(
    //     &mut self,
    //     _id: u16,
    //     _raw_window_handle: raw_window_handle::RawWindowHandle,
    //     _raw_display_handle: raw_window_handle::RawDisplayHandle,
    //     _w: i32,
    //     _h: i32,
    //     _s: f32,
    //     _open_url: &str,
    // ) {
    // }
    fn resize_event(&mut self, _id: u16, _w: i32, _h: i32, _s: f32, _rescale: bool) {}
    fn ime_event(&mut self, _id: u16, _ime: ImeState) {}
    fn mouse_motion_event(&mut self, _id: u16, _x: f32, _y: f32) {}
    fn mouse_wheel_event(&mut self, _id: u16, _x: f32, _y: f32) {}
    fn mouse_button_down_event(
        &mut self,
        _id: u16,
        _button: MouseButton,
        _x: f32,
        _y: f32,
    ) {
    }
    fn mouse_button_up_event(
        &mut self,
        _id: u16,
        _button: MouseButton,
        _x: f32,
        _y: f32,
    ) {
    }
    fn appearance_change_event(&mut self, _id: u16, _a: Appearance) {}

    // fn char_event(&mut self, _character: char, _mods: ModifiersState, _repeat: bool) {}
    fn key_down_event(
        &mut self,
        _id: u16,
        _keycode: KeyCode,
        _repeat: bool,
        _text: Option<SmolStr>,
    ) {
    }
    fn key_up_event(&mut self, _id: u16, _keycode: KeyCode) {}
    fn modifiers_event(
        &mut self,
        _id: u16,
        _keycode: Option<KeyCode>,
        _mods: ModifiersState,
    ) {
    }

    fn focus_event(&mut self, _id: u16, _focused: bool) {}

    /// Default implementation emulates mouse clicks
    fn touch_event(
        &mut self,
        window_id: u16,
        phase: TouchPhase,
        _id: u64,
        x: f32,
        y: f32,
    ) {
        if phase == TouchPhase::Started {
            self.mouse_button_down_event(window_id, MouseButton::Left, x, y);
        }

        if phase == TouchPhase::Ended {
            self.mouse_button_up_event(window_id, MouseButton::Left, x, y);
        }

        if phase == TouchPhase::Moved {
            self.mouse_motion_event(window_id, x, y);
        }
    }

    fn open_file_event(&mut self, _id: u16, _filename: String) {}
    // fn open_url_event(&mut self, _id: u16, _urls: &str) {}

    /// Represents raw hardware mouse motion event
    /// Note that these events are delivered regardless of input focus and not in pixels, but in
    /// hardware units instead. And those units may be different from pixels depending on the target platform
    fn raw_mouse_motion(&mut self, _id: u16, _dx: f32, _dy: f32) {}

    /// Window has been minimized
    /// Right now is only implemented on Android, X11 and wasm,
    /// On Andoid window_minimized_event is called on a Pause ndk callback
    /// On X11 and wasm it will be called on focus change events.
    fn window_minimized_event(&mut self) {}

    /// Window has been restored
    /// Right now is only implemented on Android, X11 and wasm,
    /// On Andoid window_minimized_event is called on a Pause ndk callback
    /// On X11 and wasm it will be called on focus change events.
    fn window_restored_event(&mut self) {}

    /// This event is sent when the userclicks the window's close button
    /// or application code calls the ctx.request_quit() function. The event
    /// handler callback code can handle this event by calling
    /// ctx.cancel_quit() to cancel the quit.
    /// If the event is ignored, the application will quit as usual.
    fn quit_requested_event(&mut self) {}

    /// A file has been dropped over the application.
    fn files_dropped_event(&mut self, _id: u16, _filepaths: Vec<std::path::PathBuf>) {}

    /// A file has been dragged over the application..
    fn files_dragged_event(
        &mut self,
        _id: u16,
        _filepaths: Vec<std::path::PathBuf>,
        _drag_state: DragState,
    ) {
    }
}

pub enum WindowEvent {
    Focus(bool),
    MouseMotion(f32, f32),
    Resize(i32, i32, f32, bool),
}

pub enum QueuedEvent {
    Window(u16, WindowEvent),
}
