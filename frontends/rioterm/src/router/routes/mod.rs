pub mod assistant;
pub mod welcome;

#[derive(PartialEq)]
pub enum RoutePath {
    Terminal,
    Welcome,
}
