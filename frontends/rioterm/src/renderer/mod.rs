pub mod navigation;
mod search;
pub mod utils;

use crate::ansi::CursorShape;
use crate::context::renderable::{Cursor, RenderableContent, RenderableContentStrategy};
use crate::context::ContextManager;
use crate::crosswords::grid::row::Row;
use crate::crosswords::pos::{Column, Line, Pos};
use crate::crosswords::square::{Flags, Square};
use crate::screen::hint::HintMatches;
use navigation::ScreenNavigation;
use rio_backend::config::colors::{
    term::{List, TermColors},
    AnsiColor, ColorArray, Colors, NamedColor,
};
use rio_backend::config::Config;
use rio_backend::event::EventProxy;
use rio_backend::sugarloaf::{
    Content, FragmentStyle, FragmentStyleDecoration, Graphic, Stretch, Style,
    SugarCursor, Sugarloaf, UnderlineInfo, UnderlineShape, Weight,
};
use std::collections::HashMap;
use std::ops::RangeInclusive;

use rustc_hash::FxHashMap;
use unicode_width::UnicodeWidthChar;

pub struct Renderer {
    is_vi_mode_enabled: bool,
    pub is_kitty_keyboard_enabled: bool,
    pub named_colors: Colors,
    pub colors: List,
    pub navigation: ScreenNavigation,
    pub config_has_blinking_enabled: bool,
    pub config_blinking_interval: u64,
    ignore_selection_fg_color: bool,
    #[allow(unused)]
    pub option_as_alt: String,
    #[allow(unused)]
    pub macos_use_unified_titlebar: bool,
    // Dynamic background keep track of the original bg color and
    // the same r,g,b with the mutated alpha channel.
    pub dynamic_background: ([f32; 4], wgpu::Color, bool),
    font_context: rio_backend::sugarloaf::font::FontLibrary,
    font_cache: FxHashMap<
        (char, rio_backend::sugarloaf::font_introspector::Attributes),
        (usize, f32),
    >,
    active_search: Option<String>,
}

impl Renderer {
    pub fn new(
        config: &Config,
        font_context: &rio_backend::sugarloaf::font::FontLibrary,
    ) -> Renderer {
        let term_colors = TermColors::default();
        let colors = List::from(&term_colors);
        let named_colors = config.colors;

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

        Renderer {
            macos_use_unified_titlebar: config.window.macos_use_unified_titlebar,
            config_blinking_interval: config.cursor.blinking_interval.clamp(350, 1200),
            option_as_alt: config.option_as_alt.to_lowercase(),
            is_kitty_keyboard_enabled: config.keyboard.use_kitty_keyboard_protocol,
            is_vi_mode_enabled: false,
            config_has_blinking_enabled: config.cursor.blinking,
            ignore_selection_fg_color: config.ignore_selection_fg_color,
            colors,
            navigation: ScreenNavigation::new(
                config.navigation.clone(),
                color_automation,
                config.padding_y,
            ),
            named_colors,
            dynamic_background,
            active_search: None,
            font_cache: FxHashMap::default(),
            font_context: font_context.clone(),
        }
    }

    #[inline]
    pub fn set_active_search(&mut self, active_search: Option<String>) {
        self.active_search = active_search;
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

        let (decoration, decoration_color) = self.compute_decoration(square);

        (
            FragmentStyle {
                color: foreground_color,
                background_color,
                font_attrs: font_attrs.into(),
                decoration,
                decoration_color,
                ..FragmentStyle::default()
            },
            content,
        )
    }

