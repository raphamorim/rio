// grid/tests.rs was originally taken from Alacritty
// https://github.com/alacritty/alacritty/blob/e35e5ad14fce8456afdd89f2b392b9924bb27471/alacritty_terminal/src/grid/tests.rs
// which is licensed under Apache 2.0 license.

use super::*;

use crate::crosswords::square::Square;

impl GridSquare for usize {
    fn is_empty(&self) -> bool {
        *self == 0
    }

    fn reset(&mut self, template: &Self) {
        *self = *template;
    }
}

// Scroll up moves lines upward.
#[test]
fn scroll_up() {
    let mut grid = Grid::<usize>::new(10, 1, 0);
    for i in 0..10 {
        grid[Line(i as i32)][Column(0)] = i;
    }

    grid.scroll_up(&(Line(0)..Line(10)), 2);

    assert_eq!(grid[Line(0)][Column(0)], 2);
    assert_eq!(grid[Line(0)].occ, 1);
    assert_eq!(grid[Line(1)][Column(0)], 3);
    assert_eq!(grid[Line(1)].occ, 1);
    assert_eq!(grid[Line(2)][Column(0)], 4);
    assert_eq!(grid[Line(2)].occ, 1);
    assert_eq!(grid[Line(3)][Column(0)], 5);
    assert_eq!(grid[Line(3)].occ, 1);
    assert_eq!(grid[Line(4)][Column(0)], 6);
    assert_eq!(grid[Line(4)].occ, 1);
    assert_eq!(grid[Line(5)][Column(0)], 7);
    assert_eq!(grid[Line(5)].occ, 1);
    assert_eq!(grid[Line(6)][Column(0)], 8);
    assert_eq!(grid[Line(6)].occ, 1);
    assert_eq!(grid[Line(7)][Column(0)], 9);
    assert_eq!(grid[Line(7)].occ, 1);
    assert_eq!(grid[Line(8)][Column(0)], 0); // was 0.
    assert_eq!(grid[Line(8)].occ, 0);
    assert_eq!(grid[Line(9)][Column(0)], 0); // was 1.
    assert_eq!(grid[Line(9)].occ, 0);
}

// Scroll down moves lines downward.
#[test]
fn scroll_down() {
    let mut grid = Grid::<usize>::new(10, 1, 0);
    for i in 0..10 {
        grid[Line(i as i32)][Column(0)] = i;
    }

    grid.scroll_down(&(Line(0)..Line(10)), 2);

    assert_eq!(grid[Line(0)][Column(0)], 0); // was 8.
    assert_eq!(grid[Line(0)].occ, 0);
    assert_eq!(grid[Line(1)][Column(0)], 0); // was 9.
    assert_eq!(grid[Line(1)].occ, 0);
    assert_eq!(grid[Line(2)][Column(0)], 0);
    assert_eq!(grid[Line(2)].occ, 1);
    assert_eq!(grid[Line(3)][Column(0)], 1);
    assert_eq!(grid[Line(3)].occ, 1);
    assert_eq!(grid[Line(4)][Column(0)], 2);
    assert_eq!(grid[Line(4)].occ, 1);
    assert_eq!(grid[Line(5)][Column(0)], 3);
    assert_eq!(grid[Line(5)].occ, 1);
    assert_eq!(grid[Line(6)][Column(0)], 4);
    assert_eq!(grid[Line(6)].occ, 1);
    assert_eq!(grid[Line(7)][Column(0)], 5);
    assert_eq!(grid[Line(7)].occ, 1);
    assert_eq!(grid[Line(8)][Column(0)], 6);
    assert_eq!(grid[Line(8)].occ, 1);
    assert_eq!(grid[Line(9)][Column(0)], 7);
    assert_eq!(grid[Line(9)].occ, 1);
}

