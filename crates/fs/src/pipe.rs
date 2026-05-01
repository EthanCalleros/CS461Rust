//! Pipe implementation (port of pipe.c).
//!
//! Idiomatic Rust improvements:
//! - `Pipe` tracks its own open/close state cleanly.
//! - Error handling uses early returns with `goto_bad` helper eliminated
//!   in favor of a cleanup guard pattern.
//! - Clear separation of lock-holding vs non-holding code paths.

#![allow(static_mut_refs)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(dead_code)]

use core::ptr;
use core::ffi::c_void;

use types::uint;
use sync::spinlockh::spinlock;
use crate::file::{File, FileType};

const PIPESIZE: usize = 512;

// External functions (from other crates, linked at final link time)
unsafe extern "C" {
    unsafe fn initlock(lk: *mut spinlock, name: *const u8);
    unsafe fn acquire(lk: *mut spinlock);
    unsafe fn release(lk: *mut spinlock);
    unsafe fn sleep(chan: *mut c_void, lk: *mut spinlock);
    unsafe fn wakeup(chan: *mut c_void);
    unsafe fn kalloc() -> *mut u8;
    unsafe fn kfree(ptr: *mut u8);
    unsafe fn my_proc_killed() -> i32;
}

// Intra-crate file functions
use crate::file::{filealloc, fileclose};

/// Pipe structure — a bounded circular buffer between two file descriptors.
#[repr(C)]
pub struct Pipe {
    lock:      spinlock,
    data:      [u8; PIPESIZE],
    nread:     uint,   // number of bytes read (total, wrapping)
    nwrite:    uint,   // number of bytes written (total, wrapping)
    readopen:  bool,   // read fd is still open
    writeopen: bool,   // write fd is still open
}

impl Pipe {
    /// Is the pipe buffer full?
    #[inline]
    fn is_full(&self) -> bool {
        self.nwrite == self.nread + PIPESIZE as u32
    }

    /// Is the pipe buffer empty?
    #[inline]
    fn is_empty(&self) -> bool {
        self.nread == self.nwrite
    }

    /// Both ends closed — pipe can be freed.
    #[inline]
    fn is_dead(&self) -> bool {
        !self.readopen && !self.writeopen
    }

    /// Read position in the circular buffer.
    #[inline]
    fn read_pos(&self) -> usize {
        (self.nread % PIPESIZE as u32) as usize
    }

    /// Write position in the circular buffer.
    #[inline]
    fn write_pos(&self) -> usize {
        (self.nwrite % PIPESIZE as u32) as usize
    }
}

/// Allocate a pipe and two file descriptors for reading/writing.
/// Returns 0 on success, -1 on failure. On failure, any partially
/// allocated resources are cleaned up automatically.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pipealloc(f0: *mut *mut File, f1: *mut *mut File) -> i32 {
    *f0 = ptr::null_mut();
    *f1 = ptr::null_mut();

    // Allocate both file table entries.
    *f0 = filealloc();
    if (*f0).is_null() {
        return cleanup(ptr::null_mut(), *f0, *f1);
    }
    *f1 = filealloc();
    if (*f1).is_null() {
        return cleanup(ptr::null_mut(), *f0, *f1);
    }

    // Allocate pipe memory (one page).
    let p = kalloc() as *mut Pipe;
    if p.is_null() {
        return cleanup(p, *f0, *f1);
    }

    // Initialize the pipe.
    (*p).readopen = true;
    (*p).writeopen = true;
    (*p).nwrite = 0;
    (*p).nread = 0;
    initlock(&raw mut (*p).lock, b"pipe\0".as_ptr());

    // Wire up the read end.
    (**f0).ftype = FileType::Pipe;
    (**f0).readable = true;
    (**f0).writable = false;
    (**f0).pipe = p;

    // Wire up the write end.
    (**f1).ftype = FileType::Pipe;
    (**f1).readable = false;
    (**f1).writable = true;
    (**f1).pipe = p;

    0
}

/// Cleanup helper — frees any allocated resources on pipe creation failure.
unsafe fn cleanup(p: *mut Pipe, f0: *mut File, f1: *mut File) -> i32 {
    if !p.is_null() {
        kfree(p as *mut u8);
    }
    if !f0.is_null() {
        fileclose(f0);
    }
    if !f1.is_null() {
        fileclose(f1);
    }
    -1
}

/// Close one end of a pipe. Frees the pipe memory when both ends are closed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pipeclose(p: *mut Pipe, writable: i32) {
    acquire(&raw mut (*p).lock);

    if writable != 0 {
        (*p).writeopen = false;
        wakeup(&raw mut (*p).nread as *mut _ as *mut c_void);
    } else {
        (*p).readopen = false;
        wakeup(&raw mut (*p).nwrite as *mut _ as *mut c_void);
    }

    let dead = (*p).is_dead();
    release(&raw mut (*p).lock);

    if dead {
        // Both ends closed — ownership drops; free the page.
        kfree(p as *mut u8);
    }
}

/// Write `n` bytes to pipe from `addr`. Blocks if pipe is full.
/// Returns number of bytes written, or -1 if read end closed / process killed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pipewrite(p: *mut Pipe, addr: *const u8, n: i32) -> i32 {
    acquire(&raw mut (*p).lock);

    for i in 0..n as usize {
        // Block while pipe is full.
        while (*p).is_full() {
            if !(*p).readopen || my_proc_killed() != 0 {
                release(&raw mut (*p).lock);
                return -1;
            }
            wakeup(&raw mut (*p).nread as *mut _ as *mut c_void);
            sleep(&raw mut (*p).nwrite as *mut _ as *mut c_void, &raw mut (*p).lock);
        }
        (*p).data[(*p).write_pos()] = *addr.add(i);
        (*p).nwrite += 1;
    }

    wakeup(&raw mut (*p).nread as *mut _ as *mut c_void);
    release(&raw mut (*p).lock);
    n
}

/// Read up to `n` bytes from pipe into `addr`. Blocks if pipe is empty.
/// Returns number of bytes actually read, or -1 if process killed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn piperead(p: *mut Pipe, addr: *mut u8, n: i32) -> i32 {
    acquire(&raw mut (*p).lock);

    // Block while pipe is empty and write end is open.
    while (*p).is_empty() && (*p).writeopen {
        if my_proc_killed() != 0 {
            release(&raw mut (*p).lock);
            return -1;
        }
        sleep(&raw mut (*p).nread as *mut _ as *mut c_void, &raw mut (*p).lock);
    }

    // Copy bytes out of the circular buffer.
    let mut count = 0i32;
    while count < n && !(*p).is_empty() {
        *addr.add(count as usize) = (*p).data[(*p).read_pos()];
        (*p).nread += 1;
        count += 1;
    }

    wakeup(&raw mut (*p).nwrite as *mut _ as *mut c_void);
    release(&raw mut (*p).lock);
    count
}
