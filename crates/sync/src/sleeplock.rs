//! Sleep locks (port of `sleeplock.c`).
//!
//! A sleep lock is a long-duration mutex that uses an inner spinlock
//! to synchronise around a `locked` flag. While the holder runs,
//! interrupts are *not* disabled (unlike a plain spinlock); contenders
//! sleep on the lock's address until the holder calls `releasesleep`,
//! which `wakeup`s waiters.
//!
//! Used by the buffer cache, log layer, and inode locking — anywhere
//! the kernel may need to block while holding the lock.

#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(static_mut_refs)]

use core::ffi::c_void;
use core::ptr;

use crate::sleeplockh::sleeplock;
use crate::spinlockh::spinlock;
use proc::my_proc;

// ---------------------------------------------------------------------
// External hooks. The spinlock primitives (`initlock`, `acquire`,
// `release`) operate on the C-style `struct spinlock` that lives in
// `spinlockh.rs` — the matching free functions don't exist as Rust
// items yet, so they're declared `extern "C"` placeholders.
//
// Likewise `sleep` / `wakeup` belong to the scheduler in `proc.c` and
// haven't been ported. Replace each `extern` with a real `use` once
// the corresponding module exists.
// ---------------------------------------------------------------------

unsafe extern "C" {
    unsafe fn initlock(lk: *mut spinlock, name: *const u8);
    unsafe fn acquire(lk: *mut spinlock);
    unsafe fn release(lk: *mut spinlock);

    unsafe fn sleep(chan: *mut c_void, lk: *mut spinlock);
    unsafe fn wakeup(chan: *mut c_void);
}

// ---------------------------------------------------------------------
// Sleep-lock API. Signatures kept identical to the C source so the
// kernel's call sites translate one-for-one.
// ---------------------------------------------------------------------

/// `void initsleeplock(struct sleeplock *lk, char *name)`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn initsleeplock(lk: *mut sleeplock, name: *const u8) {
    initlock(&raw mut (*lk).lk, b"sleep lock\0".as_ptr());
    (*lk).name = name;
    (*lk).locked = 0;
    (*lk).pid = 0;
}

/// `void acquiresleep(struct sleeplock *lk)`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn acquiresleep(lk: *mut sleeplock) {
    acquire(&raw mut (*lk).lk);
    while (*lk).locked != 0 {
        sleep(lk as *mut c_void, &raw mut (*lk).lk);
    }
    (*lk).locked = 1;
    (*lk).pid = my_proc().pid;
    release(&raw mut (*lk).lk);
}

/// `void releasesleep(struct sleeplock *lk)`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn releasesleep(lk: *mut sleeplock) {
    acquire(&raw mut (*lk).lk);
    (*lk).locked = 0;
    (*lk).pid = 0;
    wakeup(lk as *mut c_void);
    release(&raw mut (*lk).lk);
}

/// `int holdingsleep(struct sleeplock *lk)`
///
/// xv6's master branch tightens this to `lk->locked && lk->pid == proc->pid`.
/// The classic version (and your source above) only checks `locked`.
/// Keeping the classic semantics; swap to the tightened form by ORing
/// in the pid check when you need it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn holdingsleep(lk: *mut sleeplock) -> i32 {
    acquire(&raw mut (*lk).lk);
    let r = (*lk).locked as i32;
    release(&raw mut (*lk).lk);
    r
}

// Suppress "unused" warning on `ptr` until it gets a use site.
#[allow(dead_code)]
fn _keepalive() {
    let _ = ptr::null::<u8>();
}
