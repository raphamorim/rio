mod font_cache;
pub mod navigation;
mod search;
pub mod utils;

use font_cache::FontCache;

use crate::ansi::CursorShape;
use crate::context::renderable::{Cursor, RenderableContent};
use crate::context::ContextManager;
use crate::crosswords::grid::row::Row;
use crate::crosswords::pos::{Column, Line, Pos};
use crate::crosswords::square::{Flags, Square};
use crate::screen::hint::HintMatches;
use navigation::ScreenNavigation;
use rio_backend::ansi::graphics::UpdateQueues;
use rio_backend::config::colors::term::TermColors;
use rio_backend::config::colors::{
    term::{List, DIM_FACTOR},
    AnsiColor, ColorArray, Colors, NamedColor,
};
use rio_backend::config::Config;
use rio_backend::crosswords::TermDamage;
use rio_backend::event::EventProxy;
use rio_backend::sugarloaf::{
    drawable_character, Content, FragmentStyle, FragmentStyleDecoration, Graphic,
    Stretch, Style, SugarCursor, Sugarloaf, UnderlineInfo, UnderlineShape, Weight,
};
use std::collections::HashMap;
use std::ops::RangeInclusive;

use unicode_width::UnicodeWidthChar;

#[derive(Default)]
pub struct Search {
    rich_text_id: Option<usize>,
    active_search: Option<String>,
}

pub struct Renderer {
    is_vi_mode_enabled: bool,
    draw_bold_text_with_light_colors: bool,
    use_drawable_chars: bool,
    pub named_colors: Colors,
    pub colors: List,
    pub navigation: ScreenNavigation,
    unfocused_split_opacity: f32,
    last_active: usize,
    pub config_has_blinking_enabled: bool,
    pub config_blinking_interval: u64,
    ignore_selection_fg_color: bool,
    pub search: Search,
    #[allow(unused)]
    pub option_as_alt: String,
    #[allow(unused)]
    pub macos_use_unified_titlebar: bool,
    // Dynamic background keep track of the original bg color and
    // the same r,g,b with the mutated alpha channel.
    pub dynamic_background: ([f32; 4], wgpu::Color, bool),
    font_context: rio_backend::sugarloaf::font::FontLibrary,
    font_cache: FontCache,
}

impl Renderer {
    pub fn new(
        config: &Config,
        font_context: &rio_backend::sugarloaf::font::FontLibrary,
    ) -> Renderer {
        let colors = List::from(&config.colors);
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

        let mut renderer = Renderer {
            unfocused_split_opacity: config.navigation.unfocused_split_opacity,
            last_active: 0,
            use_drawable_chars: config.fonts.use_drawable_chars,
            draw_bold_text_with_light_colors: config.draw_bold_text_with_light_colors,
            macos_use_unified_titlebar: config.window.macos_use_unified_titlebar,
            config_blinking_interval: config.cursor.blinking_interval.clamp(350, 1200),
            option_as_alt: config.option_as_alt.to_lowercase(),
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
            search: Search::default(),
            font_cache: FontCache::new(),
            font_context: font_context.clone(),
        };

        // Pre-populate font cache with common characters for better performance
        renderer.font_cache.pre_populate(font_context);

        renderer
    }

    #[inline]
    pub fn set_active_search(&mut self, active_search: Option<String>) {
        self.search.active_search = active_search;
    }

