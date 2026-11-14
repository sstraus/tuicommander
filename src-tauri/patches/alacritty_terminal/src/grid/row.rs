//! Defines the Row type which makes up lines in the grid.

use std::cmp::{max, min};
use std::ops::{Index, IndexMut, Range, RangeFrom, RangeFull, RangeTo, RangeToInclusive};
use std::{iter, slice};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::grid::GridCell;
use crate::index::Column;
use crate::term::cell::ResetDiscriminant;

/// A row in the grid.
#[derive(Default, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Row<T> {
    inner: Vec<T>,

    /// Maximum number of occupied entries.
    ///
    /// This is the upper bound on the number of elements in the row, which have been modified
    /// since the last reset. All cells after this point are guaranteed to be equal.
    pub(crate) occ: usize,

    /// Set by shrink_columns when this row's WRAPLINE was created by a resize
    /// operation. grow_columns only merges rows that have this flag, preventing
    /// stale WRAPLINE from natural terminal wrapping from causing incorrect merges.
    #[cfg_attr(feature = "serde", serde(default))]
    pub reflow_wrap: bool,

    /// The predecessor of this retained row was discarded. Full erasure/reset
    /// starts new content and restores a known origin.
    #[cfg_attr(feature = "serde", serde(default))]
    pub copy_origin_unknown: bool,

    /// Set by `shrink` when it dropped background-coloured padding that reached
    /// the old right edge. Only such a row is coloured to the edge again on
    /// widen; a block that ended on the narrow edge stays as drawn (#2783-9c6e).
    #[cfg_attr(feature = "serde", serde(default))]
    pub edge_padding_trimmed: bool,
}

impl<T: PartialEq> PartialEq for Row<T> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<T: Default> Row<T> {
    /// Create a new terminal row.
    pub fn new(columns: usize) -> Row<T> {
        let inner = iter::repeat_with(T::default).take(columns).collect();

        Row {
            inner,
            occ: 0,
            reflow_wrap: false,
            copy_origin_unknown: false,
            edge_padding_trimmed: false,
        }
    }

    /// Increase the number of columns in the row.
    #[inline]
    pub fn grow(&mut self, columns: usize) {
        if self.inner.len() >= columns {
            return;
        }

        // Avoid geometric over-allocation on every terminal width increase.
        self.inner.reserve_exact(columns - self.inner.len());
        self.inner.resize_with(columns, T::default);
    }

    /// Increase the number of columns in the row, filling the new cells with `fill`.
    pub fn grow_with(&mut self, columns: usize, fill: &T)
    where
        T: Clone,
    {
        if self.inner.len() < columns {
            self.inner.resize(columns, fill.clone());
        }
    }

    /// Reduce the number of columns in the row.
    ///
    /// This will return all non-empty cells that were removed.
    pub fn shrink(&mut self, columns: usize) -> Option<Vec<T>>
    where
        T: GridCell,
    {
        if self.inner.len() <= columns {
            return None;
        }

        // Split off cells for a new row.
        let mut new_row = self.inner.split_off(columns);
        // split_off leaves the original allocation behind, even after a large resize.
        self.inner.shrink_to_fit();
        // Trailing blanks are padding to the old width, even with a background
        // colour: wrapping them would add coloured rows the program never drew.
        self.edge_padding_trimmed |= new_row
            .last()
            .is_some_and(|c| c.is_blank() && !c.is_empty());
        let index = new_row
            .iter()
            .rposition(|c| !c.is_blank())
            .map_or(0, |i| i + 1);
        new_row.truncate(index);
        new_row.shrink_to_fit();

        self.occ = min(self.occ, columns);

        if new_row.is_empty() {
            None
        } else {
            Some(new_row)
        }
    }

    /// Reset all cells in the row to the `template` cell.
    #[inline]
    pub fn reset<D>(&mut self, template: &T)
    where
        T: ResetDiscriminant<D> + GridCell,
        D: PartialEq,
    {
        debug_assert!(!self.inner.is_empty());

        // Mark all cells as dirty if template cell changed.
        let len = self.inner.len();
        if self.inner[len - 1].discriminant() != template.discriminant() {
            self.occ = len;
        }

        // Reset every dirty cell in the row.
        for item in &mut self.inner[0..self.occ] {
            item.reset(template);
        }

        self.occ = 0;
        self.reflow_wrap = false;
        self.copy_origin_unknown = false;
        self.edge_padding_trimmed = false;
    }
}

#[allow(clippy::len_without_is_empty)]
impl<T> Row<T> {
    #[inline]
    pub fn from_vec(vec: Vec<T>, occ: usize) -> Row<T> {
        Row {
            inner: vec,
            occ,
            reflow_wrap: false,
            copy_origin_unknown: false,
            edge_padding_trimmed: false,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline]
    pub fn last(&self) -> Option<&T> {
        self.inner.last()
    }

    #[inline]
    pub fn last_mut(&mut self) -> Option<&mut T> {
        self.occ = self.inner.len();
        self.inner.last_mut()
    }

    #[inline]
    pub fn append(&mut self, vec: &mut Vec<T>)
    where
        T: GridCell,
    {
        self.occ += vec.len();
        self.inner.append(vec);
    }

    #[inline]
    pub fn append_front(&mut self, mut vec: Vec<T>) {
        self.occ += vec.len();

        vec.append(&mut self.inner);
        self.inner = vec;
    }

    /// Check if all cells in the row are empty.
    #[inline]
    pub fn is_clear(&self) -> bool
    where
        T: GridCell,
    {
        self.inner.iter().all(GridCell::is_empty)
    }

    /// Check if every cell in the row draws nothing but its background colour.
    #[inline]
    pub fn is_blank(&self) -> bool
    where
        T: GridCell,
    {
        self.inner.iter().all(GridCell::is_blank)
    }

    #[inline]
    pub fn front_split_off(&mut self, at: usize) -> Vec<T> {
        self.occ = self.occ.saturating_sub(at);

        let mut split = self.inner.split_off(at);
        std::mem::swap(&mut split, &mut self.inner);
        split
    }
}

impl<'a, T> IntoIterator for &'a Row<T> {
    type IntoIter = slice::Iter<'a, T>;
    type Item = &'a T;

