use colors::Rgba;

/// Content and attributes of a single cell in the terminal grid.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub c: char,
    pub fg: Rgba,
    pub bg: Rgba,
}

impl Default for Cell {
    #[inline]
    fn default() -> Cell {
        Cell {
            c: ' ',
            bg: Rgba::default(),
            fg: Rgba::default(),
        }
    }
}