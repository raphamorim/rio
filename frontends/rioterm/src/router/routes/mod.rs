pub mod assistant;
pub mod command_palette;
pub mod dialog;
pub mod tab_switcher;
pub mod welcome;

#[derive(PartialEq)]
pub enum RoutePath {
    Assistant,
    CommandPalette,
    TabSwitcher,
    Terminal,
    Welcome,
    ConfirmQuit,
}