    #[inline]
    fn into_iter(self) -> slice::Iter<'a, T> {
        self.inner.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut Row<T> {
    type IntoIter = slice::IterMut<'a, T>;
    type Item = &'a mut T;

    #[inline]
    fn into_iter(self) -> slice::IterMut<'a, T> {
        self.occ = self.len();
        self.inner.iter_mut()
    }
}

impl<T> Index<Column> for Row<T> {
    type Output = T;

    #[inline]
    fn index(&self, index: Column) -> &T {
        &self.inner[index.0]
    }
}

impl<T> IndexMut<Column> for Row<T> {
    #[inline]
    fn index_mut(&mut self, index: Column) -> &mut T {
        self.occ = max(self.occ, *index + 1);
        &mut self.inner[index.0]
    }
}

impl<T> Index<Range<Column>> for Row<T> {
    type Output = [T];

    #[inline]
    fn index(&self, index: Range<Column>) -> &[T] {
        &self.inner[(index.start.0)..(index.end.0)]
    }
}

impl<T> IndexMut<Range<Column>> for Row<T> {
    #[inline]
    fn index_mut(&mut self, index: Range<Column>) -> &mut [T] {
        self.occ = max(self.occ, *index.end);
        &mut self.inner[(index.start.0)..(index.end.0)]
    }
}

impl<T> Index<RangeTo<Column>> for Row<T> {
    type Output = [T];

    #[inline]
    fn index(&self, index: RangeTo<Column>) -> &[T] {
        &self.inner[..(index.end.0)]
    }
}

impl<T> IndexMut<RangeTo<Column>> for Row<T> {
    #[inline]
    fn index_mut(&mut self, index: RangeTo<Column>) -> &mut [T] {
        self.occ = max(self.occ, *index.end);
        &mut self.inner[..(index.end.0)]
    }
}

impl<T> Index<RangeFrom<Column>> for Row<T> {
    type Output = [T];

    #[inline]
    fn index(&self, index: RangeFrom<Column>) -> &[T] {
        &self.inner[(index.start.0)..]
    }
}

impl<T> IndexMut<RangeFrom<Column>> for Row<T> {
    #[inline]
    fn index_mut(&mut self, index: RangeFrom<Column>) -> &mut [T] {
        self.occ = self.len();
        &mut self.inner[(index.start.0)..]
    }
}

impl<T> Index<RangeFull> for Row<T> {
    type Output = [T];

    #[inline]
    fn index(&self, _: RangeFull) -> &[T] {
        &self.inner[..]
    }
}

impl<T> IndexMut<RangeFull> for Row<T> {
    #[inline]
    fn index_mut(&mut self, _: RangeFull) -> &mut [T] {
        self.occ = self.len();
        &mut self.inner[..]
    }
}

impl<T> Index<RangeToInclusive<Column>> for Row<T> {
    type Output = [T];

    #[inline]
    fn index(&self, index: RangeToInclusive<Column>) -> &[T] {
        &self.inner[..=(index.end.0)]
    }
}

impl<T> IndexMut<RangeToInclusive<Column>> for Row<T> {
    #[inline]
    fn index_mut(&mut self, index: RangeToInclusive<Column>) -> &mut [T] {
        self.occ = max(self.occ, *index.end + 1);
        &mut self.inner[..=(index.end.0)]
    }
}

#[cfg(test)]
mod allocation_tests {
    use super::*;
    use crate::term::cell::Cell;

    /// Catches: split_off leaves the wide allocation behind after a narrow resize.
    #[test]
    fn shrink_releases_old_capacity_and_grow_does_not_double_it() {
        let mut row = Row::<Cell>::new(220);
        row[Column(219)].c = 'z';
        let removed = row.shrink(80).expect("retained tail content");
        assert_eq!(row.inner.capacity(), 80);
        assert_eq!(removed.last().unwrap().c, 'z');
        assert_eq!(removed.capacity(), removed.len());
        row.grow(221);
        assert_eq!(row.inner.capacity(), 221);
        assert_eq!(row[Column(220)].c, ' ');
        assert!(row.shrink(1).is_none());
        assert_eq!(row.inner.capacity(), 1);
    }

    /// Catches: the old unsafe initializer writes into a zero-capacity allocation.
    #[test]
    fn new_zero_columns_is_empty() {
        let row = Row::<Cell>::new(0);
        assert_eq!(row.len(), 0);
        assert_eq!(row.inner.capacity(), 0);
    }
}
