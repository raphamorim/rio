//! Experimental cell strokes, independent of glyph content and foreground color.
//!
//! Both axes use cell-width units so horizontal and vertical strokes have equal
//! physical thickness. Layers order strokes against text in the owning row;
//! centered strokes can spill into adjacent rows, whose draw order still wins.

use super::{AtlasSlot, CellText, Column, ExtrasMap, GlyphKey, GridRenderer, PreeditRow};
use super::{RasterizedGlyph, Row, Square};
use rio_backend::ansi::border_protocol::{BorderStroke, CellBorders};

// Separate from drawable, cursor, decoration, and registered-glyph atlas keys.
const BORDER_FONT_ID: u32 = 0xFFFF_FC00;

#[allow(clippy::too_many_arguments)]
pub(super) fn emit(
    row: &Row<Square>,
    cols: usize,
    y: u16,
    extras: &ExtrasMap,
    grid: &mut GridRenderer,
    cell_w: u32,
    cell_h: u32,
    layer: u8,
    preedit: Option<&PreeditRow<'_>>,
    output: &mut Vec<CellText>,
) {
    if !row.has_extras {
        return;
    }
    for x in 0..cols {
        let square = row[Column(x)];
        if preedit.is_some_and(|p| p.suppresses(square, x)) {
            continue;
        }
        let Some(borders) = cell_borders(square, extras) else {
            continue;
        };
        for (segment, stroke) in borders.iter().enumerate() {
            let Some(stroke) = stroke.filter(|stroke| stroke.layer == layer) else {
                continue;
            };
            let rect = geometry(segment, stroke, cell_w, cell_h);
            let Some(slot) = ensure_rectangle(grid, rect.width, rect.height) else {
                continue;
            };
            output.push(CellText {
                glyph_pos: [slot.x as u32, slot.y as u32],
                glyph_size: [slot.w as u32, slot.h as u32],
                bearings: [rect.x as i16, (cell_h as i32 - rect.y) as i16],
                grid_pos: [x as u16, y],
                color: [stroke.color[0], stroke.color[1], stroke.color[2], 255],
                atlas: CellText::ATLAS_GRAYSCALE,
                // Preserve the explicitly requested color, including under the cursor.
                bools: CellText::BOOL_NO_MIN_CONTRAST | CellText::BOOL_IS_CURSOR_GLYPH,
                page: slot.page,
                _pad: 0,
            });
        }
    }
}

fn cell_borders(square: Square, extras: &ExtrasMap) -> Option<&CellBorders> {
    square
        .extras_id_checked()
        .and_then(|id| extras.get(&id))
        .and_then(|extra| extra.borders.as_deref())
}

