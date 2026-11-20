// grid/resize.rs was originally taken from Alacritty
// https://github.com/alacritty/alacritty/blob/e35e5ad14fce8456afdd89f2b392b9924bb27471/alacritty_terminal/src/grid/resize.rs
// which is licensed under Apache 2.0 license.

use crate::crosswords::grid::{Dimensions, Grid, ReflowRemap};
use crate::crosswords::pos::{Boundary, Column, Line};
use crate::crosswords::square::{Square, Wide};
use crate::crosswords::Row;
use std::cmp::{max, min, Ordering};
use std::mem;

impl Grid<Square> {
    /// Resize the grid's width and/or height.
    pub fn resize(&mut self, reflow: bool, lines: usize, columns: usize) {
        // Use empty template cell for resetting cells due to resize.
        let template = mem::take(&mut self.cursor.template);

        // Only the column passes below produce a row remap; a stale
        // one from an earlier resize must not leak through.
        self.reflow_remap = None;

        match self.lines.cmp(&lines) {
            Ordering::Less => self.grow_lines(lines),
            Ordering::Greater => self.shrink_lines(lines),
            Ordering::Equal => (),
        }

        match self.columns.cmp(&columns) {
            Ordering::Less => self.grow_columns(reflow, columns),
            Ordering::Greater => self.shrink_columns(reflow, columns),
            Ordering::Equal => (),
        }

        // Restore template cell.
        self.cursor.template = template;
    }

    /// Add lines to the visible area.
    ///
    /// Rio keeps the cursor at the bottom of the terminal as long as there
    /// is scrollback available. Once scrollback is exhausted, new lines are
    /// simply added to the bottom of the screen.
    fn grow_lines(&mut self, target: usize) {
        let lines_added = target - self.lines;

        // Need to resize before updating buffer.
        self.raw.grow_visible_lines(target);
        self.lines = target;

        let history_size = self.history_size();
        let from_history = min(history_size, lines_added);

        // Move existing lines up for every line that couldn't be pulled from history.
        if from_history != lines_added {
            let delta = lines_added - from_history;
            self.scroll_up(&(Line(0)..Line(target as i32)), delta);
        }

        // Move cursor down for every line pulled from history.
        self.saved_cursor.pos.row += from_history;
        self.cursor.pos.row += from_history;

        // The viewport absorbed exactly the rows pulled out of history,
        // not the full growth (history may have had fewer). Clamp like
        // the reflow paths do: an offset past the remaining history makes
        // `visible_line_bounds` request rows the storage doesn't have.
        self.display_offset = self.display_offset.saturating_sub(from_history);
        self.decrease_scroll_limit(lines_added);
        self.display_offset = min(self.display_offset, self.history_size());
    }

    /// Remove lines from the visible area.
    ///
    /// The behavior in Terminal.app and iTerm.app is to keep the cursor at the
    /// bottom of the screen. This is achieved by pushing history "out the top"
    /// of the terminal window.
    ///
    /// Rio takes the same approach.
    fn shrink_lines(&mut self, target: usize) {
        // Scroll up to keep content inside the window.
        let required_scrolling =
            (self.cursor.pos.row.0 as usize + 1).saturating_sub(target);
        if required_scrolling > 0 {
            self.scroll_up(&(Line(0)..Line(self.lines as i32)), required_scrolling);

            // Clamp cursors to the new viewport size.
            self.cursor.pos.row = min(self.cursor.pos.row, Line(target as i32 - 1));
        }

        // Clamp saved cursor, since only primary cursor is scrolled into viewport.
        self.saved_cursor.pos.row =
            min(self.saved_cursor.pos.row, Line(target as i32 - 1));

        self.raw.rotate((self.lines - target) as isize);
        self.raw.shrink_visible_lines(target);
        self.lines = target;
        // Rows moved into history above; same defensive clamp as
        // `grow_lines` and the column reflows.
        self.display_offset = min(self.display_offset, self.history_size());
    }

