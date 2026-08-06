use crate::{Listener, Surface};
use rio_vt::ansi::graphics::{
    kitty_overlay_geometry, KittyOverlayGeometry, KittyPlacement, OverlayViewport,
    VirtualPlacement,
};
use rio_vt::ansi::kitty_virtual::{
    compute_run_geometry, IncompletePlacement, PlaceholderRun, PLACEHOLDER,
};
use rio_vt::config::colors::{AnsiColor, ColorRgb};
use rio_vt::crosswords::grid::row::Row;
use rio_vt::crosswords::grid::Dimensions;
use rio_vt::crosswords::pos::Column;
use rio_vt::crosswords::square::{ContentTag, Extras, Square};
use rio_vt::crosswords::style::Style;
use rio_vt::crosswords::Crosswords;
use rio_vt::event::sync::FairMutex;
use rio_vt::event::TerminalDamage;
use rustc_hash::FxHashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewportSelection {
    pub start_line: u16,
    pub start_col: u16,
    pub end_line: u16,
    pub end_col: u16,
    pub is_block: bool,
}

/// Virtual placements have no z of their own; kitty draws them under
/// text unless the application says otherwise, and rio's renderer pins
/// them there (see `frontends/rioterm/src/renderer/mod.rs`).
const VIRTUAL_Z_INDEX: i32 = -1;

/// One drawable kitty item: a direct overlay placement, or one row-run
/// of U+10EEEE placeholder cells from a virtual placement (`U=1`, what
/// `kitten icat --transfer-mode` emits under multiplexers).
enum KittyEntry {
    Direct {
        placement: KittyPlacement,
        image_width: usize,
        image_height: usize,
    },
    Virtual {
        run: PlaceholderRun,
        line: usize,
        start_col: usize,
        placement: VirtualPlacement,
        image_width: usize,
        image_height: usize,
    },
}

impl KittyEntry {
    fn z_index(&self) -> i32 {
        match self {
            KittyEntry::Direct { placement, .. } => placement.z_index,
            KittyEntry::Virtual { .. } => VIRTUAL_Z_INDEX,
        }
    }
}

pub struct RenderState {
    terminal: Arc<FairMutex<Crosswords<Listener>>>,
    rows: Vec<Row<Square>>,
    styles: Vec<Style>,
    extras: FxHashMap<u16, Extras>,
    columns: usize,
    cursor_line: usize,
    cursor_column: usize,
    display_offset: usize,
    selection: Option<ViewportSelection>,
    history_size: i64,
    alt_screen: bool,
    /// Kitty graphics placements (direct overlays and virtual
    /// placeholder runs), captured under the same lock as the grid
    /// snapshot and sorted by z-index, lowest first.
    kitty: Vec<KittyEntry>,
    /// Baseline for image change stamps (Instants aren't representable
    /// over the C ABI; nanoseconds relative to this are).
    epoch: std::time::Instant,
}

impl RenderState {
    pub fn new(surface: &Surface) -> Self {
        let terminal = surface.terminal();
        let columns = {
            let term = terminal.lock();
            term.grid.columns()
        };
        Self {
            terminal,
            rows: Vec::new(),
            styles: Vec::new(),
            extras: FxHashMap::default(),
            columns,
            cursor_line: 0,
            cursor_column: 0,
            display_offset: 0,
            selection: None,
            history_size: 0,
            alt_screen: false,
            kitty: Vec::new(),
            epoch: std::time::Instant::now(),
        }
    }

    pub fn update(&mut self) {
        let mut term = self.terminal.lock();
        let damage = if self.rows.is_empty() {
            TerminalDamage::Full
        } else {
            match term.peek_damage_event() {
                Some(damage) => damage,
                None => TerminalDamage::Full,
            }
        };
        term.snapshot_visible(
            &damage,
            self.columns,
            &mut self.rows,
            &mut self.styles,
            &mut self.extras,
        );
        term.reset_damage();
        term.damage_event_in_flight = false;
        self.columns = term.grid.columns();
        self.display_offset = term.display_offset();
        // Kitty dest_rows live in absolute row space: rows evicted from
        // the ring still count (rio-vt anchors placements the same way),
        // so a full scrollback must not shift placements off-screen.
        self.history_size = term.lines_evicted() as i64 + term.history_size() as i64;
        self.alt_screen = term
            .mode()
            .contains(rio_vt::crosswords::Mode::ALT_SCREEN);
        self.kitty.clear();
        for placement in term.graphics.kitty_placements.values() {
            if let Some(image) = term.graphics.get_kitty_image(placement.image_id) {
                self.kitty.push(KittyEntry::Direct {
                    placement: placement.clone(),
                    image_width: image.data.width,
                    image_height: image.data.height,
                });
            }
        }
        let virtual_runs = self.collect_virtual_runs(&term);
        self.kitty.extend(virtual_runs);
        // Under-background placements (z < i32::MIN / 2) first, then
        // under-text (z < 0), then over-text: drawing in order layers
        // correctly, and the host can split the list at those bounds.
        self.kitty.sort_by_key(KittyEntry::z_index);
        let cursor = term.cursor();
        self.cursor_line = cursor.pos.row.0.max(0) as usize;
        self.cursor_column = cursor.pos.col.0;
        self.selection = term
            .selection
            .as_ref()
            .and_then(|selection| selection.to_range(&term))
            .and_then(|range| {
                let offset = term.display_offset() as i32;
                let lines = self.rows.len() as i32;
                let start = range.start.row.0 + offset;
                let end = range.end.row.0 + offset;
                if end < 0 || start >= lines {
                    return None;
                }
                let clamped_start = start.max(0);
                let clamped_end = end.min(lines - 1);
                Some(ViewportSelection {
                    start_line: clamped_start as u16,
                    start_col: if start < 0 {
                        0
                    } else {
                        range.start.col.0 as u16
                    },
                    end_line: clamped_end as u16,
                    end_col: if end >= lines {
                        (self.columns.saturating_sub(1)) as u16
                    } else {
                        range.end.col.0 as u16
                    },
                    is_block: range.is_block,
                })
            });
    }

