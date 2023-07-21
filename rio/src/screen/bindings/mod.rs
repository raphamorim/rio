// Cria os bindings e usa struct actions
// https://github.com/alacritty/alacritty/blob/828fdab7470c8d16d2edbe2cec919169524cb2bb/alacritty/src/config/bindings.rs#L43

use crate::crosswords::vi_mode::ViMotion;
use crate::crosswords::Mode;
use bitflags::bitflags;
use std::fmt::Debug;
use winit::keyboard::Key::*;
use winit::keyboard::{Key, KeyCode, KeyLocation, ModifiersState};
// use winit::platform::scancode::KeyCodeExtScancode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontSizeAction {
    Increase,
    Decrease,
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding<T> {
    /// Modifier keys required to activate binding.
    pub mods: ModifiersState,

    /// String to send to PTY if mods and mode match.
    pub action: Action,

    /// Binding mode required to activate binding.
    pub mode: BindingMode,

    /// Excluded binding modes where the binding won't be activated.
    pub notmode: BindingMode,

    /// This property is used as part of the trigger detection code.
    ///
    /// For example, this might be a key like "G", or a mouse button.
    pub trigger: T,
}

impl<T: Eq> Binding<T> {
    #[inline]
    pub fn is_triggered_by(
        &self,
        mode: BindingMode,
        mods: ModifiersState,
        input: &T,
    ) -> bool {
        // Check input first since bindings are stored in one big list. This is
        // the most likely item to fail so prioritizing it here allows more
        // checks to be short circuited.
        self.trigger == *input
            && self.mods == mods
            && mode.contains(self.mode.clone())
            && !mode.intersects(self.notmode.clone())
    }

