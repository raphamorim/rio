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
use rio_backend::sugarloaf::{
    Content, ContentBuilder, FragmentStyle, FragmentStyleDecoration, Stretch, Style,
    SugarCursor, Sugarloaf, UnderlineInfo, UnderlineShape, Weight,
};
use std::collections::HashMap;
use std::time::{Duration, Instant};
#[cfg(not(use_wa))]
use winit::window::Theme;

use rustc_hash::FxHashMap;
use unicode_width::UnicodeWidthChar;

struct Cursor {
    state: CursorState,
    content: char,
    content_ref: char,
}

pub struct State {
    #[allow(unused)]
    pub option_as_alt: String,
    is_ime_enabled: bool,
    is_vi_mode_enabled: bool,
    pub is_kitty_keyboard_enabled: bool,
    pub last_typing: Option<Instant>,
    pub named_colors: Colors,
    font_size: f32,
    pub colors: List,
    pub navigation: ScreenNavigation,
    cursor: Cursor,
    pub selection_range: Option<SelectionRange>,
    pub has_blinking_enabled: bool,
    pub is_blinking: bool,
    ignore_selection_fg_color: bool,
    // Dynamic background keep track of the original bg color and
    // the same r,g,b with the mutated alpha channel.
    pub dynamic_background: ([f32; 4], wgpu::Color, bool),
    hyperlink_range: Option<SelectionRange>,
    width_cache: FxHashMap<char, f32>,
}

impl State {
    pub fn new(
        #[cfg(not(use_wa))] config: &Config,
        #[cfg(use_wa)] config: &Config,
        #[cfg(not(use_wa))] current_theme: Option<Theme>,
        #[cfg(use_wa)] appearance: wa::Appearance,
    ) -> State {
        let term_colors = TermColors::default();
        let colors = List::from(&term_colors);
        let mut named_colors = config.colors;

        #[cfg(not(use_wa))]
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

        #[cfg(use_wa)]
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

        let mut dynamic_background =
            (named_colors.background.0, named_colors.background.1, false);
        if config.window.opacity < 1. {
            dynamic_background.1.a = config.window.opacity as f64;
            dynamic_background.2 = true;
        } else if config.window.background_image.is_some() {
            dynamic_background.1 = wgpu::Color::TRANSPARENT;
            dynamic_background.2 = true;
        }

        let mut color_automation: HashMap<String, HashMap<String, [f32; 4]>> =
            HashMap::new();

        for rule in &config.navigation.color_automation {
            color_automation
                .entry(rule.program.clone())
                .or_default()
                .insert(rule.path.clone(), rule.color);
        }

        State {
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
                config.navigation.clone(),
                [
                    named_colors.foreground,
                    named_colors.bar,
                    named_colors.tabs,
                    named_colors.tabs_active,
                    named_colors.tabs_active_highlight,
                ],
                color_automation,
                config.padding_y,
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
            width_cache: FxHashMap::default(),
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
    fn create_style(&mut self, square: &Square) -> (FragmentStyle, char) {
        let flags = square.flags;

        let mut foreground_color = self.compute_color(&square.fg, flags);
        let mut background_color = self.compute_bg_color(square);

        let content = if square.c == '\t' || flags.contains(Flags::HIDDEN) {
            ' '
        } else {
            square.c
        };

        let width = if let Some(w) = self.width_cache.get(&content) {
            *w
        } else {
            let w = square.c.width().unwrap_or(1) as f32;
            self.width_cache.insert(square.c, w);
            w
        };

        let font_attrs = match (
            flags.contains(Flags::ITALIC),
            flags.contains(Flags::BOLD_ITALIC),
            flags.contains(Flags::BOLD),
        ) {
            (true, _, _) => (Stretch::NORMAL, Weight::NORMAL, Style::Italic),
            (_, true, _) => (Stretch::NORMAL, Weight::BOLD, Style::Italic),
            (_, _, true) => (Stretch::NORMAL, Weight::BOLD, Style::Normal),
            _ => (Stretch::NORMAL, Weight::NORMAL, Style::Normal),
        };

        if flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut background_color, &mut foreground_color);
        }

        let background_color = if self.dynamic_background.2
            && background_color[0] == self.dynamic_background.0[0]
            && background_color[1] == self.dynamic_background.0[1]
            && background_color[2] == self.dynamic_background.0[2]
        {
            None
        } else {
            Some(background_color)
        };

        let (decoration, decoration_color) = self.compute_decoration(square, false);

        (
            FragmentStyle {
                width,
                color: foreground_color,
                background_color,
                font_attrs,
                decoration,
                decoration_color,
                ..FragmentStyle::default()
            },
            content,
        )
    }

