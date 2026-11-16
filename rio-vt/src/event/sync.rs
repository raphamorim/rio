//! The lock on a terminal.
//!
//! Two kinds of thread take it. The reader thread parses into the
//! terminal in a hot loop: lock, parse a chunk, unlock, read the next
//! chunk. The renderer, the UI thread and an embedder take it briefly
//! and rarely: a frame snapshot, a keystroke, a resize. A plain mutex
//! serves the first kind and starves the second: a running thread that
//! unlocks and locks again beats a sleeping waiter every time, because
//! the waiter has to be woken and scheduled first, so under sustained
//! output a frame could wait for the whole burst.
//!
//! The second kind locks through [`TerminalLock::lock`], which counts
//! itself as demand while it waits. The reader checks
//! [`TerminalLock::has_demand`] between chunks and, holding nothing,
//! calls [`TerminalLock::yield_to_demand`], which sleeps until one
//! demander has had the lock and let go of it, or a short timeout
//! passes. The reader never competes for the lock while a demander
//! takes it, so the handoff holds whether the waiter was parked or
//! still spinning, and a demander that disappears costs the reader at
//! most the timeout.

use parking_lot::{Mutex, MutexGuard};
use parking_lot_core::{park, unpark_all, DEFAULT_PARK_TOKEN, DEFAULT_UNPARK_TOKEN};
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// How long [`TerminalLock::yield_to_demand`] waits for a demander to
/// take its turn before carrying on. A demanding critical section, the
/// renderer's snapshot or an input event, is microseconds; this bounds
/// the reader's stall if the demander was descheduled or gave up.
const HANDOFF_TIMEOUT: Duration = Duration::from_millis(1);

pub struct TerminalLock<T> {
    data: Mutex<T>,
    /// Threads inside [`Self::lock`] that do not hold the data yet.
    demand: AtomicUsize,
    /// Bumped each time a demanding holder lets go; what the reader
    /// sleeps on in [`Self::yield_to_demand`].
    handoff: AtomicU32,
    /// Yields that gave up after [`HANDOFF_TIMEOUT`] with the demander
    /// still waiting. A diagnostic: on a quiet machine it stays at zero.
    handoff_timeouts: AtomicUsize,
}

impl<T> TerminalLock<T> {
    pub fn new(data: T) -> TerminalLock<T> {
        TerminalLock {
            data: Mutex::new(data),
            demand: AtomicUsize::new(0),
            handoff: AtomicU32::new(0),
            handoff_timeouts: AtomicUsize::new(0),
        }
    }

    /// Lock the terminal, counting as demand while waiting. For every
    /// thread but the reader; the guard's release wakes a reader that
    /// yielded to it.
    pub fn lock(&self) -> DemandGuard<'_, T> {
        self.demand.fetch_add(1, Ordering::AcqRel);
        let guard = self.data.lock();
        self.demand.fetch_sub(1, Ordering::AcqRel);
        DemandGuard {
            guard: ManuallyDrop::new(guard),
            lock: self,
        }
    }

    /// Lock the terminal without signalling demand. For the reader
    /// thread, whose hot loop is what demand is measured against.
    pub fn lock_reader(&self) -> MutexGuard<'_, T> {
        self.data.lock()
    }

    /// Try to lock the terminal without signalling demand, for the
    /// reader thread; `None` if another thread holds it.
    pub fn try_lock_reader(&self) -> Option<MutexGuard<'_, T>> {
        self.data.try_lock()
    }

    /// Whether a thread is waiting in [`Self::lock`]. The reader polls
    /// this between chunks to cut its work short.
    #[inline]
    pub fn has_demand(&self) -> bool {
        self.demand.load(Ordering::Acquire) > 0
    }

    /// For the reader, holding nothing: if a thread waits in
    /// [`Self::lock`], sleep until one such thread has had the lock
    /// and let go of it, or [`HANDOFF_TIMEOUT`] passes. Returns at
    /// once when nobody waits. The handoff counter is read before the
    /// demand is checked again and validated under the parking lot's
    /// own lock, so a release between the check and the sleep is seen
    /// and the sleep skipped, never missed.
    pub fn yield_to_demand(&self) {
        if !self.has_demand() {
            return;
        }
        let generation = self.handoff.load(Ordering::Acquire);
        if !self.has_demand() {
            return;
        }
        let deadline = Instant::now() + HANDOFF_TIMEOUT;
        // SAFETY: the key is this lock's address, unique while it lives,
        // and the callbacks neither park nor panic.
        unsafe {
            park(
                self.park_key(),
                || {
                    self.handoff.load(Ordering::Acquire) == generation
                        && self.has_demand()
                },
                || {},
                |_, _| {
                    self.handoff_timeouts.fetch_add(1, Ordering::Relaxed);
                },
                DEFAULT_PARK_TOKEN,
                Some(deadline),
            );
        }
    }

    /// How many yields gave up on a demander that had not taken the
    /// lock within the timeout. Zero unless the machine is overloaded.
    pub fn handoff_timeouts(&self) -> usize {
        self.handoff_timeouts.load(Ordering::Relaxed)
    }

    fn park_key(&self) -> usize {
        &self.handoff as *const AtomicU32 as usize
    }

    /// A demanding holder let go: wake a reader sleeping for its turn.
    fn released(&self) {
        self.handoff.fetch_add(1, Ordering::AcqRel);
        // SAFETY: same key as `yield_to_demand`; no callback.
        unsafe {
            unpark_all(self.park_key(), DEFAULT_UNPARK_TOKEN);
        }
    }
}

