//! Pipe implementation (port of pipe.c).

#![allow(static_mut_refs)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(dead_code)]

use core::ptr;
use core::ffi::c_void;

use types::uint;
use sync::spinlockh::spinlock;
use crate::file::{File, FD_PIPE};

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

// Intra-crate file functions — call directly
use crate::file::{filealloc, fileclose};

/// Pipe structure.
#[repr(C)]
pub struct Pipe {
    lock:      spinlock,
    data:      [u8; PIPESIZE],
    nread:     uint,   // number of bytes read
    nwrite:    uint,   // number of bytes written
    readopen:  i32,    // read fd is still open
    writeopen: i32,    // write fd is still open
}

/// Allocate a pipe and two file descriptors for reading/writing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pipealloc(f0: *mut *mut File, f1: *mut *mut File) -> i32 {
    let mut p: *mut Pipe = ptr::null_mut();
    *f0 = ptr::null_mut();
    *f1 = ptr::null_mut();

    *f0 = filealloc();
    if (*f0).is_null() {
        return goto_bad(p, *f0, *f1);
    }
    *f1 = filealloc();
    if (*f1).is_null() {
        return goto_bad(p, *f0, *f1);
    }

    p = kalloc() as *mut Pipe;
    if p.is_null() {
        return goto_bad(p, *f0, *f1);
    }

    (*p).readopen = 1;
    (*p).writeopen = 1;
    (*p).nwrite = 0;
    (*p).nread = 0;
    initlock(&raw mut (*p).lock, b"pipe\0".as_ptr());

    (**f0).ftype = FD_PIPE;
    (**f0).readable = 1;
    (**f0).writable = 0;
    (**f0).pipe = p;

    (**f1).ftype = FD_PIPE;
    (**f1).readable = 0;
    (**f1).writable = 1;
    (**f1).pipe = p;

    return 0;
}

unsafe fn goto_bad(p: *mut Pipe, f0: *mut File, f1: *mut File) -> i32 {
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

/// Close one end of a pipe.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pipeclose(p: *mut Pipe, writable: i32) {
    acquire(&raw mut (*p).lock);
    if writable != 0 {
        (*p).writeopen = 0;
        wakeup(&raw mut (*p).nread as *mut _ as *mut c_void);
    } else {
        (*p).readopen = 0;
        wakeup(&raw mut (*p).nwrite as *mut _ as *mut c_void);
    }
    if (*p).readopen == 0 && (*p).writeopen == 0 {
        release(&raw mut (*p).lock);
        kfree(p as *mut u8);
    } else {
        release(&raw mut (*p).lock);
    }
}

/// Write n bytes to pipe.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pipewrite(p: *mut Pipe, addr: *const u8, n: i32) -> i32 {
    acquire(&raw mut (*p).lock);
    for i in 0..n {
        while (*p).nwrite == (*p).nread + PIPESIZE as u32 {
            if (*p).readopen == 0 || my_proc_killed() != 0 {
                release(&raw mut (*p).lock);
                return -1;
            }
            wakeup(&raw mut (*p).nread as *mut _ as *mut c_void);
            sleep(&raw mut (*p).nwrite as *mut _ as *mut c_void, &raw mut (*p).lock);
        }
        (*p).data[((*p).nwrite % PIPESIZE as u32) as usize] = *addr.add(i as usize);
        (*p).nwrite += 1;
    }
    wakeup(&raw mut (*p).nread as *mut _ as *mut c_void);
    release(&raw mut (*p).lock);
    n
}

/// Read n bytes from pipe.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn piperead(p: *mut Pipe, addr: *mut u8, n: i32) -> i32 {
    acquire(&raw mut (*p).lock);
    while (*p).nread == (*p).nwrite && (*p).writeopen != 0 {
        if my_proc_killed() != 0 {
            release(&raw mut (*p).lock);
            return -1;
        }
        sleep(&raw mut (*p).nread as *mut _ as *mut c_void, &raw mut (*p).lock);
    }
    let mut i = 0i32;
    while i < n {
        if (*p).nread == (*p).nwrite {
            break;
        }
        *addr.add(i as usize) = (*p).data[((*p).nread % PIPESIZE as u32) as usize];
        (*p).nread += 1;
        i += 1;
    }
    wakeup(&raw mut (*p).nwrite as *mut _ as *mut c_void);
    release(&raw mut (*p).lock);
    i
}
