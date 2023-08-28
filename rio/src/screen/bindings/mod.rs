// https://github.com/alacritty/alacritty/blob/828fdab7470c8d16d2edbe2cec919169524cb2bb/alacritty/src/config/bindings.rs#L43

use crate::crosswords::vi_mode::ViMotion;
use crate::crosswords::Mode;
use bitflags::bitflags;
use config::bindings::KeyBinding as ConfigKeyBinding;
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
        const DISAMBIGUATE_KEYS   = 0b0010_0000;
        const ALL_KEYS_AS_ESC     = 0b0100_0000;
    }
}

impl BindingMode {
    pub fn new(mode: &Mode) -> BindingMode {
        let mut binding_mode = BindingMode::empty();
        binding_mode.set(BindingMode::APP_CURSOR, mode.contains(Mode::APP_CURSOR));
        binding_mode.set(BindingMode::APP_KEYPAD, mode.contains(Mode::APP_KEYPAD));
        binding_mode.set(BindingMode::ALT_SCREEN, mode.contains(Mode::ALT_SCREEN));
        binding_mode.set(
            BindingMode::DISAMBIGUATE_KEYS,
            mode.contains(Mode::KEYBOARD_DISAMBIGUATE_ESC_CODES),
        );
        binding_mode.set(
            BindingMode::ALL_KEYS_AS_ESC,
            mode.contains(Mode::KEYBOARD_REPORT_ALL_KEYS_AS_ESC),
        );
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
    SelectNextTab,

    /// Switch to prev tab.
    #[allow(dead_code)]
    SelectPrevTab,

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
    ReceiveChar,

    /// No action.
    None,

    // Tab selections
    SelectTab1,
    SelectTab2,
    SelectTab3,
    SelectTab4,
    SelectTab5,
    SelectTab6,
    SelectTab7,
    SelectTab8,
    SelectTab9,
    SelectLastTab,
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

pub fn default_key_bindings(
    unprocessed_config_key_bindings: Vec<ConfigKeyBinding>,
) -> Vec<KeyBinding> {
    let mut bindings = bindings!(
        KeyBinding;
        Copy;  Action::Copy;
        Copy,  +BindingMode::VI; Action::ClearSelection;
        Paste, ~BindingMode::VI; Action::Paste;
        "l", ModifiersState::CONTROL; Action::ClearLogNotice;
        "l", ModifiersState::CONTROL; Action::ReceiveChar;
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
        Backspace, ModifiersState::ALT,     ~BindingMode::VI, ~BindingMode::ALL_KEYS_AS_ESC; Action::Esc("\x1b\x7f".into());
        Backspace, ModifiersState::SHIFT,   ~BindingMode::VI, ~BindingMode::ALL_KEYS_AS_ESC; Action::Esc("\x7f".into());
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

    bindings.extend(platform_key_bindings());

    config_key_bindings(unprocessed_config_key_bindings, bindings)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModeWrapper {
    pub mode: BindingMode,
    pub not_mode: BindingMode,
}

#[inline]
fn convert(config_key_binding: ConfigKeyBinding) -> Result<KeyBinding, String> {
    let (key, location) = if config_key_binding.key.chars().count() == 1 {
        (
            Key::Character(config_key_binding.key.to_lowercase().into()),
            KeyLocation::Standard,
        )
    } else {
        match config_key_binding.key.to_lowercase().as_str() {
            "home" => (Key::Home, KeyLocation::Standard),
            "space" => (Key::Space, KeyLocation::Standard),
            "delete" => (Key::Delete, KeyLocation::Standard),
            "esc" => (Key::Escape, KeyLocation::Standard),
            "insert" => (Key::Insert, KeyLocation::Standard),
            "pageup" => (Key::PageUp, KeyLocation::Standard),
            "pagedown" => (Key::PageDown, KeyLocation::Standard),
            "end" => (Key::End, KeyLocation::Standard),
            "up" => (Key::ArrowUp, KeyLocation::Standard),
            "back" => (Key::Backspace, KeyLocation::Standard),
            "down" => (Key::ArrowDown, KeyLocation::Standard),
            "left" => (Key::ArrowLeft, KeyLocation::Standard),
            "right" => (Key::ArrowRight, KeyLocation::Standard),
            "@" => (Key::Character("@".into()), KeyLocation::Standard),
            "colon" => (Key::Character(":".into()), KeyLocation::Standard),
            "." => (Key::Character(".".into()), KeyLocation::Standard),
            "return" => (Key::Enter, KeyLocation::Standard),
            "[" => (Key::Character("[".into()), KeyLocation::Standard),
            "]" => (Key::Character("]".into()), KeyLocation::Standard),
            ";" => (Key::Character(";".into()), KeyLocation::Standard),
            "\\" => (Key::Character("\\".into()), KeyLocation::Standard),
            "+" => (Key::Character("+".into()), KeyLocation::Standard),
            "," => (Key::Character(",".into()), KeyLocation::Standard),
            "/" => (Key::Character("/".into()), KeyLocation::Standard),
            "=" => (Key::Character("=".into()), KeyLocation::Standard),
            "-" => (Key::Character("-".into()), KeyLocation::Standard),
            "*" => (Key::Character("*".into()), KeyLocation::Standard),
            "1" => (Key::Character("1".into()), KeyLocation::Standard),
            "2" => (Key::Character("2".into()), KeyLocation::Standard),
            "3" => (Key::Character("3".into()), KeyLocation::Standard),
            "4" => (Key::Character("4".into()), KeyLocation::Standard),
            "5" => (Key::Character("5".into()), KeyLocation::Standard),
            "6" => (Key::Character("6".into()), KeyLocation::Standard),
            "7" => (Key::Character("7".into()), KeyLocation::Standard),
            "8" => (Key::Character("8".into()), KeyLocation::Standard),
            "9" => (Key::Character("9".into()), KeyLocation::Standard),
            "0" => (Key::Character("0".into()), KeyLocation::Standard),

            // Special case numpad.
            "numpadenter" => (Key::Enter, KeyLocation::Numpad),
            "numpadadd" => (Key::Character("+".into()), KeyLocation::Numpad),
            "numpadcomma" => (Key::Character(",".into()), KeyLocation::Numpad),
            "numpaddivide" => (Key::Character("/".into()), KeyLocation::Numpad),
            "numpadequals" => (Key::Character("=".into()), KeyLocation::Numpad),
            "numpadsubtract" => (Key::Character("-".into()), KeyLocation::Numpad),
            "numpadmultiply" => (Key::Character("*".into()), KeyLocation::Numpad),
            "numpad1" => (Key::Character("1".into()), KeyLocation::Numpad),
            "numpad2" => (Key::Character("2".into()), KeyLocation::Numpad),
            "numpad3" => (Key::Character("3".into()), KeyLocation::Numpad),
            "numpad4" => (Key::Character("4".into()), KeyLocation::Numpad),
            "numpad5" => (Key::Character("5".into()), KeyLocation::Numpad),
            "numpad6" => (Key::Character("6".into()), KeyLocation::Numpad),
            "numpad7" => (Key::Character("7".into()), KeyLocation::Numpad),
            "numpad8" => (Key::Character("8".into()), KeyLocation::Numpad),
            "numpad9" => (Key::Character("9".into()), KeyLocation::Numpad),
            "numpad0" => (Key::Character("0".into()), KeyLocation::Numpad),

            // Special cases
            "tab" => (Key::Tab, KeyLocation::Standard),
            _ => return Err("Unable to find defined 'keycode'".to_string()),
        }
    };

    let trigger = BindingKey::Keycode { key, location };

    let mut res = ModifiersState::empty();
    for modifier in config_key_binding.with.split('|') {
        match modifier.trim().to_lowercase().as_str() {
            "command" | "super" => res.insert(ModifiersState::SUPER),
            "shift" => res.insert(ModifiersState::SHIFT),
            "alt" | "option" => res.insert(ModifiersState::ALT),
            "control" => res.insert(ModifiersState::CONTROL),
            "none" => (),
            _ => (),
        }
    }

    let mut action: Action = match config_key_binding.action.to_lowercase().as_str() {
        "paste" => Action::Paste,
        "quit" => Action::Quit,
        "copy" => Action::Copy,
        "resetfontsize" => Action::ResetFontSize,
        "increasefontsize" => Action::IncreaseFontSize,
        "decreasefontsize" => Action::DecreaseFontSize,
        "createwindow" => Action::WindowCreateNew,
        "createtab" => Action::TabCreateNew,
        "closetab" => Action::TabCloseCurrent,
        "openconfigeditor" => Action::ConfigEditor,
        "selectprevtab" => Action::SelectPrevTab,
        "selectnexttab" => Action::SelectNextTab,
        "selecttab1" => Action::SelectTab1,
        "selecttab2" => Action::SelectTab2,
        "selecttab3" => Action::SelectTab3,
        "selecttab4" => Action::SelectTab4,
        "selecttab5" => Action::SelectTab5,
        "selecttab6" => Action::SelectTab6,
        "selecttab7" => Action::SelectTab7,
        "selecttab8" => Action::SelectTab8,
        "selecttab9" => Action::SelectTab9,
        "selectlasttab" => Action::SelectLastTab,
        "receivechar" => Action::ReceiveChar,
        "none" => Action::None,
        _ => Action::None,
    };

    if !config_key_binding.text.is_empty() {
        action = Action::Esc(config_key_binding.text);
    }

    if !config_key_binding.bytes.is_empty() {
        if let Ok(str_from_bytes) = std::str::from_utf8(&config_key_binding.bytes) {
            action = Action::Esc(str_from_bytes.into());
        }
    }

    let mut res_mode = ModeWrapper {
        mode: BindingMode::empty(),
        not_mode: BindingMode::empty(),
    };

    for modifier in config_key_binding.mode.split('|') {
        match modifier.trim().to_lowercase().as_str() {
            "appcursor" => res_mode.mode |= BindingMode::APP_CURSOR,
            "~appcursor" => res_mode.not_mode |= BindingMode::APP_CURSOR,
            "appkeypad" => res_mode.mode |= BindingMode::APP_KEYPAD,
            "~appkeypad" => res_mode.not_mode |= BindingMode::APP_KEYPAD,
            "alt" => res_mode.mode |= BindingMode::ALT_SCREEN,
            "~alt" => res_mode.not_mode |= BindingMode::ALT_SCREEN,
            "vi" => res_mode.mode |= BindingMode::VI,
            "~vi" => res_mode.not_mode |= BindingMode::VI,
            _ => {
                res_mode.not_mode |= BindingMode::empty();
                res_mode.mode |= BindingMode::empty();
            }
        }
    }

    Ok(KeyBinding {
        trigger,
        mods: res,
        action,
        mode: res_mode.mode,
        notmode: res_mode.not_mode,
    })
}

pub fn config_key_bindings(
    config_key_bindings: Vec<ConfigKeyBinding>,
    mut bindings: Vec<KeyBinding>,
) -> Vec<KeyBinding> {
    if config_key_bindings.is_empty() {
        return bindings;
    }

    for ckb in config_key_bindings {
        match convert(ckb) {
            Ok(key_binding) => match key_binding.action {
                Action::None | Action::ReceiveChar => {
                    let mut found_idx = None;
                    for (idx, binding) in bindings.iter().enumerate() {
                        if binding.triggers_match(&key_binding) {
                            found_idx = Some(idx);
                            break;
                        }
                    }

                    if let Some(idx) = found_idx {
                        bindings.remove(idx);
                        log::warn!(
                            "overwritted a previous key_binding with new one: {:?}",
                            key_binding
                        );
                    } else {
                        log::info!("added a new key_binding: {:?}", key_binding);
                    }

                    bindings.push(key_binding)
                }
                _ => {
                    log::info!("added a new key_binding: {:?}", key_binding);
                    bindings.push(key_binding)
                }
            },
            Err(err_message) => {
                log::error!("error loading a key binding: {:?}", err_message);
            }
        }
    }

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
        "n", ModifiersState::SUPER; Action::WindowCreateNew;
        "t", ModifiersState::SUPER; Action::TabCreateNew;
        Tab, ModifiersState::CONTROL; Action::SelectNextTab;
        Tab, ModifiersState::CONTROL | ModifiersState::SHIFT; Action::SelectPrevTab;
        "[", ModifiersState::SUPER | ModifiersState::SHIFT; Action::SelectNextTab;
        "]", ModifiersState::SUPER | ModifiersState::SHIFT; Action::SelectPrevTab;
        "w", ModifiersState::SUPER; Action::TabCloseCurrent;
        ",", ModifiersState::SUPER; Action::ConfigEditor;
        "1", ModifiersState::SUPER; Action::SelectTab1;
        "2", ModifiersState::SUPER; Action::SelectTab2;
        "3", ModifiersState::SUPER; Action::SelectTab3;
        "4", ModifiersState::SUPER; Action::SelectTab4;
        "5", ModifiersState::SUPER; Action::SelectTab5;
        "6", ModifiersState::SUPER; Action::SelectTab6;
        "7", ModifiersState::SUPER; Action::SelectTab7;
        "8", ModifiersState::SUPER; Action::SelectTab8;
        "9", ModifiersState::SUPER; Action::SelectLastTab;
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
        "n", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::WindowCreateNew;
        "t", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::TabCreateNew;
        Tab, ModifiersState::CONTROL; Action::SelectNextTab;
        "[", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::SelectNextTab;
        "]", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::SelectPrevTab;
        "w", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::TabCloseCurrent;
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
        "t", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::TabCreateNew;
        Tab, ModifiersState::CONTROL; Action::SelectNextTab;
        "w", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::TabCloseCurrent;
        "n", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::WindowCreateNew;
        "[", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::SelectNextTab;
        "]", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::SelectPrevTab;
    )
}

#[cfg(test)]
pub fn platform_key_bindings() -> Vec<KeyBinding> {
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    use winit::keyboard::ModifiersState;

    type MockBinding = Binding<usize>;

    impl Default for MockBinding {
        fn default() -> Self {
            Self {
                mods: Default::default(),
                action: Action::None,
                mode: BindingMode::empty(),
                notmode: BindingMode::empty(),
                trigger: Default::default(),
            }
        }
    }

    #[test]
    fn binding_matches_itself() {
        let binding = MockBinding::default();
        let identical_binding = MockBinding::default();

        assert!(binding.triggers_match(&identical_binding));
        assert!(identical_binding.triggers_match(&binding));
    }

    #[test]
    fn binding_matches_different_action() {
        let binding = MockBinding::default();
        let different_action = MockBinding {
            action: Action::ClearHistory,
            ..MockBinding::default()
        };

        assert!(binding.triggers_match(&different_action));
        assert!(different_action.triggers_match(&binding));
    }

    #[test]
    fn mods_binding_requires_strict_match() {
        let superset_mods = MockBinding {
            mods: ModifiersState::all(),
            ..MockBinding::default()
        };
        let subset_mods = MockBinding {
            mods: ModifiersState::ALT,
            ..MockBinding::default()
        };

        assert!(!superset_mods.triggers_match(&subset_mods));
        assert!(!subset_mods.triggers_match(&superset_mods));
    }

    #[test]
    fn binding_matches_identical_mode() {
        let b1 = MockBinding {
            mode: BindingMode::ALT_SCREEN,
            ..MockBinding::default()
        };
        let b2 = MockBinding {
            mode: BindingMode::ALT_SCREEN,
            ..MockBinding::default()
        };

        assert!(b1.triggers_match(&b2));
        assert!(b2.triggers_match(&b1));
    }

    #[test]
    fn binding_without_mode_matches_any_mode() {
        let b1 = MockBinding::default();
        let b2 = MockBinding {
            mode: BindingMode::APP_KEYPAD,
            notmode: BindingMode::ALT_SCREEN,
            ..MockBinding::default()
        };

        assert!(b1.triggers_match(&b2));
    }

    #[test]
    fn binding_with_mode_matches_empty_mode() {
        let b1 = MockBinding {
            mode: BindingMode::APP_KEYPAD,
            notmode: BindingMode::ALT_SCREEN,
            ..MockBinding::default()
        };
        let b2 = MockBinding::default();

        assert!(b1.triggers_match(&b2));
        assert!(b2.triggers_match(&b1));
    }

    #[test]
    fn binding_matches_modes() {
        let b1 = MockBinding {
            mode: BindingMode::ALT_SCREEN | BindingMode::APP_KEYPAD,
            ..MockBinding::default()
        };
        let b2 = MockBinding {
            mode: BindingMode::APP_KEYPAD,
            ..MockBinding::default()
        };

        assert!(b1.triggers_match(&b2));
        assert!(b2.triggers_match(&b1));
    }

    #[test]
    fn binding_matches_partial_intersection() {
        let b1 = MockBinding {
            mode: BindingMode::ALT_SCREEN | BindingMode::APP_KEYPAD,
            ..MockBinding::default()
        };
        let b2 = MockBinding {
            mode: BindingMode::APP_KEYPAD | BindingMode::APP_CURSOR,
            ..MockBinding::default()
        };

        assert!(b1.triggers_match(&b2));
        assert!(b2.triggers_match(&b1));
    }

    #[test]
    fn binding_mismatches_notmode() {
        let b1 = MockBinding {
            mode: BindingMode::ALT_SCREEN,
            ..MockBinding::default()
        };
        let b2 = MockBinding {
            notmode: BindingMode::ALT_SCREEN,
            ..MockBinding::default()
        };

        assert!(!b1.triggers_match(&b2));
        assert!(!b2.triggers_match(&b1));
    }

    #[test]
    fn binding_mismatches_unrelated() {
        let b1 = MockBinding {
            mode: BindingMode::ALT_SCREEN,
            ..MockBinding::default()
        };
        let b2 = MockBinding {
            mode: BindingMode::APP_KEYPAD,
            ..MockBinding::default()
        };

        assert!(!b1.triggers_match(&b2));
        assert!(!b2.triggers_match(&b1));
    }

    #[test]
    fn binding_matches_notmodes() {
        let subset_notmodes = MockBinding {
            notmode: BindingMode::VI | BindingMode::APP_CURSOR,
            ..MockBinding::default()
        };
        let superset_notmodes = MockBinding {
            notmode: BindingMode::APP_CURSOR,
            ..MockBinding::default()
        };

        assert!(subset_notmodes.triggers_match(&superset_notmodes));
        assert!(superset_notmodes.triggers_match(&subset_notmodes));
    }

    #[test]
    fn binding_matches_mode_notmode() {
        let b1 = MockBinding {
            mode: BindingMode::VI,
            notmode: BindingMode::APP_CURSOR,
            ..MockBinding::default()
        };
        let b2 = MockBinding {
            notmode: BindingMode::APP_CURSOR,
            ..MockBinding::default()
        };

        assert!(b1.triggers_match(&b2));
        assert!(b2.triggers_match(&b1));
    }

    // #[test]
    // fn binding_trigger_input() {
    //     let binding = MockBinding { trigger: 13, ..MockBinding::default() };

    //     let mods = binding.mods;
    //     let mode = binding.mode;

    //     assert!(binding.is_triggered_by(mode, mods, &13));
    //     assert!(!binding.is_triggered_by(mode, mods, &32));
    // }

    // #[test]
    // fn binding_trigger_mods() {
    //     let binding = MockBinding {
    //         mods: ModifiersState::ALT | ModifiersState::SUPER,
    //         ..MockBinding::default()
    //     };

    //     let superset_mods = ModifiersState::all();
    //     let subset_mods = ModifiersState::empty();

    //     let t = binding.trigger;
    //     let mode = binding.mode;

    //     assert!(binding.is_triggered_by(mode, binding.mods, &t));
    //     assert!(!binding.is_triggered_by(mode, superset_mods, &t));
    //     assert!(!binding.is_triggered_by(mode, subset_mods, &t));
    // }

    #[test]
    fn binding_trigger_modes() {
        let binding = MockBinding {
            mode: BindingMode::ALT_SCREEN,
            ..MockBinding::default()
        };

        let t = binding.trigger;
        let mods = binding.mods;

        assert!(!binding.is_triggered_by(BindingMode::VI, mods, &t));
        assert!(binding.is_triggered_by(BindingMode::ALT_SCREEN, mods, &t));
        assert!(binding.is_triggered_by(
            BindingMode::ALT_SCREEN | BindingMode::VI,
            mods,
            &t
        ));
    }

    #[test]
    fn binding_trigger_notmodes() {
        let binding = MockBinding {
            notmode: BindingMode::ALT_SCREEN,
            ..MockBinding::default()
        };

        let t = binding.trigger;
        let mods = binding.mods;

        assert!(binding.is_triggered_by(BindingMode::VI, mods, &t));
        assert!(!binding.is_triggered_by(BindingMode::ALT_SCREEN, mods, &t));
        assert!(!binding.is_triggered_by(
            BindingMode::ALT_SCREEN | BindingMode::VI,
            mods,
            &t
        ));
    }

    #[test]
    fn bindings_overwrite() {
        let bindings = bindings!(
            KeyBinding;
            "q", ModifiersState::SUPER; Action::Quit;
            ",", ModifiersState::SUPER; Action::ConfigEditor;
        );

        let config_bindings = vec![ConfigKeyBinding {
            key: String::from("q"),
            action: String::from("receivechar"),
            with: String::from("super"),
            bytes: vec![],
            text: String::from(""),
            mode: String::from(""),
        }];

        let new_bindings = config_key_bindings(config_bindings, bindings);

        assert_eq!(new_bindings.len(), 2);
        assert_eq!(new_bindings[1].action, Action::ReceiveChar);
    }
}
