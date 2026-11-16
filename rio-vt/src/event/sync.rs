// event/sync.rs was originally taken from Alacritty
// https://github.com/alacritty/alacritty/blob/e35e5ad14fce8456afdd89f2b392b9924bb27471/alacritty_terminal/src/sync.rs
// which is licensed under Apache 2.0 license.

use parking_lot::{Mutex, MutexGuard};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A fair mutex.
///
/// Uses an extra lock to ensure that if one thread is waiting that it will get
/// the lock before a single thread can re-lock it.
pub struct FairMutex<T> {
    /// Data.
    data: Mutex<T>,
    /// Next-to-access.
    next: Mutex<()>,
    /// Threads inside [`Self::lock`] that do not hold the data yet.
    waiters: AtomicUsize,
}

impl<T> FairMutex<T> {
    /// Create a new fair mutex.
    pub fn new(data: T) -> FairMutex<T> {
        FairMutex {
            data: Mutex::new(data),
            next: Mutex::new(()),
            waiters: AtomicUsize::new(0),
        }
    }

    /// Whether a thread is waiting in [`Self::lock`]. The holder of a
    /// lease can poll this to cut its work short and hand the lock on,
    /// so a renderer waiting for its frame is not held for a whole
    /// burst of parsing.
    #[inline]
    pub fn has_waiters(&self) -> bool {
        self.waiters.load(Ordering::Acquire) > 0
    }

    /// Acquire a lease to reserve the mutex lock.
    ///
    /// This will prevent others from acquiring a terminal lock, but block if anyone else is
    /// already holding a lease.
    pub fn lease(&self) -> MutexGuard<'_, ()> {
        self.next.lock()
    }

    /// Lock the mutex.
    pub fn lock(&self) -> MutexGuard<'_, T> {
        self.waiters.fetch_add(1, Ordering::AcqRel);
        // Must bind to a temporary or the lock will be freed before going
        // into data.lock().
        let _next = self.next.lock();
        let guard = self.data.lock();
        self.waiters.fetch_sub(1, Ordering::AcqRel);
        guard
    }

    /// Unfairly lock the mutex.
    pub fn lock_unfair(&self) -> MutexGuard<'_, T> {
        self.data.lock()
    }

    /// Unfairly try to lock the mutex.
    pub fn try_lock_unfair(&self) -> Option<MutexGuard<'_, T>> {
        self.data.try_lock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn spin_until(what: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !what() {
            assert!(Instant::now() < deadline, "timed out");
            std::thread::yield_now();
        }
    }

    // A thread blocked in `lock` is visible to the holder, and no
    // longer once it holds the data.
    #[test]
    fn a_blocked_locker_is_a_waiter_until_it_holds_the_lock() {
        let mutex = Arc::new(FairMutex::new(0u32));
        assert!(!mutex.has_waiters());
        let held = mutex.lock();

        let other = Arc::clone(&mutex);
        let (acquired_tx, acquired) = std::sync::mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let mut guard = other.lock();
            *guard += 1;
            acquired_tx.send(other.has_waiters()).unwrap();
        });
        spin_until(|| mutex.has_waiters());
        drop(held);
        assert!(
            !acquired.recv_timeout(Duration::from_secs(5)).unwrap(),
            "a holder is not a waiter"
        );
        waiter.join().unwrap();
        assert!(!mutex.has_waiters());
        assert_eq!(*mutex.lock(), 1);
    }

    // The unfair entry points the PTY thread uses never count as
    // waiting; only `lock` does.
    #[test]
    fn unfair_locks_and_leases_are_not_waiters() {
        let mutex = FairMutex::new(());
        let lease = mutex.lease();
        let guard = mutex.try_lock_unfair().unwrap();
        assert!(!mutex.has_waiters());
        drop(guard);
        drop(lease);
        let _guard = mutex.lock_unfair();
        assert!(!mutex.has_waiters());
    }
}
