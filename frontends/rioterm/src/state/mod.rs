pub mod navigation;

use crate::ansi::CursorShape;
use crate::crosswords::grid::row::Row;
use crate::crosswords::pos;
use crate::crosswords::pos::CursorState;
use crate::crosswords::square::{Flags, Square};
use crate::ime::Preedit;
use crate::selection::SelectionRange;
use navigation::ScreenNavigation;
use rio_backend::config::colors::{
    term::{List, TermColors},
    AnsiColor, ColorArray, Colors, NamedColor,
};
use rio_backend::config::Config;
use rio_backend::sugarloaf::{Sugar, SugarCursor, SugarDecoration, SugarStyle};
use rio_backend::sugarloaf::{SugarGraphic, Sugarloaf};
use std::collections::HashMap;
use std::time::{Duration, Instant};
#[cfg(not(target_os = "macos"))]
use winit::window::Theme;

struct Cursor {
    state: CursorState,
    content: char,
    content_ref: char,
}

pub struct State {
    pub option_as_alt: String,
    is_ime_enabled: bool,
    is_vi_mode_enabled: bool,
    pub is_kitty_keyboard_enabled: bool,
    pub last_typing: Option<Instant>,
    pub named_colors: Colors,
    font_size: f32,
    pub colors: List,
    navigation: ScreenNavigation,
    cursor: Cursor,
    pub selection_range: Option<SelectionRange>,
    pub has_blinking_enabled: bool,
    pub is_blinking: bool,
    ignore_selection_fg_color: bool,
    pub dynamic_background: wgpu::Color,
    hyperlink_range: Option<SelectionRange>,
    background_opacity: f32,
    foreground_opacity: f32,
}

impl State {
    pub fn new(
        #[cfg(not(target_os = "macos"))] config: &std::rc::Rc<Config>,
        #[cfg(target_os = "macos")] config: &Config,
        #[cfg(not(target_os = "macos"))] current_theme: Option<Theme>,
        #[cfg(target_os = "macos")] appearance: wa::Appearance,
    ) -> State {
        let term_colors = TermColors::default();
        let colors = List::from(&term_colors);
        let mut named_colors = config.colors;

        #[cfg(not(target_os = "macos"))]
        {
            if let Some(theme) = current_theme {
                if let Some(adaptive_colors) = &config.adaptive_colors {
                    match theme {
                        Theme::Light => {
                            named_colors = adaptive_colors.light.unwrap_or(named_colors);
                        }
                        Theme::Dark => {
                            named_colors = adaptive_colors.dark.unwrap_or(named_colors);
                        }
                    }
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            if let Some(adaptive_colors) = &config.adaptive_colors {
                match appearance {
                    wa::Appearance::Light => {
                        named_colors = adaptive_colors.light.unwrap_or(named_colors);
                    }
                    wa::Appearance::Dark => {
                        named_colors = adaptive_colors.dark.unwrap_or(named_colors);
                    }
                    // TODO
                    wa::Appearance::LightHighContrast => {}
                    wa::Appearance::DarkHighContrast => {}
                }
            }
        }

        let mut dynamic_background = named_colors.background.1;
        if config.window.background_image.is_some()
            || config.window.background_opacity < 1.
        {
            dynamic_background = wgpu::Color::TRANSPARENT;
        };

        let mut color_automation: HashMap<String, HashMap<String, [f32; 4]>> =
            HashMap::new();

        for rule in &config.navigation.color_automation {
            color_automation
                .entry(rule.program.clone())
                .or_default()
                .insert(rule.path.clone(), rule.color);
        }

        State {
            background_opacity: config.window.background_opacity,
            foreground_opacity: config.window.foreground_opacity,
            option_as_alt: config.option_as_alt.to_lowercase(),
            is_kitty_keyboard_enabled: config.keyboard.use_kitty_keyboard_protocol,
            is_ime_enabled: false,
            is_vi_mode_enabled: false,
            is_blinking: false,
            last_typing: None,
            has_blinking_enabled: config.blinking_cursor,
            ignore_selection_fg_color: config.ignore_selection_fg_color,
            colors,
            navigation: ScreenNavigation::new(
                config.navigation.mode,
                [
                    named_colors.tabs,
                    named_colors.tabs_active,
                    named_colors.foreground,
                ],
                color_automation,
                0.0,
                0.0,
                0.0,
            ),
            font_size: config.fonts.size,
            selection_range: None,
            hyperlink_range: None,
            named_colors,
            dynamic_background,
            cursor: Cursor {
                content: config.cursor,
                content_ref: config.cursor,
                state: CursorState::new(config.cursor),
            },
        }
    }

    #[inline]
    pub fn get_cursor_state_from_ref(&self) -> CursorState {
        CursorState::new(self.cursor.content_ref)
    }

    #[inline]
    pub fn get_cursor_state(&self) -> CursorState {
        self.cursor.state.clone()
    }

    #[inline]
    pub fn set_hyperlink_range(&mut self, hyperlink_range: Option<SelectionRange>) {
        self.hyperlink_range = hyperlink_range;
    }

    #[inline]
    pub fn has_hyperlink_range(&self) -> bool {
        self.hyperlink_range.is_some()
    }

    #[inline]
    fn create_sugar(&self, square: &Square) -> Sugar {
        let flags = square.flags;

        let mut foreground_color = self.compute_fg_color(square);
        let mut background_color = self.compute_bg_color(square);

        let content = if square.c == '\t' || flags.contains(Flags::HIDDEN) {
            ' '
        } else {
            square.c
        };

        let style = match (
            flags.contains(Flags::ITALIC),
            flags.contains(Flags::BOLD_ITALIC),
            flags.contains(Flags::BOLD),
        ) {
            (true, _, _) => SugarStyle::Italic,
            (_, true, _) => SugarStyle::BoldItalic,
            (_, _, true) => SugarStyle::Bold,
            _ => SugarStyle::Disabled,
        };

        if flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut background_color, &mut foreground_color);
        }

        let mut decoration = SugarDecoration::Disabled;
        if flags.contains(Flags::UNDERLINE) {
            decoration = SugarDecoration::Underline;
        } else if flags.contains(Flags::STRIKEOUT) {
            decoration = SugarDecoration::Strikethrough;
        }

        Sugar {
            content,
            repeated: 0,
            foreground_color,
            background_color,
            style,
            decoration,
            media: None,
            cursor: SugarCursor::Disabled,
        }
    }

