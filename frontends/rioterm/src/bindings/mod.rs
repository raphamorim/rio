// Binding<T>, MouseAction, BindingMode, Action, default_key_bindings and including their comments
// was originally taken from https://github.com/alacritty/alacritty/blob/e35e5ad14fce8456afdd89f2b392b9924bb27471/alacritty/src/config/bindings.rs
// which is licensed under Apache 2.0 license.

pub mod kitty_keyboard;

use crate::crosswords::vi_mode::ViMotion;
use crate::crosswords::Mode;
use bitflags::bitflags;
use rio_backend::config::bindings::KeyBinding as ConfigKeyBinding;
use rio_backend::config::keyboard::Keyboard as ConfigKeyboard;
use rio_window::event::MouseButton;
use rio_window::keyboard::Key::*;
use rio_window::keyboard::NamedKey::*;
use rio_window::keyboard::{Key, KeyLocation, ModifiersState, PhysicalKey};
use std::fmt::Debug;
// use rio_window::platform::scancode::PhysicalKeyExtScancode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontSizeAction {
    Increase,
    Decrease,
    Reset,
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

/// Search mode specific actions.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SearchAction {
    /// Move the focus to the next search match.
    SearchFocusNext,
    /// Move the focus to the previous search match.
    SearchFocusPrevious,
    /// Confirm the active search.
    SearchConfirm,
    /// Cancel the active search.
    SearchCancel,
    /// Reset the search regex.
    SearchClear,
    /// Delete the last word in the search regex.
    SearchDeleteWord,
    /// Go to the previous regex in the search history.
    SearchHistoryPrevious,
    /// Go to the next regex in the search history.
    SearchHistoryNext,
}

impl From<SearchAction> for Action {
    fn from(action: SearchAction) -> Self {
        Self::Search(action)
    }
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
    Scancode(PhysicalKey),
    Keycode {
        key: Key,
        location: KeyLocation,
    },
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
        const SEARCH              = 0b0001_0000;
        const DISAMBIGUATE_KEYS   = 0b0010_0000;
        const ALL_KEYS_AS_ESC     = 0b0100_0000;
    }
}

