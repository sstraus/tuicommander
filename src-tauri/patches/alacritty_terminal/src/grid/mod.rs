//! A specialized 2D grid implementation optimized for use in a terminal.

use std::cmp::{max, min};
use std::ops::{Bound, Deref, Index, IndexMut, Range, RangeBounds};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::index::{Column, Line, Point};
use crate::term::cell::{Flags, ResetDiscriminant};
use crate::vte::ansi::{CharsetIndex, StandardCharset};

pub mod resize;
mod row;
mod storage;
#[cfg(test)]
mod tests;

pub use self::resize::ReflowMode;
pub use self::row::Row;
use self::storage::Storage;

pub trait GridCell: Sized {
    /// Check if the cell contains any content.
    fn is_empty(&self) -> bool;

    /// Check if the cell draws nothing but its background colour.
    ///
    /// Reflow drops such cells at the end of a row instead of wrapping them.
    fn is_blank(&self) -> bool {
        self.is_empty()
    }

    /// Perform an opinionated cell reset based on a template cell.
    fn reset(&mut self, template: &Self);

    fn flags(&self) -> &Flags;
    fn flags_mut(&mut self) -> &mut Flags;
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Cursor<T> {
    /// The location of this cursor.
    pub point: Point,

    /// Template cell when using this cursor.
    pub template: T,

    /// Currently configured graphic character sets.
    pub charsets: Charsets,

    /// Tracks if the next call to input will need to first handle wrapping.
    ///
    /// This is true after the last column is set with the input function. Any function that
    /// implicitly sets the line or column needs to set this to false to avoid wrapping twice.
    ///
    /// Tracking `input_needs_wrap` makes it possible to not store a cursor position that exceeds
    /// the number of columns, which would lead to index out of bounds when interacting with arrays
    /// without sanitization.
    pub input_needs_wrap: bool,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct Charsets([StandardCharset; 4]);

impl Index<CharsetIndex> for Charsets {
    type Output = StandardCharset;

    fn index(&self, index: CharsetIndex) -> &StandardCharset {
        &self.0[index as usize]
    }
}

impl IndexMut<CharsetIndex> for Charsets {
    fn index_mut(&mut self, index: CharsetIndex) -> &mut StandardCharset {
        &mut self.0[index as usize]
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Scroll {
    Delta(i32),
    PageUp,
    PageDown,
    Top,
    Bottom,
}

/// Why lines are moving up, which decides whether they become history.
///
/// The distinction is invisible on screen and decisive in the buffer, so it is
/// named at the call site rather than inferred from the region: both sources
/// can present a region starting at line 0.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollSource {
    /// Content reached the bottom and was pushed off — a linefeed, or an index
    /// past the bottom margin. It was printed, so it belongs in history.
    Overflow,
    /// A control removed lines: DL (`CSI M`) or SU (`CSI S`). Nothing scrolled
    /// anywhere, so nothing may enter history and the absolute row coordinate
    /// must not move.
    Control,
}

/// Grid based terminal content storage.
///
/// ```notrust
/// ┌─────────────────────────┐  <-- max_scroll_limit + lines
/// │                         │
/// │      UNINITIALIZED      │
/// │                         │
/// ├─────────────────────────┤  <-- self.raw.inner.len()
/// │                         │
/// │      RESIZE BUFFER      │
/// │                         │
/// ├─────────────────────────┤  <-- self.history_size() + lines
/// │                         │
/// │     SCROLLUP REGION     │
/// │                         │
/// ├─────────────────────────┤v lines
/// │                         │|
/// │     VISIBLE  REGION     │|
/// │                         │|
/// ├─────────────────────────┤^ <-- display_offset
/// │                         │
/// │    SCROLLDOWN REGION    │
/// │                         │
/// └─────────────────────────┘  <-- zero
///                           ^
///                        columns
/// ```
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Grid<T> {
    /// Current cursor for writing data.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub cursor: Cursor<T>,

    /// Last saved cursor.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub saved_cursor: Cursor<T>,

    /// Lines in the grid. Each row holds a list of cells corresponding to the
    /// columns in that row.
    raw: Storage<T>,

    /// Number of columns.
    columns: usize,

    /// Number of visible lines.
    lines: usize,

    /// Offset of displayed area.
    ///
    /// If the displayed region isn't at the bottom of the screen, it stays
    /// stationary while more text is emitted. The scrolling implementation
    /// updates this offset accordingly.
    display_offset: usize,

    /// Maximum number of lines in history.
    max_scroll_limit: usize,

    /// Total number of lines that have ever scrolled off the top of the active
    /// region into history, counted across the grid's whole lifetime. Unlike
    /// `history_size()` it never plateaus or shrinks on eviction, so callers can
    /// derive an eviction-stable absolute row coordinate (`total_scrolled() -
    /// history_size()` = lines evicted from the history top so far).
    ///
    /// Defaulted on deserialize so pre-existing `grid.json` ref fixtures (recorded
    /// before this field existed) still load; it is excluded from `PartialEq`, so the
    /// recording-based ref comparisons are unaffected by its runtime value.
    #[cfg_attr(feature = "serde", serde(default))]
    lines_scrolled: usize,
}

impl<T: GridCell + Default + PartialEq> Grid<T> {
    pub fn new(lines: usize, columns: usize, max_scroll_limit: usize) -> Grid<T> {
        Grid {
            raw: Storage::with_capacity(lines, columns),
            max_scroll_limit,
            display_offset: 0,
            saved_cursor: Cursor::default(),
            cursor: Cursor::default(),
            lines,
            columns,
            lines_scrolled: 0,
        }
    }

    /// Total lines that have ever scrolled into history over the grid's lifetime.
    /// Monotonic: unlike `history_size()` it does not plateau or shrink when the
    /// scrollback cap evicts old lines, so `total_scrolled() - history_size()`
    /// yields the number of lines evicted from the history top — the base for an
    /// eviction-stable absolute row coordinate.
    #[inline]
    pub fn total_scrolled(&self) -> usize {
        self.lines_scrolled
    }

    /// Update the size of the scrollback history.
    pub fn update_history(&mut self, history_size: usize) {
        let current_history_size = self.history_size();
        if current_history_size > history_size {
            let removed = current_history_size - history_size;
            let lost_content = (0..removed)
                .any(|offset| !self.raw[Line(self.topmost_line().0 + offset as i32)].is_clear());
            self.raw.shrink_lines(removed);
            if lost_content {
                let oldest = self.topmost_line();
                self.raw[oldest].copy_origin_unknown = true;
            }
        }
        self.display_offset = min(self.display_offset, history_size);
        self.max_scroll_limit = history_size;
    }

    pub fn scroll_display(&mut self, scroll: Scroll) {
        self.display_offset = match scroll {
            Scroll::Delta(count) => min(
                max((self.display_offset as i32) + count, 0) as usize,
                self.history_size(),
            ),
            Scroll::PageUp => min(self.display_offset + self.lines, self.history_size()),
            Scroll::PageDown => self.display_offset.saturating_sub(self.lines),
            Scroll::Top => self.history_size(),
            Scroll::Bottom => 0,
        };
    }

    /// Discard the `count` history rows closest to the screen (clamped to the
    /// history size) and return how many were removed.
    ///
    /// For a caller that learns a row it already scrolled out is about to be
    /// printed again. Keeps a scrolled-up viewport on the same rows. Rewinds `total_scrolled` by the same amount, so the
    /// eviction base (`total_scrolled() - history_size()`) is unchanged.
    pub fn drop_newest_history(&mut self, count: usize) -> usize {
        let old_history_size = self.history_size();
        let count = min(count, old_history_size);
        let lost_content = (-(count as i32)..0).any(|line| !self.raw[Line(line)].is_clear());
        if count != 0 {
            self.raw.remove_newest_history(count);
            if lost_content {
                self.raw[Line(0)].copy_origin_unknown = true;
            }
            self.lines_scrolled = self.lines_scrolled.saturating_sub(count);
            // The oldest rows keep their place, so a viewport scrolled up keeps
            // its top row by moving `count` rows closer to the screen.
            self.display_offset = self.display_offset.saturating_sub(count);
        }
        count
    }

    /// Remove duplicate history immediately before `newer` freshly repainted rows.
    /// Retained oldest rows keep their absolute coordinates, as for newest removal.
    pub fn drop_history_before(&mut self, newer: usize, count: usize) -> usize {
        let count = min(count, self.history_size().saturating_sub(newer));
        if count == 0 {
            return 0;
        }
        self.raw.remove_history_range(newer, count);
        self.raw[Line(-(newer as i32))].copy_origin_unknown = true;
        self.lines_scrolled = self.lines_scrolled.saturating_sub(count);
        if self.display_offset > newer {
            self.display_offset = self.display_offset.saturating_sub(count).max(newer);
        }
        count
    }

    fn increase_scroll_limit(&mut self, count: usize) {
        let count = min(count, self.max_scroll_limit - self.history_size());
        if count != 0 {
            self.raw.initialize(count, self.columns);
        }
    }

    fn decrease_scroll_limit(&mut self, count: usize) {
        let count = min(count, self.history_size());
        if count != 0 {
            self.raw.shrink_lines(min(count, self.history_size()));
            self.display_offset = min(self.display_offset, self.history_size());
        }
    }

    #[inline]
    pub fn scroll_down<D>(&mut self, region: &Range<Line>, positions: usize)
    where
        T: ResetDiscriminant<D>,
        D: PartialEq,
    {
        // When rotating the entire region, just reset everything.
        if region.end - region.start <= positions {
            for i in (region.start.0..region.end.0).map(Line::from) {
                self.raw[i].reset(&self.cursor.template);
            }

            return;
        }

        // Which implementation we can use depends on the existence of a scrollback history.
        //
        // Since a scrollback history prevents us from rotating the entire buffer downwards, we
        // instead have to rely on a slower, swap-based implementation.
        if self.max_scroll_limit == 0 {
            // Swap the lines fixed at the bottom to their target positions after rotation.
            //
            // Since we've made sure that the rotation will never rotate away the entire region, we
            // know that the position of the fixed lines before the rotation must already be
            // visible.
            //
            // We need to start from the top, to make sure the fixed lines aren't swapped with each
            // other.
            let screen_lines = self.screen_lines() as i32;
            for i in (region.end.0..screen_lines).map(Line::from) {
                self.raw.swap(i, i - positions as i32);
            }

            // Rotate the entire line buffer downward.
            self.raw.rotate_down(positions);

            // Ensure all new lines are fully cleared.
            for i in (0..positions).map(Line::from) {
                self.raw[i].reset(&self.cursor.template);
            }

            // Swap the fixed lines at the top back into position.
            for i in (0..region.start.0).map(Line::from) {
                self.raw.swap(i, i + positions);
            }
        } else {
            // Subregion rotation.
            let range = (region.start + positions).0..region.end.0;
            for line in range.rev().map(Line::from) {
                self.raw.swap(line, line - positions);
            }

            let range = region.start.0..(region.start + positions).0;
            for line in range.rev().map(Line::from) {
                self.raw[line].reset(&self.cursor.template);
            }
        }
    }

    /// Move lines at the bottom toward the top, treating the removed lines as
    /// content that left the screen.
    ///
    /// This is the performance-sensitive part of scrolling.
    pub fn scroll_up<D>(&mut self, region: &Range<Line>, positions: usize)
    where
        T: ResetDiscriminant<D>,
        D: PartialEq,
    {
        self.scroll_up_with(region, positions, ScrollSource::Overflow);
    }

    /// Move lines at the bottom toward the top, saying where the scroll came
    /// from.
    ///
    /// The two sources differ in one thing only, and it is not cosmetic: a line
    /// that overflowed the bottom was printed and then pushed off, so it
    /// belongs in history, while a line a control removed was never scrolled
    /// anywhere and must leave no trace. Routing DL and SU through `Overflow`
    /// is what made an agent TUI that repaints with DL manufacture scrollback
    /// rows it never printed (#834-1878).
    pub fn scroll_up_with<D>(
        &mut self,
        region: &Range<Line>,
        positions: usize,
        source: ScrollSource,
    ) where
        T: ResetDiscriminant<D>,
        D: PartialEq,
    {
        // DEFERRED (2026-06-24, re-evaluated 2026-07-06) — story 056-7545 (Ink
        // duplication into scrollback). RESOLVED FOR THE MAIN REPRO: raw-ring
        // capture (mdkb `ink-banner-dup-raw-ring-2026-07-06`) proved the grid is
        // FAITHFUL — Claude Code's Ink itself re-emits its full frame (banner
        // included) via ~28×CRLF (pushes the old frame into scrollback) + ESC[H +
        // per-line erase + reprint whenever its frame exceeds the viewport or the
        // width changes. The duplicates are IN the byte stream; any terminal
        // records them. Do NOT patch scroll_up for this. If a future repro shows
        // grid copies > raw emissions (compare search-buffer hits vs raw-ring
        // occurrences), only then suspect this code path again.

        // When rotating the entire region with fixed lines at the top, just reset everything.
        if region.end - region.start <= positions && region.start != 0 {
            for i in (region.start.0..region.end.0).map(Line::from) {
                self.raw[i].reset(&self.cursor.template);
            }

            return;
        }

        // Update display offset when not pinned to active area. A control
        // scroll adds nothing to history, so a user who is scrolled back must
        // stay where they are — shifting them would move the viewport under a
        // buffer that did not grow.
        if self.display_offset != 0 && source == ScrollSource::Overflow {
            self.display_offset = min(self.display_offset + positions, self.max_scroll_limit);
        }

        let discarded = positions.saturating_sub(self.max_scroll_limit - self.history_size());
        let evicts_predecessor = region.start == 0
            && source == ScrollSource::Overflow
            && (0..discarded)
                .any(|offset| !self.raw[Line(self.topmost_line().0 + offset as i32)].is_clear());

        // Only rotate the entire history if the active region starts at the top
        // *and* the lines are leaving the screen rather than being removed. A
        // control scroll takes the branch below, which rotates within the
        // region and touches neither history nor the counters.
        if region.start == 0 && source == ScrollSource::Overflow {
            // These lines scroll off the top of the active region into history.
            // Count them before `increase_scroll_limit` clamps to the cap, so the
            // total keeps climbing even once history is full and the oldest lines
            // are being evicted.
            self.lines_scrolled += positions;

            // Create scrollback for the new lines.
            self.increase_scroll_limit(positions);

            // Swap the lines fixed at the top to their target positions after rotation.
            //
            // Since we've made sure that the rotation will never rotate away the entire region, we
            // know that the position of the fixed lines before the rotation must already be
            // visible.
            //
            // We need to start from the bottom, to make sure the fixed lines aren't swapped with
            // each other.
            for i in (0..region.start.0).rev().map(Line::from) {
                self.raw.swap(i, i + positions);
            }

            // Rotate the entire line buffer upward.
            self.raw.rotate(-(positions as isize));

            // Swap the fixed lines at the bottom back into position.
            let screen_lines = self.screen_lines() as i32;
            for i in (region.end.0..screen_lines).rev().map(Line::from) {
                self.raw.swap(i, i - positions);
            }
        } else {
            // Rotate lines without moving anything into history.
            for i in (region.start.0..region.end.0 - positions as i32).map(Line::from) {
                self.raw.swap(i, i + positions);
            }
        }

        // Ensure all new lines are fully cleared.
        for i in (region.end.0 - positions as i32..region.end.0).map(Line::from) {
            self.raw[i].reset(&self.cursor.template);
        }
        if evicts_predecessor {
            let oldest = self.topmost_line();
            self.raw[oldest].copy_origin_unknown = true;
        }
    }

    pub fn clear_viewport<D>(&mut self)
    where
        T: ResetDiscriminant<D>,
        D: PartialEq,
    {
        // Determine how many lines to scroll up by.
        let end = Point::new(Line(self.lines as i32 - 1), Column(self.columns()));
        let mut iter = self.iter_from(end);
        while let Some(cell) = iter.prev() {
            if !cell.is_empty() || cell.point.line < 0 {
                break;
            }
        }
        debug_assert!(iter.point.line >= -1);
        let positions = (iter.point.line.0 + 1) as usize;
        let region = Line(0)..Line(self.lines as i32);

        // Clear the viewport.
        self.scroll_up(&region, positions);

        // ED2 replaces every visible row. Overflow can mark a newly blank row
        // as predecessor-lost after resetting it, so reset that provenance too.
        for line in (0..self.lines).map(Line::from) {
            self.raw[line].reset(&self.cursor.template);
        }
    }

    /// Completely reset the grid state.
    pub fn reset<D>(&mut self)
    where
        T: ResetDiscriminant<D>,
        D: PartialEq,
    {
        self.clear_history();

        self.saved_cursor = Cursor::default();
        self.cursor = Cursor::default();
        self.display_offset = 0;

        // Reset all visible lines.
        let range = self.topmost_line().0..(self.screen_lines() as i32);
        for line in range.map(Line::from) {
            self.raw[line].reset(&self.cursor.template);
        }
    }
}

impl<T> Grid<T> {
    /// Reset a visible region within the grid.
    pub fn reset_region<D, R: RangeBounds<Line>>(&mut self, bounds: R)
    where
        T: ResetDiscriminant<D> + GridCell + Default,
        D: PartialEq,
    {
        let start = match bounds.start_bound() {
            Bound::Included(line) => *line,
            Bound::Excluded(line) => *line + 1,
            Bound::Unbounded => Line(0),
        };

        let end = match bounds.end_bound() {
            Bound::Included(line) => *line + 1,
            Bound::Excluded(line) => *line,
            Bound::Unbounded => Line(self.screen_lines() as i32),
        };

        debug_assert!(start < self.screen_lines() as i32);
        debug_assert!(end <= self.screen_lines() as i32);

        for line in (start.0..end.0).map(Line::from) {
            self.raw[line].reset(&self.cursor.template);
        }
    }

    #[inline]
    pub fn clear_history(&mut self)
    where
        T: GridCell,
    {
        // Explicitly purge all lines from history without changing absolute row ids.
        let removed = self.history_size();
        let unknown_predecessor =
            (-(removed as i32)..0).any(|line| !self.raw[Line(line)].is_clear());
        self.raw.shrink_lines(removed);
        if unknown_predecessor {
            self.raw[Line(0)].copy_origin_unknown = true;
        }

        // Reset display offset.
        self.display_offset = 0;
    }

    /// Purge the history *and* the `total_scrolled` counter, so the grid starts a
    /// fresh absolute-row era.
    ///
    /// `clear_history` deliberately keeps `lines_scrolled` monotonic: consumers
    /// derive an eviction-stable absolute row id from `total_scrolled -
    /// history_size`, and a physical line must keep its id for life. That
    /// guarantee only holds *within* one era. The alternate screen is a
    /// different content universe that gets wiped on every enter/exit, so it
    /// gets its own era instead — the frame protocol flags the alt transition
    /// and consumers drop their row caches there.
    #[inline]
    pub fn reset_history_era(&mut self)
    where
        T: GridCell,
    {
        self.clear_history();
        self.lines_scrolled = 0;
    }

    /// This is used only for initializing after loading ref-tests.
    #[inline]
    pub fn initialize_all(&mut self)
    where
        T: GridCell + Default,
    {
        // Remove all cached lines to clear them of any content.
        self.truncate();

        // Initialize everything with empty new lines.
        self.raw
            .initialize(self.max_scroll_limit - self.history_size(), self.columns);
    }

    /// This is used only for truncating before saving ref-tests.
    #[inline]
    pub fn truncate(&mut self) {
        self.raw.truncate();
    }

    /// Iterate over all cells in the grid starting at a specific point.
    #[inline]
    pub fn iter_from(&self, point: Point) -> GridIterator<'_, T> {
        let end = Point::new(self.bottommost_line(), self.last_column());
        GridIterator {
            grid: self,
            point,
            end,
        }
    }

    /// Iterate over all visible cells.
    ///
    /// This is slightly more optimized than calling `Grid::iter_from` in combination with
    /// `Iterator::take_while`.
    #[inline]
    pub fn display_iter(&self) -> GridIterator<'_, T> {
        let last_column = self.last_column();
        let start = Point::new(Line(-(self.display_offset() as i32) - 1), last_column);
        let end_line = min(start.line + self.screen_lines(), self.bottommost_line());
        let end = Point::new(end_line, last_column);

        GridIterator {
            grid: self,
            point: start,
            end,
        }
    }

    #[inline]
    pub fn display_offset(&self) -> usize {
        self.display_offset
    }

    #[inline]
    pub fn cursor_cell(&mut self) -> &mut T {
        let point = self.cursor.point;
        &mut self[point.line][point.column]
    }
}

impl<T: PartialEq> PartialEq for Grid<T> {
    fn eq(&self, other: &Self) -> bool {
        // Compare struct fields and check result of grid comparison.
        self.raw.eq(&other.raw)
            && self.columns.eq(&other.columns)
            && self.lines.eq(&other.lines)
            && self.display_offset.eq(&other.display_offset)
    }
}

impl<T> Index<Line> for Grid<T> {
    type Output = Row<T>;