#[test]
fn scroll_down_with_history() {
    let mut grid = Grid::<usize>::new(10, 1, 1);
    grid.increase_scroll_limit(1);
    for i in 0..10 {
        grid[Line(i as i32)][Column(0)] = i;
    }

    grid.scroll_down(&(Line(0)..Line(10)), 2);

    assert_eq!(grid[Line(0)][Column(0)], 0); // was 8.
    assert_eq!(grid[Line(0)].occ, 0);
    assert_eq!(grid[Line(1)][Column(0)], 0); // was 9.
    assert_eq!(grid[Line(1)].occ, 0);
    assert_eq!(grid[Line(2)][Column(0)], 0);
    assert_eq!(grid[Line(2)].occ, 1);
    assert_eq!(grid[Line(3)][Column(0)], 1);
    assert_eq!(grid[Line(3)].occ, 1);
    assert_eq!(grid[Line(4)][Column(0)], 2);
    assert_eq!(grid[Line(4)].occ, 1);
    assert_eq!(grid[Line(5)][Column(0)], 3);
    assert_eq!(grid[Line(5)].occ, 1);
    assert_eq!(grid[Line(6)][Column(0)], 4);
    assert_eq!(grid[Line(6)].occ, 1);
    assert_eq!(grid[Line(7)][Column(0)], 5);
    assert_eq!(grid[Line(7)].occ, 1);
    assert_eq!(grid[Line(8)][Column(0)], 6);
    assert_eq!(grid[Line(8)].occ, 1);
    assert_eq!(grid[Line(9)][Column(0)], 7);
    assert_eq!(grid[Line(9)].occ, 1);
}

// Test that GridIterator works.
#[test]
fn test_iter() {
    let assert_indexed = |value: usize, indexed: Option<Indexed<&usize>>| {
        assert_eq!(Some(&value), indexed.map(|indexed| indexed.square));
    };

    let mut grid = Grid::<usize>::new(5, 5, 0);
    for i in 0..5 {
        for j in 0..5 {
            grid[Line(i)][Column(j)] = i as usize * 5 + j;
        }
    }

    let mut iter = grid.iter_from(Pos::new(Line(0), Column(0)));

    assert_eq!(None, iter.prev());
    assert_indexed(1, iter.next());
    assert_eq!(Column(1), iter.pos().col);
    assert_eq!(0, iter.pos().row);

    assert_indexed(2, iter.next());
    assert_indexed(3, iter.next());
    assert_indexed(4, iter.next());

    // Test line-wrapping.
    assert_indexed(5, iter.next());
    assert_eq!(Column(0), iter.pos().col);
    assert_eq!(1, iter.pos().row);

    assert_indexed(4, iter.prev());
    assert_eq!(Column(4), iter.pos().col);
    assert_eq!(0, iter.pos().row);

    // Make sure iter.cell() returns the current iterator position.
    assert_eq!(&4, iter.square());

    // Test that iter ends at end of grid.
    let mut final_iter = grid.iter_from(Pos {
        row: Line(4),
        col: Column(4),
    });
    assert_eq!(None, final_iter.next());
    assert_indexed(23, final_iter.prev());
}

#[test]
fn shrink_reflow() {
    let mut grid = Grid::<Square>::new(1, 5, 2);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = cell('2');
    grid[Line(0)][Column(2)] = cell('3');
    grid[Line(0)][Column(3)] = cell('4');
    grid[Line(0)][Column(4)] = cell('5');

    grid.resize(true, 1, 2);

    assert_eq!(grid.total_lines(), 3);

    assert_eq!(grid[Line(-2)].len(), 2);
    assert_eq!(grid[Line(-2)][Column(0)], cell('1'));
    assert_eq!(grid[Line(-2)][Column(1)], wrap_cell('2'));

    assert_eq!(grid[Line(-1)].len(), 2);
    assert_eq!(grid[Line(-1)][Column(0)], cell('3'));
    assert_eq!(grid[Line(-1)][Column(1)], wrap_cell('4'));

    assert_eq!(grid[Line(0)].len(), 2);
    assert_eq!(grid[Line(0)][Column(0)], cell('5'));
    assert_eq!(grid[Line(0)][Column(1)], Square::default());
}

