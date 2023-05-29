use crate::crosswords::grid::row::Row;
use crate::crosswords::pos;
use crate::crosswords::pos::CursorState;
use crate::crosswords::square::{Flags, Square};
use crate::ime::Preedit;
use crate::selection::SelectionRange;
use crate::tabs::TabsControl;
use colors::{
    term::{List, TermColors},
    AnsiColor, Colors, NamedColor,
};
use config::Config;
use std::rc::Rc;
use sugarloaf::core::{Sugar, SugarStack, SugarStyle};
use sugarloaf::Sugarloaf;

#[derive(Default)]
struct Cursor {
    state: CursorState,
    content: char,
    content_ref: char,
}

pub struct State {
    pub option_as_alt: bool,
    is_ime_enabled: bool,
    named_colors: Colors,
    cursor: Cursor,
    colors: List,
    selection_range: Option<SelectionRange>,
}

// TODO: Finish from
impl From<Square> for Sugar {
    #[inline]
    fn from(square: Square) -> Sugar {
        let mut style: Option<SugarStyle> = None;
        let is_italic = square.flags.contains(Flags::ITALIC);
        let is_bold_italic = square.flags.contains(Flags::BOLD_ITALIC);
        let is_bold = square.flags.contains(Flags::BOLD);

        if is_bold || is_bold_italic || is_italic {
            style = Some(SugarStyle {
                is_italic,
                is_bold_italic,
                is_bold,
            });
        }

        Sugar {
            content: square.c,
            foreground_color: [0.0, 0.0, 0.0, 1.0],
            background_color: [0.0, 0.0, 0.0, 1.0],
            style,
        }
    }
}

impl State {
    pub fn new(config: &Rc<Config>) -> State {
        let term_colors = TermColors::default();
        let colors = List::from(&term_colors);

        let option_as_alt = matches!(
            config.option_as_alt.to_lowercase().as_str(),
            "both" | "left" | "right"
        );

        State {
            option_as_alt,
            is_ime_enabled: false,
            colors,
            selection_range: None,
            named_colors: config.colors,
            cursor: Cursor {
                content: config.cursor,
                content_ref: config.cursor,
                state: CursorState::default(),
            },
        }
    }

