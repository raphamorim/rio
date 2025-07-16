//! The [`Event`] enum and assorted supporting types.
//!
//! These are sent to the closure given to [`EventLoop::run_app(...)`], where they get
//! processed and used to modify the program state. For more details, see the root-level
//! documentation.
//!
//! Some of these events represent different "parts" of a traditional event-handling loop. You could
//! approximate the basic ordering loop of [`EventLoop::run_app(...)`] like this:
//!
//! ```rust,ignore
//! let mut start_cause = StartCause::Init;
//!
//! while !elwt.exiting() {
//!     app.new_events(event_loop, start_cause);
//!
//!     for event in (window events, user events, device events) {
//!         // This will pick the right method on the application based on the event.
//!         app.handle_event(event_loop, event);
//!     }
//!
//!     for window_id in (redraw windows) {
//!         app.window_event(event_loop, window_id, RedrawRequested);
//!     }
//!
//!     app.about_to_wait(event_loop);
//!     start_cause = wait_if_necessary();
//! }
//!
//! app.exiting(event_loop);
//! ```
//!
//! This leaves out timing details like [`ControlFlow::WaitUntil`] but hopefully
//! describes what happens in what order.
//!
//! [`EventLoop::run_app(...)`]: crate::event_loop::EventLoop::run_app
//! [`ControlFlow::WaitUntil`]: crate::event_loop::ControlFlow::WaitUntil
use std::path::PathBuf;
use std::sync::{Mutex, Weak};
#[cfg(not(web_platform))]
use std::time::Instant;

use smol_str::SmolStr;
#[cfg(web_platform)]
use web_time::Instant;

use crate::dpi::{PhysicalPosition, PhysicalSize};
use crate::error::ExternalError;
use crate::event_loop::AsyncRequestSerial;
use crate::keyboard::{self, ModifiersKeyState, ModifiersKeys, ModifiersState};
use crate::platform_impl;
#[cfg(doc)]
use crate::window::Window;
use crate::window::{ActivationToken, Theme, WindowId};

/// Describes a generic event.
///
/// See the module-level docs for more information on the event loop manages each event.
#[derive(Debug, Clone, PartialEq)]
pub enum Event<T: 'static> {
    /// See [`ApplicationHandler::new_events`] for details.
    ///
    /// [`ApplicationHandler::new_events`]: crate::application::ApplicationHandler::new_events
    NewEvents(StartCause),

    /// See [`ApplicationHandler::window_event`] for details.
    ///
    /// [`ApplicationHandler::window_event`]: crate::application::ApplicationHandler::window_event
    WindowEvent {
        window_id: WindowId,
        event: WindowEvent,
    },

    /// See [`ApplicationHandler::device_event`] for details.
    ///
    /// [`ApplicationHandler::device_event`]: crate::application::ApplicationHandler::device_event
    DeviceEvent {
        device_id: DeviceId,
        event: DeviceEvent,
    },

    /// See [`ApplicationHandler::user_event`] for details.
    ///
    /// [`ApplicationHandler::user_event`]: crate::application::ApplicationHandler::user_event
    UserEvent(T),

    /// See [`ApplicationHandler::suspended`] for details.
    ///
    /// [`ApplicationHandler::suspended`]: crate::application::ApplicationHandler::suspended
    Suspended,

    /// See [`ApplicationHandler::resumed`] for details.
    ///
    /// [`ApplicationHandler::resumed`]: crate::application::ApplicationHandler::resumed
    Resumed,

    /// See [`ApplicationHandler::about_to_wait`] for details.
    ///
    /// [`ApplicationHandler::about_to_wait`]: crate::application::ApplicationHandler::about_to_wait
    AboutToWait,

    /// See [`ApplicationHandler::exiting`] for details.
    ///
    /// [`ApplicationHandler::exiting`]: crate::application::ApplicationHandler::exiting
    LoopExiting,

    /// See [`ApplicationHandler::memory_warning`] for details.
    ///
    /// [`ApplicationHandler::memory_warning`]: crate::application::ApplicationHandler::memory_warning
    MemoryWarning,

    Opened {
        urls: Vec<String>,
    },

    OpenConfig,
    HookEvent(KeyEvent, Modifiers),
}

impl<T> Event<T> {
    #[allow(clippy::result_large_err)]
    pub fn map_nonuser_event<U>(self) -> Result<Event<U>, Event<T>> {
        use self::Event::*;
        match self {
            UserEvent(_) => Err(self),
            WindowEvent { window_id, event } => Ok(WindowEvent { window_id, event }),
            DeviceEvent { device_id, event } => Ok(DeviceEvent { device_id, event }),
            NewEvents(cause) => Ok(NewEvents(cause)),
            AboutToWait => Ok(AboutToWait),
            LoopExiting => Ok(LoopExiting),
            Suspended => Ok(Suspended),
            Resumed => Ok(Resumed),
            MemoryWarning => Ok(MemoryWarning),
            Opened { urls } => Ok(Opened { urls }),
            OpenConfig => Ok(OpenConfig),
            HookEvent(hook, modifiers) => Ok(HookEvent(hook, modifiers)),
        }
    }
}

