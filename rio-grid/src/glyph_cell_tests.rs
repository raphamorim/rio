// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use super::*;

fn cells_for_clusters(starts: &[u32], clusters: &[u32]) -> Vec<u16> {
    let glyphs: Vec<_> = clusters
        .iter()
        .enumerate()
        .map(|(id, &cluster)| ShapedGlyph {
            id: id as u16,
            x: 0.0,
            y: 0.0,
            advance: 10.0,
            cluster,
        })
        .collect();
    attribute_glyphs_to_cells(&glyphs, starts)
        .iter()
        .enumerate()
        .map(|(i, &(id, cell))| {
            assert_eq!(id, i as u16, "mapping must preserve glyph identity/order");
            cell
        })
        .collect()
}

#[test]
fn descending_clusters_keep_their_logical_cells() {
    // שלום in UTF-16 (CoreText) and UTF-8 (swash).
    assert_eq!(
        cells_for_clusters(&[0, 1, 2, 3], &[3, 2, 1, 0]),
        [3, 2, 1, 0]
    );
    assert_eq!(
        cells_for_clusters(&[0, 2, 4, 6], &[6, 4, 2, 0]),
        [3, 2, 1, 0]
    );
}

#[test]
fn mixed_direction_clusters_can_change_direction_more_than_once() {
    // LTR endpoints do not imply that the clusters between them are LTR.
    assert_eq!(
        cells_for_clusters(&[0, 1, 2, 3, 4, 5], &[0, 4, 3, 2, 1, 5]),
        [0, 4, 3, 2, 1, 5]
    );
    // Conversely, an RTL span can contain an ascending numeric subrun.
    assert_eq!(
        cells_for_clusters(&[0, 1, 2, 3, 4, 5], &[5, 3, 4, 2, 0, 1]),
        [5, 3, 4, 2, 0, 1]
    );
}

#[test]
fn descending_marks_and_ligatures_stay_with_their_source_cell() {
    // Cell 0 contains a base and two marks, cell 1 a surrogate pair,
    // cell 2 another base. Marks need not point at the cell's first unit.
    assert_eq!(
        cells_for_clusters(&[0, 3, 5], &[5, 3, 2, 0, 1]),
        [2, 1, 0, 0, 0]
    );
    // A ligature can skip a cell start, and several glyphs can share it.
    assert_eq!(
        cells_for_clusters(&[0, 1, 2, 3], &[3, 1, 1, 0]),
        [3, 1, 1, 0]
    );
}