impl BindingMode {
    pub fn new(mode: &Mode, search: bool) -> BindingMode {
        let mut binding_mode = BindingMode::empty();
        binding_mode.set(BindingMode::APP_CURSOR, mode.contains(Mode::APP_CURSOR));
        binding_mode.set(BindingMode::APP_KEYPAD, mode.contains(Mode::APP_KEYPAD));
        binding_mode.set(BindingMode::ALT_SCREEN, mode.contains(Mode::ALT_SCREEN));
        binding_mode.set(BindingMode::SEARCH, search);
        binding_mode.set(
            BindingMode::DISAMBIGUATE_KEYS,
            mode.contains(Mode::DISAMBIGUATE_ESC_CODES),
        );
        binding_mode.set(
            BindingMode::ALL_KEYS_AS_ESC,
            mode.contains(Mode::REPORT_ALL_KEYS_AS_ESC),
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
            "searchforward" => Some(Action::SearchForward),
            "searchbackward" => Some(Action::SearchBackward),
            "searchconfirm" => Some(Action::Search(SearchAction::SearchConfirm)),
            "searchcancel" => Some(Action::Search(SearchAction::SearchCancel)),
            "searchclear" => Some(Action::Search(SearchAction::SearchClear)),
            "searchfocusnext" => Some(Action::Search(SearchAction::SearchFocusNext)),
            "searchfocusprevious" => {
                Some(Action::Search(SearchAction::SearchFocusPrevious))
            }
            "searchdeleteword" => Some(Action::Search(SearchAction::SearchDeleteWord)),
            "searchhistorynext" => Some(Action::Search(SearchAction::SearchHistoryNext)),
            "searchhistoryprevious" => {
                Some(Action::Search(SearchAction::SearchHistoryPrevious))
            }
            "clearhistory" => Some(Action::ClearHistory),
            "resetfontsize" => Some(Action::ResetFontSize),
            "increasefontsize" => Some(Action::IncreaseFontSize),
            "decreasefontsize" => Some(Action::DecreaseFontSize),
            "createwindow" => Some(Action::WindowCreateNew),
            "createtab" => Some(Action::TabCreateNew),
            "movecurrenttabtoprev" => Some(Action::MoveCurrentTabToPrev),
            "movecurrenttabtonext" => Some(Action::MoveCurrentTabToNext),
            "closetab" => Some(Action::TabCloseCurrent),
            "closesplitortab" => Some(Action::CloseCurrentSplitOrTab),
            "closeunfocusedtabs" => Some(Action::TabCloseUnfocused),
            "openconfigeditor" => Some(Action::ConfigEditor),
            "selectprevtab" => Some(Action::SelectPrevTab),
            "selectnexttab" => Some(Action::SelectNextTab),
            "selectlasttab" => Some(Action::SelectLastTab),
            "receivechar" => Some(Action::ReceiveChar),
            "scrollpageup" => Some(Action::ScrollPageUp),
            "scrollpagedown" => Some(Action::ScrollPageDown),
            "scrollhalfpageup" => Some(Action::ScrollHalfPageUp),
            "scrollhalfpagedown" => Some(Action::ScrollHalfPageDown),
            "scrolltotop" => Some(Action::ScrollToTop),
            "scrolltobottom" => Some(Action::ScrollToBottom),
            "splitright" => Some(Action::SplitRight),
            "splitdown" => Some(Action::SplitDown),
            "selectnextsplit" => Some(Action::SelectNextSplit),
            "selectprevsplit" => Some(Action::SelectPrevSplit),
            "selectnextsplitortab" => Some(Action::SelectNextSplitOrTab),
            "selectprevsplitortab" => Some(Action::SelectPrevSplitOrTab),
            "movedividerup" => Some(Action::MoveDividerUp),
            "movedividerdown" => Some(Action::MoveDividerDown),
            "movedividerleft" => Some(Action::MoveDividerLeft),
            "movedividerright" => Some(Action::MoveDividerRight),
            "togglevimode" => Some(Action::ToggleViMode),
            "togglefullscreen" => Some(Action::ToggleFullscreen),
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

    /// Activate hint mode with the given hint index
    Hint(std::rc::Rc<rio_backend::config::hints::Hint>),

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

    /// Move current tab to previous slot.
    MoveCurrentTabToPrev,

    /// Move current tab to next slot.
    MoveCurrentTabToNext,

    /// Switch to next tab.
    SelectNextTab,

    /// Switch to prev tab.
    SelectPrevTab,

    /// Close tab.
    TabCloseCurrent,

    CloseCurrentSplitOrTab,

    /// Close all other tabs (leave only the current tab).
    TabCloseUnfocused,

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

    // Tab selections
    SelectTab(usize),
    SelectLastTab,

    Search(SearchAction),
    /// Start a forward buffer search.
    SearchForward,

    /// Start a backward buffer search.
    SearchBackward,

    /// Split horizontally
    SplitRight,

    /// Split vertically
    SplitDown,

    /// Select next split
    SelectNextSplit,

    /// Select previous split
    SelectPrevSplit,

    /// Select next split if available if not next tab
    SelectNextSplitOrTab,

    /// Select previous split if available if not previous tab
    SelectPrevSplitOrTab,

    /// Move divider up
    MoveDividerUp,

    /// Move divider down
    MoveDividerDown,

    /// Move divider left
    MoveDividerLeft,

    /// Move divider right
    MoveDividerRight,

    /// Allow receiving char input.
    ReceiveChar,

    /// No action.
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

pub fn default_mouse_bindings() -> Vec<MouseBinding> {
    bindings!(
        MouseBinding;
        MouseButton::Right;                            MouseAction::ExpandSelection;
        MouseButton::Right,   ModifiersState::CONTROL; MouseAction::ExpandSelection;
        MouseButton::Middle, ~BindingMode::VI;         Action::PasteSelection;
    )
}

pub fn default_key_bindings(config: &rio_backend::config::Config) -> Vec<KeyBinding> {
    let mut bindings = bindings!(
        KeyBinding;
        Key::Named(Copy);  Action::Copy;
        Key::Named(Copy),  +BindingMode::VI; Action::ClearSelection;
        Key::Named(Paste), ~BindingMode::VI; Action::Paste;
        Key::Character("l".into()), ModifiersState::CONTROL; Action::ClearLogNotice;
        "l",  ModifiersState::CONTROL, ~BindingMode::VI; Action::Esc("\x0c".into());
        Key::Named(Home),     ModifiersState::SHIFT, ~BindingMode::ALT_SCREEN; Action::ScrollToTop;
        Key::Named(End),      ModifiersState::SHIFT, ~BindingMode::ALT_SCREEN; Action::ScrollToBottom;
        Key::Named(PageUp),   ModifiersState::SHIFT, ~BindingMode::ALT_SCREEN; Action::ScrollPageUp;
        Key::Named(PageDown), ModifiersState::SHIFT, ~BindingMode::ALT_SCREEN; Action::ScrollPageDown;
        Key::Named(Home),  +BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1bOH".into());
        Key::Named(End),   +BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1bOF".into());
        Key::Named(ArrowUp),    +BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1bOA".into());
        Key::Named(ArrowDown),  +BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1bOB".into());
        Key::Named(ArrowRight), +BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1bOC".into());
        Key::Named(ArrowLeft),  +BindingMode::APP_CURSOR, ~BindingMode::VI;
            Action::Esc("\x1bOD".into());

        // VI Mode
        Key::Named(Space), ModifiersState::ALT | ModifiersState::SHIFT; Action::ToggleViMode;
        "/", +BindingMode::VI, ~BindingMode::SEARCH; Action::SearchForward;
        "n", +BindingMode::VI, ~BindingMode::SEARCH; SearchAction::SearchFocusNext;
        "n",  ModifiersState::SHIFT, +BindingMode::VI, ~BindingMode::SEARCH; SearchAction::SearchFocusPrevious;
        Key::Named(Enter), +BindingMode::SEARCH, ~BindingMode::VI; SearchAction::SearchFocusNext;
        Key::Named(Enter), +BindingMode::SEARCH, +BindingMode::VI; SearchAction::SearchConfirm;
        Key::Named(Escape), +BindingMode::SEARCH; SearchAction::SearchCancel;
        Key::Named(Enter), ModifiersState::SHIFT, +BindingMode::SEARCH, ~BindingMode::VI; SearchAction::SearchFocusPrevious;
        "i", +BindingMode::VI, ~BindingMode::SEARCH; Action::ToggleViMode;
        "c", ModifiersState::CONTROL, +BindingMode::VI; Action::ToggleViMode;
        Key::Named(Escape), +BindingMode::VI; Action::ClearSelection;
        "i", +BindingMode::VI, ~BindingMode::SEARCH; Action::ScrollToBottom;
        "g", +BindingMode::VI, ~BindingMode::SEARCH; Action::ScrollToTop;
        "g", ModifiersState::SHIFT, +BindingMode::VI, ~BindingMode::SEARCH; Action::ScrollToBottom;
        "b", ModifiersState::CONTROL, +BindingMode::VI, ~BindingMode::SEARCH; Action::ScrollPageUp;
        "f", ModifiersState::CONTROL, +BindingMode::VI, ~BindingMode::SEARCH; Action::ScrollPageDown;
        "u", ModifiersState::CONTROL, +BindingMode::VI, ~BindingMode::SEARCH; Action::ScrollHalfPageUp;
        "d", ModifiersState::CONTROL, +BindingMode::VI, ~BindingMode::SEARCH; Action::ScrollHalfPageDown;
        "y", ModifiersState::CONTROL,  +BindingMode::VI, ~BindingMode::SEARCH; Action::Scroll(1);
        "e", ModifiersState::CONTROL,  +BindingMode::VI, ~BindingMode::SEARCH; Action::Scroll(-1);
        "y", +BindingMode::VI, ~BindingMode::SEARCH; Action::Copy;
        "y", +BindingMode::VI, ~BindingMode::SEARCH; Action::ClearSelection;
        "v", +BindingMode::VI, ~BindingMode::SEARCH; ViAction::ToggleNormalSelection;
        "v", ModifiersState::SHIFT, +BindingMode::VI, ~BindingMode::SEARCH; ViAction::ToggleLineSelection;
        "v", ModifiersState::CONTROL, +BindingMode::VI, ~BindingMode::SEARCH; ViAction::ToggleBlockSelection;
        "v", ModifiersState::ALT, +BindingMode::VI, ~BindingMode::SEARCH; ViAction::ToggleSemanticSelection;
        "z", +BindingMode::VI, ~BindingMode::SEARCH; ViAction::CenterAroundViCursor;
        "k", +BindingMode::VI, ~BindingMode::SEARCH; ViMotion::Up;
        "j", +BindingMode::VI, ~BindingMode::SEARCH; ViMotion::Down;
        "h", +BindingMode::VI, ~BindingMode::SEARCH; ViMotion::Left;
        "l", +BindingMode::VI, ~BindingMode::SEARCH; ViMotion::Right;
        Key::Named(ArrowUp), +BindingMode::VI; ViMotion::Up;
        Key::Named(ArrowDown), +BindingMode::VI; ViMotion::Down;
        Key::Named(ArrowLeft), +BindingMode::VI; ViMotion::Left;
        Key::Named(ArrowRight), +BindingMode::VI; ViMotion::Right;
        Key::Named(ArrowUp), ModifiersState::SUPER, ~BindingMode::VI; Action::None;
        Key::Named(ArrowDown), ModifiersState::SUPER, ~BindingMode::VI; Action::None;
        Key::Named(ArrowLeft), ModifiersState::SUPER, ~BindingMode::VI; Action::None;
        Key::Named(ArrowRight), ModifiersState::SUPER, ~BindingMode::VI; Action::None;
        "0",                          +BindingMode::VI, ~BindingMode::SEARCH;
            ViMotion::First;
        "4",   ModifiersState::SHIFT, +BindingMode::VI, ~BindingMode::SEARCH;
            ViMotion::Last;
        "6",   ModifiersState::SHIFT, +BindingMode::VI, ~BindingMode::SEARCH;
            ViMotion::FirstOccupied;
        "h",      ModifiersState::SHIFT, +BindingMode::VI, ~BindingMode::SEARCH;
            ViMotion::High;
        "m",      ModifiersState::SHIFT, +BindingMode::VI, ~BindingMode::SEARCH;
            ViMotion::Middle;
        "l",      ModifiersState::SHIFT, +BindingMode::VI, ~BindingMode::SEARCH;
            ViMotion::Low;
        "b",                             +BindingMode::VI, ~BindingMode::SEARCH;
            ViMotion::SemanticLeft;
        "w",                             +BindingMode::VI, ~BindingMode::SEARCH;
            ViMotion::SemanticRight;
        "e",                             +BindingMode::VI, ~BindingMode::SEARCH;
            ViMotion::SemanticRightEnd;
        "b",      ModifiersState::SHIFT, +BindingMode::VI, ~BindingMode::SEARCH;
            ViMotion::WordLeft;
        "w",      ModifiersState::SHIFT, +BindingMode::VI, ~BindingMode::SEARCH;
            ViMotion::WordRight;
        "e",      ModifiersState::SHIFT, +BindingMode::VI, ~BindingMode::SEARCH;
            ViMotion::WordRightEnd;
        "5",   ModifiersState::SHIFT, +BindingMode::VI, ~BindingMode::SEARCH;
            ViMotion::Bracket;
    );

    bindings.extend(bindings!(
        KeyBinding;
        Key::Named(ArrowUp), ~BindingMode::APP_CURSOR, ~BindingMode::VI, ~BindingMode::SEARCH, ~BindingMode::ALL_KEYS_AS_ESC; Action::Esc("\x1b[A".into());
        Key::Named(ArrowDown), ~BindingMode::APP_CURSOR, ~BindingMode::VI, ~BindingMode::SEARCH, ~BindingMode::ALL_KEYS_AS_ESC; Action::Esc("\x1b[B".into());
        Key::Named(ArrowRight), ~BindingMode::APP_CURSOR, ~BindingMode::VI, ~BindingMode::SEARCH, ~BindingMode::ALL_KEYS_AS_ESC; Action::Esc("\x1b[C".into());
        Key::Named(ArrowLeft),  ~BindingMode::APP_CURSOR, ~BindingMode::VI, ~BindingMode::SEARCH, ~BindingMode::ALL_KEYS_AS_ESC; Action::Esc("\x1b[D".into());
        Key::Named(Insert),     ~BindingMode::VI, ~BindingMode::SEARCH, ~BindingMode::ALL_KEYS_AS_ESC, ~BindingMode::DISAMBIGUATE_KEYS; Action::Esc("\x1b[2~".into());
        Key::Named(Delete),     ~BindingMode::VI, ~BindingMode::SEARCH, ~BindingMode::ALL_KEYS_AS_ESC, ~BindingMode::DISAMBIGUATE_KEYS; Action::Esc("\x1b[3~".into());
        Key::Named(PageUp),     ~BindingMode::VI, ~BindingMode::SEARCH, ~BindingMode::ALL_KEYS_AS_ESC, ~BindingMode::DISAMBIGUATE_KEYS; Action::Esc("\x1b[5~".into());
        Key::Named(PageDown),   ~BindingMode::VI, ~BindingMode::SEARCH, ~BindingMode::ALL_KEYS_AS_ESC, ~BindingMode::DISAMBIGUATE_KEYS; Action::Esc("\x1b[6~".into());
        Key::Named(Backspace),  ~BindingMode::VI, ~BindingMode::SEARCH, ~BindingMode::ALL_KEYS_AS_ESC; Action::Esc("\x7f".into());
        Key::Named(Backspace), ModifiersState::ALT,     ~BindingMode::VI, ~BindingMode::SEARCH, ~BindingMode::ALL_KEYS_AS_ESC, ~BindingMode::DISAMBIGUATE_KEYS; Action::Esc("\x1b\x7f".into());
        Key::Named(Backspace), ModifiersState::SHIFT,   ~BindingMode::VI, ~BindingMode::SEARCH, ~BindingMode::ALL_KEYS_AS_ESC, ~BindingMode::DISAMBIGUATE_KEYS; Action::Esc("\x7f".into());
        Key::Named(F1), ~BindingMode::VI, ~BindingMode::SEARCH, ~BindingMode::ALL_KEYS_AS_ESC, ~BindingMode::DISAMBIGUATE_KEYS; Action::Esc("\x1bOP".into());
        Key::Named(F2), ~BindingMode::VI, ~BindingMode::SEARCH, ~BindingMode::ALL_KEYS_AS_ESC, ~BindingMode::DISAMBIGUATE_KEYS; Action::Esc("\x1bOQ".into());
        Key::Named(F3), ~BindingMode::VI, ~BindingMode::SEARCH, ~BindingMode::ALL_KEYS_AS_ESC, ~BindingMode::DISAMBIGUATE_KEYS; Action::Esc("\x1bOR".into());
        Key::Named(F4), ~BindingMode::VI, ~BindingMode::SEARCH, ~BindingMode::ALL_KEYS_AS_ESC, ~BindingMode::DISAMBIGUATE_KEYS; Action::Esc("\x1bOS".into());
        Key::Named(Tab),       ModifiersState::SHIFT,   ~BindingMode::VI,   ~BindingMode::SEARCH, ~BindingMode::ALL_KEYS_AS_ESC, ~BindingMode::DISAMBIGUATE_KEYS; Action::Esc("\x1b[Z".into());
        Key::Named(Tab),       ModifiersState::SHIFT | ModifiersState::ALT, ~BindingMode::VI, ~BindingMode::SEARCH, ~BindingMode::ALL_KEYS_AS_ESC, ~BindingMode::DISAMBIGUATE_KEYS; Action::Esc("\x1b\x1b[Z".into());
    ));

    bindings.extend(platform_key_bindings(
        config.navigation.has_navigation_key_bindings(),
        config.navigation.use_split,
        config.keyboard,
    ));

    // Add hint bindings
    bindings.extend(create_hint_bindings(&config.hints.rules));

    config_key_bindings(config.bindings.keys.to_owned(), bindings)
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
            "home" => (Key::Named(Home), KeyLocation::Standard),
            "space" => (Key::Named(Space), KeyLocation::Standard),
            "delete" => (Key::Named(Delete), KeyLocation::Standard),
            "esc" => (Key::Named(Escape), KeyLocation::Standard),
            "insert" => (Key::Named(Insert), KeyLocation::Standard),
            "pageup" => (Key::Named(PageUp), KeyLocation::Standard),
            "pagedown" => (Key::Named(PageDown), KeyLocation::Standard),
            "end" => (Key::Named(End), KeyLocation::Standard),
            "up" => (Key::Named(ArrowUp), KeyLocation::Standard),
            "back" => (Key::Named(Backspace), KeyLocation::Standard),
            "down" => (Key::Named(ArrowDown), KeyLocation::Standard),
            "left" => (Key::Named(ArrowLeft), KeyLocation::Standard),
            "right" => (Key::Named(ArrowRight), KeyLocation::Standard),
            "@" => (Key::Character("@".into()), KeyLocation::Standard),
            "colon" => (Key::Character(":".into()), KeyLocation::Standard),
            "." => (Key::Character(".".into()), KeyLocation::Standard),
            "return" => (Key::Named(Enter), KeyLocation::Standard),
            "[" => (Key::Character("[".into()), KeyLocation::Standard),
            "]" => (Key::Character("]".into()), KeyLocation::Standard),
            "{" => (Key::Character("{".into()), KeyLocation::Standard),
            "}" => (Key::Character("}".into()), KeyLocation::Standard),
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
            "numpadenter" => (Key::Named(Enter), KeyLocation::Numpad),
            "numpadadd" => (Key::Character("+".into()), KeyLocation::Numpad),
            "numpadcomma" => (Key::Character(",".into()), KeyLocation::Numpad),
            "numpaddecimal" => (Key::Character(".".into()), KeyLocation::Numpad),
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
            "tab" => (Key::Named(Tab), KeyLocation::Standard),
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

    let mut action: Action = config_key_binding.action.into();
    if !config_key_binding.esc.is_empty() {
        action = Action::Esc(config_key_binding.esc);
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
            Ok(key_binding) => {
                // Remove any default binding that would conflict with this user binding
                // This ensures user bindings always take precedence and prevents conflicts
                bindings.retain(|b| !b.triggers_match(&key_binding));

                tracing::info!("added a new key_binding: {:?}", key_binding);
                bindings.push(key_binding)
            }
            Err(err_message) => {
                tracing::error!("error loading a key binding: {:?}", err_message);
            }
        }
    }

    bindings
}

/// Create hint bindings from configuration
pub fn create_hint_bindings(
    hints_config: &[rio_backend::config::hints::Hint],
) -> Vec<KeyBinding> {
    let mut hint_bindings = Vec::new();

    for hint_config in hints_config {
        if let Some(binding_config) = &hint_config.binding {
            // Parse key using the same logic as in convert()
            let (key, location) = match binding_config.key.to_lowercase().as_str() {
                // Letters
                single_char if single_char.len() == 1 => {
                    (Key::Character(single_char.into()), KeyLocation::Standard)
                }
                // Named keys
                "space" => (Key::Named(Space), KeyLocation::Standard),
                "enter" | "return" => (Key::Named(Enter), KeyLocation::Standard),
                "escape" | "esc" => (Key::Named(Escape), KeyLocation::Standard),
                "tab" => (Key::Named(Tab), KeyLocation::Standard),
                "backspace" => (Key::Named(Backspace), KeyLocation::Standard),
                "delete" => (Key::Named(Delete), KeyLocation::Standard),
                "insert" => (Key::Named(Insert), KeyLocation::Standard),
                "home" => (Key::Named(Home), KeyLocation::Standard),
                "end" => (Key::Named(End), KeyLocation::Standard),
                "pageup" => (Key::Named(PageUp), KeyLocation::Standard),
                "pagedown" => (Key::Named(PageDown), KeyLocation::Standard),
                "up" => (Key::Named(ArrowUp), KeyLocation::Standard),
                "down" => (Key::Named(ArrowDown), KeyLocation::Standard),
                "left" => (Key::Named(ArrowLeft), KeyLocation::Standard),
                "right" => (Key::Named(ArrowRight), KeyLocation::Standard),
                // Function keys
                "f1" => (Key::Named(F1), KeyLocation::Standard),
                "f2" => (Key::Named(F2), KeyLocation::Standard),
                "f3" => (Key::Named(F3), KeyLocation::Standard),
                "f4" => (Key::Named(F4), KeyLocation::Standard),
                "f5" => (Key::Named(F5), KeyLocation::Standard),
                "f6" => (Key::Named(F6), KeyLocation::Standard),
                "f7" => (Key::Named(F7), KeyLocation::Standard),
                "f8" => (Key::Named(F8), KeyLocation::Standard),
                "f9" => (Key::Named(F9), KeyLocation::Standard),
                "f10" => (Key::Named(F10), KeyLocation::Standard),
                "f11" => (Key::Named(F11), KeyLocation::Standard),
                "f12" => (Key::Named(F12), KeyLocation::Standard),
                _ => {
                    tracing::warn!(
                        "Unknown key '{}' in hint binding",
                        binding_config.key
                    );
                    continue;
                }
            };

            // Parse modifiers
            let mut mods = ModifiersState::empty();
            for mod_str in &binding_config.mods {
                match mod_str.to_lowercase().as_str() {
                    "control" | "ctrl" => mods |= ModifiersState::CONTROL,
                    "shift" => mods |= ModifiersState::SHIFT,
                    "alt" | "option" => mods |= ModifiersState::ALT,
                    "super" | "cmd" | "command" => mods |= ModifiersState::SUPER,
                    _ => {
                        tracing::warn!("Unknown modifier '{}' in hint binding", mod_str);
                    }
                }
            }

            let hint_binding = KeyBinding {
                trigger: BindingKey::Keycode { key, location },
                mods,
                mode: BindingMode::empty(),
                notmode: BindingMode::SEARCH | BindingMode::VI,
                action: Action::Hint(std::rc::Rc::new(hint_config.clone())),
            };

            hint_bindings.push(hint_binding);
        }
    }

    hint_bindings
}

// Macos
#[cfg(all(target_os = "macos", not(test)))]
pub fn platform_key_bindings(
    use_navigation_key_bindings: bool,
    use_splits: bool,
    config_keyboard: ConfigKeyboard,
) -> Vec<KeyBinding> {
    let mut key_bindings = bindings!(
        KeyBinding;
        "0", ModifiersState::SUPER; Action::ResetFontSize;
        "=", ModifiersState::SUPER; Action::IncreaseFontSize;
        "+", ModifiersState::SUPER; Action::IncreaseFontSize;
        "+", ModifiersState::SUPER; Action::IncreaseFontSize;
        "-", ModifiersState::SUPER; Action::DecreaseFontSize;
        "-", ModifiersState::SUPER; Action::DecreaseFontSize;
        Key::Named(Insert), ModifiersState::SHIFT, ~BindingMode::VI, ~BindingMode::SEARCH;
            Action::Esc("\x1b[2;2~".into());
        "k", ModifiersState::SUPER, ~BindingMode::VI, ~BindingMode::SEARCH;
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
        ",", ModifiersState::SUPER; Action::ConfigEditor;

        // Search
        "f", ModifiersState::SUPER, ~BindingMode::SEARCH; Action::SearchForward;
        "b", ModifiersState::SUPER, ~BindingMode::SEARCH; Action::SearchBackward;
        "c", ModifiersState::CONTROL, +BindingMode::SEARCH; SearchAction::SearchCancel;
        "u", ModifiersState::CONTROL, +BindingMode::SEARCH; SearchAction::SearchClear;
        "w", ModifiersState::CONTROL,  +BindingMode::SEARCH; SearchAction::SearchDeleteWord;
        "p", ModifiersState::CONTROL,  +BindingMode::SEARCH; SearchAction::SearchHistoryPrevious;
        "n", ModifiersState::CONTROL,  +BindingMode::SEARCH; SearchAction::SearchHistoryNext;
        Key::Named(ArrowUp), +BindingMode::SEARCH; SearchAction::SearchHistoryPrevious;
        Key::Named(ArrowDown), +BindingMode::SEARCH; SearchAction::SearchHistoryNext;
    );

    if use_navigation_key_bindings {
        key_bindings.extend(bindings!(
            KeyBinding;
            "t", ModifiersState::SUPER; Action::TabCreateNew;
            Key::Named(Tab), ModifiersState::CONTROL; Action::SelectNextTab;
            Key::Named(Tab), ModifiersState::CONTROL | ModifiersState::SHIFT; Action::SelectPrevTab;
            "w", ModifiersState::SUPER; Action::CloseCurrentSplitOrTab;
            "[", ModifiersState::SUPER | ModifiersState::SHIFT; Action::SelectPrevTab;
            "]", ModifiersState::SUPER | ModifiersState::SHIFT; Action::SelectNextTab;
            "1", ModifiersState::SUPER; Action::SelectTab(0);
            "2", ModifiersState::SUPER; Action::SelectTab(1);
            "3", ModifiersState::SUPER; Action::SelectTab(2);
            "4", ModifiersState::SUPER; Action::SelectTab(3);
            "5", ModifiersState::SUPER; Action::SelectTab(4);
            "6", ModifiersState::SUPER; Action::SelectTab(5);
            "7", ModifiersState::SUPER; Action::SelectTab(6);
            "8", ModifiersState::SUPER; Action::SelectTab(7);
            "9", ModifiersState::SUPER; Action::SelectLastTab;
        ));
    }

    if config_keyboard.disable_ctlseqs_alt {
        key_bindings.extend(bindings!(
            KeyBinding;
            Key::Named(ArrowLeft), ModifiersState::ALT,  ~BindingMode::VI;
                Action::Esc("\x1bb".into());
            Key::Named(ArrowRight), ModifiersState::ALT,  ~BindingMode::VI;
                Action::Esc("\x1bf".into());
        ));
    }

    if use_splits {
        key_bindings.extend(bindings!(
            KeyBinding;
            "d", ModifiersState::SUPER, ~BindingMode::SEARCH, ~BindingMode::VI; Action::SplitRight;
            "d", ModifiersState::SUPER | ModifiersState::SHIFT, ~BindingMode::SEARCH, ~BindingMode::VI; Action::SplitDown;
            "]", ModifiersState::SUPER, ~BindingMode::SEARCH, ~BindingMode::VI; Action::SelectNextSplit;
            "[", ModifiersState::SUPER, ~BindingMode::SEARCH, ~BindingMode::VI; Action::SelectPrevSplit;
            Key::Named(ArrowUp), ModifiersState::CONTROL | ModifiersState::SUPER, ~BindingMode::SEARCH, ~BindingMode::VI; Action::MoveDividerUp;
            Key::Named(ArrowDown), ModifiersState::CONTROL | ModifiersState::SUPER, ~BindingMode::SEARCH, ~BindingMode::VI; Action::MoveDividerDown;
            Key::Named(ArrowLeft), ModifiersState::CONTROL | ModifiersState::SUPER, ~BindingMode::SEARCH, ~BindingMode::VI; Action::MoveDividerLeft;
            Key::Named(ArrowRight), ModifiersState::CONTROL | ModifiersState::SUPER, ~BindingMode::SEARCH, ~BindingMode::VI; Action::MoveDividerRight;
        ));
    }

    key_bindings
}

// Not Windows, Macos
#[cfg(not(any(target_os = "macos", target_os = "windows", test)))]
pub fn platform_key_bindings(
    use_navigation_key_bindings: bool,
    use_splits: bool,
    _: ConfigKeyboard,
) -> Vec<KeyBinding> {
    let mut key_bindings = bindings!(
        KeyBinding;
        "v", ModifiersState::CONTROL | ModifiersState::SHIFT, ~BindingMode::VI; Action::Paste;
        "c", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::Copy;
        "c", ModifiersState::CONTROL | ModifiersState::SHIFT,
            +BindingMode::VI; Action::ClearSelection;
        Key::Named(Insert),   ModifiersState::SHIFT, ~BindingMode::VI; Action::PasteSelection;
        "0", ModifiersState::CONTROL;  Action::ResetFontSize;
        "=", ModifiersState::CONTROL;  Action::IncreaseFontSize;
        "+", ModifiersState::CONTROL;  Action::IncreaseFontSize;
        "+", ModifiersState::CONTROL;  Action::IncreaseFontSize;
        "-", ModifiersState::CONTROL;  Action::DecreaseFontSize;
        "-", ModifiersState::CONTROL;  Action::DecreaseFontSize;
        "n", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::WindowCreateNew;
        ",", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::ConfigEditor;

        // Search
        "f", ModifiersState::CONTROL | ModifiersState::SHIFT, ~BindingMode::SEARCH; Action::SearchForward;
        "b", ModifiersState::CONTROL | ModifiersState::SHIFT, ~BindingMode::SEARCH; Action::SearchBackward;
        "c", ModifiersState::CONTROL, +BindingMode::SEARCH; SearchAction::SearchCancel;
        "u", ModifiersState::CONTROL, +BindingMode::SEARCH; SearchAction::SearchClear;
        "w", ModifiersState::CONTROL,  +BindingMode::SEARCH; SearchAction::SearchDeleteWord;
        "p", ModifiersState::CONTROL,  +BindingMode::SEARCH; SearchAction::SearchHistoryPrevious;
        "n", ModifiersState::CONTROL,  +BindingMode::SEARCH; SearchAction::SearchHistoryNext;
        Key::Named(ArrowUp), +BindingMode::SEARCH; SearchAction::SearchHistoryPrevious;
        Key::Named(ArrowDown), +BindingMode::SEARCH; SearchAction::SearchHistoryNext;
    );

    if use_navigation_key_bindings {
        key_bindings.extend(bindings!(
            KeyBinding;
            "t", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::TabCreateNew;
            Key::Named(Tab), ModifiersState::CONTROL; Action::SelectNextTab;
            Key::Named(Tab), ModifiersState::CONTROL | ModifiersState::SHIFT; Action::SelectPrevTab;
            "[", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::SelectPrevTab;
            "]", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::SelectNextTab;
            "w", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::CloseCurrentSplitOrTab;
        ));
    }

    if use_splits {
        key_bindings.extend(bindings!(
            KeyBinding;
            "r", ModifiersState::CONTROL | ModifiersState::SHIFT, ~BindingMode::SEARCH, ~BindingMode::VI; Action::SplitRight;
            "d", ModifiersState::CONTROL | ModifiersState::SHIFT, ~BindingMode::SEARCH, ~BindingMode::VI; Action::SplitDown;
            "]", ModifiersState::CONTROL | ModifiersState::SHIFT, ~BindingMode::SEARCH, ~BindingMode::VI; Action::SelectNextSplit;
            "[", ModifiersState::CONTROL | ModifiersState::SHIFT, ~BindingMode::SEARCH, ~BindingMode::VI; Action::SelectPrevSplit;
            Key::Named(ArrowUp), ModifiersState::CONTROL | ModifiersState::SHIFT | ModifiersState::ALT, ~BindingMode::SEARCH, ~BindingMode::VI; Action::MoveDividerUp;
            Key::Named(ArrowDown), ModifiersState::CONTROL | ModifiersState::SHIFT | ModifiersState::ALT, ~BindingMode::SEARCH, ~BindingMode::VI; Action::MoveDividerDown;
            Key::Named(ArrowLeft), ModifiersState::CONTROL | ModifiersState::SHIFT | ModifiersState::ALT, ~BindingMode::SEARCH, ~BindingMode::VI; Action::MoveDividerLeft;
            Key::Named(ArrowRight), ModifiersState::CONTROL | ModifiersState::SHIFT | ModifiersState::ALT, ~BindingMode::SEARCH, ~BindingMode::VI; Action::MoveDividerRight;
        ));
    }

    key_bindings
}

// Windows
#[cfg(all(target_os = "windows", not(test)))]
pub fn platform_key_bindings(
    use_navigation_key_bindings: bool,
    use_splits: bool,
    _: ConfigKeyboard,
) -> Vec<KeyBinding> {
    let mut key_bindings = bindings!(
        KeyBinding;
        "v", ModifiersState::CONTROL | ModifiersState::SHIFT, ~BindingMode::VI; Action::Paste;
        "c", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::Copy;
        "c", ModifiersState::CONTROL | ModifiersState::SHIFT, +BindingMode::VI; Action::ClearSelection;
        Key::Named(Insert), ModifiersState::SHIFT, ~BindingMode::VI; Action::PasteSelection;
        "0", ModifiersState::CONTROL; Action::ResetFontSize;
        "=", ModifiersState::CONTROL; Action::IncreaseFontSize;
        "+", ModifiersState::CONTROL; Action::IncreaseFontSize;
        "+", ModifiersState::CONTROL; Action::IncreaseFontSize;
        "-", ModifiersState::CONTROL; Action::DecreaseFontSize;
        "-", ModifiersState::CONTROL; Action::DecreaseFontSize;
        Key::Named(Enter), ModifiersState::ALT; Action::ToggleFullscreen;
        "n", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::WindowCreateNew;
        ",", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::ConfigEditor;
        // This is actually a Windows Powershell shortcut
        // https://github.com/alacritty/alacritty/issues/2930
        // https://github.com/raphamorim/rio/issues/220#issuecomment-1761651339
        Key::Named(Backspace), ModifiersState::CONTROL, ~BindingMode::VI; Action::Esc("\u{0017}".into());
        Key::Named(Space), ModifiersState::CONTROL | ModifiersState::SHIFT; Action::ToggleViMode;

        // Search
        "f", ModifiersState::CONTROL | ModifiersState::SHIFT, ~BindingMode::SEARCH; Action::SearchForward;
        "b", ModifiersState::CONTROL | ModifiersState::SHIFT, ~BindingMode::SEARCH; Action::SearchBackward;
        "c", ModifiersState::CONTROL, +BindingMode::SEARCH; SearchAction::SearchCancel;
        "u", ModifiersState::CONTROL, +BindingMode::SEARCH; SearchAction::SearchClear;
        "w", ModifiersState::CONTROL,  +BindingMode::SEARCH; SearchAction::SearchDeleteWord;
        "p", ModifiersState::CONTROL,  +BindingMode::SEARCH; SearchAction::SearchHistoryPrevious;
        "n", ModifiersState::CONTROL,  +BindingMode::SEARCH; SearchAction::SearchHistoryNext;
        Key::Named(ArrowUp), +BindingMode::SEARCH; SearchAction::SearchHistoryPrevious;
        Key::Named(ArrowDown), +BindingMode::SEARCH; SearchAction::SearchHistoryNext;
    );

    if use_navigation_key_bindings {
        key_bindings.extend(bindings!(
            KeyBinding;
            "t", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::TabCreateNew;
            Key::Named(Tab), ModifiersState::CONTROL; Action::SelectNextTab;
            Key::Named(Tab), ModifiersState::CONTROL | ModifiersState::SHIFT; Action::SelectPrevTab;
            "w", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::CloseCurrentSplitOrTab;
            "[", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::SelectPrevTab;
            "]", ModifiersState::CONTROL | ModifiersState::SHIFT; Action::SelectNextTab;
        ));
    }

    if use_splits {
        key_bindings.extend(bindings!(
            KeyBinding;
            "r", ModifiersState::CONTROL | ModifiersState::SHIFT, ~BindingMode::SEARCH, ~BindingMode::VI; Action::SplitRight;
            "d", ModifiersState::CONTROL | ModifiersState::SHIFT, ~BindingMode::SEARCH, ~BindingMode::VI; Action::SplitDown;
            "]", ModifiersState::CONTROL | ModifiersState::SHIFT, ~BindingMode::SEARCH, ~BindingMode::VI; Action::SelectNextSplit;
            "[", ModifiersState::CONTROL | ModifiersState::SHIFT, ~BindingMode::SEARCH, ~BindingMode::VI; Action::SelectPrevSplit;
            Key::Named(ArrowUp), ModifiersState::CONTROL | ModifiersState::SHIFT | ModifiersState::ALT, ~BindingMode::SEARCH, ~BindingMode::VI; Action::MoveDividerUp;
            Key::Named(ArrowDown), ModifiersState::CONTROL | ModifiersState::SHIFT | ModifiersState::ALT, ~BindingMode::SEARCH, ~BindingMode::VI; Action::MoveDividerDown;
            Key::Named(ArrowLeft), ModifiersState::CONTROL | ModifiersState::SHIFT | ModifiersState::ALT, ~BindingMode::SEARCH, ~BindingMode::VI; Action::MoveDividerLeft;
            Key::Named(ArrowRight), ModifiersState::CONTROL | ModifiersState::SHIFT | ModifiersState::ALT, ~BindingMode::SEARCH, ~BindingMode::VI; Action::MoveDividerRight;
        ));
    }

    // Note: Hint bindings are added separately in Screen::new() based on config

    key_bindings
}

#[cfg(test)]
pub fn platform_key_bindings(_: bool, _: bool, _: ConfigKeyboard) -> Vec<KeyBinding> {
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    use rio_window::keyboard::ModifiersState;

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
            esc: String::from(""),
            mode: String::from(""),
        }];

