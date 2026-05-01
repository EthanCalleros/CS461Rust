//! Buffer cache types — port of `struct buf` from buf.h.
//!
//! The key Rust improvement here is `BufGuard`: an RAII wrapper returned
//! by `bread()` that automatically calls `brelse()` when it goes out of
//! scope. This makes it impossible to forget to release a buffer.

#![allow(non_upper_case_globals)]
#![allow(dead_code)]

use core::ops::{Deref, DerefMut};
use param::BSIZE;
use sync::sleeplockh::sleeplock;

pub const B_VALID: u32 = 0x2; // buffer has been read from disk
pub const B_DIRTY: u32 = 0x4; // buffer needs to be written to disk

/// Raw buffer structure — holds a cached copy of a disk block.
/// Should not be used directly; prefer `BufGuard` for safe access.
#[repr(C)]
pub struct Buf {
    pub flags:   u32,
    pub dev:     u32,
    pub blockno: u32,
    pub lock:    sleeplock,
    pub refcnt:  u32,
    pub prev:    *mut Buf,
    pub next:    *mut Buf,
    pub qnext:   *mut Buf,
    pub data:    [u8; BSIZE],
}

impl Buf {
    /// Check whether this buffer holds valid data from disk.
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.flags & B_VALID != 0
    }

    /// Check whether this buffer has been modified and needs writeback.
    #[inline]
    pub fn is_dirty(&self) -> bool {
        self.flags & B_DIRTY != 0
    }

    /// Mark the buffer as dirty (modified, needs writeback).
    #[inline]
    pub fn mark_dirty(&mut self) {
        self.flags |= B_DIRTY;
    }

    /// Mark the buffer as valid (data matches disk).
    #[inline]
    pub fn mark_valid(&mut self) {
        self.flags |= B_VALID;
    }
}

/// RAII guard for a locked buffer. Automatically calls `brelse()` on drop,
/// ensuring buffers are always properly released even on early returns or panics.
///
/// # Usage
/// ```ignore
/// let buf = bio::read(dev, blockno);  // returns BufGuard
/// // access buf.data[..]
/// bio::write(&mut buf);  // bwrite equivalent
/// // buf is released here when it goes out of scope
/// ```
pub struct BufGuard {
    buf: *mut Buf,
}

impl BufGuard {
    /// Wrap a raw buffer pointer into a guard.
    /// The buffer must already be locked (via acquiresleep).
    ///
    /// # Safety
    /// `ptr` must be a valid, locked buffer from the buffer cache.
    #[inline]
    pub unsafe fn from_raw(ptr: *mut Buf) -> Self {
        debug_assert!(!ptr.is_null());
        Self { buf: ptr }
    }

    /// Get the raw pointer (for passing to extern "C" functions that
    /// need it, like `iderw` or `log_write`).
    #[inline]
    pub fn as_raw(&self) -> *mut Buf {
        self.buf
    }

    /// Consume the guard WITHOUT calling brelse.
    /// Use this when transferring ownership (rare).
    #[inline]
    pub fn into_raw(self) -> *mut Buf {
        let ptr = self.buf;
        core::mem::forget(self);
        ptr
    }
}

impl Deref for BufGuard {
    type Target = Buf;
    #[inline]
    fn deref(&self) -> &Buf {
        unsafe { &*self.buf }
    }
}

impl DerefMut for BufGuard {
    #[inline]
    fn deref_mut(&mut self) -> &mut Buf {
        unsafe { &mut *self.buf }
    }
}

impl Drop for BufGuard {
    fn drop(&mut self) {
        // SAFETY: the guard guarantees the buffer is locked and valid.
        // Calls the intra-crate release function in bio.rs.
        unsafe { crate::bio::release_buf(self.buf); }
    }
}