    pub fn columns(&self) -> usize {
        self.columns
    }

    pub fn lines(&self) -> usize {
        self.rows.len()
    }

    pub fn row_dirty(&self, line: usize) -> bool {
        self.rows.get(line).map(|row| row.dirty).unwrap_or(false)
    }

    pub fn reset_dirty(&mut self) {
        for row in &mut self.rows {
            row.dirty = false;
        }
    }

    pub fn square(&self, line: usize, column: usize) -> Option<&Square> {
        let row = self.rows.get(line)?;
        if column >= self.columns {
            return None;
        }
        Some(&row[Column(column)])
    }

    pub fn style_of(&self, square: &Square) -> Style {
        // Bg-only cells (erase fills, blank lines after `clear`) encode
        // their background inline instead of carrying a style id; reading
        // `style_id()` on one would misinterpret the color bits as an index.
        match square.content_tag() {
            ContentTag::Codepoint => self
                .styles
                .get(square.style_id() as usize)
                .copied()
                .unwrap_or_default(),
            ContentTag::BgPalette => Style {
                bg: AnsiColor::Indexed(square.bg_palette_index()),
                ..Style::default()
            },
            ContentTag::BgRgb => {
                let (r, g, b) = square.bg_rgb();
                Style {
                    bg: AnsiColor::Spec(ColorRgb { r, g, b }),
                    ..Style::default()
                }
            }
        }
    }

