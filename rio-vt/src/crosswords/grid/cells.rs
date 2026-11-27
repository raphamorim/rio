// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! A row's cells: a `Vec` of its own, or a slot of a slab that a block of
//! rows shares.
//!
//! History comes and goes a block at a time: compressed [`BLOCK_ROWS`]
//! rows at once, recycled in order as output scrolls. A buffer per row is
//! a small allocation the allocator keeps once freed (macOS doesn't lower
//! a process's footprint for a freed malloc of any size), so compressing
//! the history would hand its memory back to the allocator, not to the OS.
//! The storage instead gives its rows slots of [`Slab`]s: [`SLAB_ROWS`]
//! rows' cells in one mapping straight from the OS, unmapped once every
//! row let go of its slot (it was compressed, or took a buffer of its
//! own to grow).
//!
//! A slab-backed [`Cells`] keeps a `Vec` over its slot (capacity: the
//! slot's width) so reads and in-place writes are a `Vec`'s; anything that
//! would reallocate it moves the cells to a `Vec` of their own first.
//! Only types without drop glue go in slabs (the cell types do): a slot's
//! values are never dropped, only unmapped.
//!
//! [`BLOCK_ROWS`]: super::compress::BLOCK_ROWS

use std::fmt;
use std::mem::{self, ManuallyDrop};
use std::ops::{Deref, DerefMut};
use std::ptr::{self, NonNull};
use std::sync::Arc;

/// Rows per slab: a compressed block's worth.
pub(crate) const SLAB_ROWS: usize = super::compress::BLOCK_ROWS;

/// One mapping holding `slots` rows of `width` cells.
pub(crate) struct Slab<T> {
    ptr: NonNull<T>,
    bytes: usize,
    width: usize,
}

// The slab is only memory; the rows using its slots each own theirs.
unsafe impl<T: Send> Send for Slab<T> {}
unsafe impl<T: Sync> Sync for Slab<T> {}

impl<T> Slab<T> {
    /// A slab of `slots` rows of `width` cells, or None: the cells have
    /// drop glue, or the mapping failed (rows then get `Vec`s).
    fn new(slots: usize, width: usize) -> Option<Slab<T>> {
        if mem::needs_drop::<T>() || width == 0 || mem::size_of::<T>() == 0 {
            return None;
        }
        let bytes = slots.checked_mul(width)?.checked_mul(mem::size_of::<T>())?;
        let ptr = NonNull::new(take(bytes, mem::align_of::<T>())?.cast::<T>())?;
        #[cfg(test)]
        LIVE.with(|n| n.set(n.get() + 1));
        Some(Slab { ptr, bytes, width })
    }

    /// The first cell of `slot`.
    fn slot(&self, slot: usize) -> *mut T {
        // In bounds: slots are handed out below `slots`.
        unsafe { self.ptr.as_ptr().add(slot * self.width) }
    }
}