    #[inline]
    fn set_hyperlink_in_sugar(&self, mut sugar: Sugar) -> Sugar {
        sugar.decoration = SugarDecoration::Underline;
        sugar
    }

    #[inline]
    fn create_sugar_line(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        row: &Row<Square>,
        has_cursor: bool,
        current_line: pos::Line,
    ) {
        sugarloaf.start_line();

        let columns: usize = row.len();
        for column in 0..columns {
            let square = &row.inner[column];

            if square.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }

            if square.flags.contains(Flags::GRAPHICS) {
                sugarloaf.insert_on_current_line(&self.create_graphic_sugar(square));
                continue;
            }

            if has_cursor && column == self.cursor.state.pos.col {
                sugarloaf.insert_on_current_line(&self.create_cursor(square));
            } else if self.hyperlink_range.is_some()
                && square.hyperlink().is_some()
                && self
                    .hyperlink_range
                    .unwrap()
                    .contains(pos::Pos::new(current_line, pos::Column(column)))
            {
                let sugar = self.create_sugar(square);
                sugarloaf.insert_on_current_line(&self.set_hyperlink_in_sugar(sugar));
            } else if self.selection_range.is_some()
                && self
                    .selection_range
                    .unwrap()
                    .contains(pos::Pos::new(current_line, pos::Column(column)))
            {
                let content = if square.c == '\t' || square.flags.contains(Flags::HIDDEN)
                {
                    ' '
                } else {
                    square.c
                };

                let selected_sugar = Sugar {
                    content,
                    foreground_color: if self.ignore_selection_fg_color {
                        self.compute_fg_color(square)
                    } else {
                        self.named_colors.selection_foreground
                    },
                    background_color: self.named_colors.selection_background,
                    ..Sugar::default()
                };
                sugarloaf.insert_on_current_line(&selected_sugar);
            } else {
                sugarloaf.insert_on_current_line(&self.create_sugar(square));
            }

            // Render last column and break row
            if column == (columns - 1) {
                break;
            }
        }

