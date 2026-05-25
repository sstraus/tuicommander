//! Grid resize and reflow.

use std::cmp::{Ordering, max, min};
use std::mem;

use crate::index::{Boundary, Column, Line};
use crate::term::cell::{Flags, ResetDiscriminant};

use crate::grid::row::Row;
use crate::grid::{Dimensions, Grid, GridCell};

/// Controls which rows participate in reflow during column resize.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReflowMode {
    /// No reflow — rows are truncated/padded.
    None,
    /// Reflow all rows (screen + history). Original alacritty behavior.
    All,
    /// Reflow only history (scrollback) rows; screen rows are truncated/padded.
    /// Preserves cursor-addressed TUI positioning on the visible screen while
    /// keeping scrollback readable across resize cycles.
    HistoryOnly,
}

impl<T: GridCell + Default + PartialEq> Grid<T> {
    /// Resize the grid's width and/or height.
    pub fn resize<D>(&mut self, reflow: ReflowMode, lines: usize, columns: usize)
    where
        T: ResetDiscriminant<D>,
        D: PartialEq,
    {
        // Use empty template cell for resetting cells due to resize.
        let template = mem::take(&mut self.cursor.template);

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
    /// Alacritty keeps the cursor at the bottom of the terminal as long as there
    /// is scrollback available. Once scrollback is exhausted, new lines are
    /// simply added to the bottom of the screen.
    fn grow_lines<D>(&mut self, target: usize)
    where
        T: ResetDiscriminant<D>,
        D: PartialEq,
    {
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
        self.saved_cursor.point.line += from_history;
        self.cursor.point.line += from_history;

        self.display_offset = self.display_offset.saturating_sub(lines_added);
        self.decrease_scroll_limit(lines_added);
    }

    /// Remove lines from the visible area.
    ///
    /// The behavior in Terminal.app and iTerm.app is to keep the cursor at the
    /// bottom of the screen. This is achieved by pushing history "out the top"
    /// of the terminal window.
    ///
    /// Alacritty takes the same approach.
    fn shrink_lines<D>(&mut self, target: usize)
    where
        T: ResetDiscriminant<D>,
        D: PartialEq,
    {
        // Scroll up to keep content inside the window.
        let required_scrolling = (self.cursor.point.line.0 as usize + 1).saturating_sub(target);
        if required_scrolling > 0 {
            self.scroll_up(&(Line(0)..Line(self.lines as i32)), required_scrolling);

            // Clamp cursors to the new viewport size.
            self.cursor.point.line = min(self.cursor.point.line, Line(target as i32 - 1));
        }

        // Clamp saved cursor, since only primary cursor is scrolled into viewport.
        self.saved_cursor.point.line = min(self.saved_cursor.point.line, Line(target as i32 - 1));

        self.raw.rotate((self.lines - target) as isize);
        self.raw.shrink_visible_lines(target);
        self.lines = target;
    }

    /// Grow number of columns in each row, reflowing if necessary.
    fn grow_columns(&mut self, reflow: ReflowMode, columns: usize) {
        // Storage layout after take_all() + rezero():
        //   inner[0] = bottom screen, ..., inner[L-1] = top screen,
        //   inner[L] = newest history, ..., inner[L+H-1] = oldest history.
        // So: screen rows have i < self.lines, history rows have i >= self.lines.
        let screen_lines = self.lines;

        let reflow_for = |i: usize| -> bool {
            match reflow {
                ReflowMode::None => false,
                ReflowMode::All => true,
                ReflowMode::HistoryOnly => i >= screen_lines,
            }
        };

        // Check if a row needs to be unwrapped (joined with the row above).
        // Screen rows merge on any WRAPLINE (standard terminal behavior — the
        // shell expects this after SIGWINCH). History rows require reflow_wrap
        // to avoid merging stale wraps from previous widths.
        let should_reflow = |row: &Row<T>, buf_idx: usize| -> bool {
            let len = Column(row.len());
            reflow_for(buf_idx)
                && len.0 > 0
                && len < columns
                && row[len - 1].flags().contains(Flags::WRAPLINE)
                && (buf_idx < screen_lines || row.reflow_wrap)
        };

        self.columns = columns;

        let mut reversed: Vec<Row<T>> = Vec::with_capacity(self.raw.len());
        // Track the original buffer index of each row pushed to `reversed`,
        // so we can determine history/screen for the `should_reflow` check.
        let mut reversed_idx: Vec<usize> = Vec::with_capacity(self.raw.len());
        let mut cursor_line_delta = 0;

        let any_screen_reflow = reflow == ReflowMode::All;

        // Remove the linewrap special case, by moving the cursor outside of the grid.
        if self.cursor.input_needs_wrap && any_screen_reflow {
            self.cursor.input_needs_wrap = false;
            self.cursor.point.column += 1;
        }

        let mut rows = self.raw.take_all();

        for (i, mut row) in rows.drain(..).enumerate().rev() {
            // Check if reflowing should be performed on the last pushed row.
            // Also require the current row to be a history row — screen rows must
            // never be absorbed into a history row (HistoryOnly boundary guard).
            let last_buf_idx = reversed_idx.last().copied().unwrap_or(0);
            let last_row = match reversed.last_mut() {
                Some(last_row) if should_reflow(last_row, last_buf_idx) && reflow_for(i) => {
                    last_row
                }
                _ => {
                    reversed.push(row);
                    reversed_idx.push(i);
                    continue;
                }
            };

            // Remove wrap flag before appending additional cells.
            if let Some(cell) = last_row.last_mut() {
                cell.flags_mut().remove(Flags::WRAPLINE);
            }

            // Remove leading spacers when reflowing wide char to the previous line.
            let mut last_len = last_row.len();
            if last_len >= 1
                && last_row[Column(last_len - 1)]
                    .flags()
                    .contains(Flags::LEADING_WIDE_CHAR_SPACER)
            {
                last_row.shrink(last_len - 1);
                last_len -= 1;
            }

            // Don't try to pull more cells from the next line than available.
            let mut num_wrapped = columns - last_len;
            let len = min(row.len(), num_wrapped);

            // Insert leading spacer when there's not enough room for reflowing wide char.
            let mut cells = if row[Column(len - 1)].flags().contains(Flags::WIDE_CHAR) {
                num_wrapped -= 1;

                let mut cells = row.front_split_off(len - 1);

                let mut spacer = T::default();
                spacer.flags_mut().insert(Flags::LEADING_WIDE_CHAR_SPACER);
                cells.push(spacer);

                cells
            } else {
                row.front_split_off(len)
            };

            // Add removed cells to previous row and reflow content.
            last_row.append(&mut cells);

            let cursor_buffer_line = self.lines - self.cursor.point.line.0 as usize - 1;

            if i == cursor_buffer_line && any_screen_reflow {
                // Resize cursor's line and reflow the cursor if necessary.
                let mut target = self.cursor.point.sub(self, Boundary::Cursor, num_wrapped);

                // Clamp to the last column, if no content was reflown with the cursor.
                if target.column.0 == 0 && row.is_clear() {
                    self.cursor.input_needs_wrap = true;
                    target = target.sub(self, Boundary::Cursor, 1);
                }
                self.cursor.point.column = target.column;

                // Get required cursor line changes. Since `num_wrapped` is smaller than `columns`
                // this will always be either `0` or `1`.
                let line_delta = self.cursor.point.line - target.line;

                if row.is_clear() {
                    last_row.reflow_wrap = row.reflow_wrap;
                    if line_delta != 0 {
                        continue;
                    }
                    // Cursor at Line(0): sub() clamped by Boundary::Cursor so
                    // line_delta is 0 even though the row was fully absorbed into
                    // the previous entry.  Position cursor at end of merged row.
                    self.cursor.point.column = Column(columns - 1);
                    cursor_line_delta += 1;
                    continue;
                }

                cursor_line_delta += line_delta.0 as usize;
            } else if row.is_clear() {
                last_row.reflow_wrap = row.reflow_wrap;

                if i < self.display_offset {
                    // Since we removed a line, rotate down the viewport.
                    self.display_offset = self.display_offset.saturating_sub(1);
                }

                // Rotate cursor down if content below them was pulled from history.
                if i < cursor_buffer_line {
                    self.cursor.point.line += 1;
                }

                // Don't push line into the new buffer.
                continue;
            }

            if let Some(cell) = last_row.last_mut() {
                // Set wrap flag if next line still has cells.
                cell.flags_mut().insert(Flags::WRAPLINE);
            }
            last_row.reflow_wrap = true;

            reversed.push(row);
            reversed_idx.push(i);
        }

        // Make sure we have at least the viewport filled.
        // Append blanks at the end of `reversed` so that after the final
        // drain(..).rev() they become the topmost screen rows (inner[0..delta]).
        if reversed.len() < self.lines {
            let delta = self.lines - reversed.len();
            self.cursor.point.line = max(self.cursor.point.line - delta as i32, Line(0));
            reversed.resize_with(self.lines, || Row::new(columns));
        }

        // Pull content down to put cursor in correct position, or move cursor up if there's no
        // more lines to delete below the cursor.
        if cursor_line_delta != 0 {
            let cursor_buffer_line = self.lines - self.cursor.point.line.0 as usize - 1;
            let available = min(cursor_buffer_line, reversed.len() - self.lines);
            let overflow = cursor_line_delta.saturating_sub(available);
            reversed.truncate(reversed.len() + overflow - cursor_line_delta);
            self.cursor.point.line = max(self.cursor.point.line - overflow, Line(0));
        }

        // Reverse iterator and fill all rows that are still too short.
        let mut new_raw = Vec::with_capacity(reversed.len());
        for mut row in reversed.drain(..).rev() {
            if row.len() < columns {
                row.grow(columns);
            }
            new_raw.push(row);
        }

        self.raw.replace_inner(new_raw);

        // Clamp display offset in case lines above it got merged.
        self.display_offset = min(self.display_offset, self.history_size());
    }

    /// Shrink number of columns in each row, reflowing if necessary.
    fn shrink_columns(&mut self, reflow: ReflowMode, columns: usize) {
        self.columns = columns;

        // Same storage layout as grow_columns: screen at i < self.lines,
        // history at i >= self.lines.
        let screen_lines = self.lines;
        let reflow_for = |i: usize| -> bool {
            match reflow {
                ReflowMode::None => false,
                ReflowMode::All => true,
                ReflowMode::HistoryOnly => i >= screen_lines,
            }
        };
        let any_screen_reflow = reflow == ReflowMode::All;

        // Remove the linewrap special case, by moving the cursor outside of the grid.
        if self.cursor.input_needs_wrap && any_screen_reflow {
            self.cursor.input_needs_wrap = false;
            self.cursor.point.column += 1;
        }

        let mut new_raw = Vec::with_capacity(self.raw.len());
        let mut buffered: Option<Vec<T>> = None;

        let mut rows = self.raw.take_all();
        for (i, mut row) in rows.drain(..).enumerate().rev() {
            let reflow_this = reflow_for(i);

            // Append lines left over from the previous row.
            // Only prepend if this row is also a history row — screen rows must never
            // receive history overflow (HistoryOnly boundary guard). The history row
            // that produced the overflow was already pushed with WRAPLINE set, so
            // discarding buffered here leaves history consistent.
            if let Some(buffered) = buffered.take() {
                if reflow_this {
                    // Add a column for every cell added before the cursor, if it goes beyond the
                    // new width it is then later reflown.
                    let cursor_buffer_line = self.lines - self.cursor.point.line.0 as usize - 1;
                    if i == cursor_buffer_line {
                        self.cursor.point.column += buffered.len();
                    }

                    row.append_front(buffered);
                }
            }

            loop {
                // Remove all cells which require reflowing.
                let mut wrapped = match row.shrink(columns) {
                    Some(wrapped) if reflow_this => wrapped,
                    _ => {
                        let cursor_buffer_line = self.lines - self.cursor.point.line.0 as usize - 1;
                        if reflow_this
                            && i == cursor_buffer_line
                            && self.cursor.point.column > columns
                        {
                            // If there are empty cells before the cursor, we assume it is explicit
                            // whitespace and need to wrap it like normal content.
                            Vec::new()
                        } else {
                            // Since it fits, just push the existing line without any reflow.
                            new_raw.push(row);
                            break;
                        }
                    }
                };

                // Insert spacer if a wide char would be wrapped into the last column.
                if row.len() >= columns
                    && row[Column(columns - 1)].flags().contains(Flags::WIDE_CHAR)
                {
                    let mut spacer = T::default();
                    spacer.flags_mut().insert(Flags::LEADING_WIDE_CHAR_SPACER);

                    let wide_char = mem::replace(&mut row[Column(columns - 1)], spacer);
                    wrapped.insert(0, wide_char);
                }

                // Remove wide char spacer before shrinking.
                let len = wrapped.len();
                if len > 0
                    && wrapped[len - 1]
                        .flags()
                        .contains(Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    if len == 1 {
                        row[Column(columns - 1)].flags_mut().insert(Flags::WRAPLINE);
                        row.reflow_wrap = true;
                        new_raw.push(row);
                        break;
                    } else {
                        // Remove the leading spacer from the end of the wrapped row.
                        wrapped[len - 2].flags_mut().insert(Flags::WRAPLINE);
                        wrapped.truncate(len - 1);
                    }
                }

                new_raw.push(row);

                // Set line as wrapped if cells got removed.
                if let Some(r) = new_raw.last_mut() {
                    if let Some(cell) = r.last_mut() {
                        cell.flags_mut().insert(Flags::WRAPLINE);
                    }
                    r.reflow_wrap = true;
                }

                // Buffer overflow for next iteration only if i >= 1: at i=0
                // (bottom of ring buffer) there is no subsequent iteration to
                // consume the buffer, so overflow must be handled inline via
                // the else branch below.
                if wrapped
                    .last()
                    .map(|c| c.flags().contains(Flags::WRAPLINE) && i >= 1)
                    .unwrap_or(false)
                    && wrapped.len() < columns
                {
                    // Make sure previous wrap flag doesn't linger around.
                    if let Some(cell) = wrapped.last_mut() {
                        cell.flags_mut().remove(Flags::WRAPLINE);
                    }

                    // Add removed cells to start of next row.
                    buffered = Some(wrapped);
                    break;
                } else {
                    // Reflow cursor if a line below it is deleted.
                    let cursor_buffer_line = self.lines - self.cursor.point.line.0 as usize - 1;
                    if (i == cursor_buffer_line && self.cursor.point.column < columns)
                        || i < cursor_buffer_line
                    {
                        self.cursor.point.line = max(self.cursor.point.line - 1, Line(0));
                    }

                    // Reflow the cursor if it is on this line beyond the width.
                    if i == cursor_buffer_line && self.cursor.point.column >= columns {
                        // Since only a single new line is created, we subtract only `columns`
                        // from the cursor instead of reflowing it completely.
                        self.cursor.point.column -= columns;
                    }

                    // Make sure new row is at least as long as new width.
                    let occ = wrapped.len();
                    if occ < columns {
                        wrapped.resize_with(columns, T::default);
                    }
                    row = Row::from_vec(wrapped, occ);

                    if i < self.display_offset {
                        // Since we added a new line, rotate up the viewport.
                        self.display_offset += 1;
                    }
                }
            }
        }

        // Reverse iterator and use it as the new grid storage.
        let mut reversed: Vec<Row<T>> = new_raw.drain(..).rev().collect();
        reversed.truncate(self.max_scroll_limit + self.lines);
        self.raw.replace_inner(reversed);

        // Clamp display offset in case some lines went off.
        self.display_offset = min(self.display_offset, self.history_size());

        // Reflow the primary cursor, or clamp it if reflow is disabled.
        if !any_screen_reflow {
            self.cursor.point.column = min(self.cursor.point.column, Column(columns - 1));
        } else if self.cursor.point.column == columns
            && !self[self.cursor.point.line][Column(columns - 1)]
                .flags()
                .contains(Flags::WRAPLINE)
        {
            self.cursor.input_needs_wrap = true;
            self.cursor.point.column -= 1;
        } else {
            self.cursor.point = self.cursor.point.grid_clamp(self, Boundary::Cursor);
        }

        // Clamp the saved cursor to the grid.
        self.saved_cursor.point.column = min(self.saved_cursor.point.column, Column(columns - 1));
    }
}
