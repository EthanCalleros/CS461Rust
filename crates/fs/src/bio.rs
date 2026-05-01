//! Buffer cache (port of bio.c).
//!
//! Idiomatic Rust improvements over the C version:
//! - `BufGuard` RAII wrapper ensures buffers are always released (no leak).
//! - Internal functions use safe Rust return types.
//! - The public `extern "C"` API is a thin shim for cross-crate FFI.

#![allow(static_mut_refs)]
#![allow(unsafe_op_in_unsafe_fn)]

use core::ptr;
use core::ffi::c_void;

use param::NBUF;
use crate::buf::{Buf, BufGuard, B_VALID, B_DIRTY};
use sync::spinlockh::spinlock;
use sync::sleeplockh::sleeplock;

// External functions from other modules
unsafe extern "C" {
    unsafe fn initlock(lk: *mut spinlock, name: *const u8);
    unsafe fn acquire(lk: *mut spinlock);
    unsafe fn release(lk: *mut spinlock);
    unsafe fn initsleeplock(lk: *mut sleeplock, name: *const u8);
    unsafe fn acquiresleep(lk: *mut sleeplock);
    unsafe fn releasesleep(lk: *mut sleeplock);
    unsafe fn holdingsleep(lk: *mut sleeplock) -> i32;
    unsafe fn iderw(b: *mut Buf);
}

/// The buffer cache structure.
struct Bcache {
    lock: spinlock,
    buf:  [Buf; NBUF as usize],
    // Linked list of all buffers, through prev/next.
    // head.next is most recently used.
    head: Buf,
}

// Storage for bcache — zero-initialized, set up in binit().
#[repr(C, align(16))]
struct BcacheStorage([u8; core::mem::size_of::<Bcache>()]);
static mut BCACHE_STORAGE: BcacheStorage = BcacheStorage([0u8; core::mem::size_of::<Bcache>()]);

#[inline]
unsafe fn bcache() -> &'static mut Bcache {
    &mut *(&raw mut BCACHE_STORAGE as *mut _ as *mut Bcache)
}

// -----------------------------------------------------------------------
// Internal (idiomatic) API — returns BufGuard for RAII safety
// -----------------------------------------------------------------------

/// Initialize the buffer cache. Create linked list of buffers.
pub unsafe fn init() {
    let bc = bcache();

    initlock(&raw mut bc.lock, b"bcache\0".as_ptr());

    // Create circular doubly-linked list of buffers.
    bc.head.prev = &raw mut bc.head;
    bc.head.next = &raw mut bc.head;

    for i in 0..NBUF as usize {
        let b = &raw mut bc.buf[i];
        (*b).next = bc.head.next;
        (*b).prev = &raw mut bc.head;
        initsleeplock(&raw mut (*b).lock, b"buffer\0".as_ptr());
        (*bc.head.next).prev = b;
        bc.head.next = b;
    }
}

/// Look through buffer cache for block on device dev.
/// If not found, allocate a buffer.
/// Returns a locked buffer wrapped in a BufGuard (auto-released on drop).
unsafe fn get(dev: u32, blockno: u32) -> *mut Buf {
    let bc = bcache();

    acquire(&raw mut bc.lock);

    // Is the block already cached?
    let mut b = bc.head.next;
    while b != &raw mut bc.head {
        if (*b).dev == dev && (*b).blockno == blockno {
            (*b).refcnt += 1;
            release(&raw mut bc.lock);
            acquiresleep(&raw mut (*b).lock);
            return b;
        }
        b = (*b).next;
    }

    // Not cached; recycle an unused, clean buffer.
    b = bc.head.prev;
    while b != &raw mut bc.head {
        if (*b).refcnt == 0 && ((*b).flags & B_DIRTY) == 0 {
            (*b).dev = dev;
            (*b).blockno = blockno;
            (*b).flags = 0;
            (*b).refcnt = 1;
            release(&raw mut bc.lock);
            acquiresleep(&raw mut (*b).lock);
            return b;
        }
        b = (*b).prev;
    }
    panic!("bget: no buffers");
}

/// Read a block from disk and return a `BufGuard`.
/// The guard auto-releases the buffer when dropped.
pub unsafe fn read(dev: u32, blockno: u32) -> BufGuard {
    let b = get(dev, blockno);
    if !(*b).is_valid() {
        iderw(b);
    }
    BufGuard::from_raw(b)
}

/// Write buffer contents to disk. Must be locked.
pub unsafe fn write(guard: &mut BufGuard) {
    if holdingsleep(&raw mut (*guard.as_raw()).lock) == 0 {
        panic!("bwrite");
    }
    guard.mark_dirty();
    iderw(guard.as_raw());
}

/// Release a locked buffer and move to MRU position.
/// Called automatically by `BufGuard::drop()` — you rarely call this directly.
pub unsafe fn release_buf(b: *mut Buf) {
    if holdingsleep(&raw mut (*b).lock) == 0 {
        panic!("brelse");
    }

    releasesleep(&raw mut (*b).lock);

    let bc = bcache();
    acquire(&raw mut bc.lock);
    (*b).refcnt -= 1;
    if (*b).refcnt == 0 {
        // Move to head of MRU list.
        (*(*b).next).prev = (*b).prev;
        (*(*b).prev).next = (*b).next;
        (*b).next = bc.head.next;
        (*b).prev = &raw mut bc.head;
        (*bc.head.next).prev = b;
        bc.head.next = b;
    }
    release(&raw mut bc.lock);
}

// -----------------------------------------------------------------------
// extern "C" shims — for cross-crate FFI compatibility
// -----------------------------------------------------------------------

/// C-compatible: initialize buffer cache.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binit() {
    init();
}

/// C-compatible: read a block, return raw pointer.
/// Caller is responsible for calling `brelse`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bread(dev: u32, blockno: u32) -> *mut Buf {
    let guard = read(dev, blockno);
    guard.into_raw() // caller takes ownership; must call brelse
}

/// C-compatible: write buffer to disk.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bwrite(b: *mut Buf) {
    if holdingsleep(&raw mut (*b).lock) == 0 {
        panic!("bwrite");
    }
    (*b).flags |= B_DIRTY;
    iderw(b);
}

/// C-compatible: release a buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn brelse(b: *mut Buf) {
    release_buf(b);
}
