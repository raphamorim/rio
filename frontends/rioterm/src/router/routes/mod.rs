pub mod assistant;
pub mod command_palette;
pub mod dialog;
pub mod welcome;

#[derive(PartialEq)]
pub enum RoutePath {
    Assistant,
    CommandPalette,
    Terminal,
    Welcome,
    ConfirmQuit,
}
