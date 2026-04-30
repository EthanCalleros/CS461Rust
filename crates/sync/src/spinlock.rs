//! Spinlock — minimal stub that satisfies the `kalloc` API surface.
//!
//! TODO: replace with a real test-and-set implementation once
//! `arch::registers` exposes the atomic primitives we need
//! (`xchg`, `cli`/`sti`, `pushcli`/`popcli`). For now `acquire()` is
//! a no-op; this is enough for type-checking and single-CPU bring-up.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};

pub struct Spinlock<T> {
    name: &'static str,
    data: UnsafeCell<T>,
}

// SAFETY: callers are responsible for going through `acquire()` to
// access `data`. Once we have a real lock, the Sync impl is trivially
// justified by mutual exclusion.
unsafe impl<T: Send> Sync for Spinlock<T> {}

impl<T> Spinlock<T> {
    /// Construct a new lock. `const fn` so it can initialise a static.
    pub const fn new(data: T, name: &'static str) -> Self {
        Self { name, data: UnsafeCell::new(data) }
    }

    /// Take the lock and return a guard that releases on drop.
    pub fn acquire(&self) -> SpinlockGuard<'_, T> {
        // TODO: real CAS / xchg loop; disable interrupts via pushcli().
        SpinlockGuard { lock: self }
    }

    /// Lock name (used for diagnostics / panics).
    pub fn name(&self) -> &'static str {
        self.name
    }
}

pub struct SpinlockGuard<'a, T> {
    lock: &'a Spinlock<T>,
}

impl<T> Deref for SpinlockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: holding the guard implies exclusive access.
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for SpinlockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: holding the guard implies exclusive access.
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for SpinlockGuard<'_, T> {
    fn drop(&mut self) {
        // TODO: release lock; popcli() to restore interrupt state.
    }
}