/// Describes the reason the event loop is resuming.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartCause {
    /// Sent if the time specified by [`ControlFlow::WaitUntil`] has been reached. Contains the
    /// moment the timeout was requested and the requested resume time. The actual resume time is
    /// guaranteed to be equal to or after the requested resume time.
    ///
    /// [`ControlFlow::WaitUntil`]: crate::event_loop::ControlFlow::WaitUntil
    ResumeTimeReached {
        start: Instant,
        requested_resume: Instant,
    },

    /// Sent if the OS has new events to send to the window, after a wait was requested. Contains
    /// the moment the wait was requested and the resume time, if requested.
    WaitCancelled {
        start: Instant,
        requested_resume: Option<Instant>,
    },

    /// Sent if the event loop is being resumed after the loop's control flow was set to
    /// [`ControlFlow::Poll`].
    ///
    /// [`ControlFlow::Poll`]: crate::event_loop::ControlFlow::Poll
    Poll,

    /// Sent once, immediately after `run` is called. Indicates that the loop was just initialized.
    Init,

    /// Menu or dock can trigger it
    CreateWindow,

    /// Macos only, executed once app is requesting a reopen
    MacOSReopen,
}

/// Describes an event from a [`Window`].
#[derive(Debug, Clone, PartialEq)]
pub enum WindowEvent {
    /// The activation token was delivered back and now could be used.
    #[cfg_attr(
        not(any(x11_platform, wayland_platform)),
        allow(rustdoc::broken_intra_doc_links)
    )]
    /// Delivered in response to [`request_activation_token`].
    ///
    /// [`request_activation_token`]: crate::platform::startup_notify::WindowExtStartupNotify::request_activation_token
    ActivationTokenDone {
        serial: AsyncRequestSerial,
        token: ActivationToken,
    },

    /// The size of the window has changed. Contains the client area's new dimensions.
    Resized(PhysicalSize<u32>),

    /// The position of the window has changed. Contains the window's new position.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / Web / Wayland:** Unsupported.
    Moved(PhysicalPosition<i32>),

    /// The window has been requested to close.
    CloseRequested,

    /// The window has been destroyed.
    Destroyed,

    /// A file has been dropped into the window.
    ///
    /// When the user drops multiple files at once, this event will be emitted for each file
    /// separately.
    DroppedFile(PathBuf),

    /// A file is being hovered over the window.
    ///
    /// When the user hovers multiple files at once, this event will be emitted for each file
    /// separately.
    HoveredFile(PathBuf),

    /// A file was hovered, but has exited the window.
    ///
    /// There will be a single `HoveredFileCancelled` event triggered even if multiple files were
    /// hovered.
    HoveredFileCancelled,

    /// The window gained or lost focus.
    ///
    /// The parameter is true if the window has gained focus, and false if it has lost focus.
    Focused(bool),

    /// An event from the keyboard has been received.
    ///
    /// ## Platform-specific
    /// - **Windows:** The shift key overrides NumLock. In other words, while shift is held down,
    ///   numpad keys act as if NumLock wasn't active. When this is used, the OS sends fake key
    ///   events which are not marked as `is_synthetic`.
    KeyboardInput {
        device_id: DeviceId,
        event: KeyEvent,

        /// If `true`, the event was generated synthetically by winit
        /// in one of the following circumstances:
        ///
        /// * Synthetic key press events are generated for all keys pressed when a window gains
        ///   focus. Likewise, synthetic key release events are generated for all keys pressed when
        ///   a window goes out of focus. ***Currently, this is only functional on X11 and
        ///   Windows***
        ///
        /// Otherwise, this value is always `false`.
        is_synthetic: bool,
    },

    /// The keyboard modifiers have changed.
    ModifiersChanged(Modifiers),

    /// An event from an input method.
    ///
    /// **Note:** You have to explicitly enable this event using [`Window::set_ime_allowed`].
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / Web / Orbital:** Unsupported.
    Ime(Ime),

    /// The cursor has moved on the window.
    ///
    /// ## Platform-specific
    ///
    /// - **Web:** Doesn't take into account CSS [`border`], [`padding`], or [`transform`].
    ///
    /// [`border`]: https://developer.mozilla.org/en-US/docs/Web/CSS/border
    /// [`padding`]: https://developer.mozilla.org/en-US/docs/Web/CSS/padding
    /// [`transform`]: https://developer.mozilla.org/en-US/docs/Web/CSS/transform
    CursorMoved {
        device_id: DeviceId,

        /// (x,y) coords in pixels relative to the top-left corner of the window. Because the range
        /// of this data is limited by the display area and it may have been transformed by
        /// the OS to implement effects such as cursor acceleration, it should not be used
        /// to implement non-cursor-like interactions such as 3D camera control.
        position: PhysicalPosition<f64>,
    },

    /// The cursor has entered the window.
    ///
    /// ## Platform-specific
    ///
    /// - **Web:** Doesn't take into account CSS [`border`], [`padding`], or [`transform`].
    ///
    /// [`border`]: https://developer.mozilla.org/en-US/docs/Web/CSS/border
    /// [`padding`]: https://developer.mozilla.org/en-US/docs/Web/CSS/padding
    /// [`transform`]: https://developer.mozilla.org/en-US/docs/Web/CSS/transform
    CursorEntered { device_id: DeviceId },

    /// The cursor has left the window.
    ///
    /// ## Platform-specific
    ///
    /// - **Web:** Doesn't take into account CSS [`border`], [`padding`], or [`transform`].
    ///
    /// [`border`]: https://developer.mozilla.org/en-US/docs/Web/CSS/border
    /// [`padding`]: https://developer.mozilla.org/en-US/docs/Web/CSS/padding
    /// [`transform`]: https://developer.mozilla.org/en-US/docs/Web/CSS/transform
    CursorLeft { device_id: DeviceId },

    /// A mouse wheel movement or touchpad scroll occurred.
    MouseWheel {
        device_id: DeviceId,
        delta: MouseScrollDelta,
        phase: TouchPhase,
    },

    /// An mouse button press has been received.
    MouseInput {
        device_id: DeviceId,
        state: ElementState,
        button: MouseButton,
    },

    /// Two-finger pinch gesture, often used for magnification.
    ///
    /// ## Platform-specific
    ///
    /// - Only available on **macOS** and **iOS**.
    /// - On iOS, not recognized by default. It must be enabled when needed.
    PinchGesture {
        device_id: DeviceId,
        /// Positive values indicate magnification (zooming in) and  negative
        /// values indicate shrinking (zooming out).
        ///
        /// This value may be NaN.
        delta: f64,
        phase: TouchPhase,
    },

    /// N-finger pan gesture
    ///
    /// ## Platform-specific
    ///
    /// - Only available on **iOS**.
    /// - On iOS, not recognized by default. It must be enabled when needed.
    PanGesture {
        device_id: DeviceId,
        /// Change in pixels of pan gesture from last update.
        delta: PhysicalPosition<f32>,
        phase: TouchPhase,
    },

    /// Double tap gesture.
    ///
    /// On a Mac, smart magnification is triggered by a double tap with two fingers
    /// on the trackpad and is commonly used to zoom on a certain object
    /// (e.g. a paragraph of a PDF) or (sort of like a toggle) to reset any zoom.
    /// The gesture is also supported in Safari, Pages, etc.
    ///
    /// The event is general enough that its generating gesture is allowed to vary
    /// across platforms. It could also be generated by another device.
    ///
    /// Unfortunately, neither [Windows](https://support.microsoft.com/en-us/windows/touch-gestures-for-windows-a9d28305-4818-a5df-4e2b-e5590f850741)
    /// nor [Wayland](https://wayland.freedesktop.org/libinput/doc/latest/gestures.html)
    /// support this gesture or any other gesture with the same effect.
    ///
    /// ## Platform-specific
    ///
    /// - Only available on **macOS 10.8** and later, and **iOS**.
    /// - On iOS, not recognized by default. It must be enabled when needed.
    DoubleTapGesture { device_id: DeviceId },

    /// Two-finger rotation gesture.
    ///
    /// Positive delta values indicate rotation counterclockwise and
    /// negative delta values indicate rotation clockwise.
    ///
    /// ## Platform-specific
    ///
    /// - Only available on **macOS** and **iOS**.
    /// - On iOS, not recognized by default. It must be enabled when needed.
    RotationGesture {
        device_id: DeviceId,
        /// change in rotation in degrees
        delta: f32,
        phase: TouchPhase,
    },

    /// Touchpad pressure event.
    ///
    /// At the moment, only supported on Apple forcetouch-capable macbooks.
    /// The parameters are: pressure level (value between 0 and 1 representing how hard the
    /// touchpad is being pressed) and stage (integer representing the click level).
    TouchpadPressure {
        device_id: DeviceId,
        pressure: f32,
        stage: i64,
    },

    /// Motion on some analog axis. May report data redundant to other, more specific events.
    AxisMotion {
        device_id: DeviceId,
        axis: AxisId,
        value: f64,
    },

    /// Touch event has been received
    ///
    /// ## Platform-specific
    ///
    /// - **Web:** Doesn't take into account CSS [`border`], [`padding`], or [`transform`].
    /// - **macOS:** Unsupported.
    ///
    /// [`border`]: https://developer.mozilla.org/en-US/docs/Web/CSS/border
    /// [`padding`]: https://developer.mozilla.org/en-US/docs/Web/CSS/padding
    /// [`transform`]: https://developer.mozilla.org/en-US/docs/Web/CSS/transform
    Touch(Touch),

    /// The window's scale factor has changed.
    ///
    /// The following user actions can cause DPI changes:
    ///
    /// * Changing the display's resolution.
    /// * Changing the display's scale factor (e.g. in Control Panel on Windows).
    /// * Moving the window to a display with a different scale factor.
    ///
    /// To update the window size, use the provided [`InnerSizeWriter`] handle. By default, the
    /// window is resized to the value suggested by the OS, but it can be changed to any value.
    ///
    /// For more information about DPI in general, see the [`dpi`] crate.
    ScaleFactorChanged {
        scale_factor: f64,
        /// Handle to update inner size during scale changes.
        ///
        /// See [`InnerSizeWriter`] docs for more details.
        inner_size_writer: InnerSizeWriter,
    },

    /// The system window theme has changed.
    ///
    /// Applications might wish to react to this to change the theme of the content of the window
    /// when the system changes the window theme.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / X11 / Wayland / Orbital:** Unsupported.
    ThemeChanged(Theme),

    /// The window has been occluded (completely hidden from view).
    ///
    /// This is different to window visibility as it depends on whether the window is closed,
    /// minimised, set invisible, or fully occluded by another window.
    ///
    /// ## Platform-specific
    ///
    /// ### iOS
    ///
    /// On iOS, the `Occluded(false)` event is emitted in response to an
    /// [`applicationWillEnterForeground`] callback which means the application should start
    /// preparing its data. The `Occluded(true)` event is emitted in response to an
    /// [`applicationDidEnterBackground`] callback which means the application should free
    /// resources (according to the [iOS application lifecycle]).
    ///
    /// [`applicationWillEnterForeground`]: https://developer.apple.com/documentation/uikit/uiapplicationdelegate/1623076-applicationwillenterforeground
    /// [`applicationDidEnterBackground`]: https://developer.apple.com/documentation/uikit/uiapplicationdelegate/1622997-applicationdidenterbackground
    /// [iOS application lifecycle]: https://developer.apple.com/documentation/uikit/app_and_environment/managing_your_app_s_life_cycle
    ///
    /// ### Others
    ///
    /// - **Web:** Doesn't take into account CSS [`border`], [`padding`], or [`transform`].
    /// - **Android / Wayland / Windows / Orbital:** Unsupported.
    ///
    /// [`border`]: https://developer.mozilla.org/en-US/docs/Web/CSS/border
    /// [`padding`]: https://developer.mozilla.org/en-US/docs/Web/CSS/padding
    /// [`transform`]: https://developer.mozilla.org/en-US/docs/Web/CSS/transform
    Occluded(bool),

    /// Emitted when a window should be redrawn.
    ///
    /// This gets triggered in two scenarios:
    /// - The OS has performed an operation that's invalidated the window's contents (such as
    ///   resizing the window).
    /// - The application has explicitly requested a redraw via [`Window::request_redraw`].
    ///
    /// Winit will aggregate duplicate redraw requests into a single event, to
    /// help avoid duplicating rendering work.
    RedrawRequested,
}

