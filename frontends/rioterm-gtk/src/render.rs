//! Draws a terminal the way Rio does: rows snapshotted from `Crosswords`,
//! emitted by rio-grid into Sugarloaf's CPU grid, on a frame the widget
//! hands GTK.

use crate::session::Terminal;
use rio_backend::config::Config;
use rio_grid::{HintTag, Palette, RowHint};
use rio_vt::ansi::graphics::UpdateQueues;
use rio_vt::config::colors::term::TermColors;
use rio_vt::crosswords::grid::row::Row;
use rio_vt::crosswords::square::{Extras, Square};
use rio_vt::crosswords::style::Style;
use rio_vt::event::TerminalDamage;
use rio_vt::selection::SelectionRange;
use rustc_hash::FxHashMap;
use sugarloaf::context::cpu::CpuFrame;
use sugarloaf::font::fonts::SugarloafFonts;
use sugarloaf::font::FontLibrary;
use sugarloaf::grid::cell::{CellBg, CellText};
use sugarloaf::grid::{GridRenderer, GridUniforms};
use sugarloaf::layout::RootStyle;
use sugarloaf::{GraphicDataEntry, GraphicOverlay, Sugarloaf, SugarloafWindowSize};

/// A highlighted run on a visible row: `line`, columns `lo..=hi`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: u16,
    pub lo: u16,
    pub hi: u16,
    pub tag: HintTag,
}

pub struct Inputs<'a> {
    pub cursor: [u8; 3],
    pub focused: bool,
    pub blinking: bool,
    pub blink_visible: bool,
    pub preedit: Option<&'a str>,
    pub spans: &'a [Span],
}

pub struct Renderer {
    sugarloaf: Sugarloaf<'static>,
    grid: GridRenderer,
    rasterizer: rio_grid::GridGlyphRasterizer,
    pub palette: Palette,
    fonts: SugarloafFonts,
    /// The config's font size, before any zoom.
    base_size: f32,
    font_size: f32,
    line_height: f32,
    scale: f32,
    /// Top, right, bottom, left, physical pixels.
    padding: [f32; 4],
    cols: usize,
    lines: usize,
    rows: Vec<Row<Square>>,
    styles: Vec<Vec<Style>>,
    extras: FxHashMap<u16, Extras>,
    term_colors: TermColors,
    shaped_cursor: Option<(usize, usize)>,
    had_overlay: bool,
    images: Images,
    bg: Vec<CellBg>,
    fg: Vec<CellText>,
    hints: Vec<RowHint>,
}

impl Renderer {
    pub fn new(config: &Config, scale: f32) -> Self {
        let fonts = config.fonts.clone();
        let font_size = fonts.size;
        let line_height = config.line_height;
        let (font_library, _) = FontLibrary::new(fonts.clone());
        let mut sugarloaf = Sugarloaf::new_offscreen(
            SugarloafWindowSize {
                width: 1.0,
                height: 1.0,
            },
            scale,
            &fonts.features,
            &font_library,
            RootStyle::new(scale, font_size, line_height),
        );
        sugarloaf.set_background_color(Some(background(config)));
        let grid = GridRenderer::new(&sugarloaf.ctx, 1, 1);
        Self {
            sugarloaf,
            grid,
            rasterizer: rio_grid::GridGlyphRasterizer::new(),
            palette: palette(config),
            fonts,
            base_size: font_size,
            font_size,
            line_height,
            scale,
            padding: [0.0; 4],
            cols: 1,
            lines: 1,
            rows: Vec::new(),
            styles: Vec::new(),
            extras: FxHashMap::default(),
            term_colors: TermColors::default(),
            shaped_cursor: None,
            had_overlay: false,
            images: Images::default(),
            bg: Vec::new(),
            fg: Vec::new(),
            hints: Vec::new(),
        }
    }

    /// Cell size in physical pixels.
    pub fn cell_size(&self) -> (f32, f32) {
        let (_, m) = self.sugarloaf.compute_cell_metrics(
            self.font_size,
            self.line_height,
            self.scale,
        );
        (m.cell_width as f32, m.cell_height as f32)
    }

    pub fn font_size(&self) -> f32 {
        self.font_size
    }