    // TODO: Square.into()
    #[inline]
    fn create_sugar_from_square(&self, square: &Square) -> Sugar {
        let flags = square.flags;

        let foreground_color = match square.fg {
            AnsiColor::Named(NamedColor::Black) => self.named_colors.black,
            AnsiColor::Named(NamedColor::Background) => self.named_colors.background.0,
            AnsiColor::Named(NamedColor::Blue) => self.named_colors.blue,
            AnsiColor::Named(NamedColor::LightBlack) => self.named_colors.light_black,
            AnsiColor::Named(NamedColor::LightBlue) => self.named_colors.light_blue,
            AnsiColor::Named(NamedColor::LightCyan) => self.named_colors.light_cyan,
            AnsiColor::Named(NamedColor::LightForeground) => {
                self.named_colors.light_foreground
            }
            AnsiColor::Named(NamedColor::LightGreen) => self.named_colors.light_green,
            AnsiColor::Named(NamedColor::LightMagenta) => self.named_colors.light_magenta,
            AnsiColor::Named(NamedColor::LightRed) => self.named_colors.light_red,
            AnsiColor::Named(NamedColor::LightWhite) => self.named_colors.light_white,
            AnsiColor::Named(NamedColor::LightYellow) => self.named_colors.light_yellow,
            AnsiColor::Named(NamedColor::Cursor) => self.named_colors.cursor,
            AnsiColor::Named(NamedColor::Cyan) => self.named_colors.cyan,
            AnsiColor::Named(NamedColor::DimBlack) => self.named_colors.dim_black,
            AnsiColor::Named(NamedColor::DimBlue) => self.named_colors.dim_blue,
            AnsiColor::Named(NamedColor::DimCyan) => self.named_colors.dim_cyan,
            AnsiColor::Named(NamedColor::DimForeground) => {
                self.named_colors.dim_foreground
            }
            AnsiColor::Named(NamedColor::DimGreen) => self.named_colors.dim_green,
            AnsiColor::Named(NamedColor::DimMagenta) => self.named_colors.dim_magenta,
            AnsiColor::Named(NamedColor::DimRed) => self.named_colors.dim_red,
            AnsiColor::Named(NamedColor::DimWhite) => self.named_colors.dim_white,
            AnsiColor::Named(NamedColor::DimYellow) => self.named_colors.dim_yellow,
            AnsiColor::Named(NamedColor::Foreground) => self.named_colors.foreground,
            AnsiColor::Named(NamedColor::Green) => self.named_colors.green,
            AnsiColor::Named(NamedColor::Magenta) => self.named_colors.magenta,
            AnsiColor::Named(NamedColor::Red) => self.named_colors.red,
            AnsiColor::Named(NamedColor::White) => self.named_colors.white,
            AnsiColor::Named(NamedColor::Yellow) => self.named_colors.yellow,
            AnsiColor::Spec(rgb) => rgb.to_arr(),
            AnsiColor::Indexed(index) => {
                let index = match (flags & Flags::DIM_BOLD, index) {
                    (Flags::DIM, 8..=15) => index as usize - 8,
                    (Flags::DIM, 0..=7) => NamedColor::DimBlack as usize + index as usize,
                    _ => index as usize,
                };

                self.colors[index]
            }
        };

        let background_color = match square.bg {
            AnsiColor::Named(NamedColor::Black) => self.named_colors.black,
            AnsiColor::Named(NamedColor::Background) => self.named_colors.background.0,
            AnsiColor::Named(NamedColor::Blue) => self.named_colors.blue,
            AnsiColor::Named(NamedColor::LightBlack) => self.named_colors.light_black,
            AnsiColor::Named(NamedColor::LightBlue) => self.named_colors.light_blue,
            AnsiColor::Named(NamedColor::LightCyan) => self.named_colors.light_cyan,
            AnsiColor::Named(NamedColor::LightForeground) => {
                self.named_colors.light_foreground
            }
            AnsiColor::Named(NamedColor::LightGreen) => self.named_colors.light_green,
            AnsiColor::Named(NamedColor::LightMagenta) => self.named_colors.light_magenta,
            AnsiColor::Named(NamedColor::LightRed) => self.named_colors.light_red,
            AnsiColor::Named(NamedColor::LightWhite) => self.named_colors.light_white,
            AnsiColor::Named(NamedColor::LightYellow) => self.named_colors.light_yellow,
            AnsiColor::Named(NamedColor::Cursor) => self.named_colors.cursor,
            AnsiColor::Named(NamedColor::Cyan) => self.named_colors.cyan,
            AnsiColor::Named(NamedColor::DimBlack) => self.named_colors.dim_black,
            AnsiColor::Named(NamedColor::DimBlue) => self.named_colors.dim_blue,
            AnsiColor::Named(NamedColor::DimCyan) => self.named_colors.dim_cyan,
            AnsiColor::Named(NamedColor::DimForeground) => {
                self.named_colors.dim_foreground
            }
            AnsiColor::Named(NamedColor::DimGreen) => self.named_colors.dim_green,
            AnsiColor::Named(NamedColor::DimMagenta) => self.named_colors.dim_magenta,
            AnsiColor::Named(NamedColor::DimRed) => self.named_colors.dim_red,
            AnsiColor::Named(NamedColor::DimWhite) => self.named_colors.dim_white,
            AnsiColor::Named(NamedColor::DimYellow) => self.named_colors.dim_yellow,
            AnsiColor::Named(NamedColor::Foreground) => self.named_colors.foreground,
            AnsiColor::Named(NamedColor::Green) => self.named_colors.green,
            AnsiColor::Named(NamedColor::Magenta) => self.named_colors.magenta,
            AnsiColor::Named(NamedColor::Red) => self.named_colors.red,
            AnsiColor::Named(NamedColor::White) => self.named_colors.white,
            AnsiColor::Named(NamedColor::Yellow) => self.named_colors.yellow,
            AnsiColor::Spec(rgb) => rgb.to_arr(),
            AnsiColor::Indexed(idx) => self.colors[idx as usize],
        };

        let mut style: Option<SugarStyle> = None;
        let is_italic = flags.contains(Flags::ITALIC);
        let is_bold_italic = flags.contains(Flags::BOLD_ITALIC);
        let is_bold = flags.contains(Flags::BOLD);

        if is_bold || is_bold_italic || is_italic {
            style = Some(SugarStyle {
                is_italic,
                is_bold_italic,
                is_bold,
            });
        }

        Sugar {
            content: square.c,
            foreground_color,
            background_color,
            style,
        }
    }

