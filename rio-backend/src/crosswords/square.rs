// square.rs was originally taken from Alacritty as cell.rs https://github.com/alacritty/alacritty/blob/e35e5ad14fce8456afdd89f2b392b9924bb27471/alacritty_terminal/src/term/cell.rs
// which is licensed under Apache 2.0 license.

use crate::ansi::graphics::GraphicsCell;
use crate::config::colors::{AnsiColor, NamedColor};
use crate::crosswords::grid::GridSquare;
use crate::crosswords::Column;
use crate::crosswords::Row;
use bitflags::bitflags;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

bitflags! {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct Flags: u16 {
        const INVERSE                   = 0b0000_0000_0000_0001;
        const BOLD                      = 0b0000_0000_0000_0010;
        const ITALIC                    = 0b0000_0000_0000_0100;
        const BOLD_ITALIC               = 0b0000_0000_0000_0110;
        const UNDERLINE                 = 0b0000_0000_0000_1000;
        const WRAPLINE                  = 0b0000_0000_0001_0000;
        const WIDE_CHAR                 = 0b0000_0000_0010_0000;
        const WIDE_CHAR_SPACER          = 0b0000_0000_0100_0000;
        const DIM                       = 0b0000_0000_1000_0000;
        const DIM_BOLD                  = 0b0000_0000_1000_0010;
        const HIDDEN                    = 0b0000_0001_0000_0000;
        const STRIKEOUT                 = 0b0000_0010_0000_0000;
        const LEADING_WIDE_CHAR_SPACER  = 0b0000_0100_0000_0000;
        const DOUBLE_UNDERLINE          = 0b0000_1000_0000_0000;
        const UNDERCURL                 = 0b0001_0000_0000_0000;
        const DOTTED_UNDERLINE          = 0b0010_0000_0000_0000;
        const DASHED_UNDERLINE          = 0b0100_0000_0000_0000;
        const GRAPHICS                  = 0b1000_0000_0000_0000;
        const ALL_UNDERLINES            = Self::UNDERLINE.bits() | Self::DOUBLE_UNDERLINE.bits()
                                        | Self::UNDERCURL.bits() | Self::DOTTED_UNDERLINE.bits()
                                        | Self::DASHED_UNDERLINE.bits();
    }
}

/// Counter for hyperlinks without explicit ID.
static HYPERLINK_ID_SUFFIX: AtomicU32 = AtomicU32::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Hyperlink {
    inner: Arc<HyperlinkInner>,
}

impl Hyperlink {
    pub fn new<T: ToString>(id: Option<T>, uri: T) -> Self {
        let inner = Arc::new(HyperlinkInner::new(id, uri));
        Self { inner }
    }

    pub fn id(&self) -> &str {
        &self.inner.id
    }

