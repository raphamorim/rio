use colors::{AnsiColor, NamedColor};
use std::sync::Arc;

/// Dynamically allocated cell content.
///
/// This storage is reserved for cell attributes which are rarely set. This allows reducing the
/// allocation required ahead of time for every cell, with some additional overhead when the extra
/// storage is actually required.
#[derive(Default, Debug, Clone, Eq, PartialEq)]
pub struct CellExtra {
    zerowidth: Vec<char>,
    // underline_color: Option<colors::AnsiColor>,

    // hyperlink: Option<Hyperlink>,
}

/// Content and attributes of a single cell in the terminal grid.
#[derive(Clone, Debug, PartialEq)]
pub struct Square {
    pub c: char,
    pub fg: AnsiColor,
    pub bg: AnsiColor,
    pub extra: Option<Arc<CellExtra>>,
}

impl Default for Square {
    #[inline]
    fn default() -> Square {
        Square {
            c: ' ',
            bg: AnsiColor::Named(NamedColor::Black),
            fg: AnsiColor::Named(NamedColor::Foreground),
            extra: None,
        }
    }
}

impl Square {
    #[allow(dead_code)]
    #[inline]
    pub fn zerowidth(&self) -> Option<&[char]> {
        self.extra.as_ref().map(|extra| extra.zerowidth.as_slice())
    }

    /// Write a new zerowidth character to this cell.
    #[inline]
    pub fn push_zerowidth(&mut self, character: char) {
        let extra = self.extra.get_or_insert(Default::default());
        Arc::make_mut(extra).zerowidth.push(character);
    }
}

pub trait CrosswordsSquare: Sized {
    /// Check if the cell contains any content.
    fn is_empty(&self) -> bool;

    /// Perform an opinionated cell reset based on a template cell.
    fn reset(&mut self, template: &Self);
}

impl CrosswordsSquare for Square {
    #[inline]
    fn is_empty(&self) -> bool {
        (self.c == ' ' || self.c == '\t')
            && self.extra.as_ref().map(|extra| extra.zerowidth.is_empty()) != Some(false)
    }

    #[inline]
    fn reset(&mut self, template: &Self) {
        *self = Square {
            bg: template.bg,
            ..Square::default()
        };
    }
}

pub trait ResetDiscriminant<T> {
    /// Value based on which equality for the reset will be determined.
    fn discriminant(&self) -> T;
}

impl<T: Copy> ResetDiscriminant<T> for T {
    fn discriminant(&self) -> T {
        *self
    }
}

impl ResetDiscriminant<AnsiColor> for Square {
    fn discriminant(&self) -> AnsiColor {
        self.bg
    }
}