    pub fn set_scale(&mut self, scale: f32) {
        if scale == self.scale {
            return;
        }
        self.scale = scale;
        self.sugarloaf.rescale(scale);
        self.grid.clear_atlas();
        self.grid.request_full_rebuild();
    }

    pub fn set_font_size(&mut self, size: f32) {
        let size = size.clamp(4.0, 200.0);
        if size == self.font_size {
            return;
        }
        self.font_size = size;
        self.fonts.size = size;
        self.sugarloaf.style_mut().font_size = size;
        let (font_library, _) = FontLibrary::new(self.fonts.clone());
        self.sugarloaf.update_font(&font_library);
        self.rasterizer.clear_font_caches();
        self.sugarloaf.rescale(self.scale);
        self.grid.clear_atlas();
        self.grid.request_full_rebuild();
    }

    /// A new config: colors, and fonts when they changed (which resets a
    /// zoom).
    pub fn reconfigure(&mut self, config: &Config) {
        self.palette = palette(config);
        self.sugarloaf
            .set_background_color(Some(background(config)));
        let mut fonts = config.fonts.clone();
        fonts.size = self.fonts.size;
        if fonts != self.fonts
            || config.line_height != self.line_height
            || self.base_size != config.fonts.size
        {
            self.fonts = config.fonts.clone();
            self.base_size = config.fonts.size;
            self.font_size = config.fonts.size;
            self.line_height = config.line_height;
            let style = self.sugarloaf.style_mut();
            style.font_size = self.font_size;
            style.line_height = self.line_height.max(1.0);
            let (font_library, _) = FontLibrary::new(self.fonts.clone());
            self.sugarloaf.update_font(&font_library);
            self.rasterizer.clear_font_caches();
            self.sugarloaf.rescale(self.scale);
            self.grid.clear_atlas();
        }
        self.grid.request_full_rebuild();
    }

    /// The frame size in physical pixels, its padding, and the grid it
    /// holds.
    pub fn resize(
        &mut self,
        width: u32,
        height: u32,
        padding: [f32; 4],
        cols: usize,
        lines: usize,
    ) {
        let window = self.sugarloaf.window_size();
        if window.width as u32 != width || window.height as u32 != height {
            self.sugarloaf.resize(width, height);
        }
        self.padding = padding;
        if (cols, lines) != (self.cols, self.lines) {
            self.cols = cols;
            self.lines = lines;
            self.grid.resize(cols as u32, lines as u32);
            self.rows.clear();
        }
    }

    /// Image pixels the terminal sent: new ones in, dropped ones out.
    pub fn apply_graphics(&mut self, queues: UpdateQueues) {
        for data in queues.pending {
            let key = sugarloaf::atlas_image_key(data.id.get());
            self.sugarloaf
                .image_data
                .insert(key, GraphicDataEntry::from_graphic_data(data));
        }
        for key in queues.remove_queue {
            self.sugarloaf.remove_image(key);
        }
        for (image_id, data) in queues.pending_images {
            self.sugarloaf.image_data.insert(
                sugarloaf::kitty_image_key(image_id),
                GraphicDataEntry::from_graphic_data(data),
            );
        }
    }

    pub fn invalidate(&mut self) {
        self.grid.request_full_rebuild();
    }