    pub fn uri(&self) -> &str {
        &self.inner.uri
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct HyperlinkInner {
    /// Identifier for the given hyperlink.
    id: String,

    /// Resource identifier of the hyperlink.
    uri: String,
}

impl HyperlinkInner {
    pub fn new<T: ToString>(id: Option<T>, uri: T) -> Self {
        let id = match id {
            Some(id) => id.to_string(),
            None => {
                let mut id = HYPERLINK_ID_SUFFIX
                    .fetch_add(1, Ordering::Relaxed)
                    .to_string();
                id.push_str("_rio");
                id
            }
        };

        Self {
            id,
            uri: uri.to_string(),
        }
    }
}

/// Dynamically allocated cell content.
///
/// This storage is reserved for cell attributes which are rarely set. This allows reducing the
/// allocation required ahead of time for every cell, with some additional overhead when the extra
/// storage is actually required.
#[derive(Default, Debug, Clone, Eq, PartialEq)]
pub struct CellExtra {
    zerowidth: Vec<char>,
    underline_color: Option<crate::config::colors::AnsiColor>,

    hyperlink: Option<Hyperlink>,

    graphics: Option<GraphicsCell>,
}

/// Content and attributes of a single cell in the terminal grid.
#[derive(Clone, Debug, PartialEq)]
pub struct Square {
    pub c: char,
    pub fg: AnsiColor,
    pub bg: AnsiColor,
    pub extra: Option<Arc<CellExtra>>,
    pub flags: Flags,
}

impl Default for Square {
    #[inline]
    fn default() -> Square {
        Square {
            c: ' ',
            bg: AnsiColor::Named(NamedColor::Background),
            fg: AnsiColor::Named(NamedColor::Foreground),
            extra: None,
            flags: Flags::empty(),
        }
    }
}

impl Square {
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

    /// Graphic present in the cell.
    #[inline]
    pub fn graphics(&self) -> Option<&GraphicsCell> {
        self.extra
            .as_deref()
            .and_then(|extra| extra.graphics.as_ref())
    }

    /// Extract the graphics value from the cell.
    #[inline]
    pub fn take_graphics(&mut self) -> Option<GraphicsCell> {
        if let Some(extra) = &mut self.extra {
            if extra.graphics.is_some() {
                return Arc::make_mut(extra).graphics.take();
            }
        }

        None
    }

    /// Write the graphic data in the cell.
    #[inline]
    pub fn set_graphics(&mut self, graphics_cell: GraphicsCell) {
        let extra = self.extra.get_or_insert_with(Default::default);
        Arc::make_mut(extra).graphics = Some(graphics_cell);

        self.flags_mut().insert(Flags::GRAPHICS);
    }

    #[inline(never)]
    pub fn clear_wide(&mut self) {
        self.flags.remove(Flags::WIDE_CHAR);
        if let Some(extra) = self.extra.as_mut() {
            Arc::make_mut(extra).zerowidth = Vec::new();
        }
        self.c = ' ';
    }

    pub fn set_underline_color(
        &mut self,
        color: Option<crate::config::colors::AnsiColor>,
    ) {
        // If we reset color and we don't have zerowidth we should drop extra storage.
        if color.is_none()
            && self.extra.as_ref().is_none_or(|extra| {
                extra.zerowidth.is_empty() && extra.hyperlink.is_none()
            })
        {
            self.extra = None;
        } else {
            let extra = self.extra.get_or_insert(Default::default());
            Arc::make_mut(extra).underline_color = color;
        }
    }

    /// Underline color stored in this cell.
    #[inline]
    pub fn underline_color(&self) -> Option<crate::config::colors::AnsiColor> {
        self.extra.as_ref()?.underline_color
    }

    /// Set hyperlink.
    pub fn set_hyperlink(&mut self, hyperlink: Option<Hyperlink>) {
        let should_drop = hyperlink.is_none()
            && self.extra.as_ref().is_none_or(|extra| {
                extra.zerowidth.is_empty() && extra.underline_color.is_none()
            });

        if should_drop {
            self.extra = None;
        } else {
            let extra = self.extra.get_or_insert(Default::default());
            Arc::make_mut(extra).hyperlink = hyperlink;
        }
    }

    /// Hyperlink stored in this cell.
    #[inline]
    pub fn hyperlink(&self) -> Option<Hyperlink> {
        self.extra.as_ref()?.hyperlink.clone()
    }
}

impl GridSquare for Square {
    #[inline]
    fn is_empty(&self) -> bool {
        (self.c == ' ' || self.c == '\t')
            && self.bg == AnsiColor::Named(NamedColor::Background)
            && self.fg == AnsiColor::Named(NamedColor::Foreground)
            && !self.flags.intersects(
                Flags::INVERSE
                    | Flags::ALL_UNDERLINES
                    | Flags::STRIKEOUT
                    | Flags::WRAPLINE
                    | Flags::WIDE_CHAR_SPACER
                    | Flags::LEADING_WIDE_CHAR_SPACER
                    | Flags::GRAPHICS,
            )
            && self.extra.as_ref().map(|extra| extra.zerowidth.is_empty()) != Some(false)
    }

    #[inline]
    fn reset(&mut self, template: &Self) {
        *self = Square {
            bg: template.bg,
            ..Square::default()
        };
    }

    #[inline]
    fn flags(&self) -> &Flags {
        &self.flags
    }

    #[inline]
    fn flags_mut(&mut self) -> &mut Flags {
        &mut self.flags
    }
}

pub trait LineLength {
    /// Calculate the occupied line length.
    fn line_length(&self) -> Column;
}

impl LineLength for Row<Square> {
    fn line_length(&self) -> Column {
        let mut length = Column(0);

        if self[Column(self.len() - 1)].flags.contains(Flags::WRAPLINE) {
            return Column(self.len());
        }

        for (index, cell) in self[..].iter().rev().enumerate() {
            if cell.c != ' '
                || cell.extra.as_ref().map(|extra| extra.zerowidth.is_empty())
                    == Some(false)
            {
                length = Column(self.len() - index);
                break;
            }
        }

        length
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

impl From<AnsiColor> for Square {
    #[inline]
    fn from(color: AnsiColor) -> Self {
        Self {
            bg: color,
            ..Square::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::mem;

    use crate::crosswords::grid::row::Row;
    use crate::crosswords::pos::Column;

    #[test]
    fn test_square_size_is_below_cap() {
        // Expected cell size on 64-bit architectures.
        const EXPECTED_SIZE: usize = 24;

        // Ensure that cell size isn't growning by accident.
        assert!(mem::size_of::<Square>() <= EXPECTED_SIZE);
    }

    #[test]
    fn test_line_length_works() {
        let mut row = Row::<Square>::new(10);
        row[Column(5)].c = 'a';

        assert_eq!(row.line_length(), Column(6));
    }

    #[test]
    fn test_line_length_works_with_wrapline() {
        let mut row = Row::<Square>::new(10);
        row[Column(9)].flags.insert(super::Flags::WRAPLINE);

        assert_eq!(row.line_length(), Column(10));
    }
}