/// Identifier of an input device.
///
/// Whenever you receive an event arising from a particular input device, this event contains a
/// `DeviceId` which identifies its origin. Note that devices may be virtual (representing an
/// on-screen cursor and keyboard focus) or physical. Virtual devices typically aggregate inputs
/// from multiple physical devices.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(pub(crate) platform_impl::DeviceId);

impl DeviceId {
    /// Returns a dummy id, useful for unit testing.
    ///
    /// # Safety
    ///
    /// The only guarantee made about the return value of this function is that
    /// it will always be equal to itself and to future values returned by this function.
    /// No other guarantees are made. This may be equal to a real `DeviceId`.
    ///
    /// **Passing this into a winit function will result in undefined behavior.**
    pub const unsafe fn dummy() -> Self {
        #[allow(unused_unsafe)]
        DeviceId(unsafe { platform_impl::DeviceId::dummy() })
    }
}

/// Represents raw hardware events that are not associated with any particular window.
///
/// Useful for interactions that diverge significantly from a conventional 2D GUI, such as 3D camera
/// or first-person game controls. Many physical actions, such as mouse movement, can produce both
/// device and window events. Because window events typically arise from virtual devices
/// (corresponding to GUI cursors and keyboard focus) the device IDs may not match.
///
/// Note that these events are delivered regardless of input focus.
#[derive(Clone, Debug, PartialEq)]
pub enum DeviceEvent {
    Added,
    Removed,

