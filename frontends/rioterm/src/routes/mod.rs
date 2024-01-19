pub mod assistant;
#[cfg(not(target_os = "macos"))]
pub mod dialog;
pub mod welcome;

#[derive(PartialEq)]
pub enum RoutePath {
    Assistant,
    Terminal,
    Welcome,
    #[cfg(not(target_os = "macos"))]
    ConfirmQuit,
}
