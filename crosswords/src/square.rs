use colors::Rgba;

/// Content and attributes of a single cell in the terminal grid.
#[derive(Clone, Debug, PartialEq)]
pub struct Square {
    pub c: char,
    pub fg: Rgba,
    pub bg: Rgba,
}

impl Default for Square {
    #[inline]
    fn default() -> Square {
        Square {
            c: ' ',
            bg: Rgba::default(),
            fg: Rgba::default(),
        }
    }
}