    #[inline]
    fn create_style(
        &mut self,
        square: &Square,
        term_colors: &TermColors,
    ) -> (FragmentStyle, char) {
        let flags = square.flags;

        let mut foreground_color = self.compute_color(&square.fg, flags, term_colors);
        let mut background_color = self.compute_bg_color(square, term_colors);

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

        let (decoration, decoration_color) = self.compute_decoration(square, term_colors);

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
        term_colors: &TermColors,
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
                decoration_color =
                    Some(self.compute_color(&color, square.flags, term_colors));
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
        term_colors: &TermColors,
        is_active: bool,
    ) {
        // let start = std::time::Instant::now();
        let cursor = &renderable_content.cursor;
        let hyperlink_range = renderable_content.hyperlink_range;
        let selection_range = renderable_content.selection_range;
        let columns: usize = row.len();
        let mut content = String::with_capacity(columns);
        let mut last_char_was_space = false;
        let mut last_style = FragmentStyle::default();

        // Collect all characters that need font lookups to batch them
        let mut font_lookups = Vec::new();
        let mut styles_and_chars = Vec::with_capacity(columns);

        // First pass: collect all styles and identify font cache misses
        for column in 0..columns {
            let square = &row.inner[column];

            if square.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }

            let (mut style, square_content) =
                if has_cursor && column == cursor.state.pos.col {
                    self.create_cursor_style(square, cursor, is_active, term_colors)
                } else {
                    self.create_style(square, term_colors)
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
                    self.compute_color(&square.fg, square.flags, term_colors)
                } else {
                    self.named_colors.selection_foreground
                };
                style.background_color = Some(self.named_colors.selection_background);
            } else if search_hints.is_some()
                && search_hints
                    .as_mut()
                    .is_some_and(|search| search.advance(Pos::new(line, Column(column))))
            {
                let is_focused = focused_match
                    .as_ref()
                    .is_some_and(|fm| fm.contains(&Pos::new(line, Column(column))));
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

            if !is_active {
                style.color[3] = self.unfocused_split_opacity;
                if let Some(mut background_color) = style.background_color {
                    background_color[3] = self.unfocused_split_opacity;
                }
            }

            if square.flags.contains(Flags::GRAPHICS) {
                let graphic = &square.graphics().unwrap()[0];
                style.media = Some(Graphic {
                    id: graphic.texture.id,
                    offset_x: graphic.offset_x,
                    offset_y: graphic.offset_y,
                });
                style.background_color = None;
            }

            // Handle drawable characters
            if self.use_drawable_chars {
                if let Some(character) = drawable_character(square_content) {
                    style.drawable_char = Some(character);
                }
            }

            let has_drawable_char = style.drawable_char.is_some();
            if !has_drawable_char {
                if let Some((font_id, width)) =
                    self.font_cache.get(&(square_content, style.font_attrs))
                {
                    style.font_id = *font_id;
                    style.width = *width;
                } else {
                    // Mark this character for font lookup
                    font_lookups.push((
                        styles_and_chars.len(),
                        square_content,
                        style.font_attrs,
                    ));
                }
            }

            styles_and_chars.push((style, square_content, column));
        }

        // Batch font lookups with a single lock acquisition
        if !font_lookups.is_empty() {
            let font_ctx = self.font_context.inner.read();
            for (style_index, square_content, font_attrs) in font_lookups {
                let mut width = square_content.width().unwrap_or(1) as f32;
                let style = &mut styles_and_chars[style_index].0;

                if let Some((font_id, is_emoji)) =
                    font_ctx.find_best_font_match(square_content, style)
                {
                    style.font_id = font_id;
                    if is_emoji {
                        width = 2.0;
                    }
                }
                style.width = width;

                self.font_cache
                    .insert((square_content, font_attrs), (style.font_id, style.width));
            }
        }

        // Second pass: render the line using the resolved styles
        for (style, square_content, column) in styles_and_chars {
            // Handle drawable characters
            if style.drawable_char.is_some() {
                if !content.is_empty() {
                    if let Some(line) = line_opt {
                        builder.add_text_on_line(line, &content, last_style);
                    } else {
                        builder.add_text(&content, last_style);
                    }
                    content.clear();
                }

                last_style = style;
                content.push(' '); // Ignore font shaping
            } else {
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
            }

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

        // let duration = start.elapsed();
        // println!(
        //     "Time elapsed in --renderer.update.create_line() is: {:?}",
        //     duration
        // );
    }