#[derive(Debug, PartialEq, Eq)]
struct Rectangle {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn geometry(segment: usize, stroke: BorderStroke, cell_w: u32, cell_h: u32) -> Rectangle {
    let thickness = ((cell_w as f32 * stroke.width as f32 / 256.0).round() as u32).max(1);
    let centered = stroke.placement == 1;
    let half = thickness as i32 / 2;
    let edge_inset = if centered { half } else { thickness as i32 };
    let mut rect = Rectangle {
        x: 0,
        y: 0,
        width: cell_w,
        height: thickness,
    };
    match segment {
        0 => rect.y = if centered { -half } else { 0 },
        1 => {
            rect.x = cell_w as i32 - edge_inset;
            rect.width = thickness;
            rect.height = cell_h;
        }
        2 => rect.y = cell_h as i32 - edge_inset,
        3 => {
            rect.x = if centered { -half } else { 0 };
            rect.width = thickness;
            rect.height = cell_h;
        }
        4 => rect.y = cell_h as i32 / 2 - half,
        5 => {
            rect.x = cell_w as i32 / 2 - half;
            rect.width = thickness;
            rect.height = cell_h;
        }
        // Each arm includes the whole center square. This makes adjoining
        // perpendicular arms meet without a missing outside-corner pixel.
        6 => {
            rect.y = cell_h as i32 / 2 - half;
            rect.width = (cell_w as i32 / 2 - half) as u32 + thickness;
        }
        7 => {
            rect.x = cell_w as i32 / 2 - half;
            rect.y = cell_h as i32 / 2 - half;
            rect.width = cell_w - rect.x as u32;
        }
        8 => {
            rect.x = cell_w as i32 / 2 - half;
            rect.width = thickness;
            rect.height = (cell_h as i32 / 2 - half) as u32 + thickness;
        }
        9 => {
            rect.x = cell_w as i32 / 2 - half;
            rect.y = cell_h as i32 / 2 - half;
            rect.width = thickness;
            rect.height = cell_h - rect.y as u32;
        }
        _ => unreachable!("ten border segments"),
    }
    rect
}

fn ensure_rectangle(
    grid: &mut GridRenderer,
    width: u32,
    height: u32,
) -> Option<AtlasSlot> {
    let width = u16::try_from(width).ok()?;
    let height = u16::try_from(height).ok()?;
    let key = GlyphKey {
        font_id: BORDER_FONT_ID,
        glyph_id: u32::from(width) | (u32::from(height) << 16),
        size_bucket: 0,
    };
    if let Some(slot) = grid.lookup_glyph(key) {
        return Some(slot);
    }
    let bytes = vec![255; usize::from(width) * usize::from(height)];
    grid.insert_glyph(
        key,
        RasterizedGlyph {
            width,
            height,
            bearing_x: 0,
            bearing_y: 0,
            bytes: &bytes,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(width: u8, placement: u8) -> BorderStroke {
        BorderStroke {
            color: [31, 127, 255],
            width,
            placement,
            layer: 0,
        }
    }

    #[test]
    fn background_payload_cannot_alias_border_extras() {
        let mut square = Square::default();
        square.set_bg_rgb(255, 127, 63);
        let alias = square
            .extras_id()
            .expect("RGB payload overlaps extras bits");
        let mut extras = ExtrasMap::default();
        extras.insert(
            alias,
            super::super::Extras {
                borders: Some(std::sync::Arc::new([Some(stroke(16, 0)); 10])),
                ..Default::default()
            },
        );
        assert!(cell_borders(square, &extras).is_none());
        let mut text = Square::from_char(' ');
        text.set_extras_id(Some(alias));
        assert!(cell_borders(text, &extras).is_some());
    }

    #[test]
    fn both_axes_have_equal_physical_thickness() {
        let horizontal = geometry(0, stroke(32, 0), 16, 36);
        let vertical = geometry(3, stroke(32, 0), 16, 36);
        assert_eq!(horizontal.height, 2);
        assert_eq!(vertical.width, horizontal.height);
        assert_eq!(geometry(0, stroke(1, 0), 8, 18).height, 1);
    }

    #[test]
    fn inside_edges_stay_inside_cell() {
        for segment in 0..10 {
            let rect = geometry(segment, stroke(32, 0), 16, 36);
            assert!(rect.x >= 0 && rect.y >= 0);
            assert!(rect.x + rect.width as i32 <= 16);
            assert!(rect.y + rect.height as i32 <= 36);
        }
    }

    fn contains(rect: &Rectangle, x: i32, y: i32) -> bool {
        x >= rect.x
            && x < rect.x + rect.width as i32
            && y >= rect.y
            && y < rect.y + rect.height as i32
    }

    #[test]
    fn opposite_arms_cover_full_center_lines_at_odd_sizes() {
        for (w, h, width) in [(16, 36, 32), (17, 35, 32), (23, 47, 32), (9, 19, 1)] {
            for (first, second, full) in [(6, 7, 4), (8, 9, 5)] {
                let first = geometry(first, stroke(width, 0), w, h);
                let second = geometry(second, stroke(width, 0), w, h);
                let full = geometry(full, stroke(width, 0), w, h);
                for y in -1..=h as i32 {
                    for x in -1..=w as i32 {
                        assert_eq!(
                            contains(&first, x, y) || contains(&second, x, y),
                            contains(&full, x, y)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn perpendicular_arms_make_square_corners_without_spurs() {
        for (w, h) in [(16, 36), (17, 35), (23, 47)] {
            let horizontal = geometry(4, stroke(32, 0), w, h);
            let vertical = geometry(5, stroke(32, 0), w, h);
            for (horizontal_arm, vertical_arm) in [(6, 8), (6, 9), (7, 8), (7, 9)] {
                let a = geometry(horizontal_arm, stroke(32, 0), w, h);
                let b = geometry(vertical_arm, stroke(32, 0), w, h);
                for y in 0..h as i32 {
                    for x in 0..w as i32 {
                        let on_horizontal = contains(&horizontal, x, y)
                            && if horizontal_arm == 6 {
                                x < vertical.x + vertical.width as i32
                            } else {
                                x >= vertical.x
                            };
                        let on_vertical = contains(&vertical, x, y)
                            && if vertical_arm == 8 {
                                y < horizontal.y + horizontal.height as i32
                            } else {
                                y >= horizontal.y
                            };
                        assert_eq!(
                            contains(&a, x, y) || contains(&b, x, y),
                            on_horizontal || on_vertical
                        );
                        if contains(&horizontal, x, y) && contains(&vertical, x, y) {
                            assert!(contains(&a, x, y) && contains(&b, x, y));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn centered_edges_overlap_and_cross_bisects_cell() {
        assert_eq!(geometry(0, stroke(32, 1), 16, 36).y, -1);
        assert_eq!(geometry(1, stroke(32, 1), 16, 36).x, 15);
        assert_eq!(geometry(2, stroke(32, 1), 16, 36).y, 35);
        assert_eq!(geometry(3, stroke(32, 1), 16, 36).x, -1);
        assert_eq!(geometry(4, stroke(32, 0), 16, 36).y, 17);
        assert_eq!(geometry(5, stroke(32, 0), 16, 36).x, 7);
    }
}
