pub mod assistant;
pub mod command_palette;
pub mod custom_cursor;
pub mod island;
pub mod scrollbar;
pub mod search;
pub mod trail_cursor;
pub mod utils;

use crate::context::renderable::TerminalSnapshot;
use rio_backend::crosswords::LineDamage;
use rio_backend::event::TerminalDamage;
use taffy::NodeId;

use crate::ansi::CursorShape;
use crate::context::renderable::{Cursor, PendingUpdate, RenderableContent};
use crate::context::ContextManager;
use crate::crosswords::grid::row::Row;
use crate::crosswords::pos::{Column, Line, Pos};
use crate::crosswords::square::{Square, Wide};
use crate::crosswords::style::{Style as CellStyle, StyleFlags, StyleSet};
use rio_backend::config::colors::term::TermColors;
use rio_backend::config::colors::{
    term::{List, DIM_FACTOR},
    AnsiColor, ColorArray, Colors, NamedColor,
};
use rio_backend::config::navigation::Navigation;
use rio_backend::config::Config;
use rio_backend::event::EventProxy;
use rio_backend::sugarloaf::font_introspector::Attributes;
use rio_backend::sugarloaf::{
    drawable_character, is_private_user_area, CursorKind, Graphic, SpanStyle,
    SpanStyleDecoration, Stretch, Style, SugarCursor, Sugarloaf, UnderlineInfo,
    UnderlineShape, Weight,
};
use std::collections::BTreeSet;
use std::ops::RangeInclusive;

pub struct Renderer {
    is_vi_mode_enabled: bool,
    is_game_mode_enabled: bool,
    draw_bold_text_with_light_colors: bool,
    use_drawable_chars: bool,
    pub named_colors: Colors,
    pub colors: List,
    pub navigation: Navigation,
    pub margin: rio_backend::config::layout::Margin,
    pub island: Option<island::Island>,
    pub command_palette: command_palette::CommandPalette,
    unfocused_split_opacity: f32,
    unfocused_split_fill: Option<ColorArray>,
    last_active: Option<NodeId>,
    pub config_has_blinking_enabled: bool,
    pub config_blinking_interval: u64,
    ignore_selection_fg_color: bool,
    pub search: search::SearchOverlay,
    pub assistant: assistant::AssistantOverlay,
    pub scrollbar: scrollbar::Scrollbar,
    #[allow(unused)]
    pub option_as_alt: String,
    #[allow(unused)]
    pub macos_use_unified_titlebar: bool,
    // Dynamic background keep track of the original bg color and
    // the same r,g,b with the mutated alpha channel.
    pub dynamic_background: ([f32; 4], wgpu::Color, bool),
    pub custom_mouse_cursor: bool,
    pub trail_cursor_enabled: bool,
    pub trail_cursor: trail_cursor::TrailCursor,
}

/// Check if two styles are compatible for shaping (can be in the same text run)
/// Background color is intentionally excluded because it doesn't affect text shaping.
/// This allows runs with varying background colors to share cache entries,
/// dramatically improving cache hit rates for highlighted/selected text.
fn styles_are_compatible_for_shaping(a: &SpanStyle, b: &SpanStyle) -> bool {
    // PUA glyphs (and any glyph with a per-codepoint Nerd Font
    // constraint) must each be in their own run so the compositor can
    // individually constrain / scale them.
    if a.pua_constraint.is_some()
        || b.pua_constraint.is_some()
        || a.nerd_font_constraint.is_some()
        || b.nerd_font_constraint.is_some()
    {
        return false;
    }
    a.font_id == b.font_id
        && a.color == b.color
        && a.font_attrs == b.font_attrs
        && a.decoration == b.decoration
        && a.decoration_color == b.decoration_color
        && a.cursor == b.cursor
        && a.media == b.media
        && a.drawable_char == b.drawable_char
        && a.font_vars == b.font_vars
        && a.width == b.width
    // note: background_color is intentionally excluded
}

#[inline]
fn is_powerline(c: char) -> bool {
    ('\u{E0B0}'..='\u{E0D7}').contains(&c)
}

/// Compute the constraint width for a PUA (Nerd Font) glyph based on adjacent cells.
/// Returns 1.0 (fit in 1 cell) or 2.0 (expand to 2 cells).
fn pua_constraint_width(row: &Row<Square>, col: usize, cols: usize) -> f32 {
    // At end of line -> constrain to 1 cell
    if col + 1 >= cols {
        return 1.0;
    }

    // If previous cell is also a PUA glyph (but not a graphics element
    // like powerline), constrain to 1 so consecutive icons align properly.
    if col > 0 {
        let prev = row.inner[col - 1].c();
        if is_private_user_area(&prev) && !is_powerline(prev) {
            return 1.0;
        }
    }

    // If next cell is empty, space, or wide char spacer, expand to 2 cells.
    let next = &row.inner[col + 1];
    if next.c() == '\0' || next.c() == ' ' || matches!(next.wide(), Wide::Spacer) {
        return 2.0;
    }

    // Next cell is occupied -> constrain to 1 cell
    1.0
}

impl Renderer {
    pub fn new(config: &Config) -> Renderer {
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

        let island = if config.navigation.is_enabled() {
            Some(island::Island::new(
                named_colors.tabs,
                named_colors.tabs_active,
                named_colors.tab_border,
                config.navigation.hide_if_single,
            ))
        } else {
            None
        };

        Renderer {
            unfocused_split_opacity: config.navigation.unfocused_split_opacity,
            unfocused_split_fill: config.navigation.unfocused_split_fill,
            last_active: None,
            use_drawable_chars: config.fonts.use_drawable_chars,
            draw_bold_text_with_light_colors: config.draw_bold_text_with_light_colors,
            macos_use_unified_titlebar: config.window.macos_use_unified_titlebar,
            config_blinking_interval: config.cursor.blinking_interval.clamp(350, 1200),
            option_as_alt: config.option_as_alt.to_lowercase(),
            is_vi_mode_enabled: false,
            config_has_blinking_enabled: config.cursor.blinking,
            ignore_selection_fg_color: config.ignore_selection_fg_color,
            colors,
            navigation: config.navigation.clone(),
            margin: config.margin,
            island,
            command_palette: {
                let mut palette = command_palette::CommandPalette::new();
                palette.has_adaptive_theme = config.adaptive_colors.is_some();
                palette
            },
            named_colors,
            dynamic_background,
            search: search::SearchOverlay::default(),
            assistant: assistant::AssistantOverlay::default(),
            scrollbar: scrollbar::Scrollbar::new(config.enable_scroll_bar),
            is_game_mode_enabled: config.renderer.strategy.is_game(),
            custom_mouse_cursor: config.effects.custom_mouse_cursor,
            trail_cursor_enabled: config.effects.trail_cursor,
            trail_cursor: trail_cursor::TrailCursor::new(),
        }
    }