#[test]
fn shrink_reflow_twice() {
    let mut grid = Grid::<Square>::new(1, 5, 2);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = cell('2');
    grid[Line(0)][Column(2)] = cell('3');
    grid[Line(0)][Column(3)] = cell('4');
    grid[Line(0)][Column(4)] = cell('5');

    grid.resize(true, 1, 4);
    grid.resize(true, 1, 2);

    assert_eq!(grid.total_lines(), 3);

    assert_eq!(grid[Line(-2)].len(), 2);
    assert_eq!(grid[Line(-2)][Column(0)], cell('1'));
    assert_eq!(grid[Line(-2)][Column(1)], wrap_cell('2'));

    assert_eq!(grid[Line(-1)].len(), 2);
    assert_eq!(grid[Line(-1)][Column(0)], cell('3'));
    assert_eq!(grid[Line(-1)][Column(1)], wrap_cell('4'));

    assert_eq!(grid[Line(0)].len(), 2);
    assert_eq!(grid[Line(0)][Column(0)], cell('5'));
    assert_eq!(grid[Line(0)][Column(1)], Square::default());
}

#[test]
fn shrink_reflow_empty_cell_inside_line() {
    let mut grid = Grid::<Square>::new(1, 5, 3);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = Square::default();
    grid[Line(0)][Column(2)] = cell('3');
    grid[Line(0)][Column(3)] = cell('4');
    grid[Line(0)][Column(4)] = Square::default();

    grid.resize(true, 1, 2);

    assert_eq!(grid.total_lines(), 2);

    assert_eq!(grid[Line(-1)].len(), 2);
    assert_eq!(grid[Line(-1)][Column(0)], cell('1'));
    assert_eq!(grid[Line(-1)][Column(1)], wrap_cell('\0'));

    assert_eq!(grid[Line(0)].len(), 2);
    assert_eq!(grid[Line(0)][Column(0)], cell('3'));
    assert_eq!(grid[Line(0)][Column(1)], cell('4'));

    grid.resize(true, 1, 1);

    assert_eq!(grid.total_lines(), 4);

    assert_eq!(grid[Line(-3)].len(), 1);
    assert_eq!(grid[Line(-3)][Column(0)], wrap_cell('1'));

    assert_eq!(grid[Line(-2)].len(), 1);
    assert_eq!(grid[Line(-2)][Column(0)], wrap_cell('\0'));

    assert_eq!(grid[Line(-1)].len(), 1);
    assert_eq!(grid[Line(-1)][Column(0)], wrap_cell('3'));

    assert_eq!(grid[Line(0)].len(), 1);
    assert_eq!(grid[Line(0)][Column(0)], cell('4'));
}

#[test]
fn grow_reflow() {
    let mut grid = Grid::<Square>::new(2, 2, 0);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = wrap_cell('2');
    grid[Line(1)][Column(0)] = cell('3');
    grid[Line(1)][Column(1)] = Square::default();

    grid.resize(true, 2, 3);

    assert_eq!(grid.total_lines(), 2);

    assert_eq!(grid[Line(0)].len(), 3);
    assert_eq!(grid[Line(0)][Column(0)], cell('1'));
    assert_eq!(grid[Line(0)][Column(1)], cell('2'));
    assert_eq!(grid[Line(0)][Column(2)], cell('3'));

    // Make sure rest of grid is empty.
    assert_eq!(grid[Line(1)].len(), 3);
    assert_eq!(grid[Line(1)][Column(0)], Square::default());
    assert_eq!(grid[Line(1)][Column(1)], Square::default());
    assert_eq!(grid[Line(1)][Column(2)], Square::default());
}

