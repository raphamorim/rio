// Originally retired from https://github.com/not-fl3/macroquad licensed under MIT (https://github.com/not-fl3/macroquad/blob/master/LICENSE-MIT) and slightly modified

#[derive(Debug, Copy, Clone, PartialEq, Hash, Eq)]
pub enum MouseButton {
    Right,
    Left,
    Middle,
    Unknown,
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

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct KeyMods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub logo: bool,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum TouchPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

/// A trait defining event callbacks.
pub trait EventHandler {
    fn update(&mut self);
    fn draw(&mut self);
    fn resize_event(&mut self, _width: f32, _height: f32) {}
    fn mouse_motion_event(&mut self, _x: f32, _y: f32) {}
    fn mouse_wheel_event(&mut self, _x: f32, _y: f32) {}
    fn mouse_button_down_event(&mut self, _button: MouseButton, _x: f32, _y: f32) {}
    fn mouse_button_up_event(&mut self, _button: MouseButton, _x: f32, _y: f32) {}

    fn char_event(&mut self, _character: char, _keymods: KeyMods, _repeat: bool) {}
    fn key_down_event(&mut self, _keycode: KeyCode, _keymods: KeyMods, _repeat: bool) {}
    fn key_up_event(&mut self, _keycode: KeyCode, _keymods: KeyMods) {}

    /// Default implementation emulates mouse clicks
    fn touch_event(&mut self, phase: TouchPhase, _id: u64, x: f32, y: f32) {
        if phase == TouchPhase::Started {
            self.mouse_button_down_event(MouseButton::Left, x, y);
        }

        if phase == TouchPhase::Ended {
            self.mouse_button_up_event(MouseButton::Left, x, y);
        }

        if phase == TouchPhase::Moved {
            self.mouse_motion_event(x, y);
        }
    }

    /// Represents raw hardware mouse motion event
    /// Note that these events are delivered regardless of input focus and not in pixels, but in
    /// hardware units instead. And those units may be different from pixels depending on the target platform
    fn raw_mouse_motion(&mut self, _dx: f32, _dy: f32) {}

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
    /// Applications can request the number of dropped files with
    /// `ctx.dropped_file_count()`, path of an individual file with
    /// `ctx.dropped_file_path()`, and for wasm targets the file bytes
    /// can be requested with `ctx.dropped_file_bytes()`.
    fn files_dropped_event(&mut self) {}
}
