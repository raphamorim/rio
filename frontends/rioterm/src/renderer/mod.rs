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

use crate::context::renderable::{PendingUpdate, RenderableContent};
use crate::context::ContextManager;
use crate::crosswords::pos::Pos;
use crate::crosswords::style::{Style as CellStyle, StyleFlags};
use rio_backend::config::colors::term::TermColors;
use rio_backend::config::colors::{
    term::{List, DIM_FACTOR},
    AnsiColor, ColorArray, Colors, NamedColor,
};
use rio_backend::config::navigation::Navigation;
use rio_backend::config::Config;
use rio_backend::event::EventProxy;
use rio_backend::sugarloaf::Sugarloaf;
use std::collections::BTreeSet;
use std::ops::RangeInclusive;

pub struct Renderer {
    is_vi_mode_enabled: bool,
    is_game_mode_enabled: bool,
    draw_bold_text_with_light_colors: bool,
    #[allow(dead_code)] // grid path doesn't consult this yet
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
    /// Last `rio_backend::sugarloaf::Color` we applied to sugarloaf's window clear via
    /// `set_background_color`. Lets the per-frame "derive bg from
    /// active panel's OSC state" loop avoid redundant resyncs.
    last_window_bg: Option<rio_backend::sugarloaf::Color>,
    pub config_has_blinking_enabled: bool,
    pub config_blinking_interval: u64,
    pub(crate) ignore_selection_fg_color: bool,
    pub search: search::SearchOverlay,
    pub assistant: assistant::AssistantOverlay,
    pub scrollbar: scrollbar::Scrollbar,
    #[allow(unused)]
    pub option_as_alt: String,
    #[allow(unused)]
    pub macos_use_unified_titlebar: bool,
    pub window_opacity: f64,
    pub use_window_background_for_transparency: bool,
    // Dynamic background keep track of the original bg color and
    // the same r,g,b with the mutated alpha channel.
    pub dynamic_background: ([f32; 4], rio_backend::sugarloaf::Color, bool),
    pub custom_mouse_cursor: bool,
    pub trail_cursor_enabled: bool,
    pub trail_cursor: trail_cursor::TrailCursor,
}

impl Renderer {
    #[inline]
    fn should_use_window_background_for_transparency(config: &Config) -> bool {
        #[cfg(target_os = "macos")]
        {
            config.window.opacity < 1.0
                && config.renderer.backend
                    == rio_backend::config::renderer::Backend::Metal
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = config;
            false
        }
    }

    #[inline]
    fn apply_window_opacity_to_background(
        &self,
        mut color: rio_backend::sugarloaf::Color,
    ) -> rio_backend::sugarloaf::Color {
        if self.window_opacity < 1.0 {
            color.a = self.window_opacity;
        }
        color
    }

