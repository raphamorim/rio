// grid/row.rs was originally taken from Alacritty
// https://github.com/alacritty/alacritty/blob/e35e5ad14fce8456afdd89f2b392b9924bb27471/alacritty_terminal/src/grid/row.rs
// which is licensed under Apache 2.0 license.

use crate::grid::GridSquare;
use crate::pos::Column;
use core::cmp::min;
use std::cmp::max;
use std::ops::{Index, IndexMut, Range, RangeFrom, RangeFull, RangeTo, RangeToInclusive};
use std::{ptr, slice};

/// A row in the grid.
#[derive(Clone, Debug)]
pub struct Row<T> {
    pub inner: Vec<T>,

    /// Maximum number of occupied entries.
    ///
    /// This is the upper bound on the number of elements in the row, which have been modified
    /// since the last reset. All cells after this point are guaranteed to be equal.
    pub(crate) occ: usize,

    /// Set when at least one cell in the row contains a kitty Unicode
    /// graphics-protocol placeholder (U+10EEEE). The renderer skips the
    /// placeholder scan on rows where this is `false`.
    pub kitty_virtual_placeholder: bool,

    pub has_extras: bool,

    /// Per-row dirty bit set on every write through `IndexMut` /
    /// `last_mut` / `iter_mut` / `reset` / `append*` / `front_split_off`.
    /// Read + cleared by the renderer's snapshot path so it can copy
    /// only the rows that changed since the last snapshot. Defaults
    /// to `true` on `new()` and `default()` so a freshly-constructed
    /// row triggers its first snapshot.
    pub dirty: bool,
}

impl<T> Default for Row<T> {
    fn default() -> Self {
        Self {
            inner: Vec::new(),
            occ: 0,
            kitty_virtual_placeholder: false,
            has_extras: false,
            dirty: true,
        }
    }
}

impl<T: PartialEq> PartialEq for Row<T> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<T: Clone + Default> Row<T> {
    /// Create a new terminal row.
    ///
    /// Ideally the `template` should be `Copy` in all performance sensitive scenarios.
    pub fn new(columns: usize) -> Row<T> {
        debug_assert!(columns >= 1);

        let mut inner: Vec<T> = Vec::with_capacity(columns);

        // This is a slightly optimized version of `std::vec::Vec::resize`.
        unsafe {
            let mut ptr = inner.as_mut_ptr();

            for _ in 1..columns {
                ptr::write(ptr, T::default());
                ptr = ptr.offset(1);
            }
            ptr::write(ptr, T::default());

            inner.set_len(columns);
        }

        Row {
            inner,
            occ: 0,
            kitty_virtual_placeholder: false,
            has_extras: false,
            dirty: true,
        }
    }

    /// Copy `src` into `self` in place, reusing the existing `inner` Vec
    /// allocation. Equivalent to `*self = src.clone()` but skips the
    /// per-call `Vec` allocation when `self.inner.capacity() >=
    /// src.inner.len()` (the common case for renderer frame buffers).
    /// Does not touch `self.dirty` — the snapshot path manages that
    /// flag on the source row, not on the destination buffer.
    #[inline]
    pub fn copy_from(&mut self, src: &Self) {
        self.inner.clone_from(&src.inner);
        self.occ = src.occ;
        self.kitty_virtual_placeholder = src.kitty_virtual_placeholder;
        self.has_extras = src.has_extras;
    }

    /// Increase the number of columns in the row.
    #[inline]
    pub fn grow(&mut self, columns: usize) {
        if self.inner.len() >= columns {
            return;
        }

        self.inner.resize_with(columns, T::default);
    }

    pub fn shrink(&mut self, columns: usize) -> Option<Vec<T>>
    where
        T: GridSquare,
    {
        if self.inner.len() <= columns {
            return None;
        }

        // Split off cells for a new row.
        let mut new_row = self.inner.split_off(columns);
        let index = new_row
            .iter()
            .rposition(|c| !c.is_empty())
            .map_or(0, |i| i + 1);
        new_row.truncate(index);

        self.occ = min(self.occ, columns);
        self.dirty = true;

        if new_row.is_empty() {
            None
        } else {
            Some(new_row)
        }
    }

    /// Reset all cells in the row to the `template` cell.
    #[inline]
    pub fn reset(&mut self, template: &T)
    where
        T: GridSquare,
    {
        debug_assert!(!self.inner.is_empty());

        // Always reset every cell in the row. The previous implementation
        // skipped untouched cells when the template's discriminant matched
        // the rightmost cell — that optimization was based on the inline
        // bg-color field on the cell, which no longer exists.
        let len = self.inner.len();
        for item in &mut self.inner[0..len] {
            item.reset(template);
        }
        self.occ = 0;
        self.kitty_virtual_placeholder = false;
        self.has_extras = false;
        self.dirty = true;
    }
}

#[allow(clippy::len_without_is_empty)]
impl<T> Row<T> {
    #[inline]
    pub fn from_vec(vec: Vec<T>, occ: usize) -> Row<T> {
        Row {
            inner: vec,
            occ,
            kitty_virtual_placeholder: false,
            has_extras: true,
            dirty: true,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Number of occupied entries — the upper bound on cells modified since
    /// the last reset. Exposed as an accessor so cross-crate consumers
    /// (rio-backend tests) can read it without the field being public.
    #[inline]
    pub fn occ(&self) -> usize {
        self.occ
    }

    #[inline]
    pub fn last(&self) -> Option<&T> {
        self.inner.last()
    }

    #[inline]
    pub fn last_mut(&mut self) -> Option<&mut T> {
        self.occ = self.inner.len();
        self.dirty = true;
        self.inner.last_mut()
    }

    #[inline]
    pub fn append(&mut self, vec: &mut Vec<T>)
    where
        T: GridSquare,
    {
        self.occ += vec.len();
        self.dirty = true;
        self.has_extras = true;
        self.inner.append(vec);
    }

    #[inline]
    pub fn append_front(&mut self, mut vec: Vec<T>) {
        self.occ += vec.len();
        self.dirty = true;
        self.has_extras = true;

        vec.append(&mut self.inner);
        self.inner = vec;
    }

    #[inline]
    pub fn front_split_off(&mut self, at: usize) -> Vec<T> {
        self.occ = self.occ.saturating_sub(at);
        self.dirty = true;

        let mut split = self.inner.split_off(at);
        std::mem::swap(&mut split, &mut self.inner);
        split
    }

    #[inline]
    pub fn is_clear(&self) -> bool
    where
        T: GridSquare,
    {
        self.inner.iter().all(GridSquare::is_empty)
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
        self.dirty = true;
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
        self.dirty = true;
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
        self.dirty = true;
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
        self.dirty = true;
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
        self.dirty = true;
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
        self.dirty = true;
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
        self.occ = max(self.occ, *index.end);
        self.dirty = true;
        &mut self.inner[..=(index.end.0)]
    }
}