    /// Change in physical position of a pointing device.
    ///
    /// This represents raw, unfiltered physical motion. Not to be confused with
    /// [`WindowEvent::CursorMoved`].
    MouseMotion {
        /// (x, y) change in position in unspecified units.
        ///
        /// Different devices may use different units.
        delta: (f64, f64),
    },

    /// Physical scroll event
    MouseWheel {
        delta: MouseScrollDelta,
    },

    /// Motion on some analog axis. This event will be reported for all arbitrary input devices
    /// that winit supports on this platform, including mouse devices.  If the device is a mouse
    /// device then this will be reported alongside the MouseMotion event.
    Motion {
        axis: AxisId,
        value: f64,
    },

    Button {
        button: ButtonId,
        state: ElementState,
    },

    Key(RawKeyEvent),
}

/// Describes a keyboard input as a raw device event.
///
/// Note that holding down a key may produce repeated `RawKeyEvent`s. The
/// operating system doesn't provide information whether such an event is a
/// repeat or the initial keypress. An application may emulate this by, for
/// example keeping a Map/Set of pressed keys and determining whether a keypress
/// corresponds to an already pressed key.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct RawKeyEvent {
    pub physical_key: keyboard::PhysicalKey,
    pub state: ElementState,
}

/// Describes a keyboard input targeting a window.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct KeyEvent {
    /// Represents the position of a key independent of the currently active layout.
    ///
    /// It also uniquely identifies the physical key (i.e. it's mostly synonymous with a scancode).
    /// The most prevalent use case for this is games. For example the default keys for the player
    /// to move around might be the W, A, S, and D keys on a US layout. The position of these keys
    /// is more important than their label, so they should map to Z, Q, S, and D on an "AZERTY"
    /// layout. (This value is `KeyCode::KeyW` for the Z key on an AZERTY layout.)
    ///
    /// ## Caveats
    ///
    /// - Certain niche hardware will shuffle around physical key positions, e.g. a keyboard that
    ///   implements DVORAK in hardware (or firmware)
    /// - Your application will likely have to handle keyboards which are missing keys that your
    ///   own keyboard has.
    /// - Certain `KeyCode`s will move between a couple of different positions depending on what
    ///   layout the keyboard was manufactured to support.
    ///
    ///  **Because of these caveats, it is important that you provide users with a way to configure
    ///  most (if not all) keybinds in your application.**
    ///
    /// ## `Fn` and `FnLock`
    ///
    /// `Fn` and `FnLock` key events are *exceedingly unlikely* to be emitted by Winit. These keys
    /// are usually handled at the hardware or OS level, and aren't surfaced to applications. If
    /// you somehow see this in the wild, we'd like to know :)
    pub physical_key: keyboard::PhysicalKey,

    // Allowing `broken_intra_doc_links` for `logical_key`, because
    // `key_without_modifiers` is not available on all platforms
    #[cfg_attr(
        not(any(windows_platform, macos_platform, x11_platform, wayland_platform)),
        allow(rustdoc::broken_intra_doc_links)
    )]
    /// This value is affected by all modifiers except <kbd>Ctrl</kbd>.
    ///
    /// This has two use cases:
    /// - Allows querying whether the current input is a Dead key.
    /// - Allows handling key-bindings on platforms which don't
    ///   support [`key_without_modifiers`].
    ///
    /// If you use this field (or [`key_without_modifiers`] for that matter) for keyboard
    /// shortcuts, **it is important that you provide users with a way to configure your
    /// application's shortcuts so you don't render your application unusable for users with an
    /// incompatible keyboard layout.**
    ///
    /// ## Platform-specific
    /// - **Web:** Dead keys might be reported as the real key instead
    ///   of `Dead` depending on the browser/OS.
    ///
    /// [`key_without_modifiers`]: crate::platform::modifier_supplement::KeyEventExtModifierSupplement::key_without_modifiers
    pub logical_key: keyboard::Key,

    /// Contains the text produced by this keypress.
    ///
    /// In most cases this is identical to the content
    /// of the `Character` variant of `logical_key`.
    /// However, on Windows when a dead key was pressed earlier
    /// but cannot be combined with the character from this
    /// keypress, the produced text will consist of two characters:
    /// the dead-key-character followed by the character resulting
    /// from this keypress.
    ///
    /// An additional difference from `logical_key` is that
    /// this field stores the text representation of any key
    /// that has such a representation. For example when
    /// `logical_key` is `Key::Named(NamedKey::Enter)`, this field is `Some("\r")`.
    ///
    /// This is `None` if the current keypress cannot
    /// be interpreted as text.
    ///
    /// See also: `text_with_all_modifiers()`
    pub text: Option<SmolStr>,

    /// Contains the location of this key on the keyboard.
    ///
    /// Certain keys on the keyboard may appear in more than once place. For example, the "Shift"
    /// key appears on the left side of the QWERTY keyboard as well as the right side. However,
    /// both keys have the same symbolic value. Another example of this phenomenon is the "1"
    /// key, which appears both above the "Q" key and as the "Keypad 1" key.
    ///
    /// This field allows the user to differentiate between keys like this that have the same
    /// symbolic value but different locations on the keyboard.
    ///
    /// See the [`KeyLocation`] type for more details.
    ///
    /// [`KeyLocation`]: crate::keyboard::KeyLocation
    pub location: keyboard::KeyLocation,

    /// Whether the key is being pressed or released.
    ///
    /// See the [`ElementState`] type for more details.
    pub state: ElementState,

    /// Whether or not this key is a key repeat event.
    ///
    /// On some systems, holding down a key for some period of time causes that key to be repeated
    /// as though it were being pressed and released repeatedly. This field is `true` if and only
    /// if this event is the result of one of those repeats.
    ///
    /// # Example
    ///
    /// In games, you often want to ignore repated key events - this can be
    /// done by ignoring events where this property is set.
    ///
    /// ```
    /// use rio_window::event::{ElementState, KeyEvent, WindowEvent};
    /// use rio_window::keyboard::{KeyCode, PhysicalKey};
    /// # let window_event = WindowEvent::RedrawRequested; // To make the example compile
    /// match window_event {
    ///     WindowEvent::KeyboardInput {
    ///         event:
    ///             KeyEvent {
    ///                 physical_key: PhysicalKey::Code(KeyCode::KeyW),
    ///                 state: ElementState::Pressed,
    ///                 repeat: false,
    ///                 ..
    ///             },
    ///         ..
    ///     } => {
    ///         // The physical key `W` was pressed, and it was not a repeat
    ///     },
    ///     _ => {}, // Handle other events
    /// }
    /// ```
    pub repeat: bool,

    /// Platform-specific key event information.
    ///
    /// On Windows, Linux and macOS, this type contains the key without modifiers and the text with
    /// all modifiers applied.
    ///
    /// On Android, iOS, Redox and Web, this type is a no-op.
    pub(crate) platform_specific: platform_impl::KeyEventExtra,
}