#[test]
fn ascending_clusters_and_empty_runs_are_unchanged() {
    assert_eq!(
        cells_for_clusters(&[0, 1, 2, 3], &[0, 1, 2, 3]),
        [0, 1, 2, 3]
    );
    assert_eq!(cells_for_clusters(&[0, 3, 5], &[0, 1, 3, 5]), [0, 0, 1, 2]);
    assert!(cells_for_clusters(&[], &[]).is_empty());
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use rio_backend::config::colors::{ColorArray, Colors};
    use rio_backend::sugarloaf::font::constants::FONT_CASCADIA_CODE_NF;
    use rio_backend::sugarloaf::font::macos::{
        discover_fallback, font_has_char, shape_text_utf16, FontHandle,
    };
    use rio_backend::sugarloaf::grid::cpu::CpuGridRenderer;

    #[test]
    fn coretext_hebrew_and_mixed_runs_map_back_to_source_characters() {
        let primary = FontHandle::from_bytes(FONT_CASCADIA_CODE_NF).unwrap();
        let font = discover_fallback(&primary, 'ש').expect("macOS Hebrew fallback");
        for text in ["שלום", "abcשלום123", "שלום123abc"] {
            let starts: Vec<_> = (0..text.chars().count() as u32).collect();
            let chars: Vec<_> = text.chars().collect();
            assert!(chars.iter().all(|&ch| font_has_char(&font, ch)));
            let utf16: Vec<_> = text.encode_utf16().collect();
            let glyphs: Vec<_> = shape_text_utf16(&font, &utf16, 28.0)
                .iter()
                .map(|g| ShapedGlyph {
                    id: g.id,
                    x: g.x,
                    y: g.y,
                    advance: g.advance,
                    cluster: g.cluster,
                })
                .collect();
            assert_eq!(glyphs.len(), chars.len());
            assert!(glyphs.windows(2).any(|g| g[0].cluster > g[1].cluster));
            let mapped = attribute_glyphs_to_cells(&glyphs, &starts);
            let mut cells: Vec<_> = mapped.iter().map(|&(_, cell)| cell).collect();
            cells.sort_unstable();
            assert_eq!(cells, (0..chars.len() as u16).collect::<Vec<_>>());
            for (id, cell) in mapped {
                // Hebrew letters have no contextual forms here: shaping
                // the source character alone independently identifies it.
                let ch: Vec<_> =
                    chars[cell as usize].to_string().encode_utf16().collect();
                assert_eq!(id, shape_text_utf16(&font, &ch, 28.0)[0].id);
            }
        }
    }

    #[test]
    fn coretext_arabic_persian_and_marks_map_to_containing_cells() {
        let primary = FontHandle::from_bytes(FONT_CASCADIA_CODE_NF).unwrap();
        for (base, cells) in [
            ('م', vec!["م", "ر", "ح", "ب", "ا"]),
            ('ف', vec!["ف", "ا", "ر", "س", "ی"]),
            ('م', vec!["مَ", "رْ", "حَ", "بً", "ا"]),
            ('ש', vec!["שָׁ", "ל", "וֹ", "ם"]),
            ('م', vec!["a", "b", "م", "ر", "ح", "ب", "ا", "1", "2"]),
            ('ف', vec!["ف", "ا", "ر", "س", "ی", "1", "2", "a", "b"]),
            // Lam-alef may form a ligature spanning two terminal cells.
            ('س', vec!["س", "ل", "ا", "م"]),
        ] {
            let font = discover_fallback(&primary, base).expect("macOS RTL fallback");
            assert!(font_has_char(&font, base));
            let mut starts = Vec::new();
            let mut utf16 = Vec::new();
            for cell in &cells {
                starts.push(utf16.len() as u32);
                utf16.extend(cell.encode_utf16());
            }
            let glyphs: Vec<_> = shape_text_utf16(&font, &utf16, 28.0)
                .iter()
                .map(|g| ShapedGlyph {
                    id: g.id,
                    x: g.x,
                    y: g.y,
                    advance: g.advance,
                    cluster: g.cluster,
                })
                .collect();
            assert!(!glyphs.is_empty());
            assert!(glyphs.iter().all(|g| g.id != 0));
            assert!(glyphs.windows(2).any(|g| g[0].cluster > g[1].cluster));
            for (g, (id, cell)) in glyphs
                .iter()
                .zip(attribute_glyphs_to_cells(&glyphs, &starts))
            {
                assert_eq!(id, g.id);
                let end = starts
                    .get(cell as usize + 1)
                    .copied()
                    .unwrap_or(utf16.len() as u32);
                assert!(
                    starts[cell as usize] <= g.cluster && g.cluster < end,
                    "glyph {g:?} belongs outside cell {cell} in {cells:?}"
                );
            }
        }
    }

    struct TestPalette(Colors);

    impl GridPalette for TestPalette {
        fn named_colors(&self) -> &Colors {
            &self.0
        }
        fn compute_color(
            &self,
            color: &AnsiColor,
            _: StyleFlags,
            _: &TermColors,
        ) -> ColorArray {
            match color {
                AnsiColor::Indexed(index) => [*index as f32 / 255.0, 0.0, 0.0, 1.0],
                _ => self.0.foreground,
            }
        }
        fn compute_bg_color(&self, _: &Style, _: &TermColors) -> ColorArray {
            self.0.background.0
        }
        fn color(&self, _: usize, _: &TermColors) -> ColorArray {
            self.0.foreground
        }
        fn use_drawable_chars(&self) -> bool {
            false
        }
        fn opacity_cells(&self) -> bool {
            false
        }
        fn cell_bg_alpha(&self) -> u8 {
            255
        }
        fn ignore_selection_fg_color(&self) -> bool {
            false
        }
    }

    #[test]
    fn emitted_rtl_glyphs_keep_columns_and_colors_across_run_breaks() {
        // Exercise the real row emitter and rasterizer with an in-memory
        // CPU atlas; no window or GPU is required. Cursor and selection
        // split shaping runs, but must not move any Hebrew letter.
        let font_library = FontLibrary::default();
        let mut rasterizer = GridGlyphRasterizer::new();
        let palette = TestPalette(Colors {
            selection_foreground: [0.0, 1.0, 0.0, 1.0],
            ..Colors::default()
        });
        for text in [
            "שלום",
            "abcשלום123",
            "مرحبا",
            "فارسی",
            "abcمرحبا123",
            "abcفارسی123",
        ] {
            let cols = text.chars().count();
            let mut row = Row::<Square>::new(cols);
            let mut styles = Vec::new();
            for (col, ch) in text.chars().enumerate() {
                row[Column(col)].set_c(ch);
                row[Column(col)].set_style_id(col as u16 + 1);
                styles.push(Style {
                    fg: AnsiColor::Indexed(col as u8 + 1),
                    ..Style::default()
                });
            }
            let mut grid = GridRenderer::Cpu(CpuGridRenderer::new(cols as u32, 1));
            let mut emit = |cursor, selection| {
                let mut fg = Vec::new();
                build_row_fg(
                    &row,
                    cols,
                    0,
                    &styles,
                    &ExtrasMap::default(),
                    &palette,
                    &TermColors::default(),
                    &mut rasterizer,
                    &mut grid,
                    28.0,
                    17.0,
                    34.0,
                    selection,
                    &[],
                    None,
                    &font_library,
                    0,
                    cursor,
                    &mut fg,
                );
                fg.sort_by_key(|g| g.grid_pos[0]);
                assert_eq!(fg.len(), cols, "one visible glyph per letter in {text}");
                for (col, g) in fg.iter().enumerate() {
                    assert_eq!(g.grid_pos, [col as u16, 0], "{text}");
                    let expected = if cell_in_row_sel(selection, col as u16) {
                        [0, 255, 0, 255]
                    } else {
                        [col as u8 + 1, 0, 0, 255]
                    };
                    assert_eq!(g.color, expected, "cell {col} of {text}");
                }
                fg.iter()
                    .map(|g| (g.glyph_pos, g.glyph_size, g.bearings))
                    .collect::<Vec<_>>()
            };
            let baseline = emit(None, None);
            for col in 0..cols as u16 {
                let at_cursor = emit(Some(col), None);
                let selection = Some(RowSelection {
                    lo: col,
                    hi: (col + 1).min(cols as u16 - 1),
                });
                let selected = emit(None, selection);
                // Hebrew glyph identity is stable too; Arabic/Persian
                // contextual forms can change when a run is split.
                if text.contains('ש') {
                    assert_eq!(at_cursor, baseline, "cursor at {col}");
                    assert_eq!(selected, baseline, "selection at {col}");
                }
            }
        }
    }
}