    #[inline]
    fn compute_color(
        &self,
        color: &AnsiColor,
        flags: Flags,
        term_colors: &TermColors,
    ) -> ColorArray {
        match color {
            AnsiColor::Named(ansi) => {
                match (
                    self.draw_bold_text_with_light_colors,
                    flags & Flags::DIM_BOLD,
                ) {
                    // If no bright foreground is set, treat it like the BOLD flag doesn't exist.
                    (_, Flags::DIM_BOLD)
                        if ansi == &NamedColor::Foreground
                            && self.named_colors.light_foreground.is_none() =>
                    {
                        self.color(NamedColor::DimForeground as usize, term_colors)
                    }
                    // Draw bold text in bright colors *and* contains bold flag.
                    (true, Flags::BOLD) => {
                        self.color(ansi.to_light() as usize, term_colors)
                    }
                    // Cell is marked as dim and not bold.
                    (_, Flags::DIM) | (false, Flags::DIM_BOLD) => {
                        self.color(ansi.to_dim() as usize, term_colors)
                    }
                    // None of the above, keep original color..
                    _ => self.color(*ansi as usize, term_colors),
                }
            }
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

                self.color(index, term_colors)
            }
        }
    }

    #[inline]
    fn compute_bg_color(&self, square: &Square, term_colors: &TermColors) -> ColorArray {
        match square.bg {
            AnsiColor::Named(ansi) => self.color(ansi as usize, term_colors),
            AnsiColor::Spec(rgb) => match square.flags & Flags::DIM {
                Flags::DIM => (&(rgb * DIM_FACTOR)).into(),
                _ => (&rgb).into(),
            },
            AnsiColor::Indexed(idx) => {
                let idx = match (
                    self.draw_bold_text_with_light_colors,
                    square.flags & Flags::DIM_BOLD,
                    idx,
                ) {
                    (true, Flags::BOLD, 0..=7) => idx as usize + 8,
                    (false, Flags::DIM, 8..=15) => idx as usize - 8,
                    (false, Flags::DIM, 0..=7) => {
                        NamedColor::DimBlack as usize + idx as usize
                    }
                    _ => idx as usize,
                };

                self.color(idx, term_colors)
            }
        }
    }

    #[inline]
    fn create_cursor_style(
        &self,
        square: &Square,
        cursor: &Cursor,
        is_active: bool,
        term_colors: &TermColors,
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

        let mut color = self.compute_color(&square.fg, square.flags, term_colors);
        let mut background_color = self.compute_bg_color(square, term_colors);
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
            term_colors[NamedColor::Cursor].unwrap_or(self.named_colors.cursor)
        } else {
            self.named_colors.vi_cursor
        };

        let (decoration, decoration_color) = self.compute_decoration(square, term_colors);
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

    // Get the RGB value for a color index.
    #[inline]
    pub fn color(&self, color: usize, term_colors: &TermColors) -> ColorArray {
        term_colors[color].unwrap_or(self.colors[color])
    }

    #[inline]
    fn update_search_rich_text(&mut self, content: &mut Content) {
        if let Some(active_search_content) = &self.search.active_search {
            if let Some(search_rich_text) = self.search.rich_text_id {
                if active_search_content.is_empty() {
                    content
                        .sel(search_rich_text)
                        .clear()
                        .new_line()
                        .add_text(
                            &String::from("Search: type something..."),
                            FragmentStyle {
                                color: [
                                    self.named_colors.foreground[0],
                                    self.named_colors.foreground[1],
                                    self.named_colors.foreground[2],
                                    self.named_colors.foreground[3] - 0.3,
                                ],
                                ..FragmentStyle::default()
                            },
                        )
                        .build();
                } else {
                    let style = FragmentStyle {
                        color: self.named_colors.foreground,
                        ..FragmentStyle::default()
                    };
                    let line = content.sel(search_rich_text);
                    line.clear().new_line().add_text("Search: ", style);

                    // Collect characters that need font lookups
                    let mut font_lookups = Vec::new();
                    let mut char_styles = Vec::new();

                    for character in active_search_content.chars() {
                        let mut char_style = style;
                        if let Some((font_id, width)) =
                            self.font_cache.get(&(character, style.font_attrs))
                        {
                            char_style.font_id = *font_id;
                            char_style.width = *width;
                        } else {
                            font_lookups.push((char_styles.len(), character));
                        }
                        char_styles.push((char_style, character));
                    }

                    // Batch font lookups with a single lock acquisition
                    if !font_lookups.is_empty() {
                        let font_ctx = self.font_context.inner.read();
                        for (style_index, character) in font_lookups {
                            let mut width = character.width().unwrap_or(1) as f32;
                            let char_style = &mut char_styles[style_index].0;

                            if let Some((font_id, is_emoji)) =
                                font_ctx.find_best_font_match(character, char_style)
                            {
                                char_style.font_id = font_id;
                                if is_emoji {
                                    width = 2.0;
                                }
                            }
                            char_style.width = width;
                        }
                    }

                    // Render all characters
                    for (char_style, character) in char_styles {
                        line.add_text_on_line(
                            // Add on first line
                            1,
                            &character.to_string(),
                            char_style,
                        );
                    }

                    line.build();
                }
            }
        }
    }

    #[inline]
    pub fn run(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        context_manager: &mut ContextManager<EventProxy>,
        hints: &mut Option<HintMatches>,
        focused_match: &Option<RangeInclusive<Pos>>,
    ) {
        // let start = std::time::Instant::now();

        // In case rich text for search was not created
        let has_search = self.search.active_search.is_some();
        if has_search && self.search.rich_text_id.is_none() {
            let search_rich_text = sugarloaf.create_temp_rich_text();
            sugarloaf.set_rich_text_font_size(&search_rich_text, 12.0);
            self.search.rich_text_id = Some(search_rich_text);
        }

        let mut graphic_queues: Option<Vec<UpdateQueues>> = None;

        // let grid_start = std::time::Instant::now();
        let grid = context_manager.current_grid_mut();
        let active_index = grid.current;
        let mut has_active_changed = false;
        if self.last_active != active_index {
            has_active_changed = true;

            self.last_active = active_index;
        }

        for (index, grid_context) in grid.contexts_mut().iter_mut().enumerate() {
            let is_active = active_index == index;
            let context = grid_context.context_mut();

            let mut has_ime = false;
            if let Some(preedit) = context.ime.preedit() {
                if let Some(content) = preedit.text.chars().next() {
                    context.renderable_content.cursor.content = content;
                    context.renderable_content.cursor.is_ime_enabled = true;
                    has_ime = true;
                }
            }

            if !has_ime {
                context.renderable_content.cursor.is_ime_enabled = false;
                context.renderable_content.cursor.content =
                    context.renderable_content.cursor.content_ref;
            }

            // let duration = start.elapsed();
            // println!("Time elapsed in antes is: {:?}", duration);
            // let renderable_content = context.renderable_content();
            let force_full_damage = has_active_changed
                || context.renderable_content.has_pending_updates
                || is_active && hints.is_some();

            let mut specific_lines = None;
            let (colors, display_offset, blinking_cursor, visible_rows) = {
                let mut terminal = context.terminal.lock();
                let result = (
                    terminal.colors,
                    terminal.display_offset(),
                    terminal.blinking_cursor,
                    terminal.visible_rows(),
                );

                context.renderable_content.cursor.state = terminal.cursor();

                if let Some(queues_to_add) = terminal.graphics_take_queues() {
                    if let Some(ref mut queues) = graphic_queues {
                        queues.push(queues_to_add);
                    } else {
                        graphic_queues = Some(vec![queues_to_add]);
                    }
                }

                // Check for partial damage to optimize rendering
                if !force_full_damage && !terminal.is_fully_damaged() {
                    if let TermDamage::Partial(lines) = terminal.damage() {
                        // Pre-allocate with estimated capacity to reduce allocations
                        let capacity =
                            lines.size_hint().1.unwrap_or(lines.size_hint().0).min(256);
                        let mut own_lines =
                            std::collections::HashSet::with_capacity(capacity);
                        for line in lines {
                            own_lines.insert(line.line);
                        }
                        // Only set specific_lines if there are actually damaged lines
                        if !own_lines.is_empty() {
                            specific_lines = Some(own_lines);
                        }
                    };
                }
                terminal.reset_damage();
                result
            };

            // If the last line is bigger than the actual visible rows, then some resize
            // has happened. In this case, request full draw.
            if let Some(ref lines) = specific_lines {
                if let Some(max_value) = lines.iter().max() {
                    if max_value > &(visible_rows.len() - 1) {
                        specific_lines = None;
                    }
                }
            }

            // let duration = start.elapsed();
            // println!("Time elapsed in antes-antes is: {:?}", duration);
            let rich_text_id = context.rich_text_id;

            let mut is_cursor_visible =
                context.renderable_content.cursor.state.is_visible();
            context.renderable_content.has_blinking_enabled = blinking_cursor;
            if blinking_cursor {
                let has_selection = context.renderable_content.selection_range.is_some();
                if !has_selection {
                    let mut should_blink = true;
                    if let Some(last_typing_time) = context.renderable_content.last_typing
                    {
                        if last_typing_time.elapsed() < std::time::Duration::from_secs(1)
                        {
                            should_blink = false;
                        }
                    }

                    if should_blink {
                        let now = std::time::Instant::now();
                        let should_toggle = if let Some(last_blink) = context.renderable_content.last_blink_toggle {
                            now.duration_since(last_blink).as_millis() >= self.config_blinking_interval as u128
                        } else {
                            // First time: start with cursor visible and set initial timing
                            context.renderable_content.is_blinking_cursor_visible = true;
                            context.renderable_content.last_blink_toggle = Some(now);
                            false // Don't toggle on first frame
                        };

                        if should_toggle {
                            context.renderable_content.is_blinking_cursor_visible =
                                !context.renderable_content.is_blinking_cursor_visible;
                            context.renderable_content.last_blink_toggle = Some(now);
                            
                            if let Some(ref mut lines) = specific_lines {
                                lines.insert(
                                    context.renderable_content.cursor.state.pos.row.0
                                        as usize,
                                );
                            }
                        }
                    } else {
                        // When not blinking (e.g., during typing), ensure cursor is visible
                        context.renderable_content.is_blinking_cursor_visible = true;
                        // Reset blink timing when not blinking so it starts fresh when blinking resumes
                        context.renderable_content.last_blink_toggle = None;
                    }
                } else {
                    // When there's a selection, keep cursor visible and reset blink timing
                    context.renderable_content.is_blinking_cursor_visible = true;
                    context.renderable_content.last_blink_toggle = None;
                }

                is_cursor_visible = context.renderable_content.is_blinking_cursor_visible;
            }

            if !is_active && context.renderable_content.cursor.state.is_visible() {
                is_cursor_visible = true;
            }

            context.renderable_content.has_pending_updates = false;
            let content = sugarloaf.content();
            match specific_lines {
                None => {
                    // let start = std::time::Instant::now();
                    content.sel(rich_text_id);
                    content.clear();
                    for (i, row) in visible_rows.iter().enumerate() {
                        let has_cursor = is_cursor_visible
                            && context.renderable_content.cursor.state.pos.row == i;
                        self.create_line(
                            content,
                            row,
                            has_cursor,
                            None,
                            Line((i as i32) - display_offset as i32),
                            &context.renderable_content,
                            hints,
                            focused_match,
                            &colors,
                            is_active,
                        );
                    }
                    content.build();
                    // let duration = start.elapsed();
                    // println!("Time elapsed in -renderer.TermDamage::Full is: {:?}", duration);
                }
                Some(lines) => {
                    content.sel(rich_text_id);
                    for line in lines {
                        let has_cursor = is_cursor_visible
                            && context.renderable_content.cursor.state.pos.row == line;
                        content.clear_line(line);
                        if let Some(visible_row) = visible_rows.get(line) {
                            self.create_line(
                                content,
                                visible_row,
                                has_cursor,
                                Some(line),
                                Line(line as i32),
                                &context.renderable_content,
                                hints,
                                focused_match,
                                &colors,
                                is_active,
                            );
                        }
                    }
                }
            };
        }

        if let Some(op) = graphic_queues.take() {
            for queues in op {
                for graphic_data in queues.pending {
                    sugarloaf.graphics.insert(graphic_data);
                }

                for graphic_data in queues.remove_queue {
                    sugarloaf.graphics.remove(&graphic_data);
                }
            }
        }

        self.update_search_rich_text(sugarloaf.content());

        let window_size = sugarloaf.window_size();
        let scale_factor = sugarloaf.scale_factor();
        let mut objects = Vec::with_capacity(15);
        self.navigation.build_objects(
            sugarloaf,
            (window_size.width, window_size.height, scale_factor),
            &self.named_colors,
            context_manager,
            self.search.active_search.is_some(),
            &mut objects,
        );

        if has_search {
            if let Some(rich_text_id) = self.search.rich_text_id {
                search::draw_search_bar(
                    &mut objects,
                    rich_text_id,
                    &self.named_colors,
                    (window_size.width, window_size.height, scale_factor),
                );
            }

            self.search.active_search = None;
            self.search.rich_text_id = None;
        }

        context_manager.extend_with_grid_objects(&mut objects);
        sugarloaf.set_objects(objects);

        sugarloaf.render();

        // let duration = start.elapsed();
        // println!("Time elapsed in -renderer.update() is: {:?}", duration);
    }
}