/// Describes keyboard modifiers event.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Modifiers {
    pub(crate) state: ModifiersState,

    // NOTE: Currently pressed modifiers keys.
    //
    // The field providing a metadata, it shouldn't be used as a source of truth.
    pub(crate) pressed_mods: ModifiersKeys,
}

impl Modifiers {
    /// The state of the modifiers.
    pub fn state(&self) -> ModifiersState {
        self.state
    }

    /// The state of the left shift key.
    pub fn lshift_state(&self) -> ModifiersKeyState {
        self.mod_state(ModifiersKeys::LSHIFT)
    }

    /// The state of the right shift key.
    pub fn rshift_state(&self) -> ModifiersKeyState {
        self.mod_state(ModifiersKeys::RSHIFT)
    }

    /// The state of the left alt key.
    pub fn lalt_state(&self) -> ModifiersKeyState {
        self.mod_state(ModifiersKeys::LALT)
    }

    /// The state of the right alt key.
    pub fn ralt_state(&self) -> ModifiersKeyState {
        self.mod_state(ModifiersKeys::RALT)
    }

    /// The state of the left control key.
    pub fn lcontrol_state(&self) -> ModifiersKeyState {
        self.mod_state(ModifiersKeys::LCONTROL)
    }

    /// The state of the right control key.
    pub fn rcontrol_state(&self) -> ModifiersKeyState {
        self.mod_state(ModifiersKeys::RCONTROL)
    }