    #[inline]
    fn compute_decoration(
        &self,
        square: &Square,
    ) -> (Option<FragmentStyleDecoration>, Option<[f32; 4]>) {
        let mut decoration = None;
        let mut decoration_color = None;

        if square.flags.contains(Flags::UNDERLINE) {
            decoration = Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                offset: -1.0,
                size: 1.0,
                is_doubled: false,
                shape: UnderlineShape::Regular,
            }));
        } else if square.flags.contains(Flags::STRIKEOUT) {
            decoration = Some(FragmentStyleDecoration::Strikethrough);
        } else if square.flags.contains(Flags::DOUBLE_UNDERLINE) {
            decoration = Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                offset: -1.0,
                size: 1.0,
                is_doubled: true,
                shape: UnderlineShape::Regular,
            }));
        } else if square.flags.contains(Flags::DOTTED_UNDERLINE) {
            decoration = Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                offset: -1.0,
                size: 2.0,
                is_doubled: false,
                shape: UnderlineShape::Dotted,
            }));
        } else if square.flags.contains(Flags::DASHED_UNDERLINE) {
            decoration = Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                offset: -1.0,
                size: 2.0,
                is_doubled: false,
                shape: UnderlineShape::Dashed,
            }));
        } else if square.flags.contains(Flags::UNDERCURL) {
            decoration = Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                offset: -1.0,
                size: 2.0,
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
    #[allow(clippy::too_many_arguments)]
    fn create_line(
        &mut self,
        builder: &mut Content,
        row: &Row<Square>,
        has_cursor: bool,
        line_opt: Option<usize>,
        line: Line,
        renderable_content: &RenderableContent,
        search_hints: &mut Option<HintMatches>,
        focused_match: &Option<RangeInclusive<Pos>>,
        is_active: bool,
    ) {
        let cursor = &renderable_content.cursor;
        let hyperlink_range = renderable_content.hyperlink_range;
        let selection_range = renderable_content.selection_range;
        let columns: usize = row.len();
        let mut content = String::default();
        let mut last_char_was_space = false;
        let mut last_style = FragmentStyle::default();

        for column in 0..columns {
            let square = &row.inner[column];

            if square.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }

            let (mut style, square_content) =
                if has_cursor && column == cursor.state.pos.col {
                    self.create_cursor_style(square, cursor, is_active)
                } else {
                    self.create_style(square)
                };

            if hyperlink_range.is_some()
                && square.hyperlink().is_some()
                && hyperlink_range
                    .unwrap()
                    .contains(Pos::new(line, Column(column)))
            {
                style.decoration =
                    Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                        offset: -1.0,
                        size: -1.0,
                        is_doubled: false,
                        shape: UnderlineShape::Regular,
                    }));
            } else if selection_range.is_some()
                && selection_range
                    .unwrap()
                    .contains(Pos::new(line, Column(column)))
            {
                style.color = if self.ignore_selection_fg_color {
                    self.compute_color(&square.fg, square.flags)
                } else {
                    self.named_colors.selection_foreground
                };
                style.background_color = Some(self.named_colors.selection_background);
            } else if search_hints.is_some()
                && search_hints.as_mut().map_or(false, |search| {
                    search.advance(Pos::new(line, Column(column)))
                })
            {
                let is_focused = focused_match
                    .as_ref()
                    .map_or(false, |fm| fm.contains(&Pos::new(line, Column(column))));
                if is_focused {
                    style.color = self.named_colors.search_focused_match_foreground;
                    style.background_color =
                        Some(self.named_colors.search_focused_match_background);
                } else {
                    style.color = self.named_colors.search_match_foreground;
                    style.background_color =
                        Some(self.named_colors.search_match_background);
                }
            }

            if square.flags.contains(Flags::GRAPHICS) {
                // let graphics = square.graphics().map(|graphics| {
                //     graphics
                //         .iter()
                //         .map(|graphic| Graphic {
                //             id: graphic.texture.id,
                //             offset_x: graphic.offset_x,
                //             offset_y: graphic.offset_y,
                //         })
                //         .collect::<_>()
                // });
                // style.media = Some(graphics);
                let graphic = &square.graphics().unwrap()[0];
                style.media = Some(Graphic {
                    id: graphic.texture.id,
                    offset_x: graphic.offset_x,
                    offset_y: graphic.offset_y,
                });
                style.background_color = None;
            }

            if let Some((font_id, width)) =
                self.font_cache.get(&(square_content, style.font_attrs))
            {
                style.font_id = *font_id;
                style.width = *width;
            } else {
                let mut width = square.c.width().unwrap_or(1) as f32;
                let mut font_ctx = self.font_context.inner.lock();

                // There is no simple way to define what's emoji
                // could have to refer to the Unicode tables. However it could
                // be leading to misleading results. For example if we used
                // unicode and internationalization functionalities like
                // https://github.com/open-i18n/rust-unic/, then characters
                // like "◼" would be valid emojis. For a terminal context,
                // the character "◼" is not an emoji and should be treated as
                // single width. So, we completely rely on what font is
                // being used and then set width 2 for it.
                if let Some((font_id, is_emoji)) =
                    font_ctx.find_best_font_match(square_content, &style)
                {
                    style.font_id = font_id;
                    if is_emoji {
                        width = 2.0;
                    }
                }
                style.width = width;

                self.font_cache.insert(
                    (square_content, style.font_attrs),
                    (style.font_id, style.width),
                );
            };

            if square_content == ' ' {
                if !last_char_was_space {
                    if !content.is_empty() {
                        if let Some(line) = line_opt {
                            builder.add_text_on_line(line, &content, last_style);
                        } else {
                            builder.add_text(&content, last_style);
                        }
                        content.clear();
                    }

                    last_char_was_space = true;
                    last_style = style;
                }
            } else {
                if last_char_was_space && !content.is_empty() {
                    if let Some(line) = line_opt {
                        builder.add_text_on_line(line, &content, last_style);
                    } else {
                        builder.add_text(&content, last_style);
                    }
                    content.clear();
                }

                last_char_was_space = false;
            }

            if last_style != style {
                if !content.is_empty() {
                    if let Some(line) = line_opt {
                        builder.add_text_on_line(line, &content, last_style);
                    } else {
                        builder.add_text(&content, last_style);
                    }
                    content.clear();
                }

                last_style = style;
            }

            content.push(square_content);

            // Render last column and break row
            if column == (columns - 1) {
                if !content.is_empty() {
                    if let Some(line) = line_opt {
                        builder.add_text_on_line(line, &content, last_style);
                    } else {
                        builder.add_text(&content, last_style);
                    }
                }

                break;
            }
        }

        if let Some(line) = line_opt {
            builder.build_line(line);
        } else {
            builder.new_line();
        }
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

    #[inline]
    fn create_cursor_style(
        &self,
        square: &Square,
        cursor: &Cursor,
        is_active: bool,
    ) -> (FragmentStyle, char) {
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
        let content = if cursor.is_ime_enabled {
            cursor.content
        } else {
            square.c
        };

        if square.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut background_color, &mut color);
        }

        let has_dynamic_background = self.dynamic_background.2
            && background_color[0] == self.dynamic_background.0[0]
            && background_color[1] == self.dynamic_background.0[1]
            && background_color[2] == self.dynamic_background.0[2];
        let background_color = if has_dynamic_background
            && (cursor.state.content != CursorShape::Block && is_active)
        {
            None
        } else {
            Some(background_color)
        };

        // If IME is or cursor is block enabled, put background color
        // when cursor is over the character
        match (
            cursor.is_ime_enabled,
            (cursor.state.content == CursorShape::Block || !is_active),
        ) {
            (_, true) => {
                color = self.named_colors.background.0;
            }
            (true, false) => {
                color = self.named_colors.foreground;
            }
            (false, false) => {}
        }

        let mut style = FragmentStyle {
            color,
            background_color,
            font_attrs: font_attrs.into(),
            ..FragmentStyle::default()
        };

        let cursor_color = if !self.is_vi_mode_enabled {
            self.named_colors.cursor
        } else {
            self.named_colors.vi_cursor
        };

        let (decoration, decoration_color) = self.compute_decoration(square);
        style.decoration = decoration;
        style.decoration_color = decoration_color;

        match cursor.state.content {
            CursorShape::Underline => {
                style.decoration =
                    Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                        offset: 0.0,
                        size: 3.0,
                        is_doubled: false,
                        shape: UnderlineShape::Regular,
                    }));
                style.decoration_color = Some(cursor_color);
            }
            CursorShape::Block => {
                style.cursor = Some(SugarCursor::Block(cursor_color));
            }
            CursorShape::Beam => {
                style.cursor = Some(SugarCursor::Caret(cursor_color));
            }
            CursorShape::Hidden => {}
        }

        if !is_active {
            style.decoration = None;
            style.cursor = Some(SugarCursor::HollowBlock(cursor_color));
        }

        (style, content)
    }

    #[inline]
    pub fn set_vi_mode(&mut self, is_vi_mode_enabled: bool) {
        self.is_vi_mode_enabled = is_vi_mode_enabled;
    }

    #[inline]
    pub fn prepare_term(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        context_manager: &mut ContextManager<EventProxy>,
        hints: &mut Option<HintMatches>,
        focused_match: &Option<RangeInclusive<Pos>>,
    ) {
        let content = sugarloaf.content();
        let grid = context_manager.current_grid_mut();
        let active_index = grid.current;

        for (index, grid_context) in grid.contexts_mut().iter_mut().enumerate() {
            let is_active = active_index == index;
            let context = grid_context.context_mut();
            let rich_text_id = context.rich_text_id;
            let renderable_content = context.renderable_content();
            let mut is_cursor_visible = renderable_content.is_cursor_visible
                && renderable_content.cursor.state.is_visible();
            if !is_active && renderable_content.cursor.state.is_visible() {
                is_cursor_visible = true;
            }

            let display_offset = renderable_content.display_offset;

            match &renderable_content.strategy {
                RenderableContentStrategy::Full => {
                    content.sel(rich_text_id);
                    content.clear();
                    for (i, row) in renderable_content.inner.iter().enumerate() {
                        let has_cursor = is_cursor_visible
                            && renderable_content.cursor.state.pos.row == i;
                        self.create_line(
                            content,
                            row,
                            has_cursor,
                            None,
                            Line((i as i32) - display_offset),
                            renderable_content,
                            hints,
                            focused_match,
                            is_active,
                        );
                    }
                    content.build();
                }
                RenderableContentStrategy::Lines(lines) => {
                    content.sel(rich_text_id);
                    for line in lines {
                        let line = *line;
                        if let Some(line_data) = renderable_content.inner.get(line) {
                            let has_cursor = is_cursor_visible
                                && renderable_content.cursor.state.pos.row == line;
                            content.clear_line(line);
                            self.create_line(
                                content,
                                line_data,
                                has_cursor,
                                Some(line),
                                Line((line as i32) - display_offset),
                                renderable_content,
                                hints,
                                focused_match,
                                is_active,
                            );
                        }
                    }
                }
                RenderableContentStrategy::Noop => {}
            }
        }

        let window_size = sugarloaf.window_size();
        let scale_factor = sugarloaf.scale_factor();
        let mut objects = Vec::with_capacity(30);
        self.navigation.build_objects(
            (window_size.width, window_size.height, scale_factor),
            &self.named_colors,
            context_manager,
            self.active_search.is_some(),
            &mut objects,
        );

        if let Some(active_search_content) = &self.active_search {
            search::draw_search_bar(
                &mut objects,
                &self.named_colors,
                (window_size.width, window_size.height, scale_factor),
                active_search_content,
            );

            self.active_search = None;
        }

        for rte in context_manager.grid_objects() {
            objects.push(rte);
        }

        sugarloaf.set_objects(objects);
    }
}
