pub mod assistant;
pub mod dialog;
pub mod welcome;

#[derive(PartialEq)]
pub enum RoutePath {
    Terminal,
    Welcome,
    ConfirmQuit,
}