/// The guard of [`TerminalLock::lock`]. Letting go wakes a reader that
/// yielded to the demand.
pub struct DemandGuard<'a, T> {
    guard: ManuallyDrop<MutexGuard<'a, T>>,
    lock: &'a TerminalLock<T>,
}

impl<T> Deref for DemandGuard<'_, T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        &self.guard
    }
}

impl<T> DerefMut for DemandGuard<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        &mut self.guard
    }
}

impl<T> Drop for DemandGuard<'_, T> {
    fn drop(&mut self) {
        // The data lock goes first, so the woken reader finds it free.
        // SAFETY: the guard is dropped exactly once, here.
        unsafe { ManuallyDrop::drop(&mut self.guard) };
        self.lock.released();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn spin_until(what: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !what() {
            assert!(Instant::now() < deadline, "timed out");
            std::thread::yield_now();
        }
    }

    // A thread blocked in `lock` is demand until it holds the lock; a
    // holder is not, and neither is the reader's lock.
    #[test]
    fn a_blocked_locker_is_demand_until_it_holds_the_lock() {
        let lock = Arc::new(TerminalLock::new(0u32));
        assert!(!lock.has_demand());
        let held = lock.lock_reader();
        assert!(!lock.has_demand(), "the reader's lock is not demand");

        let other = Arc::clone(&lock);
        let (acquired_tx, acquired) = std::sync::mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let mut guard = other.lock();
            *guard += 1;
            acquired_tx.send(other.has_demand()).unwrap();
        });
        spin_until(|| lock.has_demand());
        drop(held);
        assert!(
            !acquired.recv_timeout(Duration::from_secs(5)).unwrap(),
            "a holder is not demand"
        );
        waiter.join().unwrap();
        assert!(!lock.has_demand());
        assert_eq!(*lock.lock(), 1);
    }

    // With nobody waiting, yielding costs nothing and returns at once.
    #[test]
    fn yielding_to_no_demand_returns_at_once() {
        let lock = TerminalLock::new(());
        let started = Instant::now();
        lock.yield_to_demand();
        assert!(started.elapsed() < HANDOFF_TIMEOUT);
    }

    // A reader that lets go and yields does not get the lock back
    // before the demander had it: when the yield returns the demander
    // holds the lock or has let go of it, so the reader's next lock
    // sees its work. Whether the yield slept (the demander was still
    // waking) or returned at once (it had already taken the lock) is
    // the scheduler's business.
    #[test]
    fn yielding_lets_the_demander_go_first() {
        for _ in 0..50 {
            let lock = Arc::new(TerminalLock::new(false));
            let held = lock.lock_reader();

            let other = Arc::clone(&lock);
            let demander = std::thread::spawn(move || {
                let mut guard = other.lock();
                *guard = true;
                std::thread::sleep(Duration::from_micros(200));
                drop(guard);
            });
            spin_until(|| lock.has_demand());

            drop(held);
            let timeouts = lock.handoff_timeouts();
            lock.yield_to_demand();
            if lock.handoff_timeouts() == timeouts {
                assert!(!lock.has_demand(), "the demander has the lock or had it");
                assert!(*lock.lock_reader(), "the demander went first");
            }
            // Else the demander took longer than the bound to get
            // scheduled, which only an overloaded machine does, and
            // the reader rightly moved on.
            demander.join().unwrap();
        }
    }

    // A demander that cannot take the lock, because something else
    // holds it, only costs the yielding reader the timeout.
    #[test]
    fn yielding_gives_up_after_the_timeout() {
        let lock = Arc::new(TerminalLock::new(()));
        let (release_tx, release) = std::sync::mpsc::channel::<()>();
        let (held_tx, held) = std::sync::mpsc::channel::<()>();
        let holder_lock = Arc::clone(&lock);
        let holder = std::thread::spawn(move || {
            let guard = holder_lock.lock_reader();
            held_tx.send(()).unwrap();
            release.recv().unwrap();
            drop(guard);
        });
        held.recv_timeout(Duration::from_secs(5)).unwrap();
        let demander_lock = Arc::clone(&lock);
        let demander = std::thread::spawn(move || {
            drop(demander_lock.lock());
        });
        spin_until(|| lock.has_demand());

        let yielded = Instant::now();
        lock.yield_to_demand();
        let waited = yielded.elapsed();
        assert!(waited >= HANDOFF_TIMEOUT, "gave up early: {waited:?}");
        assert_eq!(lock.handoff_timeouts(), 1);
        assert!(
            waited < HANDOFF_TIMEOUT * 50,
            "waited far past the timeout: {waited:?}"
        );
        assert!(lock.has_demand(), "the demander is still waiting");

        release_tx.send(()).unwrap();
        holder.join().unwrap();
        demander.join().unwrap();
        assert!(!lock.has_demand());
    }

    // A release that lands between the reader's check and its sleep is
    // not missed: the sleep is skipped.
    #[test]
    fn a_release_before_the_sleep_is_not_missed() {
        let lock = Arc::new(TerminalLock::new(()));
        // Stand in for a demander that released after the handoff
        // generation was read: the generation moved on, so the park
        // validation fails and the reader returns without sleeping.
        let generation = lock.handoff.load(Ordering::Acquire);
        lock.demand.fetch_add(1, Ordering::AcqRel);
        lock.released();
        let started = Instant::now();
        unsafe {
            park(
                lock.park_key(),
                || {
                    lock.handoff.load(Ordering::Acquire) == generation
                        && lock.has_demand()
                },
                || {},
                |_, _| {},
                DEFAULT_PARK_TOKEN,
                Some(Instant::now() + HANDOFF_TIMEOUT),
            );
        }
        assert!(started.elapsed() < HANDOFF_TIMEOUT);
        lock.demand.fetch_sub(1, Ordering::AcqRel);
    }
}