    #[inline]
    fn index(&self, index: Line) -> &Row<T> {
        &self.raw[index]
    }
}

impl<T> IndexMut<Line> for Grid<T> {
    #[inline]
    fn index_mut(&mut self, index: Line) -> &mut Row<T> {
        &mut self.raw[index]
    }
}

impl<T> Index<Point> for Grid<T> {
    type Output = T;

    #[inline]
    fn index(&self, point: Point) -> &T {
        &self[point.line][point.column]
    }
}

impl<T> IndexMut<Point> for Grid<T> {
    #[inline]
    fn index_mut(&mut self, point: Point) -> &mut T {
        &mut self[point.line][point.column]
    }
}

/// Grid dimensions.
pub trait Dimensions {
    /// Total number of lines in the buffer, this includes scrollback and visible lines.
    fn total_lines(&self) -> usize;

    /// Height of the viewport in lines.
    fn screen_lines(&self) -> usize;

    /// Width of the terminal in columns.
    fn columns(&self) -> usize;

    /// Index for the last column.
    #[inline]
    fn last_column(&self) -> Column {
        Column(self.columns() - 1)
    }

    /// Line farthest up in the grid history.
    #[inline]
    fn topmost_line(&self) -> Line {
        Line(-(self.history_size() as i32))
    }

