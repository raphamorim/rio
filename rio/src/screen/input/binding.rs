// Cria os bindings e usa struct actions
// 

#[derive(ConfigDeserialize, Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Write an escape sequence.
    #[config(skip)]
    Esc(String),

    /// Run given command.
    #[config(skip)]
    Command(Program),

    /// Regex keyboard hints.
    #[config(skip)]
    Hint(Hint),

    /// Move vi mode cursor.
    #[config(skip)]
    ViMotion(ViMotion),

    /// Perform vi mode action.
    #[config(skip)]
    Vi(ViAction),

    /// Perform search mode action.
    #[config(skip)]
    Search(SearchAction),

    /// Perform mouse binding exclusive action.
    #[config(skip)]
    Mouse(MouseAction),

    /// Paste contents of system clipboard.
    Paste,

    /// Store current selection into clipboard.
    Copy,

    #[cfg(not(any(target_os = "macos", windows)))]
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

    /// Scroll one line up.
    ScrollLineUp,

    /// Scroll one line down.
    ScrollLineDown,

    /// Scroll all the way to the top.
    ScrollToTop,

    /// Scroll all the way to the bottom.
    ScrollToBottom,

    /// Clear the display buffer(s) to remove history.
    ClearHistory,

    /// Hide the Alacritty window.
    Hide,

    /// Hide all windows other than Alacritty on macOS.
    #[cfg(target_os = "macos")]
    HideOtherApplications,

    /// Minimize the Alacritty window.
    Minimize,

    /// Quit Alacritty.
    Quit,

    /// Clear warning and error notices.
    ClearLogNotice,

    /// Spawn a new instance of Alacritty.
    SpawnNewInstance,

    /// Create a new Alacritty window.
    CreateNewWindow,

    /// Toggle fullscreen.
    ToggleFullscreen,

    /// Toggle maximized.
    ToggleMaximized,

    /// Toggle simple fullscreen on macOS.
    #[cfg(target_os = "macos")]
    ToggleSimpleFullscreen,

    /// Clear active selection.
    ClearSelection,

    /// Toggle vi mode.
    ToggleViMode,

    /// Allow receiving char input.
    ReceiveChar,

    /// Start a forward buffer search.
    SearchForward,

    /// Start a backward buffer search.
    SearchBackward,

    /// No action.
    None,
}

impl From<&'static str> for Action {
    fn from(s: &'static str) -> Action {
        Action::Esc(s.into())
    }
}


// macro_rules! bindings {
//     (
//         KeyBinding;
//         $(
//             $key:ident
//             $(,$mods:expr)*
//             $(,+$mode:expr)*
//             $(,~$notmode:expr)*
//             ;$action:expr
//         );*
//         $(;)*
//     ) => {{
//         bindings!(
//             KeyBinding;
//             $(
//                 Key::Keycode($key)
//                 $(,$mods)*
//                 $(,+$mode)*
//                 $(,~$notmode)*
//                 ;$action
//             );*
//         )
//     }};
//     (
//         $ty:ident;
//         $(
//             $key:expr
//             $(,$mods:expr)*
//             $(,+$mode:expr)*
//             $(,~$notmode:expr)*
//             ;$action:expr
//         );*
//         $(;)*
//     ) => {{
//         let mut v = Vec::new();

//         $(
//             let mut _mods = ModifiersState::empty();
//             $(_mods = $mods;)*
//             let mut _mode = BindingMode::empty();
//             $(_mode.insert($mode);)*
//             let mut _notmode = BindingMode::empty();
//             $(_notmode.insert($notmode);)*

//             v.push($ty {
//                 trigger: $key,
//                 mods: _mods,
//                 mode: _mode,
//                 notmode: _notmode,
//                 action: $action.into(),
//             });
//         )*

//         v
//     }};
// }