#[test]
fn grow_reflow_multiline() {
    let mut grid = Grid::<Square>::new(3, 2, 0);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = wrap_cell('2');
    grid[Line(1)][Column(0)] = cell('3');
    grid[Line(1)][Column(1)] = wrap_cell('4');
    grid[Line(2)][Column(0)] = cell('5');
    grid[Line(2)][Column(1)] = cell('6');

    grid.resize(true, 3, 6);

    assert_eq!(grid.total_lines(), 3);

    assert_eq!(grid[Line(0)].len(), 6);
    assert_eq!(grid[Line(0)][Column(0)], cell('1'));
    assert_eq!(grid[Line(0)][Column(1)], cell('2'));
    assert_eq!(grid[Line(0)][Column(2)], cell('3'));
    assert_eq!(grid[Line(0)][Column(3)], cell('4'));
    assert_eq!(grid[Line(0)][Column(4)], cell('5'));
    assert_eq!(grid[Line(0)][Column(5)], cell('6'));

    // Make sure rest of grid is empty.
    for r in (1..3).map(Line::from) {
        assert_eq!(grid[r].len(), 6);
        for c in 0..6 {
            assert_eq!(grid[r][Column(c)], Square::default());
        }
    }
}

#[test]
fn grow_reflow_disabled() {
    let mut grid = Grid::<Square>::new(2, 2, 0);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = wrap_cell('2');
    grid[Line(1)][Column(0)] = cell('3');
    grid[Line(1)][Column(1)] = Square::default();

    grid.resize(false, 2, 3);

    assert_eq!(grid.total_lines(), 2);

    assert_eq!(grid[Line(0)].len(), 3);
    assert_eq!(grid[Line(0)][Column(0)], cell('1'));
    assert_eq!(grid[Line(0)][Column(1)], wrap_cell('2'));
    assert_eq!(grid[Line(0)][Column(2)], Square::default());

    assert_eq!(grid[Line(1)].len(), 3);
    assert_eq!(grid[Line(1)][Column(0)], cell('3'));
    assert_eq!(grid[Line(1)][Column(1)], Square::default());
    assert_eq!(grid[Line(1)][Column(2)], Square::default());
}

#[test]
fn shrink_reflow_disabled() {
    let mut grid = Grid::<Square>::new(1, 5, 2);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = cell('2');
    grid[Line(0)][Column(2)] = cell('3');
    grid[Line(0)][Column(3)] = cell('4');
    grid[Line(0)][Column(4)] = cell('5');

    grid.resize(false, 1, 2);

    assert_eq!(grid.total_lines(), 1);

    assert_eq!(grid[Line(0)].len(), 2);
    assert_eq!(grid[Line(0)][Column(0)], cell('1'));
    assert_eq!(grid[Line(0)][Column(1)], cell('2'));
}

// https://github.com/rust-lang/rust-clippy/pull/6375
#[allow(clippy::all)]
fn cell(c: char) -> Square {
    let mut cell = Square::default();
    cell.set_c(c);
    cell
}

fn wrap_cell(c: char) -> Square {
    let mut cell = cell(c);
    cell.set_wrapline(true);
    cell
}

fn wide_cell(c: char) -> Square {
    let mut cell = cell(c);
    cell.set_wide(crate::crosswords::square::Wide::Wide);
    cell
}

fn spacer_cell() -> Square {
    let mut cell = Square::default();
    cell.set_wide(crate::crosswords::square::Wide::Spacer);
    cell
}

#[test]
fn shrink_reflow_remap_tracks_displaced_wide_char() {
    // A wrapped tail of exactly `columns - 1` cells is buffered into
    // the next row, whose first cell is a wide char. The spacer logic
    // displaces that wide char into the following push, so the remap
    // must record the row after the one receiving the buffered tail.
    let mut grid = Grid::<Square>::new(3, 8, 4);
    for (n, c) in "1234567".chars().enumerate() {
        grid[Line(0)][Column(n)] = cell(c);
    }
    grid[Line(0)][Column(6)] = wrap_cell('7');
    grid[Line(1)][Column(0)] = wide_cell('W');
    grid[Line(1)][Column(1)] = spacer_cell();
    grid[Line(1)][Column(2)] = cell('x');

    grid.track_reflow_remap = true;
    grid.resize(true, 3, 4);
    grid.track_reflow_remap = false;

    let remap = grid.reflow_remap.take().expect("remap must be recorded");
    assert_eq!(remap.base_abs, 0);
    // Old row 0 lands at 0; the wide-char row's first cell lands at 2
    // (position 1 holds the buffered "567" tail plus the spacer); the
    // trailing blank row lands at 3.
    assert_eq!(remap.new_pos, vec![0, 2, 3]);

    // Cross-check against where the wide char actually sits.
    let total = grid.total_lines() as i32;
    let screen = grid.screen_lines() as i32;
    let wide_line = Line(2 - (total - screen));
    assert_eq!(grid[wide_line][Column(0)], wide_cell('W'));
}