    /// Grow number of columns in each row, reflowing if necessary.
    fn grow_columns(&mut self, reflow: bool, columns: usize) {
        // Check if a row needs to be wrapped.
        let should_reflow = |row: &Row<Square>| -> bool {
            let len = Column(row.len());
            reflow && len.0 > 0 && len < columns && row[len - 1].wrapline()
        };

        // Fast path: when no row is soft-wrapped into the next there is
        // nothing to rejoin, so growing is a per-row blank extension with no
        // content movement. The full reflow loop below would, in that case,
        // re-push every row unchanged and grow it to `columns`, so this is
        // equivalent while skipping `take_all` and the two temporary buffers.
        // Skipped when tracking image placements, since the full path also
        // builds their row remap.
        if !self.track_reflow_remap && !self.raw.rows().any(should_reflow) {
            self.columns = columns;
            // Undo the linewrap special case exactly as the full path does.
            if self.cursor.should_wrap && reflow {
                self.cursor.should_wrap = false;
                self.cursor.pos.col += 1;
            }
            self.raw.grow_all_rows(columns);
            return;
        }

        self.columns = columns;

        let mut reversed: Vec<Row<Square>> = Vec::with_capacity(self.raw.len());
        let mut cursor_line_delta = 0;

        // Remove the linewrap special case, by moving the cursor outside of the grid.
        if self.cursor.should_wrap && reflow {
            self.cursor.should_wrap = false;
            self.cursor.pos.col += 1;
        }

        let mut rows = self.raw.take_all();
        let old_len = rows.len();
        // Exact row tracking for image placements: rows are walked
        // oldest-first, and a row's absolute index is `base_abs +
        // oldest-first position` on both sides of the reflow.
        let mut remap = self.track_reflow_remap.then(|| ReflowRemap {
            base_abs: self.lines_evicted(),
            new_pos: vec![-1; old_len],
        });
        // Rows of the new buffer whose head was in history. They are a
        // prefix of `reversed`, so this count tells how many history
        // rows a shorter buffer would pull onto the screen.
        let mut history_rows = 0usize;

        for (i, mut row) in rows.drain(..).enumerate().rev() {
            // Index of the merge target while `last_row` holds the
            // mutable borrow below.
            let merge_target = reversed.len().wrapping_sub(1);
            let last_is_history = reversed.len() == history_rows;

            // Check if reflowing should be performed.
            let last_row = match reversed.last_mut() {
                Some(last_row) if should_reflow(last_row) => last_row,
                _ => {
                    reversed.push(row);
                    history_rows += (i >= self.lines) as usize;
                    if let Some(r) = remap.as_mut() {
                        r.new_pos[old_len - 1 - i] = (reversed.len() - 1) as i64;
                    }
                    continue;
                }
            };

            // A screen row joining the last history row makes that row
            // part of the screen: its text was visible, so it is not
            // history that slid on.
            if i < self.lines && last_is_history {
                history_rows -= 1;
            }

            // Remove wrap flag before appending additional cells.
            if let Some(cell) = last_row.last_mut() {
                cell.set_wrapline(false);
            }

            // Remove leading spacers when reflowing wide char to the previous line.
            let mut last_len = last_row.len();
            if last_len >= 1
                && matches!(last_row[Column(last_len - 1)].wide(), Wide::LeadingSpacer)
            {
                last_row.shrink(last_len - 1);
                last_len -= 1;
            }

            // Don't try to pull more cells from the next line than available.
            let num_wrapped = columns - last_len;
            let len = min(row.len(), num_wrapped);

            // Move the wrapped cells from the front of `row` onto the end of
            // `last_row`, in place (no per-row temporary allocation).
            // `moved` is how many left this row: one fewer than `len`
            // when a wide char at the seam stays behind its spacer.
            let mut first_cell_moved = true;
            let mut moved = len;
            if matches!(row[Column(len - 1)].wide(), Wide::Wide) {
                // With a single free column, only the spacer is
                // appended and the wide char stays on this row.
                first_cell_moved = len > 1;
                moved = len - 1;

                last_row.append_front_of(&mut row, len - 1);

                let mut spacer = Square::default();
                spacer.set_wide(Wide::LeadingSpacer);
                last_row.inner.push(spacer);
                last_row.occ += 1;
            } else {
                last_row.append_front_of(&mut row, len);
            }

            // The old row's first cell just landed at the end of the
            // merge target, unless the wide-char spacer case kept it
            // on this row; then it lands with the push below (the
            // dropped-as-clear paths are unreachable while the row
            // still holds the wide char).
            if first_cell_moved {
                if let Some(r) = remap.as_mut() {
                    r.new_pos[old_len - 1 - i] = merge_target as i64;
                }
            }

            let cursor_buffer_line = self.lines - self.cursor.pos.row.0 as usize - 1;

            if i == cursor_buffer_line && reflow {
                // Move the cursor back by the cells that left its row. A
                // cursor on one of them lands where it went: after the
                // cells the previous row already had. Plain arithmetic,
                // not `Pos::sub` with a screen boundary: when the cursor
                // sits on the top row and its line continues from
                // history, the target is one row above the screen, and a
                // clamp to the top left corner would drop the column and
                // never pull the joined row down, leaving the cursor
                // detached from its text.
                let col = self.cursor.pos.col.0;
                let (mut line_delta, mut target_col): (usize, usize) = if col >= moved {
                    (0, col - moved)
                } else {
                    (1, last_len + col)
                };

                // A cursor right after the cells that left, with nothing
                // after it on this row, follows them: it is after them
                // on the previous row. Only when they filled that row to
                // the last column does it stay on it with a pending wrap.
                if target_col == 0 && row.is_clear() {
                    line_delta = 1;
                    target_col = last_len + moved;
                    if target_col >= columns {
                        self.cursor.should_wrap = true;
                        target_col = columns - 1;
                    }
                }
                self.cursor.pos.col = Column(target_col);

                // Since `num_wrapped` is smaller than `columns` the line
                // delta is always `0` or `1`. A cursor row left blank by
                // the move is dropped like any other blank row, viewport
                // adjustment included.
                if line_delta != 0 && row.is_clear() {
                    if i < self.display_offset {
                        self.display_offset = self.display_offset.saturating_sub(1);
                    }
                    continue;
                }

                cursor_line_delta += line_delta;
            } else if row.is_clear() {
                if i < self.display_offset {
                    // Since we removed a line, rotate down the viewport.
                    self.display_offset = self.display_offset.saturating_sub(1);
                }

                // Rotate cursor down if content below them was pulled from history.
                if i < cursor_buffer_line {
                    self.cursor.pos.row += 1;
                }

                // Don't push line into the new buffer.
                continue;
            }

            if let Some(cell) = last_row.last_mut() {
                // Set wrap flag if next line still has cells.
                cell.set_wrapline(true);
            }

            reversed.push(row);
            history_rows += (i >= self.lines) as usize;
            if !first_cell_moved {
                if let Some(r) = remap.as_mut() {
                    r.new_pos[old_len - 1 - i] = (reversed.len() - 1) as i64;
                }
            }
        }

        // Make sure we have at least the viewport filled.
        if reversed.len() < self.lines {
            let delta = (self.lines - reversed.len()) as i32;
            self.cursor.pos.row = max(self.cursor.pos.row - delta, Line(0));
            reversed.resize_with(self.lines, || Row::new(columns));
        }

        // The cursor moved up onto the joined row. When that row is now
        // above the screen, blank rows at the bottom are dropped to pull
        // it down, as many as needed and no more, so the cursor keeps
        // its text. Content under the cursor is never dropped: a grow
        // must not destroy output. Without blank rows the cursor stays
        // on the top row while its text is the row above the screen.
        if cursor_line_delta != 0 {
            let target = self.cursor.pos.row - cursor_line_delta as i32;
            let mut pulled_down = 0;
            if target < Line(0) {
                let blank_tail = reversed
                    .iter()
                    .rev()
                    .take_while(|row| row.is_clear())
                    .count();
                pulled_down = min(
                    min(-target.0 as usize, blank_tail),
                    reversed.len() - self.lines,
                );
                reversed.truncate(reversed.len() - pulled_down);
            }
            self.cursor.pos.row = max(target + pulled_down as i32, Line(0));
        }

        // The mirror of the room a shrink takes: rows a rejoin frees
        // slide history onto the screen only when the screen is full
        // to the bottom. A blank row under the cursor is room the
        // shrink paid with, which means it pushed nothing into history,
        // so every history row that slid on is pushed back out by a
        // blank row at the bottom and the cursor goes up with its text.
        // Cleared output thus stays cleared across a shrink and grow.
        // A cursor on a row that itself came from history keeps it.
        let history_after = reversed.len() - self.lines;
        let pulled = history_rows.saturating_sub(history_after);
        let cursor_row = self.cursor.pos.row.0 as usize;
        let has_room = cursor_row + 1 < self.lines
            && reversed.last().is_some_and(|row| row.is_clear());
        let given_back = if has_room { min(pulled, cursor_row) } else { 0 };
        if given_back > 0 {
            reversed.extend((0..given_back).map(|_| Row::new(columns)));
            self.cursor.pos.row -= given_back as i32;
            if self.display_offset > 0 {
                self.display_offset += given_back;
            }
        }

        // Restore oldest-first order in place and fill all rows that are
        // still too short. Reusing `reversed` avoids allocating a second
        // full row buffer, which matters when reflowing a long scrollback.
        reversed.reverse();
        for row in reversed.iter_mut() {
            if row.len() < columns {
                row.grow(columns);
            }
        }

        self.raw.replace_inner(reversed);

        // Clamp display offset in case lines above it got merged.
        self.display_offset = min(self.display_offset, self.history_size());

        self.reflow_remap = remap;
    }

