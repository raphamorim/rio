// Binding<T>, MouseAction, BindingMode, Action, default_key_bindings and including their comments
// was originally taken from https://github.com/alacritty/alacritty/blob/e35e5ad14fce8456afdd89f2b392b9924bb27471/alacritty/src/config/bindings.rs
// which is licensed under Apache 2.0 license.

pub mod kitty_keyboard_protocol;

use crate::crosswords::vi_mode::ViMotion;
use crate::crosswords::Mode;
use bitflags::bitflags;
use rio_backend::config::bindings::KeyBinding as ConfigKeyBinding;
use rio_backend::config::keyboard::Keyboard as ConfigKeyboard;
use std::fmt::Debug;
use wa::{KeyCode, Modifiers, MouseButton};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding<T> {
    /// Modifier keys required to activate binding.
    pub mods: Modifiers,

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
    pub fn is_triggered_by(&self, mode: BindingMode, mods: Modifiers, input: &T) -> bool {
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
    Keycode { key: KeyCode },
}

pub type KeyBinding = Binding<BindingKey>;
pub type KeyBindings = Vec<KeyBinding>;

/// Bindings that are triggered by a mouse button.
pub type MouseBinding = Binding<MouseButton>;

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

#[derive(Debug, Clone, Eq, PartialEq)]
#[allow(unused)]
pub enum Program {
    Just(String),
    WithArgs { program: String, args: Vec<String> },
}

impl Program {
    pub fn program(&self) -> &str {
        match self {
            Program::Just(program) => program,
            Program::WithArgs { program, .. } => program,
        }
    }

    pub fn args(&self) -> &[String] {
        match self {
            Program::Just(_) => &[],
            Program::WithArgs { args, .. } => args,
        }
    }
}

impl From<String> for Action {
    fn from(action: String) -> Action {
        let action = action.to_lowercase();

        let action_from_string = match action.as_str() {
            "paste" => Some(Action::Paste),
            "quit" => Some(Action::Quit),
            "copy" => Some(Action::Copy),
            "clearhistory" => Some(Action::ClearHistory),
            "resetfontsize" => Some(Action::ResetFontSize),
            "increasefontsize" => Some(Action::IncreaseFontSize),
            "decreasefontsize" => Some(Action::DecreaseFontSize),
            "createwindow" => Some(Action::WindowCreateNew),
            "createtab" => Some(Action::TabCreateNew),
            "closetab" => Some(Action::TabCloseCurrent),
            "openconfigeditor" => Some(Action::ConfigEditor),
            "selectprevtab" => Some(Action::SelectPrevTab),
            "selectnexttab" => Some(Action::SelectNextTab),
            "selectlasttab" => Some(Action::SelectLastTab),
            "receivechar" => Some(Action::ReceiveChar),
            "scrollhalfpageup" => Some(Action::ScrollHalfPageUp),
            "scrollhalfpagedown" => Some(Action::ScrollHalfPageDown),
            "scrolltotop" => Some(Action::ScrollToTop),
            "scrolltobottom" => Some(Action::ScrollToBottom),
            "togglevimode" => Some(Action::ToggleViMode),
            "none" => Some(Action::None),
            _ => None,
        };

        if action_from_string.is_some() {
            return action_from_string.unwrap_or(Action::None);
        }

        let re = regex::Regex::new(r"selecttab\(([^()]+)\)").unwrap();
        for capture in re.captures_iter(&action) {
            if let Some(matched) = capture.get(1) {
                let matched_string = matched.as_str().to_string();
                let parsed_matched_string: usize = matched_string.parse().unwrap_or(0);
                return Action::SelectTab(parsed_matched_string);
            }
        }

        let re = regex::Regex::new(r"run\(([^()]+)\)").unwrap();
        for capture in re.captures_iter(&action) {
            if let Some(matched) = capture.get(1) {
                let matched_string = matched.as_str().to_string();
                if matched_string.contains(' ') {
                    let mut vec_program_with_args: Vec<String> =
                        matched_string.split(' ').map(|s| s.to_string()).collect();
                    if vec_program_with_args.is_empty() {
                        continue;
                    }

                    let program = vec_program_with_args[0].to_string();
                    vec_program_with_args.remove(0);

                    return Action::Run(Program::WithArgs {
                        program,
                        args: vec_program_with_args,
                    });
                } else {
                    return Action::Run(Program::Just(matched_string));
                }
            }
        }

        let re = regex::Regex::new(r"scroll\(([^()]+)\)").unwrap();
        for capture in re.captures_iter(&action) {
            if let Some(matched) = capture.get(1) {
                let matched_string = matched.as_str().to_string();
                let parsed_matched_string: i32 = matched_string.parse().unwrap_or(1);
                return Action::Scroll(parsed_matched_string);
            }
        }

        Action::None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Write an escape sequence.
    Esc(String),

    /// Run given command.
    Run(Program),

    /// Scroll
    Scroll(i32),

    /// Regex keyboard hints.
    // Hint(Hint),

    // Move vi mode cursor.
    ViMotion(ViMotion),

    // Perform vi mode action.
    Vi(ViAction),
    /// Perform mouse binding exclusive action.
    Mouse(MouseAction),

    /// Paste contents of system clipboard.
    Paste,

    /// Store current selection into clipboard.
    Copy,

    #[cfg(not(any(target_os = "macos", windows)))]
    #[allow(dead_code)]
    /// Store current selection into selection buffer.
    CopySelection,

    /// Paste contents of selection buffer.
    PasteSelection,

    /// Increase font size.
    IncreaseFontSize,

    /// Decrease font size.
    DecreaseFontSize,

    /// Reset font size to the config value.
    ResetFontSize,

    /// Scroll exactly one page up.
    ScrollPageUp,

    /// Scroll exactly one page down.
    ScrollPageDown,

    /// Scroll half a page up.
    ScrollHalfPageUp,

    /// Scroll half a page down.
    ScrollHalfPageDown,

    /// Scroll all the way to the top.
    ScrollToTop,

    /// Scroll all the way to the bottom.
    ScrollToBottom,

    /// Clear the display buffer(s) to remove history.
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
    ConfigEditor,

    /// Create a new Rio tab.
    TabCreateNew,

    /// Switch to next tab.
    SelectNextTab,

    /// Switch to prev tab.
    SelectPrevTab,

    /// Close tab.
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
    SelectTab(usize),
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

impl From<ViAction> for Action {
    fn from(action: ViAction) -> Self {
        Self::Vi(action)
    }
}

/// Vi mode specific actions.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ViAction {
    /// Toggle normal vi selection.
    ToggleNormalSelection,
    /// Toggle line vi selection.
    ToggleLineSelection,
    /// Toggle block vi selection.
    ToggleBlockSelection,
    /// Toggle semantic vi selection.
    ToggleSemanticSelection,
    /// Centers the screen around the vi mode cursor.
    CenterAroundViCursor,
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
            let mut _mods = Modifiers::empty();
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
    (KeyBinding, $key:expr,) => {{
        BindingKey::Keycode { key: $key }
    }};
    ($ty:ident, $key:expr,) => {{
        $key
    }};
}

/// Mouse binding specific actions.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum MouseAction {
    /// Expand the selection to the current mouse cursor position.
    ExpandSelection,
}

impl From<MouseAction> for Action {
    fn from(action: MouseAction) -> Self {
        Self::Mouse(action)
    }
}

pub fn default_mouse_bindings() -> Vec<MouseBinding> {
    bindings!(
        MouseBinding;
        MouseButton::Right;                            MouseAction::ExpandSelection;
        MouseButton::Right,   Modifiers::CONTROL; MouseAction::ExpandSelection;
        MouseButton::Middle, ~BindingMode::VI;         Action::PasteSelection;
    )
}

pub fn default_key_bindings(
    unprocessed_config_key_bindings: Vec<ConfigKeyBinding>,
    use_navigation_key_bindings: bool,
    config_keyboard: ConfigKeyboard,
) -> Vec<KeyBinding> {
    let mut bindings = bindings!(
        KeyBinding;
        // Key::Named(Copy);  Action::Copy;
        // Key::Named(Copy),  +BindingMode::VI; Action::ClearSelection;
        // Key::Named(Paste), ~BindingMode::VI; Action::Paste;
        KeyCode::L, Modifiers::CONTROL; Action::ClearLogNotice;
        KeyCode::L,  Modifiers::CONTROL, ~BindingMode::VI; Action::Esc("\x0c".into());
        KeyCode::Tab,  Modifiers::SHIFT, ~BindingMode::VI; Action::Esc("\x1b[Z".into());
        KeyCode::Home,     Modifiers::SHIFT, ~BindingMode::ALT_SCREEN; Action::ScrollToTop;
        KeyCode::End,      Modifiers::SHIFT, ~BindingMode::ALT_SCREEN; Action::ScrollToBottom;
        KeyCode::PageUp,   Modifiers::SHIFT, ~BindingMode::ALT_SCREEN; Action::ScrollPageUp;
        KeyCode::PageDown, Modifiers::SHIFT, ~BindingMode::ALT_SCREEN; Action::ScrollPageDown;
        KeyCode::Home,  +BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1bOH".into());
        KeyCode::End,   +BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1bOF".into());
        KeyCode::Up,    +BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1bOA".into());
        KeyCode::Down,  +BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1bOB".into());
        KeyCode::Right, +BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1bOC".into());
        KeyCode::Left,  +BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1bOD".into());

        // VI Mode
        KeyCode::Space, Modifiers::ALT | Modifiers::SHIFT; Action::ToggleViMode;
        KeyCode::I, +BindingMode::VI; Action::ToggleViMode;
        KeyCode::C, Modifiers::CONTROL, +BindingMode::VI; Action::ToggleViMode;
        KeyCode::Escape, +BindingMode::VI; Action::ClearSelection;
        KeyCode::I, +BindingMode::VI; Action::ScrollToBottom;
        KeyCode::G, +BindingMode::VI; Action::ScrollToTop;
        KeyCode::G, Modifiers::SHIFT, +BindingMode::VI; Action::ScrollToBottom;
        KeyCode::B, Modifiers::CONTROL, +BindingMode::VI; Action::ScrollPageUp;
        KeyCode::F, Modifiers::CONTROL, +BindingMode::VI; Action::ScrollPageDown;
        KeyCode::U, Modifiers::CONTROL, +BindingMode::VI; Action::ScrollHalfPageUp;
        KeyCode::D, Modifiers::CONTROL, +BindingMode::VI; Action::ScrollHalfPageDown;
        KeyCode::Y, Modifiers::CONTROL,  +BindingMode::VI; Action::Scroll(1);
        KeyCode::E, Modifiers::CONTROL,  +BindingMode::VI; Action::Scroll(-1);
        KeyCode::Y, +BindingMode::VI; Action::Copy;
        KeyCode::Y, +BindingMode::VI; Action::ClearSelection;
        KeyCode::V, +BindingMode::VI; ViAction::ToggleNormalSelection;
        KeyCode::V, Modifiers::SHIFT, +BindingMode::VI; ViAction::ToggleLineSelection;
        KeyCode::V, Modifiers::CONTROL, +BindingMode::VI; ViAction::ToggleBlockSelection;
        KeyCode::V, Modifiers::ALT, +BindingMode::VI; ViAction::ToggleSemanticSelection;
        KeyCode::Z, +BindingMode::VI; ViAction::CenterAroundViCursor;
        KeyCode::K, +BindingMode::VI; ViMotion::Up;
        KeyCode::J, +BindingMode::VI; ViMotion::Down;
        KeyCode::H, +BindingMode::VI; ViMotion::Left;
        KeyCode::L, +BindingMode::VI; ViMotion::Right;
        KeyCode::Up, +BindingMode::VI; ViMotion::Up;
        KeyCode::Down, +BindingMode::VI; ViMotion::Down;
        KeyCode::Left, +BindingMode::VI; ViMotion::Left;
        KeyCode::Right, +BindingMode::VI; ViMotion::Right;
        KeyCode::Key0,                          +BindingMode::VI;
            ViMotion::First;
        KeyCode::Key4,   Modifiers::SHIFT, +BindingMode::VI;
            ViMotion::Last;
        KeyCode::Key6,   Modifiers::SHIFT, +BindingMode::VI;
            ViMotion::FirstOccupied;
        KeyCode::H,      Modifiers::SHIFT, +BindingMode::VI;
            ViMotion::High;
        KeyCode::M,      Modifiers::SHIFT, +BindingMode::VI;
            ViMotion::Middle;
        KeyCode::L,      Modifiers::SHIFT, +BindingMode::VI;
            ViMotion::Low;
        KeyCode::B,                             +BindingMode::VI;
            ViMotion::SemanticLeft;
        KeyCode::W,                             +BindingMode::VI;
            ViMotion::SemanticRight;
        KeyCode::E,                             +BindingMode::VI;
            ViMotion::SemanticRightEnd;
        KeyCode::B,      Modifiers::SHIFT, +BindingMode::VI;
            ViMotion::WordLeft;
        KeyCode::W,      Modifiers::SHIFT, +BindingMode::VI;
            ViMotion::WordRight;
        KeyCode::E,      Modifiers::SHIFT, +BindingMode::VI;
            ViMotion::WordRightEnd;
        KeyCode::Key5,   Modifiers::SHIFT, +BindingMode::VI;
            ViMotion::Bracket;
    );

    if !config_keyboard.use_kitty_keyboard_protocol {
        bindings.extend(bindings!(
                KeyBinding;
                KeyCode::Home, Modifiers::SHIFT, +BindingMode::ALT_SCREEN, ~BindingMode::VI; Action::Esc("\x1b[1;2H".into());
                KeyCode::End, Modifiers::SHIFT, +BindingMode::ALT_SCREEN, ~BindingMode::VI; Action::Esc("\x1b[1;2F".into());
                KeyCode::End,  ~BindingMode::APP_CURSOR, ~BindingMode::VI; Action::Esc("\x1b[F".into());
                KeyCode::PageUp, Modifiers::SHIFT, +BindingMode::ALT_SCREEN, ~BindingMode::VI; Action::Esc("\x1b[5;2~".into());
                KeyCode::PageDown, Modifiers::SHIFT, +BindingMode::ALT_SCREEN, ~BindingMode::VI; Action::Esc("\x1b[6;2~".into());
                KeyCode::Home,  ~BindingMode::APP_CURSOR, ~BindingMode::VI; Action::Esc("\x1b[H".into());
                KeyCode::Up, ~BindingMode::APP_CURSOR, ~BindingMode::VI; Action::Esc("\x1b[A".into());
                KeyCode::Down, ~BindingMode::APP_CURSOR, ~BindingMode::VI; Action::Esc("\x1b[B".into());
                KeyCode::Right, ~BindingMode::APP_CURSOR, ~BindingMode::VI; Action::Esc("\x1b[C".into());
                KeyCode::Left,  ~BindingMode::APP_CURSOR, ~BindingMode::VI; Action::Esc("\x1b[D".into());
                KeyCode::Backspace, ~BindingMode::VI; Action::Esc("\x7f".into());
                KeyCode::Insert, ~BindingMode::VI; Action::Esc("\x1b[2~".into());
                KeyCode::Delete, ~BindingMode::VI; Action::Esc("\x1b[3~".into());
                KeyCode::PageUp, ~BindingMode::VI; Action::Esc("\x1b[5~".into());
                KeyCode::PageDown, ~BindingMode::VI; Action::Esc("\x1b[6~".into());
                KeyCode::F1, ~BindingMode::VI; Action::Esc("\x1bOP".into());
                KeyCode::F2, ~BindingMode::VI; Action::Esc("\x1bOQ".into());
                KeyCode::F3, ~BindingMode::VI; Action::Esc("\x1bOR".into());
                KeyCode::F4, ~BindingMode::VI; Action::Esc("\x1bOS".into());
                KeyCode::F5, ~BindingMode::VI; Action::Esc("\x1b[15~".into());
                KeyCode::F6, ~BindingMode::VI; Action::Esc("\x1b[17~".into());
                KeyCode::F7, ~BindingMode::VI; Action::Esc("\x1b[18~".into());
                KeyCode::F8, ~BindingMode::VI; Action::Esc("\x1b[19~".into());
                KeyCode::F9, ~BindingMode::VI; Action::Esc("\x1b[20~".into());
                KeyCode::F10, ~BindingMode::VI; Action::Esc("\x1b[21~".into());
                KeyCode::F11, ~BindingMode::VI; Action::Esc("\x1b[23~".into());
                KeyCode::F12, ~BindingMode::VI; Action::Esc("\x1b[24~".into());
                KeyCode::F13, ~BindingMode::VI; Action::Esc("\x1b[25~".into());
                KeyCode::F14, ~BindingMode::VI; Action::Esc("\x1b[26~".into());
                KeyCode::F15, ~BindingMode::VI; Action::Esc("\x1b[28~".into());
                KeyCode::F16, ~BindingMode::VI; Action::Esc("\x1b[29~".into());
                KeyCode::F17, ~BindingMode::VI; Action::Esc("\x1b[31~".into());
                KeyCode::F18, ~BindingMode::VI; Action::Esc("\x1b[32~".into());
                KeyCode::F19, ~BindingMode::VI; Action::Esc("\x1b[33~".into());
                KeyCode::F20, ~BindingMode::VI; Action::Esc("\x1b[34~".into());
            ));

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
            Modifiers::SHIFT,
            Modifiers::ALT,
            Modifiers::SHIFT | Modifiers::ALT,
            Modifiers::CONTROL,
            Modifiers::SHIFT | Modifiers::CONTROL,
            Modifiers::ALT | Modifiers::CONTROL,
            Modifiers::SHIFT | Modifiers::ALT | Modifiers::CONTROL,
        ];

        for (index, mods) in modifiers.drain(..).enumerate() {
            // If disable_ctlseqs_alt is enabled, should ignore ALT
            // Useful for example if want same behaviour that Terminal.app have
            // Since Terminal.app does not deal with ctlseqs with ALT keys
            if index == 1 && config_keyboard.disable_ctlseqs_alt {
                continue;
            }

            let modifiers_code = index + 2;
            bindings.extend(bindings!(
                KeyBinding;
                KeyCode::Delete, mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[3;{}~", modifiers_code));
                KeyCode::Up,     mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[1;{}A", modifiers_code));
                KeyCode::Down,   mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[1;{}B", modifiers_code));
                KeyCode::Right,  mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[1;{}C", modifiers_code));
                KeyCode::Left,   mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[1;{}D", modifiers_code));
                KeyCode::F1,     mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[1;{}P", modifiers_code));
                KeyCode::F2,     mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[1;{}Q", modifiers_code));
                KeyCode::F3,     mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[1;{}R", modifiers_code));
                KeyCode::F4,     mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[1;{}S", modifiers_code));
                KeyCode::F5,     mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[15;{}~", modifiers_code));
                KeyCode::F6,     mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[17;{}~", modifiers_code));
                KeyCode::F7,     mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[18;{}~", modifiers_code));
                KeyCode::F8,     mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[19;{}~", modifiers_code));
                KeyCode::F9,     mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[20;{}~", modifiers_code));
                KeyCode::F10,    mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[21;{}~", modifiers_code));
                KeyCode::F11,    mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[23;{}~", modifiers_code));
                KeyCode::F12,    mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[24;{}~", modifiers_code));
                KeyCode::F13,    mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[25;{}~", modifiers_code));
                KeyCode::F14,    mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[26;{}~", modifiers_code));
                KeyCode::F15,    mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[28;{}~", modifiers_code));
                KeyCode::F16,    mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[29;{}~", modifiers_code));
                KeyCode::F17,    mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[31;{}~", modifiers_code));
                KeyCode::F18,    mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[32;{}~", modifiers_code));
                KeyCode::F19,    mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[33;{}~", modifiers_code));
                KeyCode::F20,    mods, ~BindingMode::VI;
                    Action::Esc(format!("\x1b[34;{}~", modifiers_code));
            ));

            // We're adding the following bindings with `Shift` manually above, so skipping them here.
            if modifiers_code != 2 {
                bindings.extend(bindings!(
                    KeyBinding;
                    KeyCode::Insert,   mods, ~BindingMode::VI;
                        Action::Esc(format!("\x1b[2;{}~", modifiers_code));
                    KeyCode::PageUp,   mods, ~BindingMode::VI;
                        Action::Esc(format!("\x1b[5;{}~", modifiers_code));
                    KeyCode::PageDown, mods, ~BindingMode::VI;
                        Action::Esc(format!("\x1b[6;{}~", modifiers_code));
                    KeyCode::End,      mods, ~BindingMode::VI;
                        Action::Esc(format!("\x1b[1;{}F", modifiers_code));
                    KeyCode::Home,     mods, ~BindingMode::VI;
                        Action::Esc(format!("\x1b[1;{}H", modifiers_code));
                ));
            }
        }
    } else {
        bindings.extend(bindings!(
            KeyBinding;
            KeyCode::Up, ~BindingMode::APP_CURSOR, ~BindingMode::VI, ~BindingMode::ALL_KEYS_AS_ESC; Action::Esc("\x1b[A".into());
            KeyCode::Down, ~BindingMode::APP_CURSOR, ~BindingMode::VI, ~BindingMode::ALL_KEYS_AS_ESC; Action::Esc("\x1b[B".into());
            KeyCode::Right, ~BindingMode::APP_CURSOR, ~BindingMode::VI, ~BindingMode::ALL_KEYS_AS_ESC; Action::Esc("\x1b[C".into());
            KeyCode::Left,  ~BindingMode::APP_CURSOR, ~BindingMode::VI, ~BindingMode::ALL_KEYS_AS_ESC; Action::Esc("\x1b[D".into());
            KeyCode::Insert,     ~BindingMode::VI, ~BindingMode::ALL_KEYS_AS_ESC, ~BindingMode::DISAMBIGUATE_KEYS; Action::Esc("\x1b[2~".into());
            KeyCode::Delete,     ~BindingMode::VI, ~BindingMode::ALL_KEYS_AS_ESC, ~BindingMode::DISAMBIGUATE_KEYS; Action::Esc("\x1b[3~".into());
            KeyCode::PageUp,     ~BindingMode::VI, ~BindingMode::ALL_KEYS_AS_ESC, ~BindingMode::DISAMBIGUATE_KEYS; Action::Esc("\x1b[5~".into());
            KeyCode::PageDown,   ~BindingMode::VI, ~BindingMode::ALL_KEYS_AS_ESC, ~BindingMode::DISAMBIGUATE_KEYS; Action::Esc("\x1b[6~".into());
            KeyCode::Backspace,  ~BindingMode::VI, ~BindingMode::ALL_KEYS_AS_ESC; Action::Esc("\x7f".into());
            KeyCode::Backspace, Modifiers::ALT,     ~BindingMode::VI, ~BindingMode::ALL_KEYS_AS_ESC; Action::Esc("\x1b\x7f".into());
            KeyCode::Backspace, Modifiers::SHIFT,   ~BindingMode::VI, ~BindingMode::ALL_KEYS_AS_ESC; Action::Esc("\x7f".into());
            KeyCode::F1, ~BindingMode::VI, ~BindingMode::ALL_KEYS_AS_ESC, ~BindingMode::DISAMBIGUATE_KEYS; Action::Esc("\x1bOP".into());
            KeyCode::F2, ~BindingMode::VI, ~BindingMode::ALL_KEYS_AS_ESC, ~BindingMode::DISAMBIGUATE_KEYS; Action::Esc("\x1bOQ".into());
            KeyCode::F3, ~BindingMode::VI, ~BindingMode::ALL_KEYS_AS_ESC, ~BindingMode::DISAMBIGUATE_KEYS; Action::Esc("\x1bOR".into());
            KeyCode::F4, ~BindingMode::VI, ~BindingMode::ALL_KEYS_AS_ESC, ~BindingMode::DISAMBIGUATE_KEYS; Action::Esc("\x1bOS".into());
            KeyCode::Tab, Modifiers::SHIFT, ~BindingMode::VI,   ~BindingMode::ALL_KEYS_AS_ESC; Action::Esc("\x1b[Z".into());
            KeyCode::Tab, Modifiers::SHIFT | Modifiers::ALT, ~BindingMode::VI, ~BindingMode::ALL_KEYS_AS_ESC; Action::Esc("\x1b\x1b[Z".into());
        ));
    }

    bindings.extend(platform_key_bindings(
        use_navigation_key_bindings,
        config_keyboard,
    ));

    config_key_bindings(unprocessed_config_key_bindings, bindings)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModeWrapper {
    pub mode: BindingMode,
    pub not_mode: BindingMode,
}

#[inline]
fn convert(config_key_binding: ConfigKeyBinding) -> Result<KeyBinding, String> {
    let key = if config_key_binding.key.chars().count() == 1 {
        config_key_binding
            .key
            .to_lowercase()
            .as_str()
            .try_into()
            .unwrap()
    } else {
        match config_key_binding.key.to_lowercase().as_str() {
            "home" => KeyCode::Home,
            "space" => KeyCode::Space,
            "delete" => KeyCode::Delete,
            "esc" => KeyCode::Escape,
            "insert" => KeyCode::Insert,
            "pageup" => KeyCode::PageUp,
            "pagedown" => KeyCode::PageDown,
            "end" => KeyCode::End,
            "up" => KeyCode::Up,
            "back" => KeyCode::Backspace,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            // "@" => (Key::Character("@".into()), KeyLocation::Standard),
            // "colon" => (Key::Character(":".into()), KeyLocation::Standard),
            // "." => (Key::Character(".".into()), KeyLocation::Standard),
            // "return" => (Key::Named(Enter), KeyLocation::Standard),
            // "[" => (Key::Character("[".into()), KeyLocation::Standard),
            // "]" => (Key::Character("]".into()), KeyLocation::Standard),
            // ";" => (Key::Character(";".into()), KeyLocation::Standard),
            // "\\" => (Key::Character("\\".into()), KeyLocation::Standard),
            // "+" => (Key::Character("+".into()), KeyLocation::Standard),
            // "," => (Key::Character(",".into()), KeyLocation::Standard),
            // "/" => (Key::Character("/".into()), KeyLocation::Standard),
            // "=" => (Key::Character("=".into()), KeyLocation::Standard),
            // "-" => (Key::Character("-".into()), KeyLocation::Standard),
            // "*" => (Key::Character("*".into()), KeyLocation::Standard),
            "1" => KeyCode::Key1,
            "2" => KeyCode::Key2,
            "3" => KeyCode::Key3,
            "4" => KeyCode::Key4,
            "5" => KeyCode::Key5,
            "6" => KeyCode::Key6,
            "7" => KeyCode::Key7,
            "8" => KeyCode::Key8,
            "9" => KeyCode::Key9,
            "0" => KeyCode::Key0,

            // Special case numpad.
            // "numpadenter" => (Key::Named(Enter), KeyLocation::Numpad),
            // "numpadadd" => (Key::Character("+".into()), KeyLocation::Numpad),
            // "numpadcomma" => (Key::Character(",".into()), KeyLocation::Numpad),
            // "numpaddivide" => (Key::Character("/".into()), KeyLocation::Numpad),
            // "numpadequals" => (Key::Character("=".into()), KeyLocation::Numpad),
            // "numpadsubtract" => (Key::Character("-".into()), KeyLocation::Numpad),
            // "numpadmultiply" => (Key::Character("*".into()), KeyLocation::Numpad),
            "numpad1" => KeyCode::Key1,
            "numpad2" => KeyCode::Key2,
            "numpad3" => KeyCode::Key3,
            "numpad4" => KeyCode::Key4,
            "numpad5" => KeyCode::Key5,
            "numpad6" => KeyCode::Key6,
            "numpad7" => KeyCode::Key7,
            "numpad8" => KeyCode::Key8,
            "numpad9" => KeyCode::Key9,
            "numpad0" => KeyCode::Key0,

            // Special cases
            "tab" => KeyCode::Tab,
            _ => return Err("Unable to find defined 'keycode'".to_string()),
        }
    };

    let trigger = BindingKey::Keycode { key };

    let mut res = Modifiers::empty();
    for modifier in config_key_binding.with.split('|') {
        match modifier.trim().to_lowercase().as_str() {
            "command" | "super" => res.insert(Modifiers::SUPER),
            "shift" => res.insert(Modifiers::SHIFT),
            "alt" | "option" => res.insert(Modifiers::ALT),
            "control" => res.insert(Modifiers::CONTROL),
            "none" => (),
            _ => (),
        }
    }

    let mut action: Action = config_key_binding.action.into();
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
                            "overwritten a previous key_binding with new one: {:?}",
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
pub fn platform_key_bindings(
    use_navigation_key_bindings: bool,
    config_keyboard: ConfigKeyboard,
) -> Vec<KeyBinding> {
    let mut key_bindings = bindings!(
        KeyBinding;
        KeyCode::Key0, Modifiers::SUPER; Action::ResetFontSize;
        KeyCode::Equal, Modifiers::SUPER; Action::IncreaseFontSize;
        // KeyCode::Plus, Modifiers::SUPER; Action::IncreaseFontSize;
        // KeyCode::Plus, Modifiers::SUPER; Action::IncreaseFontSize;
        KeyCode::Minus, Modifiers::SUPER; Action::DecreaseFontSize;
        KeyCode::Minus, Modifiers::SUPER; Action::DecreaseFontSize;
        KeyCode::Insert, Modifiers::SHIFT, ~BindingMode::VI;
            Action::Esc("\x1b[2;2~".into());
        KeyCode::K, Modifiers::SUPER, ~BindingMode::VI;
            Action::Esc("\x0c".into());
        KeyCode::K, Modifiers::SUPER, ~BindingMode::VI;  Action::ClearHistory;
        KeyCode::V, Modifiers::SUPER, ~BindingMode::VI; Action::Paste;
        KeyCode::F, Modifiers::CONTROL | Modifiers::SUPER; Action::ToggleFullscreen;
        KeyCode::C, Modifiers::SUPER; Action::Copy;
        KeyCode::C, Modifiers::SUPER, +BindingMode::VI; Action::ClearSelection;
        KeyCode::H, Modifiers::SUPER; Action::Hide;
        KeyCode::H, Modifiers::SUPER | Modifiers::ALT; Action::HideOtherApplications;
        KeyCode::M, Modifiers::SUPER; Action::Minimize;
        KeyCode::Q, Modifiers::SUPER; Action::Quit;
        KeyCode::N, Modifiers::SUPER; Action::WindowCreateNew;
        KeyCode::Comma, Modifiers::SUPER; Action::ConfigEditor;
    );

    if use_navigation_key_bindings {
        key_bindings.extend(bindings!(
            KeyBinding;
            KeyCode::T, Modifiers::SUPER; Action::TabCreateNew;
            KeyCode::Tab, Modifiers::CONTROL; Action::SelectNextTab;
            KeyCode::Tab, Modifiers::CONTROL | Modifiers::SHIFT; Action::SelectPrevTab;
            KeyCode::W, Modifiers::SUPER; Action::TabCloseCurrent;
            KeyCode::LeftBracket, Modifiers::SUPER | Modifiers::SHIFT; Action::SelectPrevTab;
            KeyCode::RightBracket, Modifiers::SUPER | Modifiers::SHIFT; Action::SelectNextTab;
            KeyCode::Key1, Modifiers::SUPER; Action::SelectTab(0);
            KeyCode::Key2, Modifiers::SUPER; Action::SelectTab(1);
            KeyCode::Key3, Modifiers::SUPER; Action::SelectTab(2);
            KeyCode::Key4, Modifiers::SUPER; Action::SelectTab(3);
            KeyCode::Key5, Modifiers::SUPER; Action::SelectTab(4);
            KeyCode::Key6, Modifiers::SUPER; Action::SelectTab(5);
            KeyCode::Key7, Modifiers::SUPER; Action::SelectTab(6);
            KeyCode::Key8, Modifiers::SUPER; Action::SelectTab(7);
            KeyCode::Key9, Modifiers::SUPER; Action::SelectLastTab;
        ));
    }

    if config_keyboard.disable_ctlseqs_alt {
        key_bindings.extend(bindings!(
            KeyBinding;
            KeyCode::Left, Modifiers::ALT,  ~BindingMode::VI;
                Action::Esc("\x1bb".into());
            KeyCode::Right, Modifiers::ALT,  ~BindingMode::VI;
                Action::Esc("\x1bf".into());
        ));
    }

    key_bindings
}

// Not Windows, Macos
#[cfg(not(any(target_os = "macos", target_os = "windows", test)))]
pub fn platform_key_bindings(
    use_navigation_key_bindings: bool,
    _: ConfigKeyboard,
) -> Vec<KeyBinding> {
    let mut key_bindings = bindings!(
        KeyBinding;
        KeyCode::V,        Modifiers::CONTROL | Modifiers::SHIFT, ~BindingMode::VI; Action::Paste;
        KeyCode::C,        Modifiers::CONTROL | Modifiers::SHIFT; Action::Copy;
        KeyCode::C,        Modifiers::CONTROL | Modifiers::SHIFT,
            +BindingMode::VI; Action::ClearSelection;
        KeyCode::Insert,   Modifiers::SHIFT, ~BindingMode::VI; Action::PasteSelection;
        "0",     Modifiers::CONTROL;  Action::ResetFontSize;
        "=",   Modifiers::CONTROL;  Action::IncreaseFontSize;
        "+",     Modifiers::CONTROL;  Action::IncreaseFontSize;
        "+",      Modifiers::CONTROL;  Action::IncreaseFontSize;
        "-",          Modifiers::CONTROL;  Action::DecreaseFontSize;
        "-", Modifiers::CONTROL;  Action::DecreaseFontSize;
        "n", Modifiers::CONTROL | Modifiers::SHIFT; Action::WindowCreateNew;
        ",", Modifiers::CONTROL | Modifiers::SHIFT; Action::ConfigEditor;
    );

    if use_navigation_key_bindings {
        key_bindings.extend(bindings!(
            KeyBinding;
            "t", Modifiers::CONTROL | Modifiers::SHIFT; Action::TabCreateNew;
            Key::Named(Tab), Modifiers::CONTROL; Action::SelectNextTab;
            Key::Named(Tab), Modifiers::CONTROL | Modifiers::SHIFT; Action::SelectPrevTab;
            "[", Modifiers::CONTROL | Modifiers::SHIFT; Action::SelectPrevTab;
            "]", Modifiers::CONTROL | Modifiers::SHIFT; Action::SelectNextTab;
            "w", Modifiers::CONTROL | Modifiers::SHIFT; Action::TabCloseCurrent;
        ));
    }

    key_bindings
}

// Windows
#[cfg(all(target_os = "windows", not(test)))]
pub fn platform_key_bindings(
    use_navigation_key_bindings: bool,
    _: ConfigKeyboard,
) -> Vec<KeyBinding> {
    let mut key_bindings = bindings!(
        KeyBinding;
        "v", Modifiers::CONTROL | Modifiers::SHIFT, ~BindingMode::VI; Action::Paste;
        "c", Modifiers::CONTROL | Modifiers::SHIFT; Action::Copy;
        "c", Modifiers::CONTROL | Modifiers::SHIFT, +BindingMode::VI; Action::ClearSelection;
        Key::Named(Insert), Modifiers::SHIFT, ~BindingMode::VI; Action::PasteSelection;
        "0", Modifiers::CONTROL; Action::ResetFontSize;
        "=", Modifiers::CONTROL; Action::IncreaseFontSize;
        "+", Modifiers::CONTROL; Action::IncreaseFontSize;
        "+", Modifiers::CONTROL; Action::IncreaseFontSize;
        "-", Modifiers::CONTROL; Action::DecreaseFontSize;
        "-", Modifiers::CONTROL; Action::DecreaseFontSize;
        Key::Named(Enter), Modifiers::ALT; Action::ToggleFullscreen;
        "n", Modifiers::CONTROL | Modifiers::SHIFT; Action::WindowCreateNew;
        ",", Modifiers::CONTROL | Modifiers::SHIFT; Action::ConfigEditor;
        // This is actually a Windows Powershell shortcut
        // https://github.com/alacritty/alacritty/issues/2930
        // https://github.com/raphamorim/rio/issues/220#issuecomment-1761651339
        Key::Named(Backspace), Modifiers::CONTROL, ~BindingMode::VI; Action::Esc("\u{0017}".into());
        Key::Named(Space), Modifiers::CONTROL | Modifiers::SHIFT, Action::ToggleViMode;
    );

    if use_navigation_key_bindings {
        key_bindings.extend(bindings!(
            KeyBinding;
            "t", Modifiers::CONTROL | Modifiers::SHIFT; Action::TabCreateNew;
            Key::Named(Tab), Modifiers::CONTROL; Action::SelectNextTab;
            Key::Named(Tab), Modifiers::CONTROL | Modifiers::SHIFT; Action::SelectPrevTab;
            "w", Modifiers::CONTROL | Modifiers::SHIFT; Action::TabCloseCurrent;
            "[", Modifiers::CONTROL | Modifiers::SHIFT; Action::SelectPrevTab;
            "]", Modifiers::CONTROL | Modifiers::SHIFT; Action::SelectNextTab;
        ));
    }

    key_bindings
}

#[cfg(test)]
pub fn platform_key_bindings(_: bool, _: ConfigKeyboard) -> Vec<KeyBinding> {
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;
    use wa::Modifiers;

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
            mods: Modifiers::all(),
            ..MockBinding::default()
        };
        let subset_mods = MockBinding {
            mods: Modifiers::ALT,
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
    //         mods: Modifiers::ALT | Modifiers::SUPER,
    //         ..MockBinding::default()
    //     };

    //     let superset_mods = Modifiers::all();
    //     let subset_mods = Modifiers::empty();

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
            KeyCode::Q, Modifiers::SUPER; Action::Quit;
            KeyCode::Comma, Modifiers::SUPER; Action::ConfigEditor;
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