#[cfg(test)]
thread_local! {
    /// Slabs mapped on this thread and not yet unmapped, for tests.
    static LIVE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Slabs this thread mapped that are still mapped.
#[cfg(test)]
pub(crate) fn live_slabs() -> usize {
    LIVE.with(|n| n.get())
}

impl<T> Drop for Slab<T> {
    fn drop(&mut self) {
        #[cfg(test)]
        LIVE.with(|n| n.set(n.get().wrapping_sub(1)));
        unsafe { give_back(self.ptr.as_ptr().cast(), self.bytes, mem::align_of::<T>()) };
    }
}

/// Mappings let go of most recently, kept for the next slab of their
/// size: blocks decoded to be read, then dropped once compressed again,
/// would otherwise fault in fresh pages on every read. On macOS a kept
/// mapping's pages are marked reusable, which takes them off the
/// footprint at once and back, usually without faulting, when it's used
/// again; so more are kept there. Elsewhere a kept mapping stays
/// resident, so a few.
static KEPT: std::sync::Mutex<Vec<(usize, usize, usize)>> =
    std::sync::Mutex::new(Vec::new());
const KEEP: usize = if cfg!(target_os = "macos") { 64 } else { 8 };

/// A mapping of `bytes`: one kept, or a new one.
fn take(bytes: usize, align: usize) -> Option<*mut u8> {
    if let Ok(mut kept) = KEPT.lock() {
        if let Some(i) = kept.iter().position(|&(_, b, a)| b == bytes && a == align) {
            let ptr = kept.swap_remove(i).0 as *mut u8;
            #[cfg(target_os = "macos")]
            unsafe {
                libc::madvise(ptr.cast(), bytes, libc::MADV_FREE_REUSE);
            }
            return Some(ptr);
        }
    }
    map(bytes, align)
}

/// Keep the mapping for `take`, or unmap it.
unsafe fn give_back(ptr: *mut u8, bytes: usize, align: usize) {
    if let Ok(mut kept) = KEPT.lock() {
        if kept.len() < KEEP {
            #[cfg(target_os = "macos")]
            libc::madvise(ptr.cast(), bytes, libc::MADV_FREE_REUSABLE);
            kept.push((ptr as usize, bytes, align));
            return;
        }
    }
    unmap(ptr, bytes, align);
}

#[cfg(unix)]
fn map(bytes: usize, _align: usize) -> Option<*mut u8> {
    // Anonymous pages: zeroed, page aligned, and given back whole.
    let ptr = unsafe {
        libc::mmap(
            ptr::null_mut(),
            bytes,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANON,
            -1,
            0,
        )
    };
    (ptr != libc::MAP_FAILED).then_some(ptr.cast())
}

#[cfg(unix)]
unsafe fn unmap(ptr: *mut u8, bytes: usize, _align: usize) {
    libc::munmap(ptr.cast(), bytes);
}

#[cfg(windows)]
fn map(bytes: usize, _align: usize) -> Option<*mut u8> {
    use windows_sys::Win32::System::Memory::{
        VirtualAlloc, MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE,
    };
    let ptr = unsafe {
        VirtualAlloc(ptr::null(), bytes, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE)
    };
    (!ptr.is_null()).then_some(ptr.cast())
}

#[cfg(windows)]
unsafe fn unmap(ptr: *mut u8, _bytes: usize, _align: usize) {
    use windows_sys::Win32::System::Memory::{VirtualFree, MEM_RELEASE};
    VirtualFree(ptr.cast(), 0, MEM_RELEASE);
}

#[cfg(not(any(unix, windows)))]
fn map(bytes: usize, align: usize) -> Option<*mut u8> {
    let layout = std::alloc::Layout::from_size_align(bytes, align).ok()?;
    let ptr = unsafe { std::alloc::alloc(layout) };
    (!ptr.is_null()).then_some(ptr)
}

#[cfg(not(any(unix, windows)))]
unsafe fn unmap(ptr: *mut u8, bytes: usize, align: usize) {
    let layout = std::alloc::Layout::from_size_align_unchecked(bytes, align);
    std::alloc::dealloc(ptr, layout);
}

/// Hands out the slots of a slab in order, mapping a new one as each is
/// used up: rows made one after another (a history growing, output
/// scrolling) share slabs, and leave them together.
pub(crate) struct SlabCursor<T> {
    slab: Option<Arc<Slab<T>>>,
    next: usize,
}

impl<T> Default for SlabCursor<T> {
    fn default() -> Self {
        SlabCursor {
            slab: None,
            next: 0,
        }
    }
}

// A copy of a storage takes its rows' cells as `Vec`s; its own cursor
// starts on a slab of its own.
impl<T> Clone for SlabCursor<T> {
    fn clone(&self) -> Self {
        SlabCursor::default()
    }
}

impl<T> fmt::Debug for SlabCursor<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SlabCursor")
            .field("next", &self.next)
            .finish()
    }
}

impl<T: Default> SlabCursor<T> {
    /// `len` default cells, in the next slot (a `Vec` when slabs can't
    /// hold them).
    pub(crate) fn cells(&mut self, len: usize) -> Cells<T> {
        let fits = self
            .slab
            .as_ref()
            .is_some_and(|s| s.width == len && self.next < SLAB_ROWS);
        if !fits {
            self.slab = Slab::new(SLAB_ROWS, len).map(Arc::new);
            self.next = 0;
        }
        let Some(slab) = &self.slab else {
            let mut vec = Vec::with_capacity(len);
            vec.resize_with(len, T::default);
            return Cells::from(vec);
        };
        let start = slab.slot(self.next);
        self.next += 1;
        unsafe {
            for i in 0..len {
                ptr::write(start.add(i), T::default());
            }
            // The slot is this row's alone (handed out once), its cells
            // written above; the `Vec` is never let reallocate or free it.
            let vec = Vec::from_raw_parts(start, len, len);
            Cells {
                vec: ManuallyDrop::new(vec),
                slab: Some(slab.clone()),
            }
        }
    }
}

/// A row's cells: a `Vec`, read and written as one. See the module docs.
pub struct Cells<T> {
    vec: ManuallyDrop<Vec<T>>,
    /// The slab `vec` is a slot of; `None` when `vec` owns its buffer.
    slab: Option<Arc<Slab<T>>>,
}