    /// The state of the left super key.
    pub fn lsuper_state(&self) -> ModifiersKeyState {
        self.mod_state(ModifiersKeys::LSUPER)
    }

    /// The state of the right super key.
    pub fn rsuper_state(&self) -> ModifiersKeyState {
        self.mod_state(ModifiersKeys::RSUPER)
    }

    fn mod_state(&self, modifier: ModifiersKeys) -> ModifiersKeyState {
        if self.pressed_mods.contains(modifier) {
            ModifiersKeyState::Pressed
        } else {
            ModifiersKeyState::Unknown
        }
    }
}

impl From<ModifiersState> for Modifiers {
    fn from(value: ModifiersState) -> Self {
        Self {
            state: value,
            pressed_mods: Default::default(),
        }
    }
}

/// Describes [input method](https://en.wikipedia.org/wiki/Input_method) events.
///
/// This is also called a "composition event".
///
/// Most keypresses using a latin-like keyboard layout simply generate a
/// [`WindowEvent::KeyboardInput`]. However, one couldn't possibly have a key for every single
/// unicode character that the user might want to type
/// - so the solution operating systems employ is to allow the user to type these using _a sequence
///   of keypresses_ instead.
///
/// A prominent example of this is accents - many keyboard layouts allow you to first click the
/// "accent key", and then the character you want to apply the accent to. In this case, some
/// platforms will generate the following event sequence:
///
/// ```ignore
/// // Press "`" key
/// Ime::Preedit("`", Some((0, 0)))
/// // Press "E" key
/// Ime::Preedit("", None) // Synthetic event generated by winit to clear preedit.
/// Ime::Commit("é")
/// ```
///
/// Additionally, certain input devices are configured to display a candidate box that allow the
/// user to select the desired character interactively. (To properly position this box, you must use
/// [`Window::set_ime_cursor_area`].)
///
/// An example of a keyboard layout which uses candidate boxes is pinyin. On a latin keyboard the
/// following event sequence could be obtained:
///
/// ```ignore
/// // Press "A" key
/// Ime::Preedit("a", Some((1, 1)))
/// // Press "B" key
/// Ime::Preedit("a b", Some((3, 3)))
/// // Press left arrow key
/// Ime::Preedit("a b", Some((1, 1)))
/// // Press space key
/// Ime::Preedit("啊b", Some((3, 3)))
/// // Press space key
/// Ime::Preedit("", None) // Synthetic event generated by winit to clear preedit.
/// Ime::Commit("啊不")
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ime {
    /// Notifies when the IME was enabled.
    ///
    /// After getting this event you could receive [`Preedit`][Self::Preedit] and
    /// [`Commit`][Self::Commit] events. You should also start performing IME related requests
    /// like [`Window::set_ime_cursor_area`].
    Enabled,

    /// Notifies when a new composing text should be set at the cursor position.
    ///
    /// The value represents a pair of the preedit string and the cursor begin position and end
    /// position. When it's `None`, the cursor should be hidden. When `String` is an empty string
    /// this indicates that preedit was cleared.
    ///
    /// The cursor position is byte-wise indexed.
    Preedit(String, Option<(usize, usize)>),

    /// Notifies when text should be inserted into the editor widget.
    ///
    /// Right before this event winit will send empty [`Self::Preedit`] event.
    Commit(String),

    /// Notifies when the IME was disabled.
    ///
    /// After receiving this event you won't get any more [`Preedit`][Self::Preedit] or
    /// [`Commit`][Self::Commit] events until the next [`Enabled`][Self::Enabled] event. You should
    /// also stop issuing IME related requests like [`Window::set_ime_cursor_area`] and clear
    /// pending preedit text.
    Disabled,
}

/// Describes touch-screen input state.
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum TouchPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

/// Represents a touch event
///
/// Every time the user touches the screen, a new [`TouchPhase::Started`] event with an unique
/// identifier for the finger is generated. When the finger is lifted, an [`TouchPhase::Ended`]
/// event is generated with the same finger id.
///
/// After a `Started` event has been emitted, there may be zero or more `Move`
/// events when the finger is moved or the touch pressure changes.
///
/// The finger id may be reused by the system after an `Ended` event. The user
/// should assume that a new `Started` event received with the same id has nothing
/// to do with the old finger and is a new finger.
///
/// A [`TouchPhase::Cancelled`] event is emitted when the system has canceled tracking this
/// touch, such as when the window loses focus, or on iOS if the user moves the
/// device against their face.
///
/// ## Platform-specific
///
/// - **Web:** Doesn't take into account CSS [`border`], [`padding`], or [`transform`].
/// - **macOS:** Unsupported.
///
/// [`border`]: https://developer.mozilla.org/en-US/docs/Web/CSS/border
/// [`padding`]: https://developer.mozilla.org/en-US/docs/Web/CSS/padding
/// [`transform`]: https://developer.mozilla.org/en-US/docs/Web/CSS/transform
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Touch {
    pub device_id: DeviceId,
    pub phase: TouchPhase,
    pub location: PhysicalPosition<f64>,
    /// Describes how hard the screen was pressed. May be `None` if the platform
    /// does not support pressure sensitivity.
    ///
    /// ## Platform-specific
    ///
    /// - Only available on **iOS** 9.0+, **Windows** 8+, **Web**, and **Android**.
    pub force: Option<Force>,
    /// Unique identifier of a finger.
    pub id: u64,
}