    /// Shrink number of columns in each row, reflowing if necessary.
    fn shrink_columns(&mut self, reflow: bool, columns: usize) {
        // Fast path: if no row has occupied content beyond `columns` there is
        // nothing to wrap down, so shrinking is a per-row truncation of
        // trailing blank cells. Wrapped rows are full width, so `occ <=
        // columns` also rules them out. Requires the cursor to fit (otherwise
        // the full path wraps it onto a new line, which is not a plain
        // truncation) and no image-placement tracking (whose remap the full
        // path builds).
        let effective_cursor_col =
            self.cursor.pos.col.0 + usize::from(self.cursor.should_wrap && reflow);
        if !self.track_reflow_remap
            && (!reflow || effective_cursor_col <= columns)
            && self.raw.rows().all(|row| row.occ <= columns)
        {
            self.columns = columns;
            if self.cursor.should_wrap && reflow {
                self.cursor.should_wrap = false;
                self.cursor.pos.col += 1;
            }
            self.raw.shrink_all_rows(columns);

            // Clamp the cursor to the new width, mirroring the full path's tail.
            if !reflow {
                self.cursor.pos.col = min(self.cursor.pos.col, Column(columns - 1));
            } else if self.cursor.pos.col == columns
                && !self[self.cursor.pos.row][Column(columns - 1)].wrapline()
            {
                self.cursor.should_wrap = true;
                self.cursor.pos.col -= 1;
            } else {
                self.cursor.pos = self.cursor.pos.grid_clamp(self, Boundary::Cursor);
            }
            self.saved_cursor.pos.col =
                min(self.saved_cursor.pos.col, Column(columns - 1));
            return;
        }

        self.columns = columns;

        // Remove the linewrap special case, by moving the cursor outside of the grid.
        if self.cursor.should_wrap && reflow {
            self.cursor.should_wrap = false;
            self.cursor.pos.col += 1;
        }

        let mut new_raw = Vec::with_capacity(self.raw.len());
        let mut buffered: Option<Vec<Square>> = None;

        let mut rows = self.raw.take_all();
        let old_len = rows.len();

        // The old row holding the cursor, counted from the bottom like
        // `i`. Fixed here: the cursor moves as its text does, and the
        // match must not drift with it.
        let mut cursor_i = self.lines - self.cursor.pos.row.0 as usize - 1;
        // Set when the cursor's cells travel to the next row in the
        // buffered tail, so that row does not shift the cursor again.
        let mut cursor_in_buffered = false;
        // Blank rows at the bottom of the screen under the cursor: room
        // a wrap can take instead of pushing rows into history. Each
        // row a wrap adds while there is room is paid for by one of
        // them, popped below the loop, so everything above stays put.
        // A blank row right under a row that wraps is that line's
        // continuation: the wrapped tail lands in it, so it is not room.
        let under_cursor = self
            .lines
            .saturating_sub(self.cursor.pos.row.0 as usize + 1);
        let blank_tail = rows
            .iter()
            .take(under_cursor)
            .take_while(|row| row.is_clear())
            .count();
        let continuation = rows
            .get(blank_tail)
            .is_some_and(|above| above.inner.last().is_some_and(|cell| cell.wrapline()));
        let mut room = blank_tail.saturating_sub(continuation as usize);
        let mut pull = 0usize;
        // See `grow_columns`; `base_abs + position` also survives the
        // cap truncation below because dropping the N oldest rows
        // advances the eviction base by the same N.
        let mut remap = self.track_reflow_remap.then(|| ReflowRemap {
            base_abs: self.lines_evicted(),
            new_pos: vec![-1; old_len],
        });

        // Each old row's first cell, as (old position, offset from the
        // head of the not-yet-pushed cell stream). Recorded at the
        // push that consumes the cell. Offsets survive across old-row
        // iterations because a displaced wide char can travel in the
        // buffered tail into the next iteration's first push.
        let mut trackers: Vec<(usize, i64)> = Vec::new();

        for (i, mut row) in rows.drain(..).enumerate().rev() {
            if remap.is_some() {
                let own_first = buffered.as_ref().map_or(0, |b| b.len()) as i64;
                trackers.push((old_len - 1 - i, own_first));
            }

            // Append lines left over from the previous row.
            if let Some(buffered) = buffered.take() {
                // Add a column for every cell added before the cursor, if it goes beyond the new
                // width it is then later reflown. A cursor that arrived in
                // the buffered cells is already placed.
                if i == cursor_i && !cursor_in_buffered {
                    self.cursor.pos.col += buffered.len();
                }
                cursor_in_buffered = false;

                row.append_front(buffered);
            }

            loop {
                // Remove all cells which require reflowing.
                let mut wrapped = match row.shrink(columns) {
                    Some(wrapped) if reflow => wrapped,
                    _ => {
                        if reflow && i == cursor_i && self.cursor.pos.col > columns {
                            // If there are empty cells before the cursor, we assume it is explicit
                            // whitespace and need to wrap it like normal content.
                            Vec::new()
                        } else {
                            // Since it fits, just push the existing line without any reflow.
                            new_raw.push(row);
                            if let Some(r) = remap.as_mut() {
                                for (p, _) in trackers.drain(..) {
                                    r.new_pos[p] = (new_raw.len() - 1) as i64;
                                }
                            }
                            break;
                        }
                    }
                };

                // Insert spacer if a wide char would be wrapped into the last column.
                let mut displaced = 0i64;
                if row.len() >= columns
                    && matches!(row[Column(columns - 1)].wide(), Wide::Wide)
                {
                    if columns < 2 {
                        // Displacing the char into the next row would never
                        // terminate here: column 0 is also the last column,
                        // so it would be displaced again every iteration
                        // while `new_raw` grew without bound. Destroy the
                        // wide char and leave a blank narrow cell, and drop the
                        // Spacer that followed it so it cannot claim a row
                        // of its own.
                        row[Column(columns - 1)] = Square::default();
                        while matches!(
                            wrapped.first().map(|cell| cell.wide()),
                            Some(Wide::Spacer)
                        ) {
                            wrapped.remove(0);
                        }
                    } else {
                        let mut spacer = Square::default();
                        spacer.set_wide(Wide::LeadingSpacer);

                        let wide_char =
                            mem::replace(&mut row[Column(columns - 1)], spacer);
                        wrapped.insert(0, wide_char);
                        displaced = 1;
                    }
                }

                // Remove wide char spacer before shrinking.
                let len = wrapped.len();
                if len > 0 && matches!(wrapped[len - 1].wide(), Wide::LeadingSpacer) {
                    if len == 1 {
                        row[Column(columns - 1)].set_wrapline(true);
                        new_raw.push(row);
                        if let Some(r) = remap.as_mut() {
                            for (p, _) in trackers.drain(..) {
                                r.new_pos[p] = (new_raw.len() - 1) as i64;
                            }
                        }
                        break;
                    } else {
                        // Remove the leading spacer from the end of the wrapped row.
                        wrapped[len - 2].set_wrapline(true);
                        wrapped.truncate(len - 1);
                    }
                }

                new_raw.push(row);
                if let Some(r) = remap.as_mut() {
                    // This push consumed `columns` cells of the
                    // stream, minus a wide char the spacer displaced
                    // into the next row.
                    let consumed = columns as i64 - displaced;
                    trackers.retain(|&(p, off)| {
                        if off < consumed {
                            r.new_pos[p] = (new_raw.len() - 1) as i64;
                            false
                        } else {
                            true
                        }
                    });
                    for t in trackers.iter_mut() {
                        t.1 -= consumed;
                    }
                }

                // Set line as wrapped if cells got removed.
                if let Some(cell) = new_raw.last_mut().and_then(|r| r.last_mut()) {
                    cell.set_wrapline(true);
                }

                // The cells this push kept on the row: `columns`, less a
                // wide char the spacer displaced into the next row.
                let kept = columns - displaced as usize;

                if wrapped
                    .last()
                    .map(|c| c.wrapline() && i >= 1)
                    .unwrap_or(false)
                    && wrapped.len() < columns
                {
                    // Make sure previous wrap flag doesn't linger around.
                    if let Some(cell) = wrapped.last_mut() {
                        cell.set_wrapline(false);
                    }

                    // The cursor travels with its cells to the next row,
                    // which then holds it.
                    if i == cursor_i && self.cursor.pos.col.0 >= kept {
                        self.cursor.pos.col -= kept;
                        self.cursor.pos.row += 1;
                        cursor_i -= 1;
                        cursor_in_buffered = true;
                    }

                    // Add removed cells to start of next row.
                    buffered = Some(wrapped);
                    break;
                } else {
                    // This push adds a row. With room below the cursor it
                    // is paid for from the bottom and nothing above moves,
                    // so the cursor's row index changes only when the new
                    // row is above its text. Without room the screen is
                    // anchored at the bottom and the cursor follows its
                    // text up.
                    let on_new_row = i == cursor_i && self.cursor.pos.col.0 >= kept;
                    if on_new_row {
                        // The cursor's cells are on the new row. Since only
                        // a single new line is created, subtract only the
                        // kept cells instead of reflowing it completely.
                        self.cursor.pos.col -= kept;
                    }
                    let above_cursor = on_new_row || i > cursor_i;
                    // A row added in history changes nothing on the
                    // screen: it takes no room and moves no cursor.
                    if i >= self.lines {
                    } else if room > 0 {
                        room -= 1;
                        pull += 1;
                        if above_cursor {
                            self.cursor.pos.row += 1;
                        }
                    } else if !above_cursor && self.cursor.pos.row > Line(0) {
                        self.cursor.pos.row -= 1;
                    }

                    // Make sure new row is at least as long as new width.
                    let occ = wrapped.len();
                    if occ < columns {
                        wrapped.resize_with(columns, Square::default);
                    }
                    row = Row::from_vec(wrapped, occ);

                    if i < self.display_offset {
                        // Since we added a new line, rotate up the viewport.
                        self.display_offset += 1;
                    }
                }
            }
        }

        // The blank bottom rows that paid for added rows go now, so the
        // screen keeps its line count with everything above in place.
        // A popped row's content is gone for the remap, and the rows
        // were below a scrolled viewport, which keeps its distance.
        let mut popped = 0usize;
        while popped < pull
            && new_raw.len() > self.lines
            && new_raw.last().is_some_and(|row| row.is_clear())
        {
            new_raw.pop();
            if let Some(r) = remap.as_mut() {
                let gone = new_raw.len() as i64;
                for pos in r.new_pos.iter_mut() {
                    if *pos == gone {
                        *pos = -1;
                    }
                }
            }
            popped += 1;
        }
        self.display_offset = self.display_offset.saturating_sub(popped);
        // A row that could not be paid for after all stays, and pushes
        // everything above it up by one, cursor included.
        let unpaid = pull - popped;
        self.cursor.pos.row = max(self.cursor.pos.row - unpaid, Line(0));

        // Put the rows in storage order, newest first, in place, reusing
        // the buffer as the new grid storage.
        new_raw.reverse();
        let mut reversed = new_raw;
        // Reflow can overflow the scrollback cap; the oldest lines
        // fall off the ring and must advance the absolute row base.
        let cap = self.max_scroll_limit + self.lines;
        if reversed.len() > cap {
            self.total_lines_scrolled += (reversed.len() - cap) as u64;
        }
        reversed.truncate(cap);
        self.raw.replace_inner(reversed);

        // Clamp display offset in case some lines went off.
        self.display_offset = min(self.display_offset, self.history_size());

        // Reflow the primary cursor, or clamp it if reflow is disabled.
        if !reflow {
            self.cursor.pos.col = min(self.cursor.pos.col, Column(columns - 1));
        } else if self.cursor.pos.col == columns
            && !self[self.cursor.pos.row][Column(columns - 1)].wrapline()
        {
            self.cursor.should_wrap = true;
            self.cursor.pos.col -= 1;
        } else {
            self.cursor.pos = self.cursor.pos.grid_clamp(self, Boundary::Cursor);
        }

        // Clamp the saved cursor to the grid.
        self.saved_cursor.pos.col = min(self.saved_cursor.pos.col, Column(columns - 1));

        self.reflow_remap = remap;
    }
}