#[test]
fn grow_reflow_remap_tracks_unmerged_wide_char() {
    // The merge target has exactly one free column, so only a leading
    // spacer is appended and the wide char stays on its own row. The
    // remap must record the pushed remainder row, not the merge
    // target.
    let mut grid = Grid::<Square>::new(2, 4, 2);
    for (n, c) in "1234".chars().enumerate() {
        grid[Line(0)][Column(n)] = cell(c);
    }
    grid[Line(0)][Column(3)] = wrap_cell('4');
    grid[Line(1)][Column(0)] = wide_cell('W');
    grid[Line(1)][Column(1)] = spacer_cell();

    grid.track_reflow_remap = true;
    grid.resize(true, 2, 5);
    grid.track_reflow_remap = false;

    let remap = grid.reflow_remap.take().expect("remap must be recorded");
    assert_eq!(remap.base_abs, 0);
    assert_eq!(remap.new_pos, vec![0, 1]);
    assert_eq!(grid[Line(1)][Column(0)], wide_cell('W'));
}

#[test]
fn extras_sweep_resets_reclaim_cadence() {
    use crate::crosswords::square::{Extras, Hyperlink};

    let mut table = ExtrasTable::new();
    for i in 0..EXTRAS_RECLAIM_CADENCE {
        table.alloc(Extras {
            zerowidth: Vec::new(),
            hyperlink: Some(Hyperlink::new(Some(i.to_string()), i.to_string())),
        });
    }
    assert!(table.should_reclaim());

    // A sweep resets the cadence even when every slot stays live, so a
    // fully-live table can't re-walk the ring on every allocation.
    let live = vec![u64::MAX; ID_BITSET_WORDS];
    table.sweep_unmarked(&live);
    assert!(!table.should_reclaim());
}

#[test]
fn style_sweep_marks_cursor_template() {
    use crate::config::colors::{AnsiColor, NamedColor};
    use crate::crosswords::style::Style;

    let mut grid: Grid<Square> = Grid::new(4, 4, 10);

    // Intern a style that ends up referenced by nothing once the
    // template moves on: it must be swept.
    grid.set_template_style(Style {
        fg: AnsiColor::Named(NamedColor::Blue),
        ..Style::default()
    });
    grid.sync_template_style();
    let dead_id = grid.template_style_id();

    // The template's current style is a live root even when no cell
    // references it.
    grid.set_template_style(Style {
        fg: AnsiColor::Named(NamedColor::Red),
        ..Style::default()
    });
    grid.sync_template_style();
    let live_id = grid.template_style_id();

    grid.reclaim_styles();
    assert_eq!(
        grid.style_set.get(live_id).fg,
        AnsiColor::Named(NamedColor::Red)
    );
    assert_eq!(grid.style_set.get(dead_id), Style::default());
}

#[test]
fn extras_reclaim_keeps_ids_of_hidden_cached_rows() {
    use crate::crosswords::square::Extras;

    let mut grid: Grid<Square> = Grid::new(4, 2, 10);
    let id = grid.alloc_extras(Extras {
        zerowidth: vec!['\u{301}'],
        hyperlink: None,
    });
    grid[Line(3)][Column(0)].set_extras_id(Some(id));
    grid[Line(3)].has_extras = true;
    let dead = grid.alloc_extras(Extras {
        zerowidth: vec!['\u{302}'],
        hyperlink: None,
    });

    // Shrinking with the cursor at the top drops the bottom rows into
    // Storage's hidden cache; their extras must stay live while the
    // unreferenced slot is freed.
    grid.resize(false, 2, 2);
    grid.reclaim_extras();
    assert!(grid.extras_table.get(id).is_some());
    assert!(grid.extras_table.get(dead).is_none());
}

