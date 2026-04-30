//! File descriptors (port of file.c / file.h).

#![allow(static_mut_refs)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(dead_code)]

use core::ptr;
use core::ffi::c_void;

use types::uint;
use param::{NFILE, NDEV, LOGSIZE};
use sync::spinlockh::spinlock;
use crate::fs::Inode;
use crate::pipe::Pipe;

// File types
pub const FD_NONE:  i32 = 0;
pub const FD_PIPE:  i32 = 1;
pub const FD_INODE: i32 = 2;

pub const I_VALID: i32 = 0x2;

pub const CONSOLE: i32 = 1;

/// In-kernel file structure.
#[repr(C)]
pub struct File {
    pub ftype:    i32,      // FD_NONE, FD_PIPE, FD_INODE
    pub ref_:     i32,      // reference count
    pub readable: u8,
    pub writable: u8,
    pub pipe:     *mut Pipe,
    pub ip:       *mut Inode,
    pub off:      uint,
}

/// Table mapping major device number to device functions.
#[repr(C)]
pub struct Devsw {
    pub read:  Option<unsafe extern "C" fn(ip: *mut Inode, off: uint, dst: *mut u8, n: i32) -> i32>,
    pub write: Option<unsafe extern "C" fn(ip: *mut Inode, off: uint, src: *mut u8, n: i32) -> i32>,
}

/// Global device switch table.
#[no_mangle]
pub static mut DEVSW: [Devsw; NDEV as usize] = {
    const EMPTY: Devsw = Devsw { read: None, write: None };
    [EMPTY; NDEV as usize]
};

/// File table structure.
struct Ftable {
    lock: spinlock,
    file: [File; NFILE as usize],
}

#[repr(C, align(16))]
struct FtableStorage([u8; core::mem::size_of::<Ftable>()]);
static mut FTABLE_STORAGE: FtableStorage = FtableStorage([0u8; core::mem::size_of::<Ftable>()]);

#[inline]
unsafe fn ftable() -> *mut Ftable {
    &raw mut FTABLE_STORAGE as *mut _ as *mut Ftable
}

// External functions (from other crates, linked at final link time)
unsafe extern "C" {
    unsafe fn initlock(lk: *mut spinlock, name: *const u8);
    unsafe fn acquire(lk: *mut spinlock);
    unsafe fn release(lk: *mut spinlock);
}

// Intra-crate functions
use crate::log::{begin_op, end_op};
use crate::fs::{iput, ilock, iunlock, stati, readi, writei};
use crate::pipe::{pipeclose, piperead, pipewrite};

/// Initialize the file table.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fileinit() {
    let ft = &mut *ftable();
    initlock(&raw mut ft.lock, b"ftable\0".as_ptr());
}

/// Allocate a file structure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn filealloc() -> *mut File {
    let ft = &mut *ftable();
    acquire(&raw mut ft.lock);
    for i in 0..NFILE as usize {
        if ft.file[i].ref_ == 0 {
            ft.file[i].ref_ = 1;
            release(&raw mut ft.lock);
            return &raw mut ft.file[i];
        }
    }
    release(&raw mut ft.lock);
    ptr::null_mut()
}

/// Increment ref count for file f.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn filedup(f: *mut File) -> *mut File {
    let ft = &mut *ftable();
    acquire(&raw mut ft.lock);
    if (*f).ref_ < 1 {
        panic!("filedup");
    }
    (*f).ref_ += 1;
    release(&raw mut ft.lock);
    f
}

/// Close file f. Decrement ref count, close when reaches 0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fileclose(f: *mut File) {
    let ft = &mut *ftable();
    acquire(&raw mut ft.lock);
    if (*f).ref_ < 1 {
        panic!("fileclose");
    }
    (*f).ref_ -= 1;
    if (*f).ref_ > 0 {
        release(&raw mut ft.lock);
        return;
    }
    // Save fields before clearing
    let ftype = (*f).ftype;
    let pipe = (*f).pipe;
    let ip = (*f).ip;
    let writable = (*f).writable;
    (*f).ftype = FD_NONE;
    release(&raw mut ft.lock);

    if ftype == FD_PIPE {
        pipeclose(pipe, writable as i32);
    } else if ftype == FD_INODE {
        begin_op();
        iput(ip);
        end_op();
    }
}

/// Get metadata about file f.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn filestat(f: *mut File, st: *mut types::stat) -> i32 {
    if (*f).ftype == FD_INODE {
        ilock((*f).ip);
        stati((*f).ip, st);
        iunlock((*f).ip);
        return 0;
    }
    -1
}

/// Read from file f.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fileread(f: *mut File, addr: *mut u8, n: i32) -> i32 {
    if (*f).readable == 0 {
        return -1;
    }
    if (*f).ftype == FD_PIPE {
        return piperead((*f).pipe, addr, n);
    }
    if (*f).ftype == FD_INODE {
        ilock((*f).ip);
        let r = readi((*f).ip, addr, (*f).off, n as uint);
        if r > 0 {
            (*f).off += r as uint;
        }
        iunlock((*f).ip);
        return r;
    }
    panic!("fileread");
}

/// Write to file f.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn filewrite(f: *mut File, addr: *mut u8, n: i32) -> i32 {
    if (*f).writable == 0 {
        return -1;
    }
    if (*f).ftype == FD_PIPE {
        return pipewrite((*f).pipe, addr, n);
    }
    if (*f).ftype == FD_INODE {
        // Write a few blocks at a time to avoid exceeding
        // the maximum log transaction size.
        let max = ((LOGSIZE as i32 - 1 - 1 - 2) / 2) * 512;
        let mut i = 0i32;
        while i < n {
            let mut n1 = n - i;
            if n1 > max {
                n1 = max;
            }

            begin_op();
            ilock((*f).ip);
            let r = writei((*f).ip, addr.add(i as usize), (*f).off, n1 as uint);
            if r > 0 {
                (*f).off += r as uint;
            }
            iunlock((*f).ip);
            end_op();

            if r < 0 {
                break;
            }
            if r != n1 {
                panic!("short filewrite");
            }
            i += r;
        }
        return if i == n { n } else { -1 };
    }
    panic!("filewrite");
}