impl<T> Cells<T> {
    #[inline]
    pub const fn new() -> Cells<T> {
        Cells {
            vec: ManuallyDrop::new(Vec::new()),
            slab: None,
        }
    }

    /// Whether the cells are in a slab.
    #[inline]
    pub fn in_slab(&self) -> bool {
        self.slab.is_some()
    }

    /// Before an operation growing the cells to `len`: a slot too small
    /// moves them to a `Vec` of their own.
    #[inline]
    fn ensure(&mut self, len: usize) {
        if self.slab.is_some() && len > self.vec.capacity() {
            self.own(len);
        }
    }

    #[cold]
    #[inline(never)]
    fn own(&mut self, capacity: usize) {
        let len = self.vec.len();
        let mut vec = Vec::with_capacity(capacity.max(len));
        unsafe {
            // Moved out: the slot's copies are never dropped (no drop
            // glue) and the slot isn't used again.
            ptr::copy_nonoverlapping(self.vec.as_ptr(), vec.as_mut_ptr(), len);
            vec.set_len(len);
        }
        self.vec = ManuallyDrop::new(vec);
        self.slab = None;
    }

    /// Move the cells to a slot of `slabs` with room for `capacity`, unless
    /// they're in one of that size already (or slabs can't hold them):
    /// before they grow to it, or to leave a slab the other rows left.
    pub(crate) fn move_into(&mut self, slabs: &mut SlabCursor<T>, capacity: usize)
    where
        T: Default,
    {
        let len = self.vec.len();
        if len > capacity || (self.slab.is_some() && self.vec.capacity() == capacity) {
            return;
        }
        let mut moved = slabs.cells(capacity);
        if moved.slab.is_none() {
            return;
        }
        unsafe {
            // Moved, not copied: the old cells have no drop glue (they fit
            // a slab) and aren't read again; the slot's defaults past `len`
            // are left as they are.
            ptr::copy_nonoverlapping(self.vec.as_ptr(), moved.vec.as_mut_ptr(), len);
            moved.vec.set_len(len);
            self.vec.set_len(0);
        }
        *self = moved;
    }

    /// The cells as a `Vec` of their own.
    #[inline]
    pub fn into_vec(mut self) -> Vec<T> {
        if self.slab.is_some() {
            self.own(0);
        }
        let vec = unsafe { ManuallyDrop::take(&mut self.vec) };
        mem::forget(self);
        vec
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.vec.capacity()
    }

    #[inline]
    pub fn as_ptr(&self) -> *const T {
        self.vec.as_ptr()
    }

    #[inline]
    pub fn push(&mut self, value: T) {
        self.ensure(self.vec.len() + 1);
        self.vec.push(value);
    }

    #[inline]
    pub fn truncate(&mut self, len: usize) {
        self.vec.truncate(len);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.vec.clear();
    }

    #[inline]
    pub fn resize_with(&mut self, len: usize, f: impl FnMut() -> T) {
        self.ensure(len);
        self.vec.resize_with(len, f);
    }

    #[inline]
    pub fn extend_from_slice(&mut self, other: &[T])
    where
        T: Clone,
    {
        self.ensure(self.vec.len() + other.len());
        self.vec.extend_from_slice(other);
    }

    /// Move `other`'s cells onto the end, leaving it empty.
    #[inline]
    pub fn append(&mut self, other: &mut Vec<T>) {
        self.ensure(self.vec.len() + other.len());
        self.vec.append(other);
    }

    /// Drop the first `count` cells, moving the rest to the front.
    #[inline]
    pub fn remove_front(&mut self, count: usize) {
        self.vec.drain(..count);
    }

    /// The cells from `at` on, as a `Vec`; these stay `..at`.
    #[inline]
    pub fn split_off(&mut self, at: usize) -> Vec<T> {
        if self.slab.is_none() {
            return self.vec.split_off(at);
        }
        let len = self.vec.len();
        assert!(at <= len, "split_off at {at} past {len}");
        let mut tail = Vec::with_capacity(len - at);
        unsafe {
            // Moved out of the slot (see `own`).
            ptr::copy_nonoverlapping(
                self.vec.as_ptr().add(at),
                tail.as_mut_ptr(),
                len - at,
            );
            tail.set_len(len - at);
            self.vec.set_len(at);
        }
        tail
    }