    pub fn new(config: &Config) -> Renderer {
        let colors = List::from(&config.colors);
        let named_colors = config.colors;

        let mut dynamic_background =
            (named_colors.background.0, named_colors.background.1, false);
        if config.window.opacity < 1. {
            dynamic_background.1.a = config.window.opacity as f64;
            dynamic_background.2 = true;
        } else if config.window.background_image.is_some() {
            dynamic_background.1 = rio_backend::sugarloaf::Color::TRANSPARENT;
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
            last_window_bg: None,
            use_drawable_chars: config.fonts.use_drawable_chars,
            draw_bold_text_with_light_colors: config.draw_bold_text_with_light_colors,
            macos_use_unified_titlebar: config.window.macos_use_unified_titlebar,
            window_opacity: config.window.opacity as f64,
            use_window_background_for_transparency:
                Self::should_use_window_background_for_transparency(config),
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
    pub(crate) fn compute_color(
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
    pub(crate) fn compute_bg_color(
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
    ) -> (Option<crate::context::renderable::WindowUpdate>, bool) {
        let mut any_panel_dirty = false;
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

            let is_dirty = context.renderable_content.pending_update.is_dirty();

            // Check if we need to render
            if !is_dirty && !force_full_damage {
                // No updates pending, skip rendering
                continue;
            }
            any_panel_dirty = true;

            // UI-side damage (scroll, selection, resize, etc.)
            let ui_terminal_damage = context
                .renderable_content
                .pending_update
                .take_terminal_damage();
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
                        // UI-only damage (overlay hover, command-palette
                        // input, etc.): cells didn't change, but the
                        // panel still has to go through the render path
                        // so UI overlays paint on top of a fresh frame.
                        // Noop propagates to `RowsToRebuild::None` in
                        // `screen::render`'s emit loop — grid keeps its
                        // resident CPU bg/fg buffers, zero row work.
                        (None, None) => TerminalDamage::Noop,
                    }
                };

                terminal.reset_damage();

                // Hand the computed damage off to the grid
                // emission path in `Screen::render`. `snapshot` is
                // still used on the non-macOS rich-text path below;
                // this just persists a copy on the context. Cheap
                // (`TerminalDamage::Partial` is a `BTreeSet` of a
                // few dozen `LineDamage` entries at most).
                context.renderable_content.last_frame_damage = damage.clone();

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
            // change on scroll and text output (like approach).
            let has_overlays = !terminal_snapshot.kitty_placements.is_empty();
            let has_virtual = !terminal_snapshot.kitty_virtual_placements.is_empty();
            if has_overlays || has_virtual {
                let layout = context.dimension;
                // Canonical integer cell stride — line_height already
                // baked into `cell.cell_height`. Same value the GPU
                // grid uniform paints with.
                let cell_width = layout.cell.cell_width as f32;
                let cell_height = layout.cell.cell_height as f32;
                let origin_x = panel_rect[0] + grid_scaled_margin.left;
                let origin_y = panel_rect[1] + grid_scaled_margin.top;

                let overlays = sugarloaf
                    .image_overlays
                    .entry(context.rich_text_id)
                    .or_default();
                overlays.clear();

                if has_overlays {
                    let history_size = terminal_snapshot.history_size as i64;
                    let display_offset = terminal_snapshot.display_offset as i64;
                    let screen_lines = terminal_snapshot.screen_lines as i64;

                    for p in &terminal_snapshot.kitty_placements {
                        let screen_row = p.dest_row - (history_size - display_offset);
                        let image_bottom_row = screen_row + p.rows as i64;
                        // Cull only if fully off-screen (like )
                        if image_bottom_row <= 0 || screen_row >= screen_lines {
                            continue;
                        }
                        overlays.push(rio_backend::sugarloaf::GraphicOverlay {
                            image_id: p.image_id,
                            x: origin_x + p.dest_col as f32 * cell_width,
                            y: origin_y + screen_row as f32 * cell_height,
                            width: p.pixel_width as f32,
                            height: p.pixel_height as f32,
                            z_index: p.z_index,
                            source_rect:
                                rio_backend::sugarloaf::GraphicOverlay::FULL_SOURCE_RECT,
                        });
                    }
                }

                if has_virtual {
                    Self::push_virtual_placeholder_overlays(
                        overlays,
                        &terminal_snapshot,
                        origin_x,
                        origin_y,
                        cell_width,
                        cell_height,
                    );
                }
            } else if terminal_snapshot.kitty_graphics_dirty {
                // Placements were removed — clear overlays
                sugarloaf.clear_image_overlays_for(context.rich_text_id);
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

            // Grid renderer is the authoritative terminal text path on
            // every platform now. The grid emits from terminal state
            // directly and resolves its own cursor cells; the
            // previously-computed damage / cursor visibility /
            // hint-match info isn't used here.
            let _ = specific_lines;
            let _ = is_cursor_visible;
            let _ = hint_matches;
            let _ = focused_match;
            let _ = rich_text_id;
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
                // Match the grid renderer's actual paint region —
                // `.round()`ed integer-pixel origin +
                // `cols * round(cell_w)` × `rows * round(cell_h)`
                // content size (same math as `GridUniforms.grid_padding`
                // / `cell_size` in `screen/mod.rs:~3717`). Using raw
                // `layout_rect` leaves a sub-pixel un-dimmed fringe at
                // the right/bottom edges of inactive splits because
                // taffy allocates fractional sizes while the grid
                // snaps to whole cells.
                let dim = grid_context.val.dimension;
                let cell_w = dim.cell.cell_width as f32;
                let cell_h = dim.cell.cell_height as f32;
                let cols = dim.columns.max(1) as f32;
                let rows = dim.lines.max(1) as f32;
                let panel_left =
                    (grid_context.layout_rect[0] + grid_scaled_margin.left).round();
                let panel_top =
                    (grid_context.layout_rect[1] + grid_scaled_margin.top).round();
                let x = panel_left / scale_factor;
                let y = panel_top / scale_factor;
                let w = (cols * cell_w) / scale_factor;
                let h = (rows * cell_h) / scale_factor;
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

        // Derive the window bg color from the currently-active panel's
        // OSC 11 state (sticky on `renderable_content.background`) on
        // every frame, not just the frame where OSC arrived. Without
        // this, switching from a panel that ran OSC 11 to one that
        // didn't keeps sugarloaf's bg stuck at the OSC color — we
        // want it to follow focus the way does (each surface's
        // `terminal.colors.background` drives its own window chrome).
        let current_context = context_manager.current_grid_mut().current_mut();
        let effective_bg = match &current_context.renderable_content.background {
            Some(crate::context::renderable::BackgroundState::Set(color)) => *color,
            // Explicit OSC 111 reset OR panel that never ran OSC 11 →
            // fall back to the config / dynamic_background (honors
            // window-opacity / background-image).
            Some(crate::context::renderable::BackgroundState::Reset) | None => {
                self.dynamic_background.1
            }
        };
        let effective_bg = self.apply_window_opacity_to_background(effective_bg);

        let window_update = if self.last_window_bg != Some(effective_bg) {
            if self.use_window_background_for_transparency {
                sugarloaf.set_background_color(None);
            } else {
                sugarloaf.set_background_color(Some(effective_bg));
            }
            self.last_window_bg = Some(effective_bg);
            // Native-window chrome (`setBackgroundColor` on macOS,
            // titlebar color on Windows) follows the same value.
            Some(crate::context::renderable::WindowUpdate::Background(
                crate::context::renderable::BackgroundState::Set(effective_bg),
            ))
        } else {
            None
        };

        (window_update, any_panel_dirty)
    }

    /// Check if the renderer needs continuous redraw (for animations)
    #[inline]
    pub fn needs_redraw(&mut self) -> bool {
        if self.trail_cursor_enabled && self.trail_cursor.is_animating() {
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
    #[cfg_attr(target_os = "macos", allow(dead_code))]
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

    /// Scan visible rows for kitty Unicode-placeholder cells (U+10EEEE) and
    /// push one `GraphicOverlay` per row-run. Implements four key behaviors
    /// of the Kitty graphics Unicode-placeholder protocol:
    ///
    /// 1. Per-row `kitty_virtual_placeholder` flag check skips rows
    ///    with no placeholders.
    /// 2. Continuation rules — a cell with missing diacritics inherits
    ///    from the previous cell on the row (`canAppend`).
    /// 3. Run aggregation — consecutive cells with same image / row /
    ///    sequential column collapse into one Placement
    ///    (`PlacementIterator.next`, `graphics_unicode.zig:36-99`).
    /// 4. Per-run source rect with aspect-fit + centering — handles
    ///    partial visibility (placement scrolled half off-screen) and
    ///    cells that fall in the centering padding
    ///    (`renderPlacement`, `graphics_unicode.zig:212-329`).
    fn push_virtual_placeholder_overlays(
        overlays: &mut Vec<rio_backend::sugarloaf::GraphicOverlay>,
        snapshot: &TerminalSnapshot,
        origin_x: f32,
        origin_y: f32,
        cell_width: f32,
        cell_height: f32,
    ) {
        use rio_backend::ansi::kitty_virtual::{
            IncompletePlacement, PlaceholderRun, PLACEHOLDER,
        };

        // Below text by default for virtual placements — apps that
        // want them above the glyphs set z-index explicitly via the
        // graphics protocol.
        const VIRTUAL_Z_INDEX: i32 = -1;

        for (line_idx, row) in snapshot.visible_rows.iter().enumerate() {
            // Per-row dirty flag: skip rows that never had a placeholder
            // written. O(visible_w · visible_h) → O(rows_with_placeholders).
            if !row.kitty_virtual_placeholder {
                continue;
            }

            // Walk the row left-to-right, building a single in-flight run.
            // When the next cell can't extend it (different image, col
            // discontinuity, etc.) we flush the run as one overlay and
            // start a new one. Mirrors `PlacementIterator.next`.
            let mut run: Option<(IncompletePlacement, usize)> = None;

            for (col_idx, square) in row.inner.iter().enumerate() {
                if square.c() != PLACEHOLDER {
                    if let Some((p, start_col)) = run.take() {
                        flush_run(
                            overlays,
                            snapshot,
                            p.complete(),
                            line_idx,
                            start_col,
                            origin_x,
                            origin_y,
                            cell_width,
                            cell_height,
                            VIRTUAL_Z_INDEX,
                        );
                    }
                    continue;
                }

                let style = snapshot.style_set.get(square.style_id());
                let combining: &[char] = square
                    .extras_id()
                    .and_then(|eid| snapshot.extras_table.get(eid))
                    .map(|e| e.zerowidth.as_slice())
                    .unwrap_or(&[]);

                let mut cell = IncompletePlacement::from_cell(
                    style.fg,
                    style.underline_color,
                    combining,
                );

                match &mut run {
                    Some((current, _)) if current.can_append(&cell) => {
                        current.append();
                    }
                    _ => {
                        if let Some((p, start_col)) = run.take() {
                            flush_run(
                                overlays,
                                snapshot,
                                p.complete(),
                                line_idx,
                                start_col,
                                origin_x,
                                origin_y,
                                cell_width,
                                cell_height,
                                VIRTUAL_Z_INDEX,
                            );
                        }
                        // Default missing row/col on the FIRST cell of a
                        // run. Without this, a subsequent cell with
                        // `Some(col)` couldn't sequentially extend a
                        // run started by a cell with `None`.
                        if cell.row.is_none() {
                            cell.row = Some(0);
                        }
                        if cell.col.is_none() {
                            cell.col = Some(0);
                        }
                        run = Some((cell, col_idx));
                    }
                }
            }

            if let Some((p, start_col)) = run {
                flush_run(
                    overlays,
                    snapshot,
                    p.complete(),
                    line_idx,
                    start_col,
                    origin_x,
                    origin_y,
                    cell_width,
                    cell_height,
                    VIRTUAL_Z_INDEX,
                );
            }
        }

        /// Look up metadata + image for a completed `PlaceholderRun`,
        /// compute its on-screen geometry via
        /// `kitty_virtual::compute_run_geometry`, and push one
        /// `GraphicOverlay`. Returns silently when the placement isn't
        /// registered, the image isn't transmitted yet, or the run lies
        /// entirely in the aspect-fit centering padding.
        #[allow(clippy::too_many_arguments)]
        fn flush_run(
            overlays: &mut Vec<rio_backend::sugarloaf::GraphicOverlay>,
            snapshot: &TerminalSnapshot,
            run: PlaceholderRun,
            screen_line: usize,
            start_screen_col: usize,
            origin_x: f32,
            origin_y: f32,
            cell_width: f32,
            cell_height: f32,
            z_index: i32,
        ) {
            let vp = snapshot
                .kitty_virtual_placements
                .get(&(run.image_id, run.placement_id))
                .or_else(|| snapshot.kitty_virtual_placements.get(&(run.image_id, 0)));
            let vp = match vp {
                Some(v) => v,
                None => return,
            };
            let img = match snapshot.kitty_images.get(&run.image_id) {
                Some(i) => i,
                None => return,
            };

            let geom = match rio_backend::ansi::kitty_virtual::compute_run_geometry(
                &run,
                vp.columns,
                vp.rows,
                img.data.width as u32,
                img.data.height as u32,
                cell_width,
                cell_height,
                origin_x,
                origin_y,
                screen_line,
                start_screen_col,
            ) {
                Some(g) => g,
                None => return,
            };

            overlays.push(rio_backend::sugarloaf::GraphicOverlay {
                image_id: run.image_id,
                x: geom.x,
                y: geom.y,
                width: geom.width,
                height: geom.height,
                z_index,
                source_rect: geom.source_rect,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rio_backend::config::renderer::Backend;
    use rio_backend::config::Config;

    #[test]
    fn test_apply_window_opacity_to_background() {
        let mut config = Config::default();
        config.window.opacity = 0.42;
        let renderer = Renderer::new(&config);

        let color =
            renderer.apply_window_opacity_to_background(rio_backend::sugarloaf::Color {
                r: 0.1,
                g: 0.2,
                b: 0.3,
                a: 1.0,
            });

        assert!((color.a - 0.42).abs() < 1e-6);
    }

    #[test]
    fn test_apply_window_opacity_keeps_opaque_background_when_window_is_opaque() {
        let renderer = Renderer::new(&Config::default());

        let color =
            renderer.apply_window_opacity_to_background(rio_backend::sugarloaf::Color {
                r: 0.1,
                g: 0.2,
                b: 0.3,
                a: 0.7,
            });

        assert_eq!(color.a, 0.7);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_should_use_window_background_for_transparency() {
        let mut config = Config::default();
        config.window.opacity = 0.5;
        config.renderer.backend = Backend::Metal;

        assert!(Renderer::should_use_window_background_for_transparency(
            &config
        ));
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn test_should_not_use_window_background_for_transparency_on_non_macos() {
        let mut config = Config::default();
        config.window.opacity = 0.5;

        assert!(!Renderer::should_use_window_background_for_transparency(
            &config
        ));
    }

    #[test]
    fn test_should_not_use_window_background_for_non_metal_backend() {
        let mut config = Config::default();
        config.window.opacity = 0.5;
        config.renderer.backend = Backend::WgpuMetal;

        assert!(!Renderer::should_use_window_background_for_transparency(
            &config
        ));
    }
}