        let new_bindings = config_key_bindings(config_bindings, bindings);

        assert_eq!(new_bindings.len(), 2);
        assert_eq!(new_bindings[1].action, Action::ReceiveChar);
    }

    #[test]
    fn bindings_conflict_resolution() {
        // Test that conflicting bindings are properly replaced
        let bindings = bindings!(
            KeyBinding;
            Key::Named(PageUp), ModifiersState::empty(); Action::Esc("\x1b[5~".into());
            Key::Named(PageDown), ModifiersState::empty(); Action::Esc("\x1b[6~".into());
        );

        // User wants to use PageUp/PageDown for scrolling
        let config_bindings = vec![
            ConfigKeyBinding {
                key: String::from("pageup"),
                action: String::from("scroll(1)"),
                with: String::from(""),
                esc: String::from(""),
                mode: String::from(""),
            },
            ConfigKeyBinding {
                key: String::from("pagedown"),
                action: String::from("scroll(-1)"),
                with: String::from(""),
                esc: String::from(""),
                mode: String::from(""),
            },
        ];

        let new_bindings = config_key_bindings(config_bindings, bindings);

        // Should have 2 bindings (the original defaults should be replaced)
        assert_eq!(new_bindings.len(), 2);

        // Check that the actions were updated to scroll actions
        let has_scroll_actions = new_bindings
            .iter()
            .any(|b| matches!(b.action, Action::Scroll(_)));
        assert!(has_scroll_actions);
    }

    #[test]
    fn bindings_alt_enter_conflict_resolution() {
        // Test Windows Alt+Enter conflict resolution
        let bindings = bindings!(
            KeyBinding;
            Key::Named(Enter), ModifiersState::ALT; Action::ToggleFullscreen;
        );

        // User wants to use Alt+Enter for a custom action
        let config_bindings = vec![ConfigKeyBinding {
            key: String::from("return"),
            action: String::from("scroll(1)"),
            with: String::from("alt"),
            esc: String::from(""),
            mode: String::from(""),
        }];

        let new_bindings = config_key_bindings(config_bindings, bindings);

        // Should have 1 binding (the original Alt+Enter should be replaced)
        assert_eq!(new_bindings.len(), 1);

        assert_eq!(&new_bindings[0].action, &Action::Scroll(1));
    }
}