    #[inline]
    #[allow(unused)]
    pub fn triggers_match(&self, binding: &Binding<T>) -> bool {
        // Check the binding's key and modifiers.
        if self.trigger != binding.trigger || self.mods != binding.mods {
            return false;
        }

        let selfmode = if self.mode.is_empty() {
            BindingMode::all()
        } else {
            self.mode.clone()
        };
        let bindingmode = if binding.mode.is_empty() {
            BindingMode::all()
        } else {
            binding.mode.clone()
        };

        if !selfmode.intersects(bindingmode) {
            return false;
        }

        // The bindings are never active at the same time when the required modes of one binding
        // are part of the forbidden bindings of the other.
        if self.mode.intersects(binding.notmode.clone())
            || binding.mode.intersects(self.notmode.clone())
        {
            return false;
        }

        true
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum BindingKey {
    #[allow(dead_code)]
    Scancode(KeyCode),
    Keycode {
        key: Key,
        location: KeyLocation,
    },
}

pub type KeyBinding = Binding<BindingKey>;
pub type KeyBindings = Vec<KeyBinding>;

bitflags! {
    /// Modes available for key bindings.
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct BindingMode: u8 {
        const APP_CURSOR          = 0b0000_0001;
        const APP_KEYPAD          = 0b0000_0010;
        const ALT_SCREEN          = 0b0000_0100;
        const VI                  = 0b0000_1000;
        const SEARCH              = 0b0001_0000;
    }
}

impl BindingMode {
    pub fn new(mode: &Mode) -> BindingMode {
        let mut binding_mode = BindingMode::empty();
        binding_mode.set(BindingMode::APP_CURSOR, mode.contains(Mode::APP_CURSOR));
        binding_mode.set(BindingMode::APP_KEYPAD, mode.contains(Mode::APP_KEYPAD));
        binding_mode.set(BindingMode::ALT_SCREEN, mode.contains(Mode::ALT_SCREEN));
        binding_mode.set(BindingMode::VI, mode.contains(Mode::VI));
        binding_mode
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Write an escape sequence.
    Esc(String),

    /// Run given command.
    // Command(Program),

    /// Regex keyboard hints.
    // Hint(Hint),

    // Move vi mode cursor.
    ViMotion(ViMotion),

    // Perform vi mode action.
    // Vi(ViAction),
    /// Perform mouse binding exclusive action.
    // Mouse(MouseAction),

    /// Paste contents of system clipboard.
    Paste,

    /// Store current selection into clipboard.
    Copy,

    #[cfg(not(any(target_os = "macos", windows)))]
    #[allow(dead_code)]
    /// Store current selection into selection buffer.
    CopySelection,

    /// Paste contents of selection buffer.
    #[allow(dead_code)]
    PasteSelection,

    /// Increase font size.
    #[allow(dead_code)]
    IncreaseFontSize,

    /// Decrease font size.
    #[allow(dead_code)]
    DecreaseFontSize,

    /// Reset font size to the config value.
    #[allow(dead_code)]
    ResetFontSize,

    /// Scroll exactly one page up.
    ScrollPageUp,

    /// Scroll exactly one page down.
    ScrollPageDown,

    /// Scroll half a page up.
    ScrollHalfPageUp,

    /// Scroll half a page down.
    ScrollHalfPageDown,

    /// Scroll one line up.
    ScrollLineUp,

    /// Scroll one line down.
    ScrollLineDown,

    /// Scroll all the way to the top.
    ScrollToTop,

    /// Scroll all the way to the bottom.
    ScrollToBottom,

    /// Clear the display buffer(s) to remove history.
    #[allow(dead_code)]
    ClearHistory,

    /// Hide the Rio window.
    #[allow(dead_code)]
    Hide,

    /// Hide all windows other than Rio on macOS.
    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    HideOtherApplications,

    /// Minimize the Rio window.
    #[allow(dead_code)]
    Minimize,

    /// Quit Rio.
    #[allow(dead_code)]
    Quit,

    /// Clear warning and error notices.
    ClearLogNotice,

    /// Spawn a new instance of Rio.
    #[allow(dead_code)]
    SpawnNewInstance,

    /// Create a new Rio window.
    #[allow(dead_code)]
    WindowCreateNew,

    /// Create config editor.
    #[allow(dead_code)]
    ConfigEditor,

    /// Create a new Rio tab.
    #[allow(dead_code)]
    TabCreateNew,

    /// Switch to next tab.
    #[allow(dead_code)]
    TabSwitchNext,

    /// Switch to prev tab.
    #[allow(dead_code)]
    TabSwitchPrev,

    /// Close tab.
    #[allow(dead_code)]
    TabCloseCurrent,

    /// Toggle fullscreen.
    #[allow(dead_code)]
    ToggleFullscreen,

    /// Toggle maximized.
    #[allow(dead_code)]
    ToggleMaximized,

    /// Toggle simple fullscreen on macOS.
    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    ToggleSimpleFullscreen,

    /// Clear active selection.
    ClearSelection,

    /// Toggle vi mode.
    ToggleViMode,

    /// Allow receiving char input.
    #[allow(dead_code)]
    ReceiveChar,

    /// No action.
    #[allow(dead_code)]
    None,
}

impl From<&'static str> for Action {
    fn from(s: &'static str) -> Action {
        Action::Esc(s.into())
    }
}

impl From<ViMotion> for Action {
    fn from(motion: ViMotion) -> Self {
        Self::ViMotion(motion)
    }
}

macro_rules! bindings {
    (
        $ty:ident;
        $(
            $key:expr
            $(=>$location:expr)?
            $(,$mods:expr)*
            $(,+$mode:expr)*
            $(,~$notmode:expr)*
            ;$action:expr
        );*
        $(;)*
    ) => {{
        let mut v = Vec::new();

        $(
            let mut _mods = ModifiersState::empty();
            $(_mods = $mods;)*
            let mut _mode = BindingMode::empty();
            $(_mode.insert($mode);)*
            let mut _notmode = BindingMode::empty();
            $(_notmode.insert($notmode);)*

            v.push($ty {
                trigger: trigger!($ty, $key, $($location)?),
                mods: _mods,
                mode: _mode,
                notmode: _notmode,
                action: $action.into(),
            });
        )*

        v
    }};
}

macro_rules! trigger {
    (KeyBinding, $key:literal, $location:expr) => {{
        BindingKey::Keycode {
            key: Character($key.into()),
            location: $location,
        }
    }};
    (KeyBinding, $key:literal,) => {{
        BindingKey::Keycode {
            key: Character($key.into()),
            location: KeyLocation::Standard,
        }
    }};
    (KeyBinding, $key:expr,) => {{
        BindingKey::Keycode {
            key: $key,
            location: KeyLocation::Standard,
        }
    }};
    ($ty:ident, $key:expr,) => {{
        $key
    }};
}

pub fn default_key_bindings() -> Vec<KeyBinding> {
    let mut bindings = bindings!(
        KeyBinding;
        Copy;  Action::Copy;
        Copy,  +BindingMode::VI; Action::ClearSelection;
        Paste, ~BindingMode::VI; Action::Paste;
        "l", ModifiersState::CONTROL; Action::ClearLogNotice;
        "l",    ModifiersState::CONTROL,  ~BindingMode::VI;
            Action::Esc("\x0c".into());
        Tab,  ModifiersState::SHIFT, ~BindingMode::VI;
            Action::Esc("\x1b[Z".into());
        Backspace, ModifiersState::ALT,   ~BindingMode::VI;
            Action::Esc("\x1b\x7f".into());
        Backspace, ModifiersState::SHIFT, ~BindingMode::VI;
            Action::Esc("\x7f".into());
        Home,     ModifiersState::SHIFT, ~BindingMode::ALT_SCREEN; Action::ScrollToTop;
        End,      ModifiersState::SHIFT, ~BindingMode::ALT_SCREEN; Action::ScrollToBottom;
        PageUp,   ModifiersState::SHIFT, ~BindingMode::ALT_SCREEN; Action::ScrollPageUp;
        PageDown, ModifiersState::SHIFT, ~BindingMode::ALT_SCREEN; Action::ScrollPageDown;
        Home,     ModifiersState::SHIFT, +BindingMode::ALT_SCREEN,
            ~BindingMode::VI; Action::Esc("\x1b[1;2H".into());
        End,      ModifiersState::SHIFT, +BindingMode::ALT_SCREEN,
            ~BindingMode::VI; Action::Esc("\x1b[1;2F".into());
        PageUp,   ModifiersState::SHIFT, +BindingMode::ALT_SCREEN,
            ~BindingMode::VI; Action::Esc("\x1b[5;2~".into());
        PageDown, ModifiersState::SHIFT, +BindingMode::ALT_SCREEN,
            ~BindingMode::VI; Action::Esc("\x1b[6;2~".into());
        Home,  +BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1bOH".into());
        Home,  ~BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1b[H".into());
        End,   +BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1bOF".into());
        End,   ~BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1b[F".into());
        ArrowUp,    +BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1bOA".into());
        ArrowUp,    ~BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1b[A".into());
        ArrowDown,  +BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1bOB".into());
        ArrowDown,  ~BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1b[B".into());
        ArrowRight, +BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1bOC".into());
        ArrowRight, ~BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1b[C".into());
        ArrowLeft,  +BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1bOD".into());
        ArrowLeft,  ~BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1b[D".into());
        Backspace,        ~BindingMode::VI; Action::Esc("\x7f".into());
        Insert,      ~BindingMode::VI; Action::Esc("\x1b[2~".into());
        Delete,      ~BindingMode::VI; Action::Esc("\x1b[3~".into());
        PageUp,      ~BindingMode::VI; Action::Esc("\x1b[5~".into());
        PageDown,    ~BindingMode::VI; Action::Esc("\x1b[6~".into());
        F1,          ~BindingMode::VI; Action::Esc("\x1bOP".into());
        F2,          ~BindingMode::VI; Action::Esc("\x1bOQ".into());
        F3,          ~BindingMode::VI; Action::Esc("\x1bOR".into());
        F4,          ~BindingMode::VI; Action::Esc("\x1bOS".into());
        F5,          ~BindingMode::VI; Action::Esc("\x1b[15~".into());
        F6,          ~BindingMode::VI; Action::Esc("\x1b[17~".into());
        F7,          ~BindingMode::VI; Action::Esc("\x1b[18~".into());
        F8,          ~BindingMode::VI; Action::Esc("\x1b[19~".into());
        F9,          ~BindingMode::VI; Action::Esc("\x1b[20~".into());
        F10,         ~BindingMode::VI; Action::Esc("\x1b[21~".into());
        F11,         ~BindingMode::VI; Action::Esc("\x1b[23~".into());
        F12,         ~BindingMode::VI; Action::Esc("\x1b[24~".into());
        F13,         ~BindingMode::VI; Action::Esc("\x1b[25~".into());
        F14,         ~BindingMode::VI; Action::Esc("\x1b[26~".into());
        F15,         ~BindingMode::VI; Action::Esc("\x1b[28~".into());
        F16,         ~BindingMode::VI; Action::Esc("\x1b[29~".into());
        F17,         ~BindingMode::VI; Action::Esc("\x1b[31~".into());
        F18,         ~BindingMode::VI; Action::Esc("\x1b[32~".into());
        F19,         ~BindingMode::VI; Action::Esc("\x1b[33~".into());
        F20,         ~BindingMode::VI; Action::Esc("\x1b[34~".into());
        // NumpadEnter, ~BindingMode::VI; Action::Esc("\n".into());
        Space, ModifiersState::SHIFT | ModifiersState::CONTROL;
            Action::ToggleViMode;
        Space, ModifiersState::SHIFT | ModifiersState::CONTROL, +BindingMode::VI;
            Action::ScrollToBottom;
        Escape,                        +BindingMode::VI;
            Action::ClearSelection;
        "i",                             +BindingMode::VI;
            Action::ToggleViMode;
        "i",                             +BindingMode::VI;
            Action::ScrollToBottom;
        "c",      ModifiersState::CONTROL,  +BindingMode::VI;
            Action::ToggleViMode;
        "y",      ModifiersState::CONTROL,  +BindingMode::VI;
            Action::ScrollLineUp;
        "e",      ModifiersState::CONTROL,  +BindingMode::VI;
            Action::ScrollLineDown;
        "g",                             +BindingMode::VI;
            Action::ScrollToTop;
        "g",      ModifiersState::SHIFT, +BindingMode::VI;
            Action::ScrollToBottom;
        "b",      ModifiersState::CONTROL,  +BindingMode::VI;
            Action::ScrollPageUp;
        "f",      ModifiersState::CONTROL,  +BindingMode::VI;
            Action::ScrollPageDown;
        "u",      ModifiersState::CONTROL,  +BindingMode::VI;
            Action::ScrollHalfPageUp;
        "d",      ModifiersState::CONTROL,  +BindingMode::VI;
            Action::ScrollHalfPageDown;
        "y",                             +BindingMode::VI; Action::Copy;
        "y",                             +BindingMode::VI;
            Action::ClearSelection;
        // V,                             +BindingMode::VI;
            // ViAction::ToggleNormalSelection;
        // V,      ModifiersState::SHIFT, +BindingMode::VI;
        //     ViAction::ToggleLineSelection;
        // V,      ModifiersState::CONTROL,  +BindingMode::VI;
        //     ViAction::ToggleBlockSelection;
        // V,      ModifiersState::ALT,   +BindingMode::VI;
        //     ViAction::ToggleSemanticSelection;
        // N,                             +BindingMode::VI;
        //     ViAction::SearchNext;
        // N,      ModifiersState::SHIFT, +BindingMode::VI;
        //     ViAction::SearchPrevious;
        // Enter,                        +BindingMode::VI;
        //     ViAction::Open;
        // Z,                             +BindingMode::VI;
            // ViAction::CenterAroundViCursor;
        "k",                             +BindingMode::VI;
            ViMotion::Up;
        "j",                             +BindingMode::VI;
            ViMotion::Down;
        "h",                             +BindingMode::VI;
            ViMotion::Left;
        "l",                             +BindingMode::VI;
            ViMotion::Right;
        ArrowUp,                            +BindingMode::VI;
            ViMotion::Up;
        ArrowDown,                          +BindingMode::VI;
            ViMotion::Down;
        ArrowLeft,                          +BindingMode::VI;
            ViMotion::Left;
        ArrowRight,                         +BindingMode::VI;
            ViMotion::Right;
        "0",                          +BindingMode::VI;
            ViMotion::First;
        "4",   ModifiersState::SHIFT, +BindingMode::VI;
            ViMotion::Last;
        "6",   ModifiersState::SHIFT, +BindingMode::VI;
            ViMotion::FirstOccupied;
        "h",      ModifiersState::SHIFT, +BindingMode::VI;
            ViMotion::High;
        "m",      ModifiersState::SHIFT, +BindingMode::VI;
            ViMotion::Middle;
        "l",      ModifiersState::SHIFT, +BindingMode::VI;
            ViMotion::Low;
        "b",                             +BindingMode::VI;
            ViMotion::SemanticLeft;
        "w",                             +BindingMode::VI;
            ViMotion::SemanticRight;
        "e",                             +BindingMode::VI;
            ViMotion::SemanticRightEnd;
        "b",      ModifiersState::SHIFT, +BindingMode::VI;
            ViMotion::WordLeft;
        "w",      ModifiersState::SHIFT, +BindingMode::VI;
            ViMotion::WordRight;
        "e",      ModifiersState::SHIFT, +BindingMode::VI;
            ViMotion::WordRightEnd;
        "5",   ModifiersState::SHIFT, +BindingMode::VI;
            ViMotion::Bracket;
    );

    //   Code     Modifiers
    // ---------+---------------------------
    //    2     | Shift
    //    3     | Alt
    //    4     | Shift + Alt
    //    5     | Control
    //    6     | Shift + Control
    //    7     | Alt + Control
    //    8     | Shift + Alt + Control
    // ---------+---------------------------
    //
    // from: https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-PC-Style-Function-Keys
    let mut modifiers = vec![
        ModifiersState::SHIFT,
        ModifiersState::SHIFT | ModifiersState::ALT,
        ModifiersState::CONTROL,
        ModifiersState::SHIFT | ModifiersState::CONTROL,
        ModifiersState::ALT | ModifiersState::CONTROL,
        ModifiersState::SHIFT | ModifiersState::ALT | ModifiersState::CONTROL,
    ];

    // In MacOs we target the same behaviour that Terminal.app has
    // Terminal.app does not deal with ctlseqs with ALT keys
    #[cfg(not(target_os = "macos"))]
    {
        modifiers.push(ModifiersState::ALT);
    }

    for (index, mods) in modifiers.drain(..).enumerate() {
        let modifiers_code = index + 2;
        bindings.extend(bindings!(
            KeyBinding;
            Delete, mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[3;{}~", modifiers_code));
            ArrowUp,     mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[1;{}A", modifiers_code));
            ArrowDown,   mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[1;{}B", modifiers_code));
            ArrowRight,  mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[1;{}C", modifiers_code));
            ArrowLeft,   mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[1;{}D", modifiers_code));
            F1,     mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[1;{}P", modifiers_code));
            F2,     mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[1;{}Q", modifiers_code));
            F3,     mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[1;{}R", modifiers_code));
            F4,     mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[1;{}S", modifiers_code));
            F5,     mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[15;{}~", modifiers_code));
            F6,     mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[17;{}~", modifiers_code));
            F7,     mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[18;{}~", modifiers_code));
            F8,     mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[19;{}~", modifiers_code));
            F9,     mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[20;{}~", modifiers_code));
            F10,    mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[21;{}~", modifiers_code));
            F11,    mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[23;{}~", modifiers_code));
            F12,    mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[24;{}~", modifiers_code));
            F13,    mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[25;{}~", modifiers_code));
            F14,    mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[26;{}~", modifiers_code));
            F15,    mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[28;{}~", modifiers_code));
            F16,    mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[29;{}~", modifiers_code));
            F17,    mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[31;{}~", modifiers_code));
            F18,    mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[32;{}~", modifiers_code));
            F19,    mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[33;{}~", modifiers_code));
            F20,    mods, ~BindingMode::VI;
                Action::Esc(format!("\x1b[34;{}~", modifiers_code));
        ));

        // We're adding the following bindings with `Shift` manually above, so skipping them here.
        if modifiers_code != 2 {
            bindings.extend(bindings!(
                KeyBinding;
                Insert,   mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[2;{}~", modifiers_code));
                PageUp,   mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[5;{}~", modifiers_code));
                PageDown, mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[6;{}~", modifiers_code));
                End,      mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[1;{}F", modifiers_code));
                Home,     mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[1;{}H", modifiers_code));
            ));
        }
    }

    bindings.extend(platform_key_bindings());

    bindings
}

// Macos
#[cfg(all(target_os = "macos", not(test)))]
pub fn platform_key_bindings() -> Vec<KeyBinding> {
    bindings!(
        KeyBinding;
        "0",           ModifiersState::SUPER; Action::ResetFontSize;
        "=",         ModifiersState::SUPER; Action::IncreaseFontSize;
        "+",           ModifiersState::SUPER; Action::IncreaseFontSize;
        "+",      ModifiersState::SUPER; Action::IncreaseFontSize;
        "-",          ModifiersState::SUPER; Action::DecreaseFontSize;
        "-", ModifiersState::SUPER; Action::DecreaseFontSize;
        Insert, ModifiersState::SHIFT, ~BindingMode::VI;
            Action::Esc("\x1b[2;2~".into());
        ArrowLeft, ModifiersState::ALT,  ~BindingMode::VI;
            Action::Esc("\x1bb".into());
        ArrowRight, ModifiersState::ALT,  ~BindingMode::VI;
            Action::Esc("\x1bf".into());
        "k", ModifiersState::SUPER, ~BindingMode::VI;
            Action::Esc("\x0c".into());
        "k", ModifiersState::SUPER, ~BindingMode::VI;  Action::ClearHistory;
        "v", ModifiersState::SUPER, ~BindingMode::VI; Action::Paste;
        "f", ModifiersState::CONTROL | ModifiersState::SUPER; Action::ToggleFullscreen;
        "c", ModifiersState::SUPER; Action::Copy;
        "c", ModifiersState::SUPER, +BindingMode::VI; Action::ClearSelection;
        "h", ModifiersState::SUPER; Action::Hide;
        "h", ModifiersState::SUPER | ModifiersState::ALT; Action::HideOtherApplications;
        "m", ModifiersState::SUPER; Action::Minimize;
        "q", ModifiersState::SUPER; Action::Quit;
        "w", ModifiersState::SUPER; Action::Quit;
        "n", ModifiersState::SUPER; Action::WindowCreateNew;
        "t", ModifiersState::SUPER; Action::TabCreateNew;
        Tab, ModifiersState::CONTROL; Action::TabSwitchNext;
        "LBracket", ModifiersState::SUPER | ModifiersState::SHIFT; Action::TabSwitchNext;
        "RBracket", ModifiersState::SUPER | ModifiersState::SHIFT; Action::TabSwitchPrev;
        "w", ModifiersState::SUPER; Action::TabCloseCurrent;
        ",", ModifiersState::SUPER; Action::ConfigEditor;
    )
}

// Not Windows, Macos
#[cfg(not(any(target_os = "macos", target_os = "windows", test)))]
pub fn platform_key_bindings() -> Vec<KeyBinding> {
    bindings!(
        KeyBinding;
        "v",        ModifiersState::CONTROL | ModifiersState::SHIFT, ~BindingMode::VI; Action::Paste;
        "c",        ModifiersState::CONTROL | ModifiersState::SHIFT; Action::Copy;
        "c",        ModifiersState::CONTROL | ModifiersState::SHIFT,
            +BindingMode::VI; Action::ClearSelection;
        Insert,   ModifiersState::SHIFT, ~BindingMode::VI; Action::PasteSelection;
        "0",     ModifiersState::CONTROL;  Action::ResetFontSize;
        "=",   ModifiersState::CONTROL;  Action::IncreaseFontSize;
        "+",     ModifiersState::CONTROL;  Action::IncreaseFontSize;
        "+",      ModifiersState::CONTROL;  Action::IncreaseFontSize;
        "-",          ModifiersState::CONTROL;  Action::DecreaseFontSize;
        "-", ModifiersState::CONTROL;  Action::DecreaseFontSize;
        "n", ModifiersState::CONTROL; Action::WindowCreateNew;
        "t", ModifiersState::CONTROL; Action::TabCreateNew;
        Tab, ModifiersState::CONTROL; Action::TabSwitchNext;
        "LBracket", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::TabSwitchNext;
        "RBracket", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::TabSwitchPrev;
        "w", ModifiersState::CONTROL; Action::TabCloseCurrent;
    )
}

// Windows
#[cfg(all(target_os = "windows", not(test)))]
pub fn platform_key_bindings() -> Vec<KeyBinding> {
    bindings!(
        KeyBinding;
        "v",        ModifiersState::CONTROL | ModifiersState::SHIFT, ~BindingMode::VI; Action::Paste;
        "c",        ModifiersState::CONTROL | ModifiersState::SHIFT; Action::Copy;
        "c",        ModifiersState::CONTROL | ModifiersState::SHIFT,
            +BindingMode::VI; Action::ClearSelection;
        Insert,   ModifiersState::SHIFT, ~BindingMode::VI; Action::PasteSelection;
        "0",     ModifiersState::CONTROL;  Action::ResetFontSize;
        "=",   ModifiersState::CONTROL;  Action::IncreaseFontSize;
        "+",     ModifiersState::CONTROL;  Action::IncreaseFontSize;
        "+",      ModifiersState::CONTROL;  Action::IncreaseFontSize;
        "-",          ModifiersState::CONTROL;  Action::DecreaseFontSize;
        "-", ModifiersState::CONTROL;  Action::DecreaseFontSize;
        Enter, ModifiersState::ALT; Action::ToggleFullscreen;
        T, ModifiersState::CONTROL; Action::TabCreateNew;
        Tab, ModifiersState::CONTROL; Action::TabSwitchNext;
        W, ModifiersState::CONTROL; Action::TabCloseCurrent;
        N, ModifiersState::CONTROL; Action::WindowCreateNew;
        LBracket, ModifiersState::CONTROL | ModifiersState::SHIFT; Action::TabSwitchNext;
        RBracket, ModifiersState::CONTROL | ModifiersState::SHIFT; Action::TabSwitchPrev;
    )
}

#[cfg(test)]
pub fn platform_key_bindings() -> Vec<KeyBinding> {
    vec![]
}