    #[inline]
    fn create_sugar_stack_with_selection(
        &mut self,
        row: &Row<Square>,
        has_cursor: bool,
        range: &SelectionRange,
        line: pos::Line,
    ) -> SugarStack {
        let mut stack: Vec<Sugar> = vec![];
        let columns: usize = row.len();
        for column in 0..columns {
            let is_selected = range.contains(pos::Pos::new(line, pos::Column(column)));
            let square = &row.inner[column];

            if has_cursor && column == self.cursor.state.pos.col {
                let mut foreground_color = self.named_colors.cursor;
                let mut background_color = self.named_colors.background.0;

                if is_selected {
                    foreground_color = self.named_colors.yellow;
                }

                if self.is_ime_enabled {
                    foreground_color = self.named_colors.background.0;
                    background_color = self.named_colors.yellow;
                }

                stack.push(Sugar {
                    content: self.cursor.content,
                    foreground_color,
                    background_color,
                    style: None,
                });
            } else if is_selected {
                let selected_sugar = Sugar {
                    content: square.c,
                    foreground_color: self.named_colors.background.0,
                    background_color: self.named_colors.light_blue,
                    style: None,
                };
                stack.push(selected_sugar);
            } else {
                stack.push(self.create_sugar_from_square(square));
            }

            // Render last column and break row
            if column == (columns - 1) {
                break;
            }
        }

        stack
    }

    #[inline]
    fn create_sugar_stack(&mut self, row: &Row<Square>, has_cursor: bool) -> SugarStack {
        let mut stack: Vec<Sugar> = vec![];
        let columns: usize = row.len();
        for column in 0..columns {
            let square = &row.inner[column];

            if has_cursor && column == self.cursor.state.pos.col {
                let mut foreground_color = self.named_colors.cursor;
                let mut background_color = self.named_colors.background.0;

                if self.is_ime_enabled {
                    foreground_color = self.named_colors.background.0;
                    background_color = self.named_colors.yellow;
                }

                stack.push(Sugar {
                    content: self.cursor.content,
                    foreground_color,
                    background_color,
                    style: None,
                });
            } else {
                stack.push(self.create_sugar_from_square(square));
            }

            // Render last column and break row
            if column == (columns - 1) {
                break;
            }
        }

        stack
    }

    pub fn set_ime(&mut self, ime_preedit: Option<&Preedit>) {
        if let Some(preedit) = ime_preedit {
            if let Some(content) = preedit.text.chars().next() {
                self.cursor.content = content;
                self.is_ime_enabled = true;
                return;
            }
        }

        self.is_ime_enabled = false;
        self.cursor.content = self.cursor.content_ref;
    }

    #[inline]
    pub fn set_selection(&mut self, selection_range: Option<SelectionRange>) {
        self.selection_range = selection_range;
    }

    #[inline]
    pub fn update(
        &mut self,
        rows: Vec<Row<Square>>,
        cursor: CursorState,
        sugarloaf: &mut Sugarloaf,
        style: sugarloaf::core::SugarloafStyle,
        _tab_style: sugarloaf::core::SugarloafStyle,
        _tabs: &TabsControl,
    ) {
        self.cursor.state = cursor;

        let is_cursor_visible = self.cursor.state.is_visible();

        if let Some(sel) = self.selection_range {
            for (i, row) in rows.iter().enumerate() {
                let has_cursor = is_cursor_visible && self.cursor.state.pos.row == i;
                let sugar_stack = self.create_sugar_stack_with_selection(
                    row,
                    has_cursor,
                    &sel,
                    pos::Line(i as i32),
                );
                sugarloaf.stack(sugar_stack, style);
            }

            return;
        }

        for (i, row) in rows.iter().enumerate() {
            let has_cursor = is_cursor_visible && self.cursor.state.pos.row == i;
            let sugar_stack = self.create_sugar_stack(row, has_cursor);
            sugarloaf.stack(sugar_stack, style);
        }

        // if tabs.len() > 1 {
        //     sugarloaf.tabs(
        //         "1, 3, 4".to_string(),
        //         tab_style,
        //         self.named_colors.tabs,
        //         self.named_colors.tabs_active,
        //     );
        // }
    }

    // pub fn topbar(&mut self, command: String) {
    //     let fps_text = if self.config.developer.enable_fps_counter {
    //         format!(" fps_{:?}", self.fps.tick())
    //     } else {
    //         String::from("")
    //     };
}