    /// Line farthest down in the grid history.
    #[inline]
    fn bottommost_line(&self) -> Line {
        Line(self.screen_lines() as i32 - 1)
    }

    /// Number of invisible lines part of the scrollback history.
    #[inline]
    fn history_size(&self) -> usize {
        self.total_lines().saturating_sub(self.screen_lines())
    }
}

impl<G> Dimensions for Grid<G> {
    #[inline]
    fn total_lines(&self) -> usize {
        self.raw.len()
    }

    #[inline]
    fn screen_lines(&self) -> usize {
        self.lines
    }

    #[inline]
    fn columns(&self) -> usize {
        self.columns
    }
}

#[cfg(test)]
impl Dimensions for (usize, usize) {
    fn total_lines(&self) -> usize {
        self.0
    }

    fn screen_lines(&self) -> usize {
        self.0
    }

    fn columns(&self) -> usize {
        self.1
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Indexed<T> {
    pub point: Point,
    pub cell: T,
}

impl<T> Deref for Indexed<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &self.cell
    }
}

/// Grid cell iterator.
pub struct GridIterator<'a, T> {
    /// Immutable grid reference.
    grid: &'a Grid<T>,

    /// Current position of the iterator within the grid.
    point: Point,

    /// Last cell included in the iterator.
    end: Point,
}