    pub fn styles(&self) -> &[Style] {
        &self.styles
    }

    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor_line, self.cursor_column)
    }

    pub fn display_offset(&self) -> usize {
        self.display_offset
    }

    /// Whether the alternate screen (full-screen TUIs) is active. Hosts
    /// use it to gate prompt-output affordances like table skins.
    pub fn alt_screen(&self) -> bool {
        self.alt_screen
    }

    pub fn selection(&self) -> Option<ViewportSelection> {
        self.selection
    }

    pub fn kitty_count(&self) -> usize {
        self.kitty.len()
    }

    /// Scan the snapshot rows for U+10EEEE placeholder cells and collapse
    /// them into row-runs (kitty virtual placements). One entry per run,
    /// resolved against the virtual placement registry and image store.
    /// The walk mirrors rioterm's renderer (itself mirroring ghostty's
    /// `PlacementIterator` in `graphics_unicode.zig`): a cell with missing
    /// diacritics inherits from its left neighbour, and consecutive cells
    /// showing sequential image columns collapse into one run.
    fn collect_virtual_runs(&self, term: &Crosswords<Listener>) -> Vec<KittyEntry> {
        let mut entries = Vec::new();
        let graphics = &term.graphics;
        if graphics.kitty_virtual_placements.is_empty() {
            return entries;
        }

        let flush = |entries: &mut Vec<KittyEntry>,
                         run: PlaceholderRun,
                         line: usize,
                         start_col: usize| {
            let placement = graphics
                .kitty_virtual_placements
                .get(&(run.image_id, run.placement_id))
                .or_else(|| graphics.kitty_virtual_placements.get(&(run.image_id, 0)));
            let Some(placement) = placement else { return };
            let Some(image) = graphics.get_kitty_image(run.image_id) else {
                return;
            };
            entries.push(KittyEntry::Virtual {
                run,
                line,
                start_col,
                placement: placement.clone(),
                image_width: image.data.width,
                image_height: image.data.height,
            });
        };

        for (line, row) in self.rows.iter().enumerate() {
            if !row.kitty_virtual_placeholder {
                continue;
            }
            let mut run: Option<(IncompletePlacement, usize)> = None;
            for (col, square) in row.inner.iter().enumerate() {
                if square.c() != PLACEHOLDER {
                    if let Some((p, start_col)) = run.take() {
                        flush(&mut entries, p.complete(), line, start_col);
                    }
                    continue;
                }

                let style = self.style_of(square);
                let combining: &[char] = square
                    .extras_id()
                    .and_then(|eid| self.extras.get(&eid))
                    .map(|extras| extras.zerowidth.as_slice())
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
                            flush(&mut entries, p.complete(), line, start_col);
                        }
                        // Default missing row/col on the FIRST cell of a
                        // run, so a later cell with an explicit column can
                        // still extend it sequentially.
                        if cell.row.is_none() {
                            cell.row = Some(0);
                        }
                        if cell.col.is_none() {
                            cell.col = Some(0);
                        }
                        run = Some((cell, col));
                    }
                }
            }
            if let Some((p, start_col)) = run {
                flush(&mut entries, p.complete(), line, start_col);
            }
        }
        entries
    }

    /// Viewport geometry for the placement at `index`, in pixels relative
    /// to the grid origin. `None` when it's scrolled out of view.
    pub fn kitty_geometry(
        &self,
        index: usize,
        cell_width: f32,
        cell_height: f32,
    ) -> Option<(u32, i32, KittyOverlayGeometry)> {
        match self.kitty.get(index)? {
            KittyEntry::Direct {
                placement,
                image_width,
                image_height,
            } => {
                let viewport = OverlayViewport {
                    cell_width,
                    cell_height,
                    origin_x: 0.0,
                    origin_y: 0.0,
                    history_size: self.history_size,
                    display_offset: self.display_offset as i64,
                    screen_lines: self.rows.len() as i64,
                };
                let geometry = kitty_overlay_geometry(
                    placement,
                    *image_width,
                    *image_height,
                    &viewport,
                )?;
                Some((placement.image_id, placement.z_index, geometry))
            }
            KittyEntry::Virtual {
                run,
                line,
                start_col,
                placement,
                image_width,
                image_height,
            } => {
                let geometry = compute_run_geometry(
                    run,
                    placement.columns,
                    placement.rows,
                    *image_width as u32,
                    *image_height as u32,
                    (placement.x, placement.y, placement.width, placement.height),
                    cell_width,
                    cell_height,
                    0.0,
                    0.0,
                    *line,
                    *start_col,
                )?;
                Some((
                    run.image_id,
                    VIRTUAL_Z_INDEX,
                    KittyOverlayGeometry {
                        x: geometry.x,
                        y: geometry.y,
                        width: geometry.width,
                        height: geometry.height,
                        source_rect: geometry.source_rect,
                    },
                ))
            }
        }
    }

    /// Pixel dimensions and a change stamp for a kitty image. The stamp
    /// changes on retransmission, so renderers can cache decoded bitmaps.
    pub fn kitty_image_info(&self, image_id: u32) -> Option<(usize, usize, u64)> {
        let term = self.terminal.lock();
        let image = term.graphics.get_kitty_image(image_id)?;
        let stamp = match image.transmission_time.checked_duration_since(self.epoch) {
            Some(duration) => duration.as_nanos() as u64,
            None => {
                u64::MAX
                    - self
                        .epoch
                        .duration_since(image.transmission_time)
                        .as_nanos() as u64
            }
        };
        Some((image.data.width, image.data.height, stamp))
    }

    /// Copy a kitty image into `buf` as RGBA8 (RGB sources gain an opaque
    /// alpha). Returns bytes written; 0 when unknown or `buf` is too small.
    pub fn kitty_image_rgba(&self, image_id: u32, buf: &mut [u8]) -> usize {
        let term = self.terminal.lock();
        let Some(image) = term.graphics.get_kitty_image(image_id) else {
            return 0;
        };
        let width = image.data.width;
        let height = image.data.height;
        let expected = width * height * 4;
        if buf.len() < expected {
            return 0;
        }
        use rio_graphics::ColorType;
        match image.data.color_type {
            ColorType::Rgba => {
                if image.data.pixels.len() < expected {
                    return 0;
                }
                buf[..expected].copy_from_slice(&image.data.pixels[..expected]);
            }
            ColorType::Rgb => {
                let source = width * height * 3;
                if image.data.pixels.len() < source {
                    return 0;
                }
                for i in 0..width * height {
                    buf[i * 4] = image.data.pixels[i * 3];
                    buf[i * 4 + 1] = image.data.pixels[i * 3 + 1];
                    buf[i * 4 + 2] = image.data.pixels[i * 3 + 2];
                    buf[i * 4 + 3] = 0xFF;
                }
            }
        }
        expected
    }

    pub fn text_row(&self, line: usize) -> String {
        let Some(row) = self.rows.get(line) else {
            return String::new();
        };
        let mut text = String::with_capacity(self.columns);
        for column in 0..self.columns {
            let square = row[Column(column)];
            // Bg-only and never-written cells have no codepoint; save them
            // as spaces so scrollback text doesn't accumulate NULs.
            let c = if square.is_bg_only() { ' ' } else { square.c() };
            text.push(if c == '\0' { ' ' } else { c });
        }
        text.trim_end().to_string()
    }
}