    fn compute_decoration(
        &self,
        square: &Square,
        skip_underline: bool,
    ) -> (Option<FragmentStyleDecoration>, Option<[f32; 4]>) {
        let mut decoration = None;
        let mut decoration_color = None;

        if square.flags.contains(Flags::UNDERLINE) {
            if !skip_underline {
                decoration = Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                    offset: -2.0,
                    size: 2.0,
                    is_doubled: false,
                    shape: UnderlineShape::Regular,
                }));
            }
        } else if square.flags.contains(Flags::STRIKEOUT) {
            decoration = Some(FragmentStyleDecoration::Strikethrough);
        } else if square.flags.contains(Flags::DOUBLE_UNDERLINE) {
            decoration = Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                offset: -4.0,
                size: 1.0,
                is_doubled: true,
                shape: UnderlineShape::Regular,
            }));
        } else if square.flags.contains(Flags::DOTTED_UNDERLINE) {
            decoration = Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                offset: -2.0,
                size: 2.0,
                is_doubled: false,
                shape: UnderlineShape::Dotted,
            }));
        } else if square.flags.contains(Flags::DASHED_UNDERLINE) {
            decoration = Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                offset: -2.0,
                size: 2.0,
                is_doubled: false,
                shape: UnderlineShape::Dashed,
            }));
        } else if square.flags.contains(Flags::UNDERCURL) {
            decoration = Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                offset: -2.0,
                size: 1.0,
                is_doubled: false,
                shape: UnderlineShape::Curly,
            }));
        }

        if decoration.is_some() {
            if let Some(color) = square.underline_color() {
                decoration_color = Some(self.compute_color(&color, square.flags));
            }
        };

        (decoration, decoration_color)
    }

    #[inline]
    fn create_line(
        &mut self,
        content_builder: &mut ContentBuilder,
        row: &Row<Square>,
        has_cursor: bool,
        current_line: pos::Line,
    ) {
        let columns: usize = row.len();
        let mut content = String::default();
        let mut last_style = FragmentStyle::default();

        for column in 0..columns {
            let square = &row.inner[column];

            if square.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }

            if square.flags.contains(Flags::GRAPHICS) {
                // &self.create_graphic_sugar(square);
                continue;
            }

            let (mut style, square_content) =
                if has_cursor && column == self.cursor.state.pos.col {
                    self.create_cursor_style(square)
                } else {
                    self.create_style(square)
                };

            if self.hyperlink_range.is_some()
                && square.hyperlink().is_some()
                && self
                    .hyperlink_range
                    .unwrap()
                    .contains(pos::Pos::new(current_line, pos::Column(column)))
            {
                style.decoration =
                    Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                        offset: -1.0,
                        size: -1.0,
                        is_doubled: false,
                        shape: UnderlineShape::Regular,
                    }));
            } else if self.selection_range.is_some()
                && self
                    .selection_range
                    .unwrap()
                    .contains(pos::Pos::new(current_line, pos::Column(column)))
            {
                style.color = if self.ignore_selection_fg_color {
                    self.compute_color(&square.fg, square.flags)
                } else {
                    self.named_colors.selection_foreground
                };
                style.background_color = Some(self.named_colors.selection_background);
            }

            if last_style != style {
                if !content.is_empty() {
                    content_builder.add_text(&content, last_style);
                }

                content.clear();
                last_style = style;
            }

            content.push(square_content);

            // Render last column and break row
            if column == (columns - 1) {
                if !content.is_empty() {
                    content_builder.add_text(&content, last_style);
                }

                break;
            }
        }

        content_builder.finish_line();
    }

    #[inline]
    #[cfg(use_wa)]
    pub fn decrease_foreground_opacity(&mut self, _acc: f32) {
        // self.foreground_opacity -= acc;
    }

    #[inline]
    #[cfg(use_wa)]
    pub fn increase_foreground_opacity(&mut self, _acc: f32) {
        // self.foreground_opacity += acc;
    }

    #[inline]
    fn compute_color(&self, color: &AnsiColor, flags: Flags) -> ColorArray {
        match color {
            AnsiColor::Named(ansi_name) => match (ansi_name, flags) {
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
                if !flags.contains(Flags::DIM) {
                    rgb.to_arr()
                } else {
                    rgb.to_arr_with_dim()
                }
            }
            AnsiColor::Indexed(index) => {
                let index = match (flags & Flags::DIM_BOLD, index) {
                    (Flags::DIM, 8..=15) => *index as usize - 8,
                    (Flags::DIM, 0..=7) => {
                        NamedColor::DimBlack as usize + *index as usize
                    }
                    _ => *index as usize,
                };

                self.colors[index]
            }
        }
    }

    #[inline]
    fn compute_bg_color(&self, square: &Square) -> ColorArray {
        match square.bg {
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
        }
    }

    // #[inline]
    // #[allow(dead_code)]
    // fn create_graphic_sugar(&self, square: &Square) -> Sugar {
    //     let media = &square.graphics().unwrap()[0].texture;
    //     Sugar {
    //         media: Some(SugarGraphic {
    //             id: media.id,
    //             width: media.width,
    //             height: media.height,
    //         }),
    //         ..Sugar::default()
    //     }
    // }

    #[inline]
    fn create_cursor_style(&self, square: &Square) -> (FragmentStyle, char) {
        let font_attrs = match (
            square.flags.contains(Flags::ITALIC),
            square.flags.contains(Flags::BOLD_ITALIC),
            square.flags.contains(Flags::BOLD),
        ) {
            (true, _, _) => (Stretch::NORMAL, Weight::NORMAL, Style::Italic),
            (_, true, _) => (Stretch::NORMAL, Weight::BOLD, Style::Italic),
            (_, _, true) => (Stretch::NORMAL, Weight::BOLD, Style::Normal),
            _ => (Stretch::NORMAL, Weight::NORMAL, Style::Normal),
        };

        let mut color = self.compute_color(&square.fg, square.flags);
        let mut background_color = self.compute_bg_color(square);
        // If IME is enabled we get the current content to cursor
        let content = if self.is_ime_enabled {
            self.cursor.content
        } else {
            square.c
        };

        if square.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut background_color, &mut color);
        }

        // If IME is enabled or is a block cursor, put background color
        // when cursor is over the character
        if self.is_ime_enabled || self.cursor.state.content == CursorShape::Block {
            color = self.named_colors.background.0;
        }

        let mut style = FragmentStyle {
            color,
            background_color: Some(background_color),
            font_attrs,
            ..FragmentStyle::default()
        };

        let cursor_color = if !self.is_vi_mode_enabled {
            self.named_colors.cursor
        } else {
            self.named_colors.vi_cursor
        };

        let mut has_underline_cursor = false;

        match self.cursor.state.content {
            CursorShape::Underline => {
                style.decoration =
                    Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                        offset: -1.0,
                        size: -1.0,
                        is_doubled: false,
                        shape: UnderlineShape::Regular,
                    }));
                style.decoration_color = Some(cursor_color);

                has_underline_cursor = true;
            }
            CursorShape::Block => {
                style.cursor = SugarCursor::Block(cursor_color);
            }
            CursorShape::Beam => {
                style.cursor = SugarCursor::Caret(cursor_color);
            }
            CursorShape::Hidden => {}
        }

        let (decoration, decoration_color) =
            self.compute_decoration(square, has_underline_cursor);
        style.decoration = decoration;
        style.decoration_color = decoration_color;

        (style, content)
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
        rows: &[Row<Square>],
        cursor: CursorState,
        sugarloaf: &mut Sugarloaf,
        context_manager: &crate::context::ContextManager<rio_backend::event::EventProxy>,
        display_offset: i32,
        has_blinking_enabled: bool,
    ) {
        let layout = sugarloaf.layout();
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

        let mut content_builder = Content::builder();

        for (i, row) in rows.iter().enumerate() {
            let has_cursor = is_cursor_visible && self.cursor.state.pos.row == i;
            self.create_line(
                &mut content_builder,
                row,
                has_cursor,
                pos::Line((i as i32) - display_offset),
            );
        }

        sugarloaf.set_content(content_builder.build());

        self.navigation.content(
            (layout.width, layout.height),
            layout.dimensions.scale,
            context_manager.titles.key.as_str(),
            &context_manager.titles.titles,
            context_manager.current_index(),
            context_manager.len(),
        );

        sugarloaf.set_objects(self.navigation.objects.clone());

        // for text in self.navigation.texts.iter() {
        //     sugarloaf.text(
        //         text.position,
        //         text.content.to_owned(),
        //         text.font_size,
        //         text.color,
        //         true,
        //     );
        // }
    }
}
