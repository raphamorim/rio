// Re-export the io::Result / Error types for convenience
// pub use std::io::{Error, ErrorKind, Read, Result, Write};

#[cfg(not(target_os = "windows"))]
pub use std::io::ErrorKind;
pub use std::io::Result;

// TODO: Delete this
/// A helper trait to provide the map_non_block function on Results.
pub trait MapNonBlock<T> {
    #[allow(dead_code)]
    fn map_non_block(self) -> Result<Option<T>>;
}

impl<T> MapNonBlock<T> for Result<T> {
    fn map_non_block(self) -> Result<Option<T>> {
        use std::io::ErrorKind::WouldBlock;

        match self {
            Ok(value) => Ok(Some(value)),
            Err(err) => {
                if let WouldBlock = err.kind() {
                    Ok(None)
                } else {
                    Err(err)
                }
            }
        }
    }
}
