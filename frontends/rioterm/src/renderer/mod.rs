/// The input policy every overlay text sink applies, in ONE place so
/// the key path and the IME commit path can never drift: non-empty and
/// free of control characters.
#[inline]
pub(crate) fn is_printable_text(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| !c.is_control())
}

pub mod assistant;
pub mod command_palette;
pub mod confirm_quit;
pub mod custom_cursor;
pub mod helpers;
pub mod island;
pub mod scrollbar;
pub mod search;
pub mod trail_cursor;
pub mod utils;

use rio_backend::event::TerminalDamage;

use crate::context::renderable::{PendingUpdate, RenderableContent};
use crate::context::ContextManager;
use crate::crosswords::style::{Style as CellStyle, StyleFlags};
use rio_backend::config::colors::term::TermColors;
use rio_backend::config::colors::{
    term::{List, DIM_FACTOR},
    AnsiColor, ColorArray, Colors, NamedColor,
};
use rio_backend::config::navigation::Navigation;
use rio_backend::config::Config;
use rio_backend::event::EventProxy;
use rio_backend::sugarloaf::text::DrawOpts;
use rio_backend::sugarloaf::Sugarloaf;

// Hint tooltip: browser-style status pill showing where the hovered
// link goes. Shares the overlay draw order with search / palette.
const TOOLTIP_FONT_SIZE: f32 = 12.0;
const TOOLTIP_PADDING_X: f32 = 9.0;
const TOOLTIP_PADDING_Y: f32 = 5.0;
const TOOLTIP_MARGIN: f32 = 6.0;
const TOOLTIP_CORNER_RADIUS: f32 = 5.0;
const TOOLTIP_MAX_WIDTH_RATIO: f32 = 0.6;
const TOOLTIP_BG_COLOR: [f32; 4] = [0.12, 0.12, 0.12, 0.96];
const TOOLTIP_TEXT_COLOR: [u8; 4] = [237, 237, 237, 255];
const TOOLTIP_DEPTH_BG: f32 = 0.1;
const TOOLTIP_ORDER: u8 = 20;