        sugarloaf.finish_line();
    }

    #[inline]
    #[cfg(target_os = "macos")]
    pub fn decrease_foreground_opacity(&mut self, acc: f32) {
        self.foreground_opacity -= acc;
    }

    #[inline]
    #[cfg(target_os = "macos")]
    pub fn increase_foreground_opacity(&mut self, acc: f32) {
        self.foreground_opacity += acc;
    }

    #[inline]
    fn compute_fg_color(&self, square: &Square) -> ColorArray {
        let mut color = match square.fg {
            AnsiColor::Named(ansi_name) => match (ansi_name, square.flags) {
                (NamedColor::Background, _) => self.named_colors.background.0,
                (NamedColor::Cursor, _) => self.named_colors.cursor,

                (NamedColor::Black, Flags::DIM) => self.named_colors.dim_black,
                (NamedColor::Black, Flags::BOLD) => self.named_colors.light_black,
                (NamedColor::Black, _) => self.named_colors.black,
                (NamedColor::Blue, Flags::DIM) => self.named_colors.dim_blue,
                (NamedColor::Blue, Flags::BOLD) => self.named_colors.light_blue,
                (NamedColor::Blue, _) => self.named_colors.blue,
                (NamedColor::Cyan, Flags::DIM) => self.named_colors.dim_cyan,
                (NamedColor::Cyan, Flags::BOLD) => self.named_colors.light_cyan,
                (NamedColor::Cyan, _) => self.named_colors.cyan,
                (NamedColor::Foreground, _) => self.named_colors.foreground,
                (NamedColor::Green, Flags::DIM) => self.named_colors.dim_green,
                (NamedColor::Green, Flags::BOLD) => self.named_colors.light_green,
                (NamedColor::Green, _) => self.named_colors.green,
                (NamedColor::Magenta, Flags::DIM) => self.named_colors.dim_magenta,
                (NamedColor::Magenta, Flags::BOLD) => self.named_colors.light_magenta,
                (NamedColor::Magenta, _) => self.named_colors.magenta,
                (NamedColor::Red, Flags::DIM) => self.named_colors.dim_red,
                (NamedColor::Red, Flags::BOLD) => self.named_colors.light_red,
                (NamedColor::Red, _) => self.named_colors.red,
                (NamedColor::White, Flags::DIM) => self.named_colors.dim_white,
                (NamedColor::White, Flags::BOLD) => self.named_colors.light_white,
                (NamedColor::White, _) => self.named_colors.white,
                (NamedColor::Yellow, Flags::DIM) => self.named_colors.dim_yellow,
                (NamedColor::Yellow, Flags::BOLD) => self.named_colors.light_yellow,
                (NamedColor::Yellow, _) => self.named_colors.yellow,
                (NamedColor::LightBlack, _) => self.named_colors.light_black,
                (NamedColor::LightBlue, _) => self.named_colors.light_blue,
                (NamedColor::LightCyan, _) => self.named_colors.light_cyan,
                (NamedColor::LightForeground, _) => self.named_colors.light_foreground,
                (NamedColor::LightGreen, _) => self.named_colors.light_green,
                (NamedColor::LightMagenta, _) => self.named_colors.light_magenta,
                (NamedColor::LightRed, _) => self.named_colors.light_red,
                (NamedColor::LightWhite, _) => self.named_colors.light_white,
                (NamedColor::LightYellow, _) => self.named_colors.light_yellow,
                (NamedColor::DimBlack, _) => self.named_colors.dim_black,
                (NamedColor::DimBlue, _) => self.named_colors.dim_blue,
                (NamedColor::DimCyan, _) => self.named_colors.dim_cyan,
                (NamedColor::DimForeground, _) => self.named_colors.dim_foreground,
                (NamedColor::DimGreen, _) => self.named_colors.dim_green,
                (NamedColor::DimMagenta, _) => self.named_colors.dim_magenta,
                (NamedColor::DimRed, _) => self.named_colors.dim_red,
                (NamedColor::DimWhite, _) => self.named_colors.dim_white,
                (NamedColor::DimYellow, _) => self.named_colors.dim_yellow,
            },
            AnsiColor::Spec(rgb) => {
                if !square.flags.contains(Flags::DIM) {
                    rgb.to_arr()
                } else {
                    rgb.to_arr_with_dim()
                }
            }
            AnsiColor::Indexed(index) => {
                let index = match (square.flags & Flags::DIM_BOLD, index) {
                    (Flags::DIM, 8..=15) => index as usize - 8,
                    (Flags::DIM, 0..=7) => NamedColor::DimBlack as usize + index as usize,
                    _ => index as usize,
                };

                self.colors[index]
            }
        };

        if self.foreground_opacity < 1. {
            color[3] = self.foreground_opacity;
        }

        color
    }

    #[inline]
    fn compute_bg_color(&self, square: &Square) -> ColorArray {
        let mut color = match square.bg {
            AnsiColor::Named(ansi_name) => match (ansi_name, square.flags) {
                (NamedColor::Background, _) => self.named_colors.background.0,
                (NamedColor::Cursor, _) => self.named_colors.cursor,

                (NamedColor::Black, Flags::DIM) => self.named_colors.dim_black,
                (NamedColor::Black, Flags::BOLD) => self.named_colors.light_black,
                (NamedColor::Black, _) => self.named_colors.black,
                (NamedColor::Blue, Flags::DIM) => self.named_colors.dim_blue,
                (NamedColor::Blue, Flags::BOLD) => self.named_colors.light_blue,
                (NamedColor::Blue, _) => self.named_colors.blue,
                (NamedColor::Cyan, Flags::DIM) => self.named_colors.dim_cyan,
                (NamedColor::Cyan, Flags::BOLD) => self.named_colors.light_cyan,
                (NamedColor::Cyan, _) => self.named_colors.cyan,
                (NamedColor::Foreground, _) => self.named_colors.foreground,
                (NamedColor::Green, Flags::DIM) => self.named_colors.dim_green,
                (NamedColor::Green, Flags::BOLD) => self.named_colors.light_green,
                (NamedColor::Green, _) => self.named_colors.green,
                (NamedColor::Magenta, Flags::DIM) => self.named_colors.dim_magenta,
                (NamedColor::Magenta, Flags::BOLD) => self.named_colors.light_magenta,
                (NamedColor::Magenta, _) => self.named_colors.magenta,
                (NamedColor::Red, Flags::DIM) => self.named_colors.dim_red,
                (NamedColor::Red, Flags::BOLD) => self.named_colors.light_red,
                (NamedColor::Red, _) => self.named_colors.red,
                (NamedColor::White, Flags::DIM) => self.named_colors.dim_white,
                (NamedColor::White, Flags::BOLD) => self.named_colors.light_white,
                (NamedColor::White, _) => self.named_colors.white,
                (NamedColor::Yellow, Flags::DIM) => self.named_colors.dim_yellow,
                (NamedColor::Yellow, Flags::BOLD) => self.named_colors.light_yellow,
                (NamedColor::Yellow, _) => self.named_colors.yellow,
                (NamedColor::LightBlack, _) => self.named_colors.light_black,
                (NamedColor::LightBlue, _) => self.named_colors.light_blue,
                (NamedColor::LightCyan, _) => self.named_colors.light_cyan,
                (NamedColor::LightForeground, _) => self.named_colors.light_foreground,
                (NamedColor::LightGreen, _) => self.named_colors.light_green,
                (NamedColor::LightMagenta, _) => self.named_colors.light_magenta,
                (NamedColor::LightRed, _) => self.named_colors.light_red,
                (NamedColor::LightWhite, _) => self.named_colors.light_white,
                (NamedColor::LightYellow, _) => self.named_colors.light_yellow,
                (NamedColor::DimBlack, _) => self.named_colors.dim_black,
                (NamedColor::DimBlue, _) => self.named_colors.dim_blue,
                (NamedColor::DimCyan, _) => self.named_colors.dim_cyan,
                (NamedColor::DimForeground, _) => self.named_colors.dim_foreground,
                (NamedColor::DimGreen, _) => self.named_colors.dim_green,
                (NamedColor::DimMagenta, _) => self.named_colors.dim_magenta,
                (NamedColor::DimRed, _) => self.named_colors.dim_red,
                (NamedColor::DimWhite, _) => self.named_colors.dim_white,
                (NamedColor::DimYellow, _) => self.named_colors.dim_yellow,
            },
            AnsiColor::Spec(rgb) => rgb.to_arr(),
            AnsiColor::Indexed(idx) => self.colors[idx as usize],
        };

        if color[3] >= 1.0 && self.background_opacity < 1. {
            color[3] = self.background_opacity;
        }

        color
    }

    #[inline]
    fn create_graphic_sugar(&self, square: &Square) -> Sugar {
        let media = &square.graphics().unwrap()[0].texture;
        Sugar {
            media: Some(SugarGraphic {
                id: media.id,
                width: media.width,
                height: media.height,
            }),
            ..Sugar::default()
        }
    }

    #[inline]
    fn create_sugar_cursor(&self) -> SugarCursor {
        let color = if !self.is_vi_mode_enabled {
            self.named_colors.cursor
        } else {
            self.named_colors.vi_cursor
        };

        match self.cursor.state.content {
            CursorShape::Block => SugarCursor::Block(color),
            CursorShape::Underline => SugarCursor::Underline(color),
            CursorShape::Beam => SugarCursor::Caret(color),
            CursorShape::Hidden => SugarCursor::Disabled,
        }
    }

    #[inline]
    fn create_cursor(&self, square: &Square) -> Sugar {
        let style = match (
            square.flags.contains(Flags::ITALIC),
            square.flags.contains(Flags::BOLD_ITALIC),
            square.flags.contains(Flags::BOLD),
        ) {
            (true, _, _) => SugarStyle::Italic,
            (_, true, _) => SugarStyle::BoldItalic,
            (_, _, true) => SugarStyle::Bold,
            _ => SugarStyle::Disabled,
        };

        let mut sugar = Sugar {
            content: square.c,
            foreground_color: self.compute_fg_color(square),
            background_color: self.compute_bg_color(square),
            cursor: self.create_sugar_cursor(),
            style,
            ..Sugar::default()
        };

        if square.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut sugar.background_color, &mut sugar.foreground_color);
        }

        // If IME is enabled we get the current content to cursor
        if self.is_ime_enabled {
            sugar.content = self.cursor.content;
        }

        // If IME is enabled or is a block cursor, put background color
        // when cursor is over the character
        if self.is_ime_enabled || self.cursor.state.content == CursorShape::Block {
            sugar.foreground_color = self.named_colors.background.0;
        }

        sugar
    }

    #[inline]
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
    pub fn set_vi_mode(&mut self, is_vi_mode_enabled: bool) {
        self.is_vi_mode_enabled = is_vi_mode_enabled;
    }

    #[inline]
    pub fn prepare_term(
        &mut self,
        rows: Vec<Row<Square>>,
        cursor: CursorState,
        sugarloaf: &mut Sugarloaf,
        context_manager: &crate::context::ContextManager<rio_backend::event::EventProxy>,
        display_offset: i32,
        has_blinking_enabled: bool,
    ) {
        let layout = sugarloaf.layout_next();
        self.cursor.state = cursor;
        let mut is_cursor_visible = self.cursor.state.is_visible();

        self.font_size = layout.font_size;

        // Only blink cursor if does not contain selection
        let has_selection = self.selection_range.is_some();
        if !has_selection && self.has_blinking_enabled && has_blinking_enabled {
            let mut should_blink = true;
            if let Some(last_typing_time) = self.last_typing {
                if last_typing_time.elapsed() < Duration::from_secs(1) {
                    should_blink = false;
                }
            }

            if should_blink {
                self.is_blinking = !self.is_blinking;
                is_cursor_visible = self.is_blinking;
            }
        }

        for (i, row) in rows.iter().enumerate() {
            let has_cursor = is_cursor_visible && self.cursor.state.pos.row == i;
            self.create_sugar_line(
                sugarloaf,
                row,
                has_cursor,
                pos::Line((i as i32) - display_offset),
            );
        }

        self.navigation.content(
            (layout.width, layout.height),
            layout.dimensions.scale,
            context_manager.titles.key.as_str(),
            &context_manager.titles.titles,
            context_manager.current_index(),
            context_manager.len(),
        );

        sugarloaf.append_rects(self.navigation.rects.to_owned());

        for text in self.navigation.texts.iter() {
            sugarloaf.text(
                text.position,
                text.content.to_owned(),
                text.font_size,
                text.color,
                true,
            );
        }
    }
}
