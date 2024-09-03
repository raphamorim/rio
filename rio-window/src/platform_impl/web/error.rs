use std::fmt;

#[derive(Debug)]
pub struct OsError(pub String);

impl fmt::Display for OsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
