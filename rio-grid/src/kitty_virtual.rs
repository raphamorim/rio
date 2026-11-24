//! Kitty virtual placements (Unicode placeholders) resolved into image
//! overlays, for any host that draws rio-grid rows.

use rio_backend::ansi::graphics::{StoredImage, VirtualPlacement};
use rio_backend::crosswords::grid::row::Row;
use rio_backend::crosswords::square::{Extras, Square};
use rio_backend::crosswords::style::Style;
use rustc_hash::FxHashMap;

/// What placeholder runs resolve against: the visible rows of a
/// snapshot and the terminal's virtual placements and images.
pub struct VirtualSources<'a> {
    pub rows: &'a [Row<Square>],
    pub row_styles: &'a [Vec<Style>],
    pub extras: &'a FxHashMap<u16, Extras>,
    pub placements: &'a FxHashMap<(u32, u32), VirtualPlacement>,
    pub images: &'a FxHashMap<u32, StoredImage>,
}

/// Scan visible rows for kitty Unicode-placeholder cells (U+10EEEE) and
/// push one `GraphicOverlay` per row-run. Implements four key behaviors
/// of the Kitty graphics Unicode-placeholder protocol:
///
/// 1. Per-row `kitty_virtual_placeholder` flag check skips rows
///    with no placeholders.
/// 2. Continuation rules: a cell with missing diacritics inherits
///    from the previous cell on the row (`canAppend`).
/// 3. Run aggregation: consecutive cells with same image / row /
///    sequential column collapse into one placement.
/// 4. Per-run source rect with aspect-fit + centering, which handles
///    partial visibility (placement scrolled half off-screen) and
///    cells that fall in the centering padding.
#[allow(clippy::too_many_arguments)]
pub fn push_virtual_placeholder_overlays(
    overlays: &mut Vec<rio_backend::sugarloaf::GraphicOverlay>,
    src: &VirtualSources<'_>,
    image_key: &dyn Fn(u32) -> u64,
    origin_x: f32,
    origin_y: f32,
    cell_width: f32,
    cell_height: f32,
    clip: (f32, f32, f32, f32),
) {
    use rio_backend::ansi::kitty_virtual::{
        IncompletePlacement, PlaceholderRun, PLACEHOLDER,
    };

    // Below text by default for virtual placements; apps that
    // want them above the glyphs set z-index explicitly via the
    // graphics protocol.
    const VIRTUAL_Z_INDEX: i32 = -1;

    for (line_idx, row) in src.rows.iter().enumerate() {
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
        let row_styles = src
            .row_styles
            .get(line_idx)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        for (col_idx, square) in row.inner.iter().enumerate() {
            if square.c() != PLACEHOLDER {
                if let Some((p, start_col)) = run.take() {
                    flush_run(
                        overlays,
                        src,
                        image_key,
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

            let style = crate::resolve_style(row_styles, col_idx);
            let combining: &[char] = square
                .extras_id()
                .and_then(|eid| src.extras.get(&eid))
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
                            src,
                            image_key,
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
                src,
                image_key,
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
        src: &VirtualSources<'_>,
        image_key: &dyn Fn(u32) -> u64,
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
            src.placements,
            run.image_id,
            run.placement_id,
        );
        let vp = match vp {
            Some(v) => v,
            None => return,
        };
        let img = match src.images.get(&run.image_id) {
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
            image_id: image_key(run.image_id),
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
