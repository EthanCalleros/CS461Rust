//! Buffer cache (port of bio.c).
//!
//! The buffer cache is a linked list of buf structures holding
//! cached copies of disk block contents. Caching disk blocks
//! in memory reduces the number of disk reads and also provides
//! a synchronization point for disk blocks used by multiple processes.
//!
//! Interface:
//! * To get a buffer for a particular disk block, call bread.
//! * After changing buffer data, call bwrite to write it to disk.
//! * When done with the buffer, call brelse.
//! * Do not use the buffer after calling brelse.
//! * Only one process at a time can use a buffer,
//!     so do not keep them longer than necessary.

#![allow(static_mut_refs)]
#![allow(unsafe_op_in_unsafe_fn)]

use core::ptr;
use core::ffi::c_void;

use param::NBUF;
use crate::buf::{Buf, B_VALID, B_DIRTY};
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
    head: Buf,
}

// SAFETY: single-threaded init; then protected by spinlock.
static mut BCACHE: *mut Bcache = ptr::null_mut();

// Storage for bcache — we use a static mutable array because
// Buf contains raw pointers and sleeplocks that aren't easily const-initable.
#[repr(C, align(16))]
struct BcacheStorage([u8; core::mem::size_of::<Bcache>()]);
static mut BCACHE_STORAGE: BcacheStorage = BcacheStorage([0u8; core::mem::size_of::<Bcache>()]);

/// Initialize the buffer cache. Create linked list of buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binit() {
    BCACHE = &raw mut BCACHE_STORAGE as *mut _ as *mut Bcache;
    let bc = &mut *BCACHE;

    initlock(&raw mut bc.lock, b"bcache\0".as_ptr());

    // Create linked list of buffers (circular doubly-linked).
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
/// In either case, return locked buffer.
unsafe fn bget(dev: u32, blockno: u32) -> *mut Buf {
    let bc = &mut *BCACHE;

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

    // Not cached; recycle an unused buffer.
    // "clean" because B_DIRTY and not locked means log.c
    // hasn't yet committed the changes to the buffer.
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

/// Return a locked buf with the contents of the indicated block.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bread(dev: u32, blockno: u32) -> *mut Buf {
    let b = bget(dev, blockno);
    if ((*b).flags & B_VALID) == 0 {
        iderw(b);
    }
    b
}

/// Write b's contents to disk. Must be locked.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bwrite(b: *mut Buf) {
    if holdingsleep(&raw mut (*b).lock) == 0 {
        panic!("bwrite");
    }
    (*b).flags |= B_DIRTY;
    iderw(b);
}

/// Release a locked buffer. Move to the head of the MRU list.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn brelse(b: *mut Buf) {
    if holdingsleep(&raw mut (*b).lock) == 0 {
        panic!("brelse");
    }

    releasesleep(&raw mut (*b).lock);

    let bc = &mut *BCACHE;
    acquire(&raw mut bc.lock);
    (*b).refcnt -= 1;
    if (*b).refcnt == 0 {
        // No one is waiting for it.
        (*(*b).next).prev = (*b).prev;
        (*(*b).prev).next = (*b).next;
        (*b).next = bc.head.next;
        (*b).prev = &raw mut bc.head;
        (*bc.head.next).prev = b;
        bc.head.next = b;
    }
    release(&raw mut bc.lock);
}