    /// The cells before `at`, as a `Vec`; these become the ones from `at`.
    #[inline]
    pub fn split_front(&mut self, at: usize) -> Vec<T> {
        let tail = self.split_off(at);
        mem::replace(self, Cells::from(tail)).into_vec()
    }
}

impl<T> Drop for Cells<T> {
    #[inline]
    fn drop(&mut self) {
        if self.slab.is_none() {
            unsafe { ManuallyDrop::drop(&mut self.vec) };
        }
        // A slot's cells have no drop glue; the slab unmaps with its last
        // row.
    }
}

impl<T> From<Vec<T>> for Cells<T> {
    #[inline]
    fn from(vec: Vec<T>) -> Cells<T> {
        Cells {
            vec: ManuallyDrop::new(vec),
            slab: None,
        }
    }
}

impl<T> FromIterator<T> for Cells<T> {
    #[inline]
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Cells::from(Vec::from_iter(iter))
    }
}

impl<T> Default for Cells<T> {
    #[inline]
    fn default() -> Self {
        Cells::new()
    }
}

impl<T> Deref for Cells<T> {
    type Target = [T];

    #[inline]
    fn deref(&self) -> &[T] {
        &self.vec
    }
}

impl<T> DerefMut for Cells<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.vec
    }
}

/// A copy is a `Vec` of its own.
impl<T: Clone> Clone for Cells<T> {
    #[inline]
    fn clone(&self) -> Self {
        Cells::from(self.vec.to_vec())
    }

    #[inline]
    fn clone_from(&mut self, source: &Self) {
        self.ensure(source.len());
        // `Vec`'s, which reuses the buffer (`ManuallyDrop`'s would not).
        let vec: &mut Vec<T> = &mut self.vec;
        vec.clone_from(&*source.vec);
    }
}

impl<T: PartialEq> PartialEq for Cells<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self[..] == other[..]
    }
}

impl<T: fmt::Debug> fmt::Debug for Cells<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self[..].fmt(f)
    }
}

impl<'a, T> IntoIterator for &'a Cells<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut Cells<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_read_and_write_like_a_vec_and_move_out_to_grow() {
        let mut cursor = SlabCursor::<u64>::default();
        let mut a = cursor.cells(4);
        let mut b = cursor.cells(4);
        assert!(a.in_slab() && b.in_slab());
        assert_eq!(&a[..], &[0; 4]);
        a[1] = 7;
        b.fill(3);
        assert_eq!(&a[..], &[0, 7, 0, 0]);
        assert_eq!(&b[..], &[3; 4]);
        // In place, within the slot.
        a.truncate(2);
        a.push(9);
        a.resize_with(4, || 1);
        assert!(a.in_slab());
        assert_eq!(&a[..], &[0, 7, 9, 1]);
        a.remove_front(1);
        assert_eq!(&a[..], &[7, 9, 1]);
        let tail = a.split_off(1);
        assert_eq!((&a[..], &tail[..]), (&[7][..], &[9, 1][..]));
        // Past the slot: a Vec of its own, the slot left alone.
        b.push(4);
        assert!(!b.in_slab());
        assert_eq!(&b[..], &[3, 3, 3, 3, 4]);
        let mut c = cursor.cells(4);
        c.clone_from(&Cells::from(vec![5; 3]));
        assert_eq!(&c[..], &[5; 3]);
        assert_eq!(c.clone(), c);
        let front = c.split_front(1);
        assert_eq!((&front[..], &c[..]), (&[5][..], &[5, 5][..]));
    }

    #[test]
    fn a_slab_goes_with_its_last_row() {
        let mut cursor = SlabCursor::<u64>::default();
        let rows: Vec<Cells<u64>> = (0..SLAB_ROWS).map(|_| cursor.cells(8)).collect();
        let slab = Arc::downgrade(rows[0].slab.as_ref().unwrap());
        // The next row starts another slab: the cursor lets go of this one.
        let next = cursor.cells(8);
        assert!(!Arc::ptr_eq(
            next.slab.as_ref().unwrap(),
            &slab.upgrade().unwrap()
        ));
        let mut rows = rows;
        rows.truncate(1);
        assert!(slab.upgrade().is_some());
        drop(rows);
        assert!(slab.upgrade().is_none());
        // A width of its own starts a slab too.
        assert_eq!(cursor.cells(3).len(), 3);
    }

    #[test]
    fn cells_with_drop_glue_get_vecs() {
        let mut cursor = SlabCursor::<String>::default();
        let mut cells = cursor.cells(2);
        assert!(!cells.in_slab());
        cells[0].push('x');
        assert_eq!(cells[0], "x");
    }
}