/// Describes the force of a touch event
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Force {
    /// On iOS, the force is calibrated so that the same number corresponds to
    /// roughly the same amount of pressure on the screen regardless of the
    /// device.
    Calibrated {
        /// The force of the touch, where a value of 1.0 represents the force of
        /// an average touch (predetermined by the system, not user-specific).
        ///
        /// The force reported by Apple Pencil is measured along the axis of the
        /// pencil. If you want a force perpendicular to the device, you need to
        /// calculate this value using the `altitude_angle` value.
        force: f64,
        /// The maximum possible force for a touch.
        ///
        /// The value of this field is sufficiently high to provide a wide
        /// dynamic range for values of the `force` field.
        max_possible_force: f64,
        /// The altitude (in radians) of the stylus.
        ///
        /// A value of 0 radians indicates that the stylus is parallel to the
        /// surface. The value of this property is Pi/2 when the stylus is
        /// perpendicular to the surface.
        altitude_angle: Option<f64>,
    },
    /// If the platform reports the force as normalized, we have no way of
    /// knowing how much pressure 1.0 corresponds to – we know it's the maximum
    /// amount of force, but as to how much force, you might either have to
    /// press really really hard, or not hard at all, depending on the device.
    Normalized(f64),
}

impl Force {
    /// Returns the force normalized to the range between 0.0 and 1.0 inclusive.
    ///
    /// Instead of normalizing the force, you should prefer to handle
    /// [`Force::Calibrated`] so that the amount of force the user has to apply is
    /// consistent across devices.
    pub fn normalized(&self) -> f64 {
        match self {
            Force::Calibrated {
                force,
                max_possible_force,
                altitude_angle,
            } => {
                let force = match altitude_angle {
                    Some(altitude_angle) => force / altitude_angle.sin(),
                    None => *force,
                };
                force / max_possible_force
            }
            Force::Normalized(force) => *force,
        }
    }
}

/// Identifier for a specific analog axis on some device.
pub type AxisId = u32;

/// Identifier for a specific button on some device.
pub type ButtonId = u32;

/// Describes the input state of a key.
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ElementState {
    Pressed,
    Released,
}

impl ElementState {
    /// True if `self == Pressed`.
    pub fn is_pressed(self) -> bool {
        self == ElementState::Pressed
    }
}

/// Describes a button of a mouse controller.
///
/// ## Platform-specific
///
/// **macOS:** `Back` and `Forward` might not work with all hardware.
/// **Orbital:** `Back` and `Forward` are unsupported due to orbital not supporting them.
#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

/// Describes a difference in the mouse scroll wheel state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MouseScrollDelta {
    /// Amount in lines or rows to scroll in the horizontal
    /// and vertical directions.
    ///
    /// Positive values indicate that the content that is being scrolled should move
    /// right and down (revealing more content left and up).
    LineDelta(f32, f32),

    /// Amount in pixels to scroll in the horizontal and
    /// vertical direction.
    ///
    /// Scroll events are expressed as a `PixelDelta` if
    /// supported by the device (eg. a touchpad) and
    /// platform.
    ///
    /// Positive values indicate that the content being scrolled should
    /// move right/down.
    ///
    /// For a 'natural scrolling' touch pad (that acts like a touch screen)
    /// this means moving your fingers right and down should give positive values,
    /// and move the content right and down (to reveal more things left and up).
    PixelDelta(PhysicalPosition<f64>),
}

/// Handle to synchronously change the size of the window from the
/// [`WindowEvent`].
#[derive(Debug, Clone)]
pub struct InnerSizeWriter {
    pub(crate) new_inner_size: Weak<Mutex<PhysicalSize<u32>>>,
}

impl InnerSizeWriter {
    #[cfg(not(orbital_platform))]
    pub(crate) fn new(new_inner_size: Weak<Mutex<PhysicalSize<u32>>>) -> Self {
        Self { new_inner_size }
    }

    /// Try to request inner size which will be set synchronously on the window.
    pub fn request_inner_size(
        &mut self,
        new_inner_size: PhysicalSize<u32>,
    ) -> Result<(), ExternalError> {
        if let Some(inner) = self.new_inner_size.upgrade() {
            *inner.lock().unwrap() = new_inner_size;
            Ok(())
        } else {
            Err(ExternalError::Ignored)
        }
    }
}

impl PartialEq for InnerSizeWriter {
    fn eq(&self, other: &Self) -> bool {
        self.new_inner_size.as_ptr() == other.new_inner_size.as_ptr()
    }
}

#[cfg(test)]
mod tests {
    use crate::dpi::PhysicalPosition;
    use crate::event;
    use std::collections::{BTreeSet, HashSet};