    /// Snapshot what changed in `terminal` and draw it. The frame, or
    /// `None` when the terminal holds the frame for a synchronized update.
    pub fn draw(
        &mut self,
        terminal: &Terminal,
        id: usize,
        inputs: &Inputs<'_>,
    ) -> Option<CpuFrame<'_>> {
        let snapshot = self.snapshot(terminal)?;
        self.emit(id, &snapshot, inputs);
        self.sugarloaf.cpu_frame()
    }

    fn snapshot(&mut self, terminal: &Terminal) -> Option<Snapshot> {
        let mut term = terminal.lock();
        if term.synchronized_output() {
            if !term.sync_update_expired(rio_vt::time::Instant::now()) {
                term.damage_event_in_flight = false;
                return None;
            }
            term.end_sync_update();
        }
        let damage = if self.rows.is_empty() {
            TerminalDamage::Full
        } else {
            term.peek_damage_event().unwrap_or(TerminalDamage::Noop)
        };
        term.reset_damage();
        term.snapshot_visible(
            &damage,
            &mut self.rows,
            &mut self.styles,
            &mut self.extras,
        );
        term.damage_event_in_flight = false;
        self.term_colors = *term.colors();
        let cursor = term.cursor();
        let display_offset = term.display_offset();
        let columns = term.columns();
        let selection = term
            .selection
            .as_ref()
            .and_then(|selection| selection.to_range(&term));
        self.images.history = term.lines_evicted() as i64 + term.history_size() as i64;
        self.images.display_offset = display_offset as i64;
        let queues = term.graphics.take_queues();
        if term.graphics.kitty_graphics_dirty {
            let graphics = &term.graphics;
            self.images.virtual_placements = graphics.kitty_virtual_placements.clone();
            self.images.kitty_images = graphics.kitty_images.clone();
            let mut placements: Vec<_> = graphics
                .kitty_placements
                .values()
                .filter(|p| graphics.kitty_images.contains_key(&p.image_id))
                .cloned()
                .collect();
            placements.sort_by_key(|p| (p.z_index, p.image_id, p.placement_id));
            self.images.kitty_placements = placements;
            self.images.atlas_placements = graphics.atlas_placements.clone();
            term.graphics.kitty_graphics_dirty = false;
        }
        drop(term);
        if let Some(queues) = queues {
            self.apply_graphics(queues);
        }

        let line = cursor.pos.row.0.max(0) as usize;
        let col = cursor.pos.col.0;
        let visible = cursor.content != rio_vt::ansi::CursorShape::Hidden;
        let shaped = visible.then_some((line, col));
        if shaped != self.shaped_cursor {
            for (line, _) in self.shaped_cursor.into_iter().chain(shaped) {
                if let Some(row) = self.rows.get_mut(line) {
                    row.dirty = true;
                }
            }
            self.shaped_cursor = shaped;
        }
        Some(Snapshot {
            columns,
            cursor: (line, col),
            cursor_visible: visible,
            cursor_shape: cursor.content,
            display_offset,
            selection,
        })
    }

    fn emit(&mut self, id: usize, snap: &Snapshot, inputs: &Inputs<'_>) {
        let size_px = self.font_size * self.scale;
        let (cell_w, cell_h) = self.cell_size();
        let font_library = self.sugarloaf.font_library().clone();
        let cols = snap.columns;
        let rows = self.rows.len().min(self.lines);
        let preedit = inputs.preedit.filter(|p| !p.is_empty());

        let overlay = snap.selection.is_some() || !inputs.spans.is_empty();
        let full = self.grid.needs_full_rebuild() || overlay || self.had_overlay;
        self.had_overlay = overlay;
        if self.grid.needs_full_rebuild() {
            self.grid.mark_full_rebuild_done();
        }
        let cols_max = cols.saturating_sub(1) as u16;

        for y in 0..rows {
            let row = &self.rows[y];
            if !full && !row.dirty {
                continue;
            }
            let row_sel = rio_grid::row_selection_for(
                snap.selection,
                y,
                cols,
                snap.display_offset as i32,
            );
            self.hints.clear();
            for span in inputs.spans.iter().filter(|s| s.line as usize == y) {
                let (lo, hi) = (span.lo.min(cols_max), span.hi.min(cols_max));
                if lo <= hi {
                    self.hints.push(RowHint {
                        lo,
                        hi,
                        tag: span.tag,
                    });
                }
            }
            rio_grid::build_row_bg(
                row,
                cols,
                &self.styles[y],
                &self.palette,
                &self.term_colors,
                row_sel,
                &self.hints,
                None,
                &mut self.bg,
            );
            let cursor_col = (snap.cursor_visible && y == snap.cursor.0)
                .then_some(snap.cursor.1 as u16);
            rio_grid::build_row_fg(
                row,
                cols,
                y as u16,
                &self.styles[y],
                &self.extras,
                &self.palette,
                &self.term_colors,
                &mut self.rasterizer,
                &mut self.grid,
                size_px,
                cell_w,
                cell_h,
                row_sel,
                &self.hints,
                None,
                &font_library,
                id,
                cursor_col,
                &mut self.fg,
            );
            self.grid.write_row(y as u32, &self.bg, &self.fg);
        }
        for row in &mut self.rows {
            row.dirty = false;
        }

        let style = rio_grid::cursor_render_style(rio_grid::CursorRenderInputs {
            visible: snap.cursor_visible,
            focused: inputs.focused,
            blink_visible: inputs.blink_visible,
            blinking: inputs.blinking,
            preedit: preedit.is_some(),
            shape: snap.cursor_shape,
        });
        let (cursor_line, cursor_col) = snap.cursor;
        let mut block = None;
        let mut tail = None;
        if let Some(style) = style {
            let color = [inputs.cursor[0], inputs.cursor[1], inputs.cursor[2], 255];
            if let Some((is_block, cell)) = rio_grid::cursor_sprite_cell(
                &mut self.grid,
                style,
                cursor_col as u16,
                cursor_line as u16,
                color,
                cell_w.round().max(1.0) as u32,
                cell_h.round().max(1.0) as u32,
            ) {
                if is_block {
                    block = Some(cell);
                } else {
                    tail = Some(cell);
                }
            }
        }
        self.grid.set_cursor(block.as_slice(), tail.as_slice());

        let bg = self.palette.named.background.0;
        let cursor_color = inputs.cursor.map(|c| c as f32 / 255.0);
        let cursor_color = [cursor_color[0], cursor_color[1], cursor_color[2], 1.0];
        let (cursor_pos, cursor_fg, cursor_bg) =
            if matches!(style, Some(rio_grid::CursorRenderStyle::Block)) {
                ([cursor_col as u32, cursor_line as u32], bg, cursor_color)
            } else {
                ([u32::MAX; 2], [0.0; 4], [0.0; 4])
            };
        let window = self.sugarloaf.window_size();
        let uniforms = GridUniforms {
            projection: sugarloaf::components::core::orthographic_projection(
                window.width,
                window.height,
            ),
            grid_padding: self.padding,
            cursor_color: cursor_fg,
            cursor_bg_color: cursor_bg,
            cell_size: [cell_w, cell_h],
            grid_size: [cols as u32, rows as u32],
            cursor_pos,
            _pad_cursor: [0; 2],
            min_contrast: 0.0,
            flags: 0,
            padding_extend: 0,
            input_colorspace: self.sugarloaf.input_colorspace(),
        };

        if let Some(preedit) = preedit {
            if snap.cursor_visible && cursor_line < rows && cursor_col < cols {
                self.draw_preedit(
                    preedit,
                    cursor_line,
                    cursor_col,
                    cols,
                    cell_w,
                    cell_h,
                    cursor_color,
                );
            }
        }
        self.place_images(cell_w, cell_h, cols, rows);
        let mut grids = [(&mut self.grid, uniforms)];
        self.sugarloaf.render_with_grids(&mut grids);
    }

    /// This frame's image overlays: sixel and iTerm2 images under the
    /// text, kitty placements by z-index, clipped to the grid.
    fn place_images(&mut self, cell_w: f32, cell_h: f32, cols: usize, rows: usize) {
        use rio_vt::ansi::graphics::{
            atlas_overlay_geometry, clip_overlay_to_rect, kitty_overlay_geometry,
            OverlayViewport,
        };
        let images = &self.images;
        let any = !images.atlas_placements.is_empty()
            || !images.kitty_placements.is_empty()
            || !images.virtual_placements.is_empty();
        if !any {
            if images.placed {
                self.sugarloaf.clear_image_overlays_for(0);
                self.images.placed = false;
            }
            return;
        }
        let origin_x = self.padding[3].round();
        let origin_y = self.padding[0].round();
        let clip = (
            origin_x,
            origin_y,
            origin_x + cols as f32 * cell_w,
            origin_y + rows as f32 * cell_h,
        );
        let viewport = OverlayViewport {
            cell_width: cell_w,
            cell_height: cell_h,
            origin_x,
            origin_y,
            history_size: images.history,
            display_offset: images.display_offset,
            screen_lines: rows as i64,
        };
        let mut overlays = Vec::new();
        let mut push = |mut overlay: GraphicOverlay| {
            if clip_overlay_to_rect(&mut overlay, clip.0, clip.1, clip.2, clip.3) {
                overlays.push(overlay);
            }
        };
        for p in &images.atlas_placements {
            if let Some(g) = atlas_overlay_geometry(p, &viewport) {
                push(GraphicOverlay {
                    image_id: p.image_key,
                    x: g.x,
                    y: g.y,
                    width: g.width,
                    height: g.height,
                    z_index: -1,
                    source_rect: g.source_rect,
                });
            }
        }
        for p in &images.kitty_placements {
            let (width, height) = images
                .kitty_images
                .get(&p.image_id)
                .map_or((0, 0), |image| (image.data.width, image.data.height));
            if let Some(g) = kitty_overlay_geometry(p, width, height, &viewport) {
                push(GraphicOverlay {
                    image_id: sugarloaf::kitty_image_key(p.image_id),
                    x: g.x,
                    y: g.y,
                    width: g.width,
                    height: g.height,
                    z_index: p.z_index,
                    source_rect: g.source_rect,
                });
            }
        }
        if !images.virtual_placements.is_empty() {
            rio_grid::push_virtual_placeholder_overlays(
                &mut overlays,
                &rio_grid::VirtualSources {
                    rows: &self.rows,
                    row_styles: &self.styles,
                    extras: &self.extras,
                    placements: &images.virtual_placements,
                    images: &images.kitty_images,
                },
                &sugarloaf::kitty_image_key,
                origin_x,
                origin_y,
                cell_w,
                cell_h,
                clip,
            );
        }
        let slot = self.sugarloaf.image_overlays.entry(0).or_default();
        *slot = overlays;
        self.images.placed = true;
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_preedit(
        &mut self,
        preedit: &str,
        line: usize,
        col: usize,
        cols: usize,
        cell_w: f32,
        cell_h: f32,
        underline: [f32; 4],
    ) {
        let scale = self.scale.max(f32::MIN_POSITIVE);
        let (cell_w, cell_h) = (cell_w / scale, cell_h / scale);
        let bg = self.palette.named.background.0;
        let fg = self.palette.named.foreground;
        let opts = sugarloaf::text::DrawOpts {
            font_size: self.font_size,
            color: rio_grid::normalized_to_u8([bg[0], bg[1], bg[2], 1.0]),
            ..Default::default()
        };
        let width = self
            .sugarloaf
            .text_mut()
            .measure(preedit, &opts)
            .max(cell_w);
        let left = self.padding[3] / scale;
        let top = self.padding[0] / scale;
        let right = left + cols as f32 * cell_w;
        let mut x = left + col as f32 * cell_w;
        if x + width > right {
            x = (right - width).max(0.0);
        }
        let y = top + line as f32 * cell_h;
        self.sugarloaf.rect(None, x, y, width, cell_h, fg, 0.0, 0);
        let thickness = (cell_h * 0.09).max(1.0);
        self.sugarloaf.rect(
            None,
            x,
            y + cell_h - thickness,
            width,
            thickness,
            underline,
            0.0,
            0,
        );
        self.sugarloaf.text_mut().draw(x, y, preedit, &opts);
    }
}