/// Longest prefix of `text` that still fits `max_width` once an
/// ellipsis is appended, or `text` untouched when it already fits.
///
/// Truncates the tail, never the head: for a URL the scheme and host
/// are the part worth reading before clicking, so they must survive.
/// `measure` is injected so this stays testable without a GPU context.
fn elide_tail(
    text: &str,
    max_width: f32,
    mut measure: impl FnMut(&str) -> f32,
) -> String {
    if measure(text) <= max_width {
        return text.to_string();
    }

    let chars: Vec<char> = text.chars().collect();
    let ellipsis = |n: usize| chars[..n].iter().collect::<String>() + "\u{2026}";

    // Largest `n` whose prefix-plus-ellipsis fits. `lo < hi` keeps
    // `mid >= 1`, so `mid - 1` cannot wrap.
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if measure(&ellipsis(mid)) <= max_width {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    ellipsis(lo)
}

/// Draw the hovered hint's target at the bottom-left, the way a browser
/// shows a link destination in its status bar.
///
/// Immediate mode: this is called only on frames where a hint is
/// highlighted, so "not drawn" is "not visible" and there is no
/// show/hide state to keep anywhere.
fn draw_hint_tooltip(
    sugarloaf: &mut Sugarloaf,
    text: &str,
    window_size: (f32, f32),
    scale_factor: f32,
) {
    let logical_width = window_size.0 / scale_factor;
    let logical_height = window_size.1 / scale_factor;

    let opts = DrawOpts {
        font_size: TOOLTIP_FONT_SIZE,
        color: TOOLTIP_TEXT_COLOR,
        ..DrawOpts::default()
    };

    let max_text_width =
        (logical_width * TOOLTIP_MAX_WIDTH_RATIO - TOOLTIP_PADDING_X * 2.0).max(0.0);

    let ui = sugarloaf.text_mut();
    let label = elide_tail(text, max_text_width, |s| ui.measure(s, &opts));

    let text_width = ui.measure(&label, &opts);
    let height = TOOLTIP_FONT_SIZE + TOOLTIP_PADDING_Y * 2.0;
    let width = text_width + TOOLTIP_PADDING_X * 2.0;
    let x = TOOLTIP_MARGIN;
    let y = (logical_height - height - TOOLTIP_MARGIN).max(0.0);

    sugarloaf.rounded_rect(
        None,
        x,
        y,
        width,
        height,
        TOOLTIP_BG_COLOR,
        TOOLTIP_DEPTH_BG,
        TOOLTIP_CORNER_RADIUS,
        TOOLTIP_ORDER,
    );
    sugarloaf.text_mut().draw(
        x + TOOLTIP_PADDING_X,
        y + TOOLTIP_PADDING_Y,
        &label,
        &opts,
    );
}

/// The window-bg clear alpha that flows into sugarloaf's
/// `set_background_color`. Stored on the renderer and re-applied on
/// every `effective_bg` write so OSC 11 changes don't reset
/// transparency to 1.0.
///
/// - Glass blur styles force `0.0` so the macOS-26 `NSGlassEffectView`
///   under the metal layer is what shows through.
/// - Otherwise it's the configured `window.opacity`, clamped to
///   `[0, 1]`.
#[inline]
fn window_bg_alpha(config: &Config) -> f32 {
    if config.window.blur.is_glass() {
        0.0
    } else {
        config.window.opacity.clamp(0.0, 1.0)
    }
}

pub use rio_backend::sugarloaf::{atlas_image_key, kitty_image_key, route_image_key};

pub struct Renderer {
    is_vi_mode_enabled: bool,
    is_game_mode_enabled: bool,
    pub is_window_focused: bool,
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
    /// Route id of the pane rendered as active last frame. Keyed on
    /// `route_id` (globally unique) rather than the grid's taffy `NodeId`:
    /// each tab owns its own taffy tree and identically-shaped trees hand
    /// out identical NodeIds, so a tab switch would compare equal and skip
    /// the full-damage refresh the incoming tab needs.
    last_active: Option<usize>,
    /// Last `rio_backend::sugarloaf::Color` we applied to sugarloaf's window clear via
    /// `set_background_color`. Lets the per-frame "derive bg from
    /// active panel's OSC state" loop avoid redundant resyncs.
    last_window_bg: Option<rio_backend::sugarloaf::Color>,
    pub config_has_blinking_enabled: bool,
    pub config_blinking_interval: u64,
    pub(crate) ignore_selection_fg_color: bool,
    pub search: search::SearchOverlay,
    pub assistant: assistant::AssistantOverlay,
    pub confirm_quit: confirm_quit::ConfirmQuit,
    pub scrollbar: scrollbar::Scrollbar,
    #[allow(unused)]
    pub option_as_alt: String,
    #[allow(unused)]
    pub macos_use_unified_titlebar: bool,
    // Dynamic background keep track of the original bg color and
    // the same r,g,b with the mutated alpha channel.
    pub dynamic_background: ([f32; 4], rio_backend::sugarloaf::Color, bool),
    /// `window.opacity-cells` — apply window opacity to cells with an
    /// SGR-set background too. Off by default. `cell_bg_alpha` is the
    /// precomputed `(window.opacity * 255) as u8` to avoid a multiply
    /// per cell.
    pub opacity_cells: bool,
    pub cell_bg_alpha: u8,
    /// Target alpha for the window-bg clear (`0..=1`). 0 in glass
    /// mode, otherwise `window.opacity`. Re-applied to `effective_bg`
    /// every frame so OSC 11 doesn't undo the user's transparency.
    pub window_bg_alpha: f32,
    pub custom_mouse_cursor: bool,
    pub trail_cursor_enabled: bool,
    pub trail_cursor: trail_cursor::TrailCursor,
}

impl Renderer {
    pub fn new(config: &Config) -> Renderer {
        let colors = List::from(&config.colors);
        let named_colors = config.colors;

        let mut dynamic_background =
            (named_colors.background.0, named_colors.background.1, false);
        // Window-bg target alpha. Cached here at init and re-applied
        // to every OSC-11-driven `effective_bg` refresh in
        // `Renderer::run` so a runtime bg change doesn't reset
        // transparency to 1.0. Glass styles force alpha = 0 so the
        // NSGlassEffectView underneath the metal layer can provide
        // the actual translucent bg — `window_bg_alpha` returns 0 in
        // that case, so the glass and `opacity < 1` paths share one
        // assignment.
        let target_bg_alpha = window_bg_alpha(config);
        if config.window.blur.is_glass() || config.window.opacity < 1. {
            dynamic_background.1.a = target_bg_alpha as f64;
            dynamic_background.2 = true;
        } else if config.window.background_image.is_some() {
            dynamic_background.1 = rio_backend::sugarloaf::Color::TRANSPARENT;
            dynamic_background.2 = true;
        }

        let island = if config.navigation.is_enabled() {
            Some(island::Island::new(
                named_colors.tabs,
                named_colors.tabs_active,
                config.navigation.hide_if_single,
                config.navigation.max_tab_width,
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
            config_blinking_interval: config.cursor.blinking_interval.clamp(350, 1200),
            option_as_alt: config.option_as_alt.to_lowercase(),
            is_vi_mode_enabled: false,
            config_has_blinking_enabled: config.cursor.blinking,
            is_window_focused: true,
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
            opacity_cells: config.window.opacity_cells,
            cell_bg_alpha: (config.window.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            window_bg_alpha: target_bg_alpha,
            search: search::SearchOverlay::default(),
            assistant: assistant::AssistantOverlay::default(),
            confirm_quit: confirm_quit::ConfirmQuit::default(),
            scrollbar: scrollbar::Scrollbar::new(config.enable_scroll_bar),
            is_game_mode_enabled: config.renderer.strategy.is_game(),
            custom_mouse_cursor: config.effects.custom_mouse_cursor,
            trail_cursor_enabled: config.effects.trail_cursor,
            trail_cursor: trail_cursor::TrailCursor::new(trail_cursor::TrailSettings {
                color: config
                    .effects
                    .trail_cursor_color
                    .as_deref()
                    .map(rio_backend::config::colors::hex_to_color_arr),
                opacity: config.effects.trail_cursor_opacity,
                decay_fast: config.effects.trail_cursor_decay[0] as f32 / 1000.0,
                decay_slow: config.effects.trail_cursor_decay[1] as f32 / 1000.0,
                start_threshold: config.effects.trail_cursor_start_threshold as f32,
            }),
        }
    }

    #[inline]
    pub fn use_drawable_chars(&self) -> bool {
        self.use_drawable_chars
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

    /// Resolve the color painted as a cell's background.
    ///
    /// DIM and BOLD are glyph intensity attributes (ECMA-48), so they
    /// tint a background only under INVERSE, where `cell_bg` already
    /// swapped fg/bg and this color is really the foreground. Ungated,
    /// faint text over an explicit background painted a darker block
    /// (tmux always sets one: it re-emits the pane's OSC 11 as SGR 48
    /// on every cell it draws).
    #[inline]
    pub(crate) fn compute_bg_color(
        &self,
        cell_style: &CellStyle,
        term_colors: &TermColors,
    ) -> ColorArray {
        let inverse = cell_style.flags.contains(StyleFlags::INVERSE);
        let dim = inverse && cell_style.flags.contains(StyleFlags::DIM);
        let bold = inverse && cell_style.flags.contains(StyleFlags::BOLD);
        match cell_style.bg {
            // A named color lands here dimmable only via the inverse
            // swap, so apply the same intensity table as the fg path
            // in `compute_color` (alacritty resolves the fg first and
            // swaps after, which yields the same result).
            AnsiColor::Named(ansi) => {
                let idx = match (self.draw_bold_text_with_light_colors, dim, bold) {
                    (_, true, true)
                        if ansi == NamedColor::Foreground
                            && self.named_colors.light_foreground.is_none() =>
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
    pub fn set_vi_mode(&mut self, is_vi_mode_enabled: bool) {
        self.is_vi_mode_enabled = is_vi_mode_enabled;
    }

    // Get the RGB value for a color index.
    #[inline]
    pub fn color(&self, color: usize, term_colors: &TermColors) -> ColorArray {
        term_colors[color].unwrap_or(self.colors[color])
    }

    /// Whether a click on the currently highlighted hint would actually
    /// reach it, i.e. whether the link under the pointer is genuinely
    /// clickable right now.
    ///
    /// `highlighted_hint` alone is not enough. It is recomputed only on
    /// cell crossings and modifier changes, so it survives an overlay
    /// opening on top of it, and each modal overlay listed here
    /// consumes the click before the hint handler runs (see the press
    /// handler in `application.rs`). Showing a target the user cannot
    /// open would be a lie, so the tooltip is gated on the same
    /// conditions. Checked per frame rather than at highlight time
    /// because an overlay can appear without the pointer moving.
    ///
    /// Positional click consumers (the scrollbar strip, the tab
    /// island, panel borders) also swallow presses but depend on
    /// where the pointer sits, which a per-frame boolean cannot
    /// express; a stale highlight over those is a pre-existing quirk
    /// of the highlight lifecycle, not of this gate.
    #[inline]
    fn hint_click_would_land(&self) -> bool {
        !self.assistant.is_active()
            && !self.command_palette.is_enabled()
            && !self.search.is_active()
            && !self.confirm_quit.is_active()
    }

    #[inline]
    pub fn run(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        context_manager: &mut ContextManager<EventProxy>,
    ) -> (Option<crate::context::renderable::WindowUpdate>, bool) {
        let mut any_panel_dirty = false;
        let grid = context_manager.current_grid_mut();
        let active_route = grid.current().route_id;
        let grid_scaled_margin = grid.get_scaled_margin();
        let mut has_active_changed = false;
        if self.last_active != Some(active_route) {
            has_active_changed = true;
            self.last_active = Some(active_route);
        }

        for (_key, grid_context) in grid.contexts_mut().iter_mut() {
            let panel_rect = grid_context.layout_rect;
            let context = grid_context.context_mut();

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

            {
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

                terminal.snapshot_visible(
                    &damage,
                    &mut context.renderable_content.visible_rows,
                    &mut context.renderable_content.row_styles,
                    &mut context.renderable_content.extras,
                );
                context.renderable_content.term_colors = terminal.colors;
                context.renderable_content.display_offset = terminal.display_offset();
                context.renderable_content.columns = terminal.columns();
                context.renderable_content.screen_lines = terminal.screen_lines();
                context.renderable_content.history_size = terminal.history_size();
                context.renderable_content.lines_evicted = terminal.lines_evicted();
                context.renderable_content.blinking_cursor = terminal.blinking_cursor;
                context.renderable_content.cursor.state = terminal.cursor();
                if terminal.graphics.kitty_graphics_dirty {
                    context.renderable_content.kitty_virtual_placements =
                        terminal.graphics.kitty_virtual_placements.clone();
                    context.renderable_content.kitty_images =
                        terminal.graphics.kitty_images.clone();
                    context.renderable_content.kitty_placements = {
                        let mut placements: Vec<_> = terminal
                            .graphics
                            .kitty_placements
                            .values()
                            .filter(|p| {
                                terminal.graphics.kitty_images.contains_key(&p.image_id)
                            })
                            .cloned()
                            .collect();
                        // Tie-break on the unique key so equal
                        // z-indexes keep a stable paint order across
                        // frames (map iteration order is not).
                        placements
                            .sort_by_key(|p| (p.z_index, p.image_id, p.placement_id));
                        placements
                    };
                    context.renderable_content.atlas_placements =
                        terminal.graphics.atlas_placements.clone();
                    context.renderable_content.kitty_graphics_dirty = true;
                    terminal.graphics.kitty_graphics_dirty = false;
                } else {
                    context.renderable_content.kitty_graphics_dirty = false;
                }
                context.renderable_content.frame_damage = damage;
                drop(terminal);
            }

            // Recalculate image overlay positions every frame when placements
            // exist. Positions depend on display_offset and history_size which
            // change on scroll and text output (like approach).
            let rc = &context.renderable_content;
            let route_id = context.route_id;
            let has_overlays = !rc.kitty_placements.is_empty();
            let has_virtual = !rc.kitty_virtual_placements.is_empty();
            let has_atlas = !rc.atlas_placements.is_empty();
            if has_overlays || has_virtual || has_atlas {
                let layout = context.dimension;
                // Canonical integer cell stride — line_height already
                // baked into `cell.cell_height`. Same value the GPU
                // grid uniform paints with.
                let cell_width = layout.cell.cell_width as f32;
                let cell_height = layout.cell.cell_height as f32;
                // Rounded like the grid's own paint origin
                // (screen/mod.rs panel_left/panel_top), so image quads
                // and the clip rect sit exactly on the painted cell
                // grid instead of up to half a pixel off.
                let origin_x = (panel_rect[0] + grid_scaled_margin.left).round();
                let origin_y = (panel_rect[1] + grid_scaled_margin.top).round();

                // Images clip to the panel's cell grid, exactly like
                // text: without this a wide image paints across split
                // dividers onto neighbor panels.
                let clip_x0 = origin_x;
                let clip_y0 = origin_y;
                let clip_x1 = origin_x + rc.columns as f32 * cell_width;
                let clip_y1 = origin_y + rc.screen_lines as f32 * cell_height;

                let overlays = sugarloaf
                    .image_overlays
                    .entry(context.rich_text_id)
                    .or_default();
                overlays.clear();

                let viewport = rio_backend::ansi::graphics::OverlayViewport {
                    cell_width,
                    cell_height,
                    origin_x,
                    origin_y,
                    // Absolute lines above the screen top: ring
                    // evictions + current history.
                    history_size: rc.lines_evicted as i64 + rc.history_size as i64,
                    display_offset: rc.display_offset as i64,
                    screen_lines: rc.screen_lines as i64,
                };

                if has_atlas {
                    // Sixel/iTerm2 grid-plane images draw below text
                    // and below kitty overlays at the same z.
                    for p in &rc.atlas_placements {
                        let Some(geometry) =
                            rio_backend::ansi::graphics::atlas_overlay_geometry(
                                p, &viewport,
                            )
                        else {
                            continue;
                        };
                        let mut overlay = rio_backend::sugarloaf::GraphicOverlay {
                            image_id: route_image_key(route_id, p.image_key),
                            x: geometry.x,
                            y: geometry.y,
                            width: geometry.width,
                            height: geometry.height,
                            z_index: -1,
                            source_rect: geometry.source_rect,
                        };
                        if rio_backend::ansi::graphics::clip_overlay_to_rect(
                            &mut overlay,
                            clip_x0,
                            clip_y0,
                            clip_x1,
                            clip_y1,
                        ) {
                            overlays.push(overlay);
                        }
                    }
                }

                if has_overlays {
                    for p in &rc.kitty_placements {
                        let (image_width, image_height) = rc
                            .kitty_images
                            .get(&p.image_id)
                            .map(|stored| (stored.data.width, stored.data.height))
                            .unwrap_or((0, 0));
                        let Some(geometry) =
                            rio_backend::ansi::graphics::kitty_overlay_geometry(
                                p,
                                image_width,
                                image_height,
                                &viewport,
                            )
                        else {
                            continue;
                        };
                        let mut overlay = rio_backend::sugarloaf::GraphicOverlay {
                            image_id: route_image_key(
                                route_id,
                                kitty_image_key(p.image_id),
                            ),
                            x: geometry.x,
                            y: geometry.y,
                            width: geometry.width,
                            height: geometry.height,
                            z_index: p.z_index,
                            source_rect: geometry.source_rect,
                        };
                        if rio_backend::ansi::graphics::clip_overlay_to_rect(
                            &mut overlay,
                            clip_x0,
                            clip_y0,
                            clip_x1,
                            clip_y1,
                        ) {
                            overlays.push(overlay);
                        }
                    }
                }

                if has_virtual {
                    Self::push_virtual_placeholder_overlays(
                        overlays,
                        rc,
                        route_id,
                        origin_x,
                        origin_y,
                        cell_width,
                        cell_height,
                        (clip_x0, clip_y0, clip_x1, clip_y1),
                    );
                }
            } else if rc.kitty_graphics_dirty {
                // All placements (kitty and atlas) were removed, so drop
                // this panel's overlay vec.
                sugarloaf.clear_image_overlays_for(context.rich_text_id);
            }

            context.renderable_content.has_blinking_enabled =
                context.renderable_content.blinking_cursor;

            if context.renderable_content.blinking_cursor {
                let has_selection = context.renderable_content.selection_range.is_some();
                if !has_selection {
                    let mut should_blink = self.is_window_focused;
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
            }
        }

        if self.scrollbar.is_enabled() {
            self.scrollbar.clear_panel_states();
            for grid_context in grid.contexts_mut().values() {
                let panel_rect = grid_context.layout_rect;
                let ctx = grid_context.context();
                let rc = &ctx.renderable_content;
                self.scrollbar
                    .push_panel_state(scrollbar::PanelScrollState {
                        rich_text_id: ctx.rich_text_id,
                        panel_rect,
                        display_offset: rc.display_offset,
                        history_size: rc.history_size,
                        screen_lines: rc.screen_lines,
                    });
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
            // Within-grid comparison: taffy keys are only meaningful
            // inside a single tab's tree.
            let active_key = grid.current;
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
            let island_bg = self
                .last_window_bg
                .map(|c| [c.r as f32, c.g as f32, c.b as f32, c.a as f32])
                .unwrap_or(self.named_colors.background.0);
            island.render(
                sugarloaf,
                (window_size.width, window_size.height, scale_factor),
                context_manager,
                island_bg,
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

        // The hint borrow (context_manager) and the draw target
        // (sugarloaf) are disjoint, so the target text passes through
        // by reference; nothing is cloned per hovered frame.
        if let Some(hint) = context_manager
            .current()
            .renderable_content
            .highlighted_hint
            .as_ref()
            .filter(|_| self.hint_click_would_land())
        {
            draw_hint_tooltip(
                sugarloaf,
                &hint.text,
                (window_size.width, window_size.height),
                scale_factor,
            );
        }

        self.command_palette.render(
            sugarloaf,
            (window_size.width, window_size.height, scale_factor),
        );

        self.confirm_quit.render(
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

        // Render panel borders (on top of terminal content). Borders
        // are flat rects today — the previous `Object` enum
        // (Rect / Quad / RichText) was only ever populated with the
        // Rect variant, so the dispatch is direct now.
        let grid_scaled_margin = context_manager.get_current_grid_scaled_margin();
        for rect in context_manager.get_panel_borders() {
            let x = (rect.x + grid_scaled_margin.left) / scale_factor;
            let y = (rect.y + grid_scaled_margin.top) / scale_factor;
            let width = rect.width / scale_factor;
            let height = rect.height / scale_factor;
            sugarloaf.rect(None, x, y, width, height, rect.color, 0.0, 1);
        }

        // Derive the window bg color from the currently-active panel's
        // OSC 11 state (sticky on `renderable_content.background`) on
        // every frame, not just the frame where OSC arrived. Without
        // this, switching from a panel that ran OSC 11 to one that
        // didn't keeps sugarloaf's bg stuck at the OSC color — we
        // want it to follow focus the way does (each surface's
        // `terminal.colors.background` drives its own window chrome).
        let current_context = context_manager.current_grid_mut().current_mut();
        let mut effective_bg = match &current_context.renderable_content.background {
            Some(crate::context::renderable::BackgroundState::Set(color)) => *color,
            // Explicit OSC 111 reset OR panel that never ran OSC 11 →
            // fall back to the config / dynamic_background (honors
            // window-opacity / background-image).
            Some(crate::context::renderable::BackgroundState::Reset) | None => {
                self.dynamic_background.1
            }
        };
        // Re-apply the configured window-bg alpha. Without this, an
        // OSC 11 sequence that sets a new bg color resets the alpha
        // to 1.0 and the window goes opaque even when
        // `window.opacity < 1`. Glass mode forces alpha 0 so the
        // backdrop view shows through.
        effective_bg.a = self.window_bg_alpha as f64;

        let window_update = if self.last_window_bg != Some(effective_bg) {
            sugarloaf.set_background_color(Some(effective_bg));
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
    #[allow(clippy::too_many_arguments)]
    fn push_virtual_placeholder_overlays(
        overlays: &mut Vec<rio_backend::sugarloaf::GraphicOverlay>,
        rc: &RenderableContent,
        route_id: usize,
        origin_x: f32,
        origin_y: f32,
        cell_width: f32,
        cell_height: f32,
        clip: (f32, f32, f32, f32),
    ) {
        use rio_backend::ansi::kitty_virtual::{
            IncompletePlacement, PlaceholderRun, PLACEHOLDER,
        };

        // Below text by default for virtual placements — apps that
        // want them above the glyphs set z-index explicitly via the
        // graphics protocol.
        const VIRTUAL_Z_INDEX: i32 = -1;

        for (line_idx, row) in rc.visible_rows.iter().enumerate() {
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
            let row_styles = rc
                .row_styles
                .get(line_idx)
                .map(Vec::as_slice)
                .unwrap_or(&[]);

            for (col_idx, square) in row.inner.iter().enumerate() {
                if square.c() != PLACEHOLDER {
                    if let Some((p, start_col)) = run.take() {
                        flush_run(
                            overlays,
                            rc,
                            route_id,
                            p.complete(),
                            line_idx,
                            start_col,
                            origin_x,
                            origin_y,
                            cell_width,
                            cell_height,
                            VIRTUAL_Z_INDEX,
                            clip,
                        );
                    }
                    continue;
                }

                let style = rio_grid::resolve_style(row_styles, col_idx);
                let combining: &[char] = square
                    .extras_id()
                    .and_then(|eid| rc.extras.get(&eid))
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
                                rc,
                                route_id,
                                p.complete(),
                                line_idx,
                                start_col,
                                origin_x,
                                origin_y,
                                cell_width,
                                cell_height,
                                VIRTUAL_Z_INDEX,
                                clip,
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
                    rc,
                    route_id,
                    p.complete(),
                    line_idx,
                    start_col,
                    origin_x,
                    origin_y,
                    cell_width,
                    cell_height,
                    VIRTUAL_Z_INDEX,
                    clip,
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
            rc: &RenderableContent,
            route_id: usize,
            run: PlaceholderRun,
            screen_line: usize,
            start_screen_col: usize,
            origin_x: f32,
            origin_y: f32,
            cell_width: f32,
            cell_height: f32,
            z_index: i32,
            clip: (f32, f32, f32, f32),
        ) {
            let vp = rio_backend::ansi::kitty_virtual::resolve_virtual_placement(
                &rc.kitty_virtual_placements,
                run.image_id,
                run.placement_id,
            );
            let vp = match vp {
                Some(v) => v,
                None => return,
            };
            let img = match rc.kitty_images.get(&run.image_id) {
                Some(i) => i,
                None => return,
            };

            let geom = match rio_backend::ansi::kitty_virtual::compute_run_geometry(
                &run,
                vp.columns,
                vp.rows,
                img.data.width as u32,
                img.data.height as u32,
                (vp.x, vp.y, vp.width, vp.height),
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

            let mut overlay = rio_backend::sugarloaf::GraphicOverlay {
                image_id: route_image_key(route_id, kitty_image_key(run.image_id)),
                x: geom.x,
                y: geom.y,
                width: geom.width,
                height: geom.height,
                z_index,
                source_rect: geom.source_rect,
            };
            if rio_backend::ansi::graphics::clip_overlay_to_rect(
                &mut overlay,
                clip.0,
                clip.1,
                clip.2,
                clip.3,
            ) {
                overlays.push(overlay);
            }
        }
    }
}

/// Bridges the frontend `Renderer` to the shared `rio-grid` emit code,
/// delegating each palette operation to the existing methods/fields.
impl rio_grid::GridPalette for Renderer {
    #[inline]
    fn named_colors(&self) -> &Colors {
        &self.named_colors
    }

    #[inline]
    fn compute_color(
        &self,
        color: &AnsiColor,
        flags: StyleFlags,
        term_colors: &TermColors,
    ) -> ColorArray {
        Renderer::compute_color(self, color, flags, term_colors)
    }

    #[inline]
    fn compute_bg_color(
        &self,
        cell_style: &CellStyle,
        term_colors: &TermColors,
    ) -> ColorArray {
        Renderer::compute_bg_color(self, cell_style, term_colors)
    }

    #[inline]
    fn color(&self, idx: usize, term_colors: &TermColors) -> ColorArray {
        Renderer::color(self, idx, term_colors)
    }

    #[inline]
    fn use_drawable_chars(&self) -> bool {
        Renderer::use_drawable_chars(self)
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

#[cfg(test)]
mod compute_bg_color_tests {
    use super::*;
    use rio_backend::config::colors::ColorRgb;

    fn renderer(draw_bold_text_with_light_colors: bool) -> Renderer {
        Renderer::new(&Config {
            draw_bold_text_with_light_colors,
            ..Config::default()
        })
    }

    fn style(bg: AnsiColor, flags: StyleFlags) -> CellStyle {
        CellStyle {
            bg,
            flags,
            ..CellStyle::default()
        }
    }

    /// The reported regression: SGR 2 scaled an explicit RGB
    /// background by DIM_FACTOR, a darker block behind the glyphs.
    #[test]
    fn dim_leaves_an_explicit_rgb_background_alone() {
        let r = renderer(false);
        let colors = TermColors::default();
        let bg = ColorRgb {
            r: 0x28,
            g: 0x2c,
            b: 0x34,
        };

        let plain =
            r.compute_bg_color(&style(AnsiColor::Spec(bg), StyleFlags::empty()), &colors);
        let dimmed =
            r.compute_bg_color(&style(AnsiColor::Spec(bg), StyleFlags::DIM), &colors);

        assert_eq!(plain, bg.to_arr());
        assert_eq!(dimmed, plain);
    }

    /// Same rule for the palette: no remap to the dim slot (0..=7)
    /// or to the non-bright half (8..=15).
    #[test]
    fn dim_leaves_an_indexed_background_alone() {
        let r = renderer(false);
        let colors = TermColors::default();

        for idx in [1u8, 9] {
            let dimmed = r.compute_bg_color(
                &style(AnsiColor::Indexed(idx), StyleFlags::DIM),
                &colors,
            );
            assert_eq!(dimmed, r.colors[idx as usize], "index {idx}");
        }
    }

    /// Under INVERSE the bg slot holds the real foreground, which
    /// keeps its intensity.
    #[test]
    fn inverse_still_dims_the_swapped_foreground() {
        let r = renderer(false);
        let colors = TermColors::default();
        let fg = ColorRgb {
            r: 0xab,
            g: 0xb2,
            b: 0xbf,
        };

        let spec = r.compute_bg_color(
            &style(AnsiColor::Spec(fg), StyleFlags::DIM | StyleFlags::INVERSE),
            &colors,
        );
        assert_eq!(spec, (fg * DIM_FACTOR).to_arr());

        let indexed = r.compute_bg_color(
            &style(AnsiColor::Indexed(1), StyleFlags::DIM | StyleFlags::INVERSE),
            &colors,
        );
        assert_eq!(indexed, r.colors[NamedColor::DimBlack as usize + 1]);
    }

    /// The default-colors case of the same rule: dim inverse text
    /// paints its block in DimForeground, exactly what the fg path
    /// resolves (alacritty gets this from resolving before the swap).
    #[test]
    fn inverse_dims_a_named_swapped_foreground() {
        let r = renderer(false);
        let colors = TermColors::default();

        let plain = r.compute_bg_color(
            &style(AnsiColor::Named(NamedColor::Foreground), StyleFlags::DIM),
            &colors,
        );
        assert_eq!(plain, r.colors[NamedColor::Foreground as usize]);

        let inverted = r.compute_bg_color(
            &style(
                AnsiColor::Named(NamedColor::Foreground),
                StyleFlags::DIM | StyleFlags::INVERSE,
            ),
            &colors,
        );
        assert_eq!(inverted, r.colors[NamedColor::DimForeground as usize]);
    }

    /// `draw-bold-text-with-light-colors` is a rule about text: it
    /// applies to a background only once INVERSE made it the text.
    #[test]
    fn bold_brightens_an_indexed_background_only_under_inverse() {
        let r = renderer(true);
        let colors = TermColors::default();

        let plain =
            r.compute_bg_color(&style(AnsiColor::Indexed(1), StyleFlags::BOLD), &colors);
        assert_eq!(plain, r.colors[1]);

        let inverted = r.compute_bg_color(
            &style(
                AnsiColor::Indexed(1),
                StyleFlags::BOLD | StyleFlags::INVERSE,
            ),
            &colors,
        );
        assert_eq!(inverted, r.colors[9]);
    }
}

#[cfg(test)]
mod hint_tooltip_tests {
    use super::elide_tail;

    /// Fixed-width stand-in for the shaper: every char is 10 logical px.
    fn measure(s: &str) -> f32 {
        s.chars().count() as f32 * 10.0
    }

    #[test]
    fn text_that_fits_is_returned_untouched() {
        assert_eq!(
            elide_tail("https://example.com", 1000.0, measure),
            "https://example.com",
        );
    }

    /// The head survives, so the scheme and host stay readable. At 100px
    /// exactly ten chars fit, nine of them real plus the ellipsis.
    #[test]
    fn long_text_keeps_its_head_and_fits_the_budget() {
        let out = elide_tail("https://example.com/a/very/long/path", 100.0, measure);
        assert_eq!(out, "https://e\u{2026}");
        assert!(measure(&out) <= 100.0);
    }

    /// Off-by-one guard on the binary search: the result must be the
    /// longest prefix that fits, never one char short or one over.
    /// With every char 10px wide the expected length is exact: a
    /// budget of `n` chars fits `n - 1` real chars plus the ellipsis,
    /// until the whole 24-char text fits untouched.
    #[test]
    fn truncation_takes_the_longest_prefix_that_fits() {
        let text = "https://example.com/path";
        for budget in 1..40usize {
            let out = elide_tail(text, budget as f32 * 10.0, measure);
            assert_eq!(
                out.chars().count(),
                budget.min(text.chars().count()),
                "budget {budget} produced {out:?}",
            );
        }
    }

    /// Nothing fits: an ellipsis alone, not a panic and not an empty pill.
    #[test]
    fn zero_budget_yields_just_an_ellipsis() {
        assert_eq!(elide_tail("abc", 0.0, measure), "\u{2026}");
    }

    /// Slicing is by char, not byte, so multibyte text cannot panic.
    #[test]
    fn multibyte_text_is_sliced_on_char_boundaries() {
        assert_eq!(elide_tail("héllo wörld", 40.0, measure), "hél\u{2026}");
    }
}

/// End-to-end guards for `rio_grid::cell_bg` driven through the
/// `Renderer` palette impl. These moved out of `grid_emit` when it
/// became the standalone `rio-grid` crate (which can't depend on the
/// frontend `Renderer`); they live here now that `Renderer` provides
/// the `GridPalette` the emit code needs.
#[cfg(test)]
mod grid_cell_bg_tests {
    use super::*;
    use rio_backend::config::colors::ColorRgb;
    use rio_backend::crosswords::square::Square;

    /// End-to-end guard for the tmux faint-text regression: tmux
    /// re-emits the pane's OSC 11 as an explicit SGR 48 on every cell,
    /// so a faint cell arrived as `Spec(bg) + DIM` and was painted at
    /// `bg * DIM_FACTOR`, a dark block against the field around it.
    #[test]
    fn dim_cell_paints_its_explicit_background_unchanged() {
        let renderer = Renderer::new(&Config::default());
        let colors = TermColors::default();
        let sq = Square::from_char('x');
        let bg = ColorRgb {
            r: 0x28,
            g: 0x2c,
            b: 0x34,
        };
        let style = CellStyle {
            bg: AnsiColor::Spec(bg),
            ..CellStyle::default()
        };

        let plain = rio_grid::cell_bg(sq, style, &renderer, &colors);
        let dimmed = rio_grid::cell_bg(
            sq,
            CellStyle {
                flags: StyleFlags::DIM,
                ..style
            },
            &renderer,
            &colors,
        );

        assert_eq!(plain, [0x28, 0x2c, 0x34, 255]);
        assert_eq!(dimmed, plain);
    }

    /// A faint cell that never had its background set still paints
    /// nothing, so window transparency keeps showing through.
    #[test]
    fn dim_cell_with_default_background_stays_unpainted() {
        let renderer = Renderer::new(&Config::default());
        let colors = TermColors::default();
        let style = CellStyle {
            flags: StyleFlags::DIM,
            ..CellStyle::default()
        };

        assert_eq!(
            rio_grid::cell_bg(Square::from_char('x'), style, &renderer, &colors),
            [0, 0, 0, 0]
        );
    }
}