    #[inline]
    pub fn set_active_search(&mut self, active_search: Option<String>) {
        self.search.set_active_search(active_search);
    }

    #[inline]
    fn create_style(
        &mut self,
        square: &Square,
        cell_style: &CellStyle,
        term_colors: &TermColors,
    ) -> (SpanStyle, char) {
        let flags = cell_style.flags;

        let mut foreground_color = self.compute_color(&cell_style.fg, flags, term_colors);
        let mut background_color = self.compute_bg_color(cell_style, term_colors);

        let content = if square.c() == '\t' || flags.contains(StyleFlags::HIDDEN) {
            ' '
        } else {
            square.c()
        };

        let font_attrs = match (
            flags.contains(StyleFlags::ITALIC),
            flags.contains(StyleFlags::ITALIC) && flags.contains(StyleFlags::BOLD),
            flags.contains(StyleFlags::BOLD),
        ) {
            (true, _, _) => (Stretch::NORMAL, Weight::NORMAL, Style::Italic),
            (_, true, _) => (Stretch::NORMAL, Weight::BOLD, Style::Italic),
            (_, _, true) => (Stretch::NORMAL, Weight::BOLD, Style::Normal),
            _ => (Stretch::NORMAL, Weight::NORMAL, Style::Normal),
        };

        if flags.contains(StyleFlags::INVERSE) {
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

        let (decoration, decoration_color) =
            self.compute_decoration(cell_style, term_colors);

        (
            SpanStyle {
                color: foreground_color,
                background_color,
                font_attrs: font_attrs.into(),
                decoration,
                decoration_color,
                ..SpanStyle::default()
            },
            content,
        )
    }

    #[inline]
    fn compute_decoration(
        &self,
        cell_style: &CellStyle,
        term_colors: &TermColors,
    ) -> (Option<SpanStyleDecoration>, Option<[f32; 4]>) {
        let mut decoration = None;
        let mut decoration_color = None;

        if cell_style.flags.contains(StyleFlags::UNDERLINE) {
            decoration = Some(SpanStyleDecoration::Underline(UnderlineInfo {
                is_doubled: false,
                shape: UnderlineShape::Regular,
            }));
        } else if cell_style.flags.contains(StyleFlags::STRIKEOUT) {
            decoration = Some(SpanStyleDecoration::Strikethrough);
        } else if cell_style.flags.contains(StyleFlags::DOUBLE_UNDERLINE) {
            decoration = Some(SpanStyleDecoration::Underline(UnderlineInfo {
                is_doubled: true,
                shape: UnderlineShape::Regular,
            }));
        } else if cell_style.flags.contains(StyleFlags::DOTTED_UNDERLINE) {
            decoration = Some(SpanStyleDecoration::Underline(UnderlineInfo {
                is_doubled: false,
                shape: UnderlineShape::Dotted,
            }));
        } else if cell_style.flags.contains(StyleFlags::DASHED_UNDERLINE) {
            decoration = Some(SpanStyleDecoration::Underline(UnderlineInfo {
                is_doubled: false,
                shape: UnderlineShape::Dashed,
            }));
        } else if cell_style.flags.contains(StyleFlags::UNDERCURL) {
            decoration = Some(SpanStyleDecoration::Underline(UnderlineInfo {
                is_doubled: false,
                shape: UnderlineShape::Curly,
            }));
        }

        if decoration.is_some() {
            if let Some(color) = cell_style.underline_color {
                decoration_color =
                    Some(self.compute_color(&color, cell_style.flags, term_colors));
            }
        };

        (decoration, decoration_color)
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    /// Check if a position is within any hint match
    fn is_position_in_hint_matches(
        matches: &[rio_backend::crosswords::search::Match],
        pos: Pos,
    ) -> bool {
        matches.iter().any(|m| m.contains(&pos))
    }

    #[allow(clippy::too_many_arguments)]
    fn create_line(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        row: &Row<Square>,
        style_set: &StyleSet,
        extras_table: &rio_backend::crosswords::grid::ExtrasTable,
        has_cursor: bool,
        line_opt: Option<usize>,
        line: Line,
        renderable_content: &RenderableContent,
        hint_matches: Option<&[rio_backend::crosswords::search::Match]>,
        focused_match: &Option<RangeInclusive<Pos>>,
        term_colors: &TermColors,
        is_active: bool,
    ) {
        // let start = std::time::Instant::now();
        let cursor = &renderable_content.cursor;
        let selection_range = renderable_content.selection_range;
        let columns: usize = row.len();
        let mut content = String::with_capacity(columns);
        let mut last_style = SpanStyle::default();

        // Collect all characters that need font lookups to batch them
        let mut font_lookups = Vec::new();
        let mut styles_and_chars = Vec::with_capacity(columns);

        // Cache the looked-up cell style across consecutive cells with the
        // same `style_id` — this is the common case (most rows have long
        // runs of identically-styled cells), and avoids the StyleSet lookup
        // per cell. Bg-only cells (Ghostty's `bg_color_palette`/`bg_color_rgb`
        // trick) skip the lookup entirely; we synthesize a Style with the
        // inline bg and reuse the same cache slot.
        let mut cached_style_id: Option<crate::crosswords::style::StyleId> = None;
        let mut cached_style: CellStyle = CellStyle::default();
        // Bg-only cells don't have a style id; track them by a tag instead
        // so the cache invalidates correctly when transitioning between
        // bg-only and codepoint cells.
        let mut cached_bg_only: u64 = u64::MAX;

        // First pass: collect all styles and identify font cache misses
        for column in 0..columns {
            let square = &row.inner[column];

            if matches!(square.wide(), Wide::Spacer) {
                continue;
            }

            // Resolve the cell style. For bg-only cells we synthesize a
            // Style with the inline bg and skip the StyleSet entirely.
            // For codepoint cells we cache the looked-up style across runs
            // of identical style_ids.
            use crate::crosswords::square::ContentTag;
            match square.content_tag() {
                ContentTag::BgPalette => {
                    let idx = square.bg_palette_index();
                    let key = 0x0100_0000 | idx as u64;
                    if cached_bg_only != key {
                        cached_style = CellStyle {
                            bg: AnsiColor::Indexed(idx),
                            ..CellStyle::default()
                        };
                        cached_bg_only = key;
                        cached_style_id = None;
                    }
                }
                ContentTag::BgRgb => {
                    let (r, g, b) = square.bg_rgb();
                    let key =
                        0x0200_0000 | ((r as u64) << 16) | ((g as u64) << 8) | b as u64;
                    if cached_bg_only != key {
                        cached_style =
                            CellStyle {
                                bg: AnsiColor::Spec(
                                    rio_backend::config::colors::ColorRgb { r, g, b },
                                ),
                                ..CellStyle::default()
                            };
                        cached_bg_only = key;
                        cached_style_id = None;
                    }
                }
                ContentTag::Codepoint => {
                    let sid = square.style_id();
                    if Some(sid) != cached_style_id {
                        cached_style = style_set.get(sid);
                        cached_style_id = Some(sid);
                        cached_bg_only = u64::MAX;
                    }
                }
            }
            let cell_style = &cached_style;

            let (mut style, mut square_content) =
                if has_cursor && column == cursor.state.pos.col {
                    self.create_cursor_style(
                        square,
                        cell_style,
                        cursor,
                        is_active,
                        term_colors,
                    )
                } else {
                    self.create_style(square, cell_style, term_colors)
                };

            // Check selection before any early returns so '\0' cells get highlights
            if let Some(ref range) = selection_range {
                let pos = Pos::new(line, Column(column));
                if range.contains(pos) {
                    style.color = if self.ignore_selection_fg_color {
                        self.compute_color(&cell_style.fg, cell_style.flags, term_colors)
                    } else {
                        self.named_colors.selection_foreground
                    };
                    style.background_color = Some(self.named_colors.selection_background);
                }
            }

            if square_content == '\0' {
                if square.has_graphics() {
                    if let Some(eid) = square.extras_id() {
                        if let Some(gc) = extras_table
                            .get(eid)
                            .and_then(|e| e.graphic.as_ref())
                            .and_then(|g| g.first())
                        {
                            style.media = Some(Graphic {
                                id: gc.texture.id,
                                offset_x: gc.offset_x,
                                offset_y: gc.offset_y,
                            });
                        }
                    }
                }
                styles_and_chars.push((style, square_content, column));
                continue;
            }

            // Apply underline for hyperlinks (OSC 8) or highlighted hints (hover).
            let should_underline = {
                if let Some(highlighted_hint) = &renderable_content.highlighted_hint {
                    let current_pos = Pos::new(line, Column(column));
                    highlighted_hint.start <= current_pos
                        && current_pos <= highlighted_hint.end
                } else {
                    false
                }
            };

            if should_underline {
                style.decoration = Some(SpanStyleDecoration::Underline(UnderlineInfo {
                    is_doubled: false,
                    shape: UnderlineShape::Regular,
                }));
            }

            // Check hints (only for non-empty cells, selection already handled above)
            if selection_range.is_none() {
                if let Some(matches) = hint_matches {
                    let pos = Pos::new(line, Column(column));
                    if Self::is_position_in_hint_matches(matches, pos) {
                        let is_focused =
                            focused_match.as_ref().is_some_and(|fm| fm.contains(&pos));
                        if is_focused {
                            style.color =
                                self.named_colors.search_focused_match_foreground;
                            style.background_color =
                                Some(self.named_colors.search_focused_match_background);
                        } else {
                            style.color = self.named_colors.search_match_foreground;
                            style.background_color =
                                Some(self.named_colors.search_match_background);
                        }
                    }
                }
            }

            // Check for hint labels at this position
            if let Some(hint_label) = self.find_hint_label_at_position(
                renderable_content,
                Pos::new(line, Column(column)),
            ) {
                // Override character with hint label character if available
                if let Some(label_char) = hint_label.label.first() {
                    square_content = *label_char;
                }

                // Apply hint label styling
                if hint_label.is_first {
                    // Use configurable hint colors
                    style.color = self.named_colors.hint_foreground;
                    style.background_color = Some(self.named_colors.hint_background);
                } else {
                    // End colors: use same foreground, slightly dimmed background
                    style.color = self.named_colors.hint_foreground;
                    let mut dimmed_bg = self.named_colors.hint_background;
                    // Dim the background slightly for continuation labels
                    dimmed_bg[0] *= 0.8;
                    dimmed_bg[1] *= 0.8;
                    dimmed_bg[2] *= 0.8;
                    style.background_color = Some(dimmed_bg);
                }

                // Make hint labels bold for better visibility
                use rio_backend::sugarloaf::font_introspector::{Attributes, Weight};
                let current_attrs = style.font_attrs;
                style.font_attrs = Attributes::new(
                    current_attrs.stretch(),
                    Weight::BOLD,
                    current_attrs.style(),
                );
            }

            // Kitty Unicode placeholder (U+10EEEE): treat as a space.
            // The overlay image renders on top — no per-cell virtual
            // placement lookup needed (matches Ghostty's approach).
            if square_content == '\u{10EEEE}' {
                square_content = ' ';
            }

            if square.has_graphics() {
                if let Some(eid) = square.extras_id() {
                    if let Some(gc) = extras_table
                        .get(eid)
                        .and_then(|e| e.graphic.as_ref())
                        .and_then(|g| g.first())
                    {
                        style.media = Some(Graphic {
                            id: gc.texture.id,
                            offset_x: gc.offset_x,
                            offset_y: gc.offset_y,
                        });
                    }
                }
            }

            // Handle drawable characters
            if self.use_drawable_chars {
                if let Some(character) = drawable_character(square_content) {
                    style.drawable_char = Some(character);
                }
            }

            let has_drawable_char = style.drawable_char.is_some();
            if !has_drawable_char {
                if let Some(cached) =
                    sugarloaf.try_glyph_cached(square_content, style.font_attrs)
                {
                    style.font_id = cached.font_id;
                    style.width = cached.width;
                    if cached.is_pua {
                        style.pua_constraint =
                            Some(pua_constraint_width(row, column, columns));
                        style.nerd_font_constraint = rio_backend::sugarloaf::font::nerd_font_attributes::get_constraint(
                            square_content as u32,
                        );
                    }
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

        // Batch font lookups with a single lock acquisition (the
        // batch lives behind sugarloaf so the cache + FontLibrary
        // stay co-located).
        if !font_lookups.is_empty() {
            let queries: Vec<(char, Attributes)> = font_lookups
                .iter()
                .map(|&(_, ch, attrs)| (ch, attrs))
                .collect();
            let resolved = sugarloaf.resolve_glyphs_batch(&queries);
            for ((style_index, ch, _), glyph) in font_lookups.iter().zip(resolved.iter())
            {
                let column = styles_and_chars[*style_index].2;
                let style = &mut styles_and_chars[*style_index].0;
                style.font_id = glyph.font_id;
                style.width = glyph.width;
                if glyph.is_pua {
                    style.pua_constraint =
                        Some(pua_constraint_width(row, column, columns));
                    style.nerd_font_constraint = rio_backend::sugarloaf::font::nerd_font_attributes::get_constraint(
                        *ch as u32,
                    );
                }
            }
        }

        // Second pass: render the line using the resolved styles.
        // Grab the content builder now — pass 1 only touched the
        // sugarloaf font cache, so we can take a fresh `&mut Content`
        // here without conflict.
        let builder = sugarloaf.content();
        // Track consecutive blank cells ('\0' and ' ') to batch into a single rect
        let mut pending_blank_width: f32 = 0.0;
        let mut pending_blank_style = SpanStyle::default();

        for (style, square_content, column) in styles_and_chars {
            // Cells carrying a graphic (sixel/iTerm2/Kitty) must go
            // through the normal text path so the renderer paints
            // their image. They are NOT blank — even when their
            // character slot is `'\0'` or `' '`.
            let has_media = style.media.is_some();
            let is_blank =
                (square_content == '\0' || square_content == ' ') && !has_media;

            // Flush pending blank run if this cell breaks the batch
            if pending_blank_width > 0.0
                && (!is_blank
                    || style.background_color != pending_blank_style.background_color
                    || style.cursor != pending_blank_style.cursor
                    || style.decoration != pending_blank_style.decoration
                    || style.decoration_color != pending_blank_style.decoration_color)
            {
                let mut rect_style = pending_blank_style;
                rect_style.width = pending_blank_width;
                if let Some(line) = line_opt {
                    builder.add_span_as_rect_on_line(line, rect_style);
                } else {
                    builder.add_span_as_rect(rect_style);
                }
                pending_blank_width = 0.0;
            }

            // Handle drawable characters
            if style.drawable_char.is_some() {
                if !content.is_empty() {
                    if let Some(line) = line_opt {
                        builder.add_span_on_line(line, &content, last_style);
                    } else {
                        builder.add_span(&content, last_style);
                    }
                    content.clear();
                }

                last_style = style;
                content.push(' '); // Ignore font shaping
            } else if is_blank {
                // Accumulate into pending blank run
                if !content.is_empty() {
                    if let Some(line) = line_opt {
                        builder.add_span_on_line(line, &content, last_style);
                    } else {
                        builder.add_span(&content, last_style);
                    }
                    content.clear();
                }
                if pending_blank_width == 0.0 {
                    pending_blank_style = style;
                }
                pending_blank_width += 1.0;
                last_style = style;
            } else {
                // Break runs when styles differ in ways that affect shaping
                // or when background color changes (for search highlights, etc.)
                if !styles_are_compatible_for_shaping(&last_style, &style)
                    || last_style.background_color != style.background_color
                {
                    if !content.is_empty() {
                        if let Some(line) = line_opt {
                            builder.add_span_on_line(line, &content, last_style);
                        } else {
                            builder.add_span(&content, last_style);
                        }
                        content.clear();
                    }

                    last_style = style;
                }

                // A '\0' cell with a graphic still needs a glyph to
                // anchor the image draw. Substitute a space so the
                // shaper produces a real run.
                let push_char = if has_media && square_content == '\0' {
                    ' '
                } else {
                    square_content
                };
                content.push(push_char);
            }

            // Render last column and break row
            if column == (columns - 1) {
                if !content.is_empty() {
                    if let Some(line) = line_opt {
                        builder.add_span_on_line(line, &content, last_style);
                    } else {
                        builder.add_span(&content, last_style);
                    }
                }

                // Flush any remaining pending blank cells
                if pending_blank_width > 0.0 {
                    let mut rect_style = pending_blank_style;
                    rect_style.width = pending_blank_width;
                    if let Some(line) = line_opt {
                        builder.add_span_as_rect_on_line(line, rect_style);
                    } else {
                        builder.add_span_as_rect(rect_style);
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
        flags: StyleFlags,
        term_colors: &TermColors,
    ) -> ColorArray {
        let dim = flags.contains(StyleFlags::DIM);
        let bold = flags.contains(StyleFlags::BOLD);
        match color {
            AnsiColor::Named(ansi) => {
                match (self.draw_bold_text_with_light_colors, dim, bold) {
                    // If no bright foreground is set, treat it like the BOLD flag doesn't exist.
                    (_, true, true)
                        if ansi == &NamedColor::Foreground
                            && self.named_colors.light_foreground.is_none() =>
                    {
                        self.color(NamedColor::DimForeground as usize, term_colors)
                    }
                    // Draw bold text in bright colors *and* contains bold flag.
                    (true, false, true) => {
                        self.color(ansi.to_light() as usize, term_colors)
                    }
                    // Cell is marked as dim and not bold.
                    (_, true, false) | (false, true, true) => {
                        self.color(ansi.to_dim() as usize, term_colors)
                    }
                    // None of the above, keep original color..
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

    #[inline]
    fn compute_bg_color(
        &self,
        cell_style: &CellStyle,
        term_colors: &TermColors,
    ) -> ColorArray {
        let dim = cell_style.flags.contains(StyleFlags::DIM);
        let bold = cell_style.flags.contains(StyleFlags::BOLD);
        match cell_style.bg {
            AnsiColor::Named(ansi) => self.color(ansi as usize, term_colors),
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
    fn create_cursor_style(
        &self,
        square: &Square,
        cell_style: &CellStyle,
        cursor: &Cursor,
        is_active: bool,
        term_colors: &TermColors,
    ) -> (SpanStyle, char) {
        let flags = cell_style.flags;
        let font_attrs = match (
            flags.contains(StyleFlags::ITALIC),
            flags.contains(StyleFlags::ITALIC) && flags.contains(StyleFlags::BOLD),
            flags.contains(StyleFlags::BOLD),
        ) {
            (true, _, _) => (Stretch::NORMAL, Weight::NORMAL, Style::Italic),
            (_, true, _) => (Stretch::NORMAL, Weight::BOLD, Style::Italic),
            (_, _, true) => (Stretch::NORMAL, Weight::BOLD, Style::Normal),
            _ => (Stretch::NORMAL, Weight::NORMAL, Style::Normal),
        };

        let mut color = self.compute_color(&cell_style.fg, flags, term_colors);
        let mut background_color = self.compute_bg_color(cell_style, term_colors);
        // If IME is enabled we get the current content to cursor
        let content = if cursor.is_ime_enabled {
            cursor.content
        } else if square.c() == '\0' {
            ' '
        } else {
            square.c()
        };

        if flags.contains(StyleFlags::INVERSE) {
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

        let mut style = SpanStyle {
            color,
            background_color,
            font_attrs: font_attrs.into(),
            ..SpanStyle::default()
        };

        let cursor_color = if !self.is_vi_mode_enabled {
            term_colors[NamedColor::Cursor].unwrap_or(self.named_colors.cursor)
        } else {
            self.named_colors.vi_cursor
        };

        let (decoration, decoration_color) =
            self.compute_decoration(cell_style, term_colors);
        style.decoration = decoration;
        style.decoration_color = decoration_color;

        match cursor.state.content {
            CursorShape::Underline => {
                style.decoration = Some(SpanStyleDecoration::Underline(UnderlineInfo {
                    is_doubled: false,
                    shape: UnderlineShape::Regular,
                }));
                style.decoration_color = Some(cursor_color);
            }
            CursorShape::Block => {
                style.cursor = Some(SugarCursor {
                    kind: CursorKind::Block,
                    color: cursor_color,
                    order: 0,
                });
            }
            CursorShape::Beam => {
                style.cursor = Some(SugarCursor {
                    kind: CursorKind::Caret,
                    color: cursor_color,
                    order: 0,
                });
            }
            CursorShape::Hidden => {}
        }

        if !is_active {
            style.decoration = None;
            style.cursor = Some(SugarCursor {
                kind: CursorKind::HollowBlock,
                color: cursor_color,
                order: 0,
            });
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
    pub fn run(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        context_manager: &mut ContextManager<EventProxy>,
        focused_match: &Option<RangeInclusive<Pos>>,
    ) -> Option<crate::context::renderable::WindowUpdate> {
        let grid = context_manager.current_grid_mut();
        let active_key = grid.current;
        let grid_scaled_margin = grid.get_scaled_margin();
        let mut has_active_changed = false;
        if self.last_active != Some(active_key) {
            has_active_changed = true;
            self.last_active = Some(active_key);
        }

        // Update per-panel scroll state for scrollbar rendering (all panels, not just dirty ones)
        if self.scrollbar.is_enabled() {
            self.scrollbar.clear_panel_states();
            for grid_context in grid.contexts_mut().values() {
                let panel_rect = grid_context.layout_rect;
                let ctx = grid_context.context();
                let terminal = ctx.terminal.lock();
                self.scrollbar
                    .push_panel_state(scrollbar::PanelScrollState {
                        rich_text_id: ctx.rich_text_id,
                        panel_rect,
                        display_offset: terminal.display_offset(),
                        history_size: terminal.history_size(),
                        screen_lines: terminal.screen_lines(),
                    });
            }
        }

        for (key, grid_context) in grid.contexts_mut().iter_mut() {
            let is_active = &active_key == key;
            let panel_rect = grid_context.layout_rect;
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

            let force_full_damage = has_active_changed || self.is_game_mode_enabled;

            // Check if we need to render
            if !context.renderable_content.pending_update.is_dirty() && !force_full_damage
            {
                // No updates pending, skip rendering
                continue;
            }

            // UI-side damage (scroll, selection, resize, etc.)
            let ui_terminal_damage = context
                .renderable_content
                .pending_update
                .take_terminal_damage();
            let _ui_damage = context.renderable_content.pending_update.take_ui_damage();
            context.renderable_content.pending_update.reset();

            // Compute snapshot at render time — extract PTY-side damage from the
            // terminal, merge with any UI-side damage, and clear the in-flight
            // flag so the PTY thread can send a new notification.
            let terminal_snapshot = {
                let mut terminal = context.terminal.lock();

                // Clear in-flight flag so PTY thread can notify again
                terminal.damage_event_in_flight = false;

                let pty_damage = terminal.peek_damage_event();

                let damage = if force_full_damage {
                    TerminalDamage::Full
                } else {
                    match (ui_terminal_damage, pty_damage) {
                        (Some(ui), Some(pty)) => {
                            PendingUpdate::merge_terminal_damages(ui, pty)
                        }
                        (Some(d), None) | (None, Some(d)) => d,
                        (None, None) => {
                            drop(terminal);
                            continue;
                        }
                    }
                };

                terminal.reset_damage();

                let snapshot = TerminalSnapshot {
                    colors: terminal.colors,
                    display_offset: terminal.display_offset(),
                    blinking_cursor: terminal.blinking_cursor,
                    visible_rows: terminal.visible_rows(),
                    style_set: terminal.grid.style_set.clone(),
                    extras_table: terminal.grid.extras_table.clone(),
                    cursor: terminal.cursor(),
                    damage,
                    columns: terminal.columns(),
                    screen_lines: terminal.screen_lines(),
                    history_size: terminal.history_size(),
                    kitty_virtual_placements: terminal
                        .graphics
                        .kitty_virtual_placements
                        .clone(),
                    kitty_images: terminal.graphics.kitty_images.clone(),
                    kitty_placements: {
                        let mut placements: Vec<_> = terminal
                            .graphics
                            .kitty_placements
                            .values()
                            .filter(|p| {
                                terminal.graphics.kitty_images.contains_key(&p.image_id)
                            })
                            .cloned()
                            .collect();
                        placements.sort_by_key(|p| p.z_index);
                        placements
                    },
                    kitty_graphics_dirty: terminal.graphics.kitty_graphics_dirty,
                };
                terminal.graphics.kitty_graphics_dirty = false;
                drop(terminal);

                snapshot
            };

            // Recalculate image overlay positions every frame when placements
            // exist. Positions depend on display_offset and history_size which
            // change on scroll and text output (like Ghostty's approach).
            if !terminal_snapshot.kitty_placements.is_empty() {
                let line_height = sugarloaf.style().line_height;
                let content = sugarloaf.content();
                content.sel(context.rich_text_id);
                content.clear_image_overlays();
                let layout = context.dimension;
                let cell_width = layout.dimension.width;
                let cell_height = layout.dimension.height * line_height;
                let origin_x = panel_rect[0] + grid_scaled_margin.left;
                let origin_y = panel_rect[1] + grid_scaled_margin.top;
                let history_size = terminal_snapshot.history_size as i64;
                let display_offset = terminal_snapshot.display_offset as i64;
                let screen_lines = terminal_snapshot.screen_lines as i64;

                for p in &terminal_snapshot.kitty_placements {
                    let screen_row = p.dest_row - (history_size - display_offset);
                    let image_bottom_row = screen_row + p.rows as i64;
                    // Cull only if fully off-screen (like Ghostty)
                    if image_bottom_row <= 0 || screen_row >= screen_lines {
                        continue;
                    }
                    content.push_image_overlay(rio_backend::sugarloaf::GraphicOverlay {
                        image_id: p.image_id,
                        x: origin_x + p.dest_col as f32 * cell_width,
                        y: origin_y + screen_row as f32 * cell_height,
                        width: p.pixel_width as f32,
                        height: p.pixel_height as f32,
                        z_index: p.z_index,
                    });
                }
            } else if terminal_snapshot.kitty_graphics_dirty {
                // Placements were removed — clear overlays
                let content = sugarloaf.content();
                content.sel(context.rich_text_id);
                content.clear_image_overlays();
            }

            // Get hint matches from renderable content
            let hint_matches = context.renderable_content.hint_matches.as_deref();

            // Update cursor state from snapshot
            context.renderable_content.cursor.state = terminal_snapshot.cursor.clone();

            let mut specific_lines: Option<BTreeSet<LineDamage>> = None;

            // Check for partial damage to optimize rendering
            if !force_full_damage {
                match &terminal_snapshot.damage {
                    TerminalDamage::Noop => {
                        // Should not reach here — Noop is handled before snapshot
                        continue;
                    }
                    TerminalDamage::Full => {
                        // Full damage, render everything
                    }
                    TerminalDamage::Partial(lines) => {
                        if !lines.is_empty() {
                            specific_lines = Some(lines.clone());
                        }
                    }
                    TerminalDamage::CursorOnly => {
                        specific_lines = Some(
                            [LineDamage {
                                line: *context.renderable_content.cursor.state.pos.row
                                    as usize,
                                damaged: true,
                            }]
                            .into_iter()
                            .collect(),
                        );
                    }
                }
            }

            let rich_text_id = context.rich_text_id;

            let mut is_cursor_visible =
                context.renderable_content.cursor.state.is_visible();
            context.renderable_content.has_blinking_enabled =
                terminal_snapshot.blinking_cursor;

            if terminal_snapshot.blinking_cursor {
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
                        let should_toggle = if let Some(last_blink) =
                            context.renderable_content.last_blink_toggle
                        {
                            now.duration_since(last_blink).as_millis()
                                >= self.config_blinking_interval as u128
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

            match specific_lines {
                None => {
                    sugarloaf.content().sel(rich_text_id).clear();
                    for (i, row) in terminal_snapshot.visible_rows.iter().enumerate() {
                        let has_cursor = is_cursor_visible
                            && context.renderable_content.cursor.state.pos.row == i;
                        self.create_line(
                            sugarloaf,
                            row,
                            &terminal_snapshot.style_set,
                            &terminal_snapshot.extras_table,
                            has_cursor,
                            None,
                            Line((i as i32) - terminal_snapshot.display_offset as i32),
                            &context.renderable_content,
                            hint_matches,
                            focused_match,
                            &terminal_snapshot.colors,
                            is_active,
                        );
                    }
                    sugarloaf.content().build();
                    // let _duration = start.elapsed();
                }
                Some(lines) => {
                    sugarloaf.content().sel(rich_text_id);
                    for line in lines {
                        let line = line.line;
                        let has_cursor = is_cursor_visible
                            && context.renderable_content.cursor.state.pos.row == line;
                        sugarloaf.content().clear_line(line);
                        if let Some(visible_row) =
                            terminal_snapshot.visible_rows.get(line)
                        {
                            self.create_line(
                                sugarloaf,
                                visible_row,
                                &terminal_snapshot.style_set,
                                &terminal_snapshot.extras_table,
                                has_cursor,
                                Some(line),
                                Line(
                                    (line as i32)
                                        - terminal_snapshot.display_offset as i32,
                                ),
                                &context.renderable_content,
                                hint_matches,
                                focused_match,
                                &terminal_snapshot.colors,
                                is_active,
                            );
                        }
                    }

                    // let _duration = start.elapsed();
                }
            }
        }

        let window_size = sugarloaf.window_size();
        let scale_factor = sugarloaf.scale_factor();

        // Dim overlay for unfocused splits. Drawn after the split content is
        // built so it composites on top. The tint comes from
        // `unfocused_split_fill` (falling back to the terminal background)
        // and its strength is `1.0 - unfocused_split_opacity`. Skipped
        // entirely when the feature is disabled.
        if self.unfocused_split_opacity < 1.0 {
            let tint = self
                .unfocused_split_fill
                .unwrap_or(self.dynamic_background.0);
            let dim_color = [
                tint[0],
                tint[1],
                tint[2],
                1.0 - self.unfocused_split_opacity,
            ];
            for (key, grid_context) in grid.contexts_mut().iter() {
                if &active_key == key {
                    continue;
                }
                let panel_rect = grid_context.layout_rect;
                let x = (panel_rect[0] + grid_scaled_margin.left) / scale_factor;
                let y = (panel_rect[1] + grid_scaled_margin.top) / scale_factor;
                let w = panel_rect[2] / scale_factor;
                let h = panel_rect[3] / scale_factor;
                sugarloaf.rect(None, x, y, w, h, dim_color, 0.0, 3);
            }
        }

        if let Some(island) = &mut self.island {
            island.render(
                sugarloaf,
                (window_size.width, window_size.height, scale_factor),
                context_manager,
            );
        }

        self.assistant.render(
            sugarloaf,
            (window_size.width, window_size.height, scale_factor),
        );

        self.search.render(
            sugarloaf,
            (window_size.width, window_size.height, scale_factor),
        );

        self.command_palette.render(
            sugarloaf,
            (window_size.width, window_size.height, scale_factor),
        );

        // Render scrollbars for each panel
        let grid_scaled_margin_sb = context_manager.get_current_grid_scaled_margin();
        let grid_margin_sb = (grid_scaled_margin_sb.left, grid_scaled_margin_sb.top);
        let panel_count = self.scrollbar.panel_states().len();
        for i in 0..panel_count {
            let state = self.scrollbar.panel_states()[i];
            self.scrollbar.render(
                sugarloaf,
                state.panel_rect,
                scale_factor,
                state.display_offset,
                state.history_size,
                state.screen_lines,
                state.rich_text_id,
                grid_margin_sb,
            );
        }

        // Render panel borders (on top of terminal content)
        let grid_scaled_margin = context_manager.get_current_grid_scaled_margin();
        for border_object in context_manager.get_panel_borders() {
            match border_object {
                rio_backend::sugarloaf::Object::Quad(quad) => {
                    // Convert from physical pixels to logical coordinates
                    let x = (quad.x + grid_scaled_margin.left) / scale_factor;
                    let y = (quad.y + grid_scaled_margin.top) / scale_factor;
                    let width = quad.width / scale_factor;
                    let height = quad.height / scale_factor;

                    let corner_radii = [
                        quad.corner_radii.top_left / scale_factor,
                        quad.corner_radii.top_right / scale_factor,
                        quad.corner_radii.bottom_right / scale_factor,
                        quad.corner_radii.bottom_left / scale_factor,
                    ];

                    // Render quad with rounded corners
                    sugarloaf.quad(
                        None,
                        x,
                        y,
                        width,
                        height,
                        quad.background_color,
                        corner_radii,
                        0.0,
                        1, // Higher order renders on top
                    );
                }
                rio_backend::sugarloaf::Object::Rect(rect) => {
                    // Simple rectangle (no rounded corners or borders)
                    let x = (rect.x + grid_scaled_margin.left) / scale_factor;
                    let y = (rect.y + grid_scaled_margin.top) / scale_factor;
                    let width = rect.width / scale_factor;
                    let height = rect.height / scale_factor;

                    sugarloaf.rect(None, x, y, width, height, rect.color, 0.0, 1);
                }
                _ => {}
            }
        }

        // Apply background color from current context if changed
        let current_context = context_manager.current_grid_mut().current_mut();
        let window_update = if let Some(bg_state) =
            current_context.renderable_content.background.take()
        {
            use crate::context::renderable::BackgroundState;
            match bg_state {
                BackgroundState::Set(color) => {
                    sugarloaf.set_background_color(Some(color));
                }
                BackgroundState::Reset => {
                    sugarloaf.set_background_color(None);
                }
            }
            Some(crate::context::renderable::WindowUpdate::Background(
                bg_state,
            ))
        } else {
            None
        };

        window_update
    }

    /// Check if the renderer needs continuous redraw (for animations)
    #[inline]
    pub fn needs_redraw(&mut self) -> bool {
        if self.trail_cursor.is_animating() {
            return true;
        }
        if self.scrollbar.needs_redraw() {
            return true;
        }
        if let Some(island) = &self.island {
            island.needs_redraw()
        } else {
            false
        }
    }

    /// Find hint label at the specified position
    fn find_hint_label_at_position<'a>(
        &self,
        renderable_content: &'a RenderableContent,
        pos: Pos,
    ) -> Option<&'a crate::context::renderable::HintLabel> {
        renderable_content
            .hint_labels
            .iter()
            .find(|label| label.position == pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rio_backend::crosswords::pos::{Column, Line, Pos};

    #[test]
    fn test_is_position_in_hint_matches() {
        let matches = vec![
            Pos::new(Line(0), Column(0))..=Pos::new(Line(0), Column(4)),
            Pos::new(Line(1), Column(5))..=Pos::new(Line(1), Column(9)),
            Pos::new(Line(5), Column(10))..=Pos::new(Line(5), Column(15)),
        ];

        // Test positions inside matches
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(0), Column(0))
        ));
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(0), Column(2))
        ));
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(0), Column(4))
        ));
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(1), Column(5))
        ));
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(1), Column(7))
        ));
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(1), Column(9))
        ));
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(5), Column(10))
        ));
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(5), Column(15))
        ));

        // Test positions outside matches
        assert!(!Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(0), Column(5))
        ));
        assert!(!Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(1), Column(4))
        ));
        assert!(!Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(1), Column(10))
        ));
        assert!(!Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(2), Column(0))
        ));
        assert!(!Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(5), Column(9))
        ));
        assert!(!Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(5), Column(16))
        ));
    }

    #[test]
    fn test_empty_hint_matches() {
        let matches: Vec<rio_backend::crosswords::search::Match> = vec![];

        // Any position should return false for empty matches
        assert!(!Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(0), Column(0))
        ));
        assert!(!Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(10), Column(20))
        ));
    }

    #[test]
    fn test_single_character_match() {
        let matches = vec![Pos::new(Line(3), Column(7))..=Pos::new(Line(3), Column(7))];

        // Test the exact position
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(3), Column(7))
        ));

        // Test adjacent positions
        assert!(!Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(3), Column(6))
        ));
        assert!(!Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(3), Column(8))
        ));
    }

    #[test]
    fn test_overlapping_matches() {
        // In practice, matches shouldn't overlap, but let's test the behavior
        let matches = vec![
            Pos::new(Line(2), Column(5))..=Pos::new(Line(2), Column(10)),
            Pos::new(Line(2), Column(8))..=Pos::new(Line(2), Column(12)),
        ];

        // Test positions in the overlap
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(2), Column(8))
        ));
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(2), Column(9))
        ));
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(2), Column(10))
        ));

        // Test positions in non-overlapping parts
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(2), Column(5))
        ));
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(2), Column(12))
        ));
    }

    /// Helper: create a row and set specific characters.
    /// All written cells get a non-default fg so they are NOT is_empty().
    /// Unwritten cells (beyond chars.len()) remain default (is_empty() == true).
    fn make_row(chars: &[char], cols: usize) -> Row<Square> {
        let mut row = Row::<Square>::new(cols);
        for (i, &ch) in chars.iter().enumerate() {
            row[Column(i)].set_c(ch);
        }
        row
    }

    // PUA icon = U+F115 (Nerd Font file icon)
    const ICON: char = '\u{F115}';

    #[test]
    fn test_pua_icon_then_nothing() {
        // symbol→nothing: 2
        let row = make_row(&[ICON], 10);
        assert_eq!(pua_constraint_width(&row, 0, 10), 2.0);
    }

    #[test]
    fn test_pua_icon_followed_by_char() {
        // symbol→character: 1
        let row = make_row(&[ICON, 'a'], 10);
        assert_eq!(pua_constraint_width(&row, 0, 10), 1.0);
    }

    #[test]
    fn test_pua_icon_followed_by_space() {
        // symbol→space: 2
        let row = make_row(&[ICON, ' ', 'a'], 10);
        assert_eq!(pua_constraint_width(&row, 0, 10), 2.0);
    }

    #[test]
    fn test_pua_two_icons() {
        // symbol→symbol: 1, 1
        let row = make_row(&[ICON, ICON], 10);
        assert_eq!(pua_constraint_width(&row, 0, 10), 1.0);
        assert_eq!(pua_constraint_width(&row, 1, 10), 1.0);
    }

    #[test]
    fn test_pua_icon_at_end_of_row() {
        // symbol at end of row: 1
        let row = make_row(&[' ', ' ', ICON], 3);
        assert_eq!(pua_constraint_width(&row, 2, 3), 1.0);
    }

    #[test]
    fn test_pua_icon_space_icon() {
        // symbol→space→symbol: 2, 2
        let row = make_row(&[ICON, ' ', ICON], 4);
        assert_eq!(pua_constraint_width(&row, 0, 4), 2.0);
        assert_eq!(pua_constraint_width(&row, 2, 4), 2.0);
    }

    #[test]
    fn test_pua_char_then_icon_then_nothing() {
        // character→symbol→nothing: 2
        let row = make_row(&['z', ICON], 4);
        assert_eq!(pua_constraint_width(&row, 1, 4), 2.0);
    }

    #[test]
    fn test_pua_char_then_icon_then_space() {
        // character→symbol→space: 2
        let row = make_row(&['z', ICON, ' '], 4);
        assert_eq!(pua_constraint_width(&row, 1, 4), 2.0);
    }

    #[test]
    fn test_pua_icon_followed_by_no_break_space() {
        // symbol→no-break space (U+00A0): 1 (not a regular space)
        let row = make_row(&[ICON, '\u{00A0}', 'z'], 10);
        assert_eq!(pua_constraint_width(&row, 0, 10), 1.0);
    }

    // Powerline U+E0B0 is in PUA range but is a graphics element.
    // Our is_private_user_area includes it, so it gets PUA treatment.
    const POWERLINE: char = '\u{E0B0}';

    #[test]
    fn test_pua_icon_then_powerline() {
        // symbol→powerline: 1 (next is not space/empty)
        let row = make_row(&[ICON, POWERLINE], 4);
        assert_eq!(pua_constraint_width(&row, 0, 4), 1.0);
    }

    #[test]
    fn test_pua_powerline_then_icon() {
        // powerline→symbol: 2 (powerline is a graphics element, excluded from prev check)
        let row = make_row(&[POWERLINE, ICON], 4);
        assert_eq!(pua_constraint_width(&row, 1, 4), 2.0);
    }

    #[test]
    fn test_pua_powerline_then_nothing() {
        // powerline→nothing: 2
        let row = make_row(&[POWERLINE], 4);
        assert_eq!(pua_constraint_width(&row, 0, 4), 2.0);
    }

    #[test]
    fn test_pua_powerline_then_space() {
        // powerline→space: 2
        let row = make_row(&[POWERLINE, ' ', 'z'], 4);
        assert_eq!(pua_constraint_width(&row, 0, 4), 2.0);
    }
}