    macro_rules! foreach_event {
        ($closure:expr) => {{
            #[allow(unused_mut)]
            let mut x = $closure;
            let did = unsafe { event::DeviceId::dummy() };

            #[allow(deprecated)]
            {
                use crate::event::Event::*;
                use crate::event::Ime::Enabled;
                use crate::event::WindowEvent::*;
                use crate::window::WindowId;

                // Mainline events.
                let wid = unsafe { WindowId::dummy() };
                x(UserEvent(()));
                x(NewEvents(event::StartCause::Init));
                x(AboutToWait);
                x(LoopExiting);
                x(Suspended);
                x(Resumed);

                // Window events.
                let with_window_event = |wev| {
                    x(WindowEvent {
                        window_id: wid,
                        event: wev,
                    })
                };

                with_window_event(CloseRequested);
                with_window_event(Destroyed);
                with_window_event(Focused(true));
                with_window_event(Moved((0, 0).into()));
                with_window_event(Resized((0, 0).into()));
                with_window_event(DroppedFile("x.txt".into()));
                with_window_event(HoveredFile("x.txt".into()));
                with_window_event(HoveredFileCancelled);
                with_window_event(Ime(Enabled));
                with_window_event(CursorMoved {
                    device_id: did,
                    position: (0, 0).into(),
                });
                with_window_event(ModifiersChanged(event::Modifiers::default()));
                with_window_event(CursorEntered { device_id: did });
                with_window_event(CursorLeft { device_id: did });
                with_window_event(MouseWheel {
                    device_id: did,
                    delta: event::MouseScrollDelta::LineDelta(0.0, 0.0),
                    phase: event::TouchPhase::Started,
                });
                with_window_event(MouseInput {
                    device_id: did,
                    state: event::ElementState::Pressed,
                    button: event::MouseButton::Other(0),
                });
                with_window_event(PinchGesture {
                    device_id: did,
                    delta: 0.0,
                    phase: event::TouchPhase::Started,
                });
                with_window_event(DoubleTapGesture { device_id: did });
                with_window_event(RotationGesture {
                    device_id: did,
                    delta: 0.0,
                    phase: event::TouchPhase::Started,
                });
                with_window_event(PanGesture {
                    device_id: did,
                    delta: PhysicalPosition::<f32>::new(0.0, 0.0),
                    phase: event::TouchPhase::Started,
                });
                with_window_event(TouchpadPressure {
                    device_id: did,
                    pressure: 0.0,
                    stage: 0,
                });
                with_window_event(AxisMotion {
                    device_id: did,
                    axis: 0,
                    value: 0.0,
                });
                with_window_event(Touch(event::Touch {
                    device_id: did,
                    phase: event::TouchPhase::Started,
                    location: (0.0, 0.0).into(),
                    id: 0,
                    force: Some(event::Force::Normalized(0.0)),
                }));
                with_window_event(ThemeChanged(crate::window::Theme::Light));
                with_window_event(Occluded(true));
            }

            #[allow(deprecated)]
            {
                use event::DeviceEvent::*;

                let with_device_event = |dev_ev| {
                    x(event::Event::DeviceEvent {
                        device_id: did,
                        event: dev_ev,
                    })
                };

                with_device_event(Added);
                with_device_event(Removed);
                with_device_event(MouseMotion {
                    delta: (0.0, 0.0).into(),
                });
                with_device_event(MouseWheel {
                    delta: event::MouseScrollDelta::LineDelta(0.0, 0.0),
                });
                with_device_event(Motion {
                    axis: 0,
                    value: 0.0,
                });
                with_device_event(Button {
                    button: 0,
                    state: event::ElementState::Pressed,
                });
            }
        }};
    }

    #[allow(clippy::redundant_clone)]
    #[test]
    fn test_event_clone() {
        foreach_event!(|event: event::Event<()>| {
            let event2 = event.clone();
            assert_eq!(event, event2);
        })
    }

    #[test]
    fn test_map_nonuser_event() {
        foreach_event!(|event: event::Event<()>| {
            let is_user = matches!(event, event::Event::UserEvent(()));
            let event2 = event.map_nonuser_event::<()>();
            if is_user {
                assert_eq!(event2, Err(event::Event::UserEvent(())));
            } else {
                assert!(event2.is_ok());
            }
        })
    }

    #[test]
    fn test_force_normalize() {
        let force = event::Force::Normalized(0.0);
        assert_eq!(force.normalized(), 0.0);

        let force2 = event::Force::Calibrated {
            force: 5.0,
            max_possible_force: 2.5,
            altitude_angle: None,
        };
        assert_eq!(force2.normalized(), 2.0);

        let force3 = event::Force::Calibrated {
            force: 5.0,
            max_possible_force: 2.5,
            altitude_angle: Some(std::f64::consts::PI / 2.0),
        };
        assert_eq!(force3.normalized(), 2.0);
    }

    #[allow(clippy::clone_on_copy)]
    #[test]
    fn ensure_attrs_do_not_panic() {
        foreach_event!(|event: event::Event<()>| {
            let _ = format!("{event:?}");
        });
        let _ = event::StartCause::Init.clone();

        let did = unsafe { crate::event::DeviceId::dummy() }.clone();
        HashSet::new().insert(did);
        let mut set = [did, did, did];
        set.sort_unstable();
        let mut set2 = BTreeSet::new();
        set2.insert(did);
        set2.insert(did);

        HashSet::new().insert(event::TouchPhase::Started.clone());
        HashSet::new().insert(event::MouseButton::Left.clone());
        HashSet::new().insert(event::Ime::Enabled);

        let _ = event::Touch {
            device_id: did,
            phase: event::TouchPhase::Started,
            location: (0.0, 0.0).into(),
            id: 0,
            force: Some(event::Force::Normalized(0.0)),
        }
        .clone();
        let _ = event::Force::Calibrated {
            force: 0.0,
            max_possible_force: 0.0,
            altitude_angle: None,
        }
        .clone();
    }
}
