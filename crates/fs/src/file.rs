//! File descriptors (port of file.c / file.h).
//!
//! Idiomatic Rust improvements:
//! - `FileType` enum replaces magic integer constants.
//! - Methods on `File` for readable/writable checks.
//! - DEVSW table uses `Option<fn>` (zero-cost nullable function ptrs).
//! - Internal logic uses match expressions instead of if-chains.

#![allow(static_mut_refs)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(dead_code)]

use core::ptr;

use types::uint;
use param::{NFILE, NDEV, LOGSIZE};
use sync::spinlockh::spinlock;
use crate::fs::Inode;
use crate::pipe::Pipe;

// Intra-crate functions
use crate::log::{begin_op, end_op};
use crate::fs::{iput, ilock, iunlock, stati, readi, writei};
use crate::pipe::{pipeclose, piperead, pipewrite};

// -----------------------------------------------------------------------
// Types
// -----------------------------------------------------------------------

/// File type — Rust enum replaces C's integer constants.
/// Using `#[repr(i32)]` for ABI compatibility.
#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    None  = 0,
    Pipe  = 1,
    Inode = 2,
}

// Keep numeric constants for extern "C" compatibility
pub const FD_NONE:  i32 = 0;
pub const FD_PIPE:  i32 = 1;
pub const FD_INODE: i32 = 2;

pub const I_VALID: i32 = 0x2;
pub const CONSOLE: i32 = 1;

/// In-kernel file structure.
#[repr(C)]
pub struct File {
    pub ftype:    FileType,
    pub ref_:     i32,          // reference count
    pub readable: bool,
    pub writable: bool,
    _pad:         [u8; 2],      // alignment padding
    pub pipe:     *mut Pipe,
    pub ip:       *mut Inode,
    pub off:      uint,
}

impl File {
    /// A zeroed/empty file slot.
    const EMPTY: Self = Self {
        ftype:    FileType::None,
        ref_:     0,
        readable: false,
        writable: false,
        _pad:     [0; 2],
        pipe:     ptr::null_mut(),
        ip:       ptr::null_mut(),
        off:      0,
    };

    #[inline]
    pub fn is_free(&self) -> bool {
        self.ref_ == 0
    }

    #[inline]
    pub fn can_read(&self) -> bool {
        self.readable
    }

    #[inline]
    pub fn can_write(&self) -> bool {
        self.writable
    }
}

/// Table mapping major device number to device read/write functions.
pub struct Devsw {
    pub read:  Option<unsafe extern "C" fn(ip: *mut Inode, off: uint, dst: *mut u8, n: i32) -> i32>,
    pub write: Option<unsafe extern "C" fn(ip: *mut Inode, off: uint, src: *mut u8, n: i32) -> i32>,
}

impl Devsw {
    const EMPTY: Self = Self { read: None, write: None };
}

/// Global device switch table.
#[unsafe(no_mangle)]
pub static mut DEVSW: [Devsw; NDEV as usize] = [Devsw::EMPTY; NDEV as usize];

// -----------------------------------------------------------------------
// File table (protected by spinlock)
// -----------------------------------------------------------------------

struct Ftable {
    lock: spinlock,
    file: [File; NFILE as usize],
}

#[repr(C, align(16))]
struct FtableStorage([u8; core::mem::size_of::<Ftable>()]);
static mut FTABLE_STORAGE: FtableStorage = FtableStorage([0u8; core::mem::size_of::<Ftable>()]);

#[inline]
unsafe fn ftable() -> &'static mut Ftable {
    &mut *(&raw mut FTABLE_STORAGE as *mut _ as *mut Ftable)
}

// External: spinlock operations
unsafe extern "C" {
    unsafe fn initlock(lk: *mut spinlock, name: *const u8);
    unsafe fn acquire(lk: *mut spinlock);
    unsafe fn release(lk: *mut spinlock);
}

// -----------------------------------------------------------------------
// File operations
// -----------------------------------------------------------------------

/// Initialize the file table.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fileinit() {
    let ft = ftable();
    initlock(&raw mut ft.lock, b"ftable\0".as_ptr());
}

/// Allocate a file structure. Returns null if table is full.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn filealloc() -> *mut File {
    let ft = ftable();
    acquire(&raw mut ft.lock);
    for f in ft.file.iter_mut() {
        if f.is_free() {
            f.ref_ = 1;
            release(&raw mut ft.lock);
            return f as *mut File;
        }
    }
    release(&raw mut ft.lock);
    ptr::null_mut()
}

/// Increment ref count for file f.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn filedup(f: *mut File) -> *mut File {
    let ft = ftable();
    acquire(&raw mut ft.lock);
    if (*f).ref_ < 1 {
        panic!("filedup");
    }
    (*f).ref_ += 1;
    release(&raw mut ft.lock);
    f
}

/// Close file f. Decrement ref count; clean up when it reaches 0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fileclose(f: *mut File) {
    let ft = ftable();
    acquire(&raw mut ft.lock);
    if (*f).ref_ < 1 {
        panic!("fileclose");
    }
    (*f).ref_ -= 1;
    if (*f).ref_ > 0 {
        release(&raw mut ft.lock);
        return;
    }

    // Last reference — save fields, then clear the slot.
    let ftype = (*f).ftype;
    let pipe = (*f).pipe;
    let ip = (*f).ip;
    let writable = (*f).writable;
    (*f).ftype = FileType::None;
    (*f).pipe = ptr::null_mut();
    (*f).ip = ptr::null_mut();
    release(&raw mut ft.lock);

    // Perform type-specific cleanup outside the lock.
    match ftype {
        FileType::Pipe => {
            pipeclose(pipe, writable as i32);
        }
        FileType::Inode => {
            begin_op();
            iput(ip);
            end_op();
        }
        FileType::None => {} // nothing to clean up
    }
}

/// Get metadata about file f.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn filestat(f: *mut File, st: *mut types::stat::stat) -> i32 {
    match (*f).ftype {
        FileType::Inode => {
            ilock((*f).ip);
            stati((*f).ip, st);
            iunlock((*f).ip);
            0
        }
        _ => -1,
    }
}

/// Read from file f.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fileread(f: *mut File, addr: *mut u8, n: i32) -> i32 {
    if !(*f).can_read() {
        return -1;
    }

    match (*f).ftype {
        FileType::Pipe => piperead((*f).pipe, addr, n),
        FileType::Inode => {
            ilock((*f).ip);
            let r = readi((*f).ip, addr, (*f).off, n as uint);
            if r > 0 {
                (*f).off += r as uint;
            }
            iunlock((*f).ip);
            r
        }
        FileType::None => panic!("fileread"),
    }
}

/// Write to file f.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn filewrite(f: *mut File, addr: *mut u8, n: i32) -> i32 {
    if !(*f).can_write() {
        return -1;
    }

    match (*f).ftype {
        FileType::Pipe => pipewrite((*f).pipe, addr, n),
        FileType::Inode => {
            // Write in chunks to avoid exceeding log transaction size.
            let max = ((LOGSIZE as i32 - 1 - 1 - 2) / 2) * 512;
            let mut i = 0i32;
            while i < n {
                let n1 = core::cmp::min(n - i, max);

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
            if i == n { n } else { -1 }
        }
        FileType::None => panic!("filewrite"),
    }
}