fn palette(config: &Config) -> Palette {
    let mut palette = Palette::new(config.colors);
    palette.draw_bold_text_with_light_colors = config.draw_bold_text_with_light_colors;
    palette.ignore_selection_fg_color = config.ignore_selection_fg_color;
    palette.opacity_cells = config.window.opacity_cells;
    palette.cell_bg_alpha = (config.window.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    palette
}

/// The frame's background, with the window's opacity.
fn background(config: &Config) -> sugarloaf::Color {
    let mut color = config.colors.background.1;
    color.a = config.window.opacity.clamp(0.0, 1.0) as f64;
    color
}

#[derive(Default)]
struct Images {
    atlas_placements: Vec<rio_vt::ansi::graphics::AtlasPlacement>,
    kitty_placements: Vec<rio_vt::ansi::graphics::KittyPlacement>,
    kitty_images: FxHashMap<u32, rio_vt::ansi::graphics::StoredImage>,
    virtual_placements: FxHashMap<(u32, u32), rio_vt::ansi::graphics::VirtualPlacement>,
    history: i64,
    display_offset: i64,
    /// Overlays were pushed to the renderer last frame.
    placed: bool,
}

struct Snapshot {
    columns: usize,
    cursor: (usize, usize),
    cursor_visible: bool,
    cursor_shape: rio_vt::ansi::CursorShape,
    display_offset: usize,
    selection: Option<SelectionRange>,
}