impl<'a, T> GridIterator<'a, T> {
    /// Current iterator position.
    pub fn point(&self) -> Point {
        self.point
    }

    /// Cell at the current iterator position.
    pub fn cell(&self) -> &'a T {
        &self.grid[self.point]
    }
}

impl<'a, T> Iterator for GridIterator<'a, T> {
    type Item = Indexed<&'a T>;

    fn next(&mut self) -> Option<Self::Item> {
        // Stop once we've reached the end of the grid.
        if self.point >= self.end {
            return None;
        }

        match self.point {
            Point { column, .. } if column >= self.grid.last_column() => {
                self.point.column = Column(0);
                self.point.line += 1;
            }
            _ => self.point.column += Column(1),
        }

        Some(Indexed {
            cell: &self.grid[self.point],
            point: self.point,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.point >= self.end {
            return (0, Some(0));
        }

        let size = if self.point.line == self.end.line {
            (self.end.column - self.point.column).0
        } else {
            let cols_on_first_line = self.grid.columns.saturating_sub(self.point.column.0 + 1);
            let middle_lines = (self.end.line - self.point.line).0 as usize - 1;
            let cols_on_last_line = self.end.column + 1;

            cols_on_first_line + middle_lines * self.grid.columns + cols_on_last_line.0
        };

        (size, Some(size))
    }
}

/// Bidirectional iterator.
pub trait BidirectionalIterator: Iterator {
    fn prev(&mut self) -> Option<Self::Item>;
}

impl<T> BidirectionalIterator for GridIterator<'_, T> {
    fn prev(&mut self) -> Option<Self::Item> {
        let topmost_line = self.grid.topmost_line();
        let last_column = self.grid.last_column();

        // Stop once we've reached the end of the grid.
        if self.point <= Point::new(topmost_line, Column(0)) {
            return None;
        }

        match self.point {
            Point {
                column: Column(0), ..
            } => {
                self.point.column = last_column;
                self.point.line -= 1;
            }
            _ => self.point.column -= Column(1),
        }

        Some(Indexed {
            cell: &self.grid[self.point],
            point: self.point,
        })
    }
}
