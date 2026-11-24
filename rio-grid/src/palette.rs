//! The color resolution most hosts want: Rio's config colors and the
//! 256-color list derived from them, with the terminal's own (OSC-set)
//! colors taking precedence.

use crate::GridPalette;
use rio_backend::config::colors::term::{List, TermColors, DIM_FACTOR};
use rio_backend::config::colors::{AnsiColor, ColorArray, Colors, NamedColor};
use rio_backend::crosswords::style::{Style, StyleFlags};

pub struct Palette {
    pub named: Colors,
    pub list: List,
    pub draw_bold_text_with_light_colors: bool,
    pub use_drawable_chars: bool,
    pub opacity_cells: bool,
    pub cell_bg_alpha: u8,
    pub ignore_selection_fg_color: bool,
}

impl Default for Palette {
    fn default() -> Self {
        Self::new(Colors::default())
    }
}

impl Palette {
    pub fn new(named: Colors) -> Self {
        Self {
            list: List::from(&named),
            named,
            draw_bold_text_with_light_colors: false,
            use_drawable_chars: true,
            opacity_cells: false,
            cell_bg_alpha: 255,
            ignore_selection_fg_color: false,
        }
    }

    /// New base colors, and the 256-color list derived from them.
    pub fn set_colors(&mut self, named: Colors) {
        self.named = named;
        self.list = List::from(&self.named);
    }

    pub fn compute_color(
        &self,
        color: &AnsiColor,
        flags: StyleFlags,
        term_colors: &TermColors,
    ) -> ColorArray {
        let dim = flags.contains(StyleFlags::DIM);
        let bold = flags.contains(StyleFlags::BOLD);
        match color {
            AnsiColor::Named(ansi) => {
                match (self.draw_bold_text_with_light_colors, dim, bold) {
                    (_, true, true)
                        if ansi == &NamedColor::Foreground
                            && self.named.light_foreground.is_none() =>
                    {
                        self.color(NamedColor::DimForeground as usize, term_colors)
                    }
                    (true, false, true) => {
                        self.color(ansi.to_light() as usize, term_colors)
                    }
                    (_, true, false) | (false, true, true) => {
                        self.color(ansi.to_dim() as usize, term_colors)
                    }
                    _ => self.color(*ansi as usize, term_colors),
                }
            }
            AnsiColor::Spec(rgb) => {
                if !dim {
                    rgb.to_arr()
                } else {
                    rgb.to_arr_with_dim()
                }
            }
            AnsiColor::Indexed(index) => {
                let index = match (dim, index) {
                    (true, 8..=15) => *index as usize - 8,
                    (true, 0..=7) => NamedColor::DimBlack as usize + *index as usize,
                    _ => *index as usize,
                };
                self.color(index, term_colors)
            }
        }
    }

    pub fn compute_bg_color(
        &self,
        cell_style: &Style,
        term_colors: &TermColors,
    ) -> ColorArray {
        let inverse = cell_style.flags.contains(StyleFlags::INVERSE);
        let dim = inverse && cell_style.flags.contains(StyleFlags::DIM);
        let bold = inverse && cell_style.flags.contains(StyleFlags::BOLD);
        match cell_style.bg {
            AnsiColor::Named(ansi) => {
                let idx = match (self.draw_bold_text_with_light_colors, dim, bold) {
                    (_, true, true)
                        if ansi == NamedColor::Foreground
                            && self.named.light_foreground.is_none() =>
                    {
                        NamedColor::DimForeground as usize
                    }
                    (true, false, true) => ansi.to_light() as usize,
                    (_, true, false) | (false, true, true) => ansi.to_dim() as usize,
                    _ => ansi as usize,
                };
                self.color(idx, term_colors)
            }
            AnsiColor::Spec(rgb) => {
                if dim {
                    (&(rgb * DIM_FACTOR)).into()
                } else {
                    (&rgb).into()
                }
            }
            AnsiColor::Indexed(idx) => {
                let idx = match (self.draw_bold_text_with_light_colors, dim, bold, idx) {
                    (true, false, true, 0..=7) => idx as usize + 8,
                    (false, true, false, 8..=15) => idx as usize - 8,
                    (false, true, false, 0..=7) => {
                        NamedColor::DimBlack as usize + idx as usize
                    }
                    _ => idx as usize,
                };
                self.color(idx, term_colors)
            }
        }
    }

    #[inline]
    pub fn color(&self, idx: usize, term_colors: &TermColors) -> ColorArray {
        term_colors[idx].unwrap_or(self.list[idx])
    }
}

impl GridPalette for Palette {
    #[inline]
    fn named_colors(&self) -> &Colors {
        &self.named
    }
    #[inline]
    fn compute_color(
        &self,
        color: &AnsiColor,
        flags: StyleFlags,
        term_colors: &TermColors,
    ) -> ColorArray {
        Palette::compute_color(self, color, flags, term_colors)
    }
    #[inline]
    fn compute_bg_color(
        &self,
        cell_style: &Style,
        term_colors: &TermColors,
    ) -> ColorArray {
        Palette::compute_bg_color(self, cell_style, term_colors)
    }
    #[inline]
    fn color(&self, idx: usize, term_colors: &TermColors) -> ColorArray {
        Palette::color(self, idx, term_colors)
    }
    #[inline]
    fn use_drawable_chars(&self) -> bool {
        self.use_drawable_chars
    }
    #[inline]
    fn opacity_cells(&self) -> bool {
        self.opacity_cells
    }
    #[inline]
    fn cell_bg_alpha(&self) -> u8 {
        self.cell_bg_alpha
    }
    #[inline]
    fn ignore_selection_fg_color(&self) -> bool {
        self.ignore_selection_fg_color
    }
}
