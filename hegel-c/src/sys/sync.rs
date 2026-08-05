//! The engine's blocking mutex and lazy-initialisation primitives.
//!
//! Both are built from atomics plus the platform park/unpark pair in
//! [`crate::sys`], so nothing here allocates thread-local storage, registers
//! a TLS destructor, or installs a process-global hook — the load-time state
//! that makes a shared library unsafe to `dlclose`.
//!
//! [`Mutex`] is a three-state futex lock: uncontended locking and unlocking
//! are a single atomic operation each, and a contended waiter parks on the
//! lock word itself. It does not track poisoning — every caller in the
//! engine treats a panic while a lock is held as "keep going with whatever
//! state is there", so there is nothing for a poison flag to say.
//!
//! [`Lazy`] is a lock-free "first result wins" cell: racing initialisers all
//! run, and the first to publish its value wins while the others drop theirs.
//! Every use in the engine initialises a pure function of constants, so which
//! racer wins is unobservable.

use alloc::boxed::Box;
use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU32, Ordering};

use once_cell::race::OnceBox;

use super::imp;

/// Nobody holds the lock.
const UNLOCKED: u32 = 0;
/// The lock is held and no thread is parked on it, so unlocking need not
/// wake anyone.
const LOCKED: u32 = 1;
/// The lock is held and at least one thread may be parked on it, so
/// unlocking must wake a waiter.
const CONTENDED: u32 = 2;

/// A mutual-exclusion lock around a `T`.
///
/// Modelled on `std::sync::Mutex` minus poisoning: [`lock`](Self::lock)
/// hands back the guard directly rather than a `Result`, and
/// [`try_lock`](Self::try_lock) reports contention as `None`. Like
/// `std::sync::Mutex` it is unconditionally unwind-safe: a panic while
/// the lock is held releases it and leaves the value however the panicking
/// code left it, which is exactly what the engine wants from its caches and
/// per-handle state.
pub struct Mutex<T> {
    state: AtomicU32,
    value: UnsafeCell<T>,
}

// SAFETY: the lock word serialises access to `value`, so sharing a `&Mutex<T>`
// between threads only ever hands `T` from one thread to another, which is
// exactly what `T: Send` permits.
unsafe impl<T: Send> Sync for Mutex<T> {}
// SAFETY: moving the mutex moves the `T` it owns.
unsafe impl<T: Send> Send for Mutex<T> {}

impl<T> core::panic::UnwindSafe for Mutex<T> {}
impl<T> core::panic::RefUnwindSafe for Mutex<T> {}

impl<T> Mutex<T> {
    /// A new unlocked mutex holding `value`.
    pub const fn new(value: T) -> Mutex<T> {
        Mutex {
            state: AtomicU32::new(UNLOCKED),
            value: UnsafeCell::new(value),
        }
    }

    /// Acquire the lock, blocking until it is free.
    pub fn lock(&self) -> MutexGuard<'_, T> {
        if self
            .state
            .compare_exchange(UNLOCKED, LOCKED, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            self.lock_contended();
        }
        MutexGuard {
            mutex: self,
            not_send: PhantomData,
        }
    }

    /// Acquire the lock if it is free right now, returning `None` rather
    /// than blocking if another thread holds it.
    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        self.state
            .compare_exchange(UNLOCKED, LOCKED, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| MutexGuard {
                mutex: self,
                not_send: PhantomData,
            })
    }

    #[cold]
    fn lock_contended(&self) {
        while self.state.swap(CONTENDED, Ordering::Acquire) != UNLOCKED {
            imp::park(&self.state, CONTENDED);
        }
    }

    fn unlock(&self) {
        if self.state.swap(UNLOCKED, Ordering::Release) == CONTENDED {
            imp::unpark(&self.state);
        }
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.try_lock() {
            Some(guard) => f.debug_struct("Mutex").field("value", &*guard).finish(),
            None => f.debug_struct("Mutex").field("value", &"<locked>").finish(),
        }
    }
}

/// Exclusive access to a [`Mutex`]'s value, releasing the lock on drop.
pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
    not_send: PhantomData<*const T>,
}

// SAFETY: the guard hands out `&T` through `Deref`, so sharing it between
// threads shares the value, which is exactly what `T: Sync` permits. The
// guard stays `!Send` (via `not_send`) to match `std::sync::MutexGuard`.
unsafe impl<T: Sync> Sync for MutexGuard<'_, T> {}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: holding the guard means holding the lock, so no other
        // reference to the value exists.
        unsafe { &*self.mutex.value.get() }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: holding the guard means holding the lock, so no other
        // reference to the value exists.
        unsafe { &mut *self.mutex.value.get() }
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.unlock();
    }
}

/// A value computed on first use and kept for the lifetime of the program.
///
/// Stands in for `std::sync::LazyLock`: `Lazy::new(f)` in a `static`, then
/// deref to get the value. Unlike `LazyLock` it never blocks — threads that
/// race the first access each run `f` and the first to finish publishes its
/// result — so it cannot deadlock on a re-entrant initialiser and needs no
/// lock of its own.
pub struct Lazy<T, F = fn() -> T> {
    cell: OnceBox<T>,
    init: F,
}

impl<T, F> Lazy<T, F> {
    /// A lazy value that will call `init` on first access.
    pub const fn new(init: F) -> Lazy<T, F> {
        Lazy {
            cell: OnceBox::new(),
            init,
        }
    }
}

impl<T, F: Fn() -> T> Lazy<T, F> {
    /// The value, computing it if this is the first access.
    pub fn force(this: &Lazy<T, F>) -> &T {
        this.cell.get_or_init(|| Box::new((this.init)()))
    }
}

impl<T, F: Fn() -> T> Deref for Lazy<T, F> {
    type Target = T;

    fn deref(&self) -> &T {
        Lazy::force(self)
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/sys/sync_tests.rs"]
mod tests;
