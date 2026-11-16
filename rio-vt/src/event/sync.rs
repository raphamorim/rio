//! The lock on a terminal.
//!
//! Two kinds of thread take it. The reader thread parses into the
//! terminal in a hot loop: lock, parse a chunk, unlock, read the next
//! chunk. The renderer, the UI thread and an embedder take it briefly
//! and rarely: a frame snapshot, a keystroke, a resize. A plain mutex
//! serves the first kind and starves the second: a running thread that
//! unlocks and locks again beats a waiter every time, because the
//! waiter has to be woken and scheduled first, and a waiter that is
//! still spinning rather than parked when the lock is let go is not
//! even there to be handed it. Under sustained output the reader is
//! exactly such a loop, so a frame could wait for a whole burst.
//!
//! The second kind locks through [`TerminalLock::lock`], which counts
//! itself as demand while it waits. The reader checks
//! [`TerminalLock::has_demand`] when it lets go of a chunk and, if a
//! thread is waiting, hands the lock on fairly and then
//! [`TerminalLock::yield_to_demand`]s: it waits until the demanders
//! have taken the lock, or a short bound passes, before it asks for
//! the lock again. That is the handoff an unfair mutex never does on
//! its own, and it holds whether the waiter was parked or still
//! spinning. The counter is a scheduling hint, not a synchronization
//! boundary: the mutex orders the data, the bound limits staleness.

use parking_lot::{Mutex, MutexGuard};
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(any(feature = "pty", test))]
use std::time::{Duration, Instant};

/// How long [`TerminalLock::yield_to_demand`] waits for a demander to
/// take the lock before the reader carries on. A demanding critical
/// section, the renderer's snapshot or an input event, is
/// microseconds; this bounds the reader's stall if the demander was
/// descheduled or gave up.
#[cfg(any(feature = "pty", test))]
pub(crate) const HANDOFF_TIMEOUT: Duration = Duration::from_millis(1);

pub struct TerminalLock<T> {
    data: Mutex<T>,
    /// Threads inside [`Self::lock`] that do not hold the data yet.
    demand: AtomicUsize,
    /// Yields that gave up at the bound with a demander still waiting:
    /// it was descheduled that long. A diagnostic, zero on a quiet
    /// machine.
    #[cfg(any(feature = "pty", test))]
    bound_hits: AtomicUsize,
}

impl<T> TerminalLock<T> {
    pub fn new(data: T) -> TerminalLock<T> {
        TerminalLock {
            data: Mutex::new(data),
            demand: AtomicUsize::new(0),
            #[cfg(any(feature = "pty", test))]
            bound_hits: AtomicUsize::new(0),
        }
    }

    /// Lock the terminal, counting as demand while waiting. For every
    /// thread but the reader.
    pub fn lock(&self) -> MutexGuard<'_, T> {
        self.demand.fetch_add(1, Ordering::Relaxed);
        let guard = self.data.lock();
        self.demand.fetch_sub(1, Ordering::Relaxed);
        guard
    }

    /// Lock the terminal without signalling demand. For the reader
    /// thread, whose hot loop is what demand is measured against.
    #[cfg(any(feature = "pty", test))]
    pub(crate) fn lock_reader(&self) -> MutexGuard<'_, T> {
        self.data.lock()
    }

    /// Try to lock the terminal without signalling demand, for the
    /// reader thread; `None` if another thread holds it.
    #[cfg(any(feature = "pty", test))]
    pub(crate) fn try_lock_reader(&self) -> Option<MutexGuard<'_, T>> {
        self.data.try_lock()
    }

    /// Whether a thread is waiting in [`Self::lock`].
    #[inline]
    #[cfg(any(feature = "pty", test))]
    pub(crate) fn has_demand(&self) -> bool {
        self.demand.load(Ordering::Relaxed) > 0
    }

    /// For the reader, holding nothing: if threads wait in
    /// [`Self::lock`], wait until they have taken the lock, or
    /// [`HANDOFF_TIMEOUT`] passes, before asking for it again. Spins
    /// briefly, then yields the processor, which is what a waiter that
    /// has not run yet needs most.
    #[cfg(any(feature = "pty", test))]
    pub(crate) fn yield_to_demand(&self) {
        if !self.has_demand() {
            return;
        }
        let deadline = Instant::now() + HANDOFF_TIMEOUT;
        let mut spins = 0u32;
        while self.has_demand() {
            if Instant::now() >= deadline {
                self.bound_hits.fetch_add(1, Ordering::Relaxed);
                return;
            }
            if spins < 64 {
                std::hint::spin_loop();
            } else {
                std::thread::yield_now();
            }
            spins += 1;
        }
    }

    /// How many yields gave up at the bound with a demander still
    /// waiting. Zero unless the machine is overloaded.
    #[cfg(test)]
    pub(crate) fn bound_hits(&self) -> usize {
        self.bound_hits.load(Ordering::Relaxed)
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
        lock.yield_to_demand();
        assert_eq!(lock.bound_hits(), 0, "the yield did not wait at all");
    }

    // A reader that lets go fairly and yields does not get the lock
    // back before the demander has it: when the yield returns the
    // demander holds the lock or has let go of it, so the reader's
    // next lock sees its work.
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

            MutexGuard::unlock_fair(held);
            let hits = lock.bound_hits();
            lock.yield_to_demand();
            if lock.bound_hits() == hits {
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
    // holds it, only costs the yielding reader the bound.
    #[test]
    fn yielding_gives_up_after_the_bound() {
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
        assert_eq!(lock.bound_hits(), 1);
        assert!(
            waited < HANDOFF_TIMEOUT * 50,
            "waited far past the bound: {waited:?}"
        );
        assert!(lock.has_demand(), "the demander is still waiting");

        release_tx.send(()).unwrap();
        holder.join().unwrap();
        demander.join().unwrap();
        assert!(!lock.has_demand());
    }
}