#[test]
fn row_hint_follows_reset_template() {
    use crate::config::colors::{AnsiColor, NamedColor};
    use crate::crosswords::style::Style;

    let mut grid: Grid<Square> = Grid::new(2, 4, 0);
    grid.set_template_style(Style {
        fg: AnsiColor::Named(NamedColor::Red),
        ..Style::default()
    });
    grid.sync_template_style();
    let styled_template = grid.cursor.template;
    assert!(styled_template.carries_style());

    let mut row: Row<Square> = Row::new(4);
    assert!(!row.has_styles);
    row.reset(&styled_template);
    assert!(row.has_styles);

    row.reset(&Square::default());
    assert!(!row.has_styles);
}

#[test]
fn row_hint_propagates_and_clears() {
    let mut styled: Row<Square> = Row::new(4);
    styled[Column(0)].set_style_id(9);
    styled.has_styles = true;

    let mut dst: Row<Square> = Row::new(4);
    dst.copy_from(&styled);
    assert!(dst.has_styles);

    dst.recycle(4);
    assert!(!dst.has_styles);
}

#[test]
fn row_splices_recompute_hint_exactly() {
    // Unstyled cells moving in must not pin the row into the sweep walk.
    let mut dst: Row<Square> = Row::new(2);
    let mut plain = vec![Square::default(), Square::default()];
    dst.append(&mut plain);
    assert!(!dst.has_styles);

    let mut styled_vec = vec![Square::default().with_style_id(3)];
    dst.append(&mut styled_vec);
    assert!(dst.has_styles);

    // append_front_of scans only the moved span.
    let mut src: Row<Square> = Row::new(4);
    src[Column(3)].set_style_id(5);
    src.has_styles = true;
    let mut dst2: Row<Square> = Row::new(1);
    dst2.append_front_of(&mut src, 2);
    assert!(!dst2.has_styles);
    let mut dst3: Row<Square> = Row::new(1);
    dst3.append_front_of(&mut src, 2);
    assert!(dst3.has_styles);

    // from_vec derives the hint from its contents.
    assert!(!Row::from_vec(vec![Square::default(); 3], 0).has_styles);
    assert!(Row::from_vec(vec![Square::default().with_style_id(1)], 1).has_styles);
}

#[test]
fn bg_only_cells_do_not_count_as_styled() {
    let mut bg = Square::default();
    bg.set_bg_rgb(10, 20, 30);
    assert!(!bg.carries_style());

    let mut row: Row<Square> = Row::new(2);
    let mut moved = vec![bg];
    row.append(&mut moved);
    assert!(!row.has_styles);
}

#[test]
fn resolver_fast_path_matches_slow_path() {
    use crate::config::colors::{AnsiColor, NamedColor};
    use crate::crosswords::style::Style;

    let mut grid: Grid<Square> = Grid::new(2, 3, 0);
    grid.set_template_style(Style {
        fg: AnsiColor::Named(NamedColor::Blue),
        ..Style::default()
    });
    grid.sync_template_style();
    let id = grid.template_style_id();

    // Styled row: full resolution.
    let mut styled: Row<Square> = Row::new(3);
    styled[Column(1)].set_style_id(id);
    styled.has_styles = true;
    let mut out = Vec::new();
    grid.resolve_row_styles(&styled, &mut out);
    assert_eq!(out.len(), 3);
    assert_eq!(out[0], Style::default());
    assert_eq!(out[1].fg, AnsiColor::Named(NamedColor::Blue));

    // Unstyled row: fast fill must be indistinguishable.
    let plain: Row<Square> = Row::new(3);
    let mut fast = Vec::new();
    grid.resolve_row_styles(&plain, &mut fast);
    assert_eq!(fast, vec![Style::default(); 3]);
}
