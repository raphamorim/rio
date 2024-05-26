pub mod assistant;
#[cfg(not(use_wa))]
pub mod dialog;
pub mod welcome;

#[derive(PartialEq)]
pub enum RoutePath {
    Assistant,
    Terminal,
    Welcome,
    #[cfg(not(use_wa))]
    ConfirmQuit,
}
