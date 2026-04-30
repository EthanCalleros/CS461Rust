//! Logging layer (port of log.c).
//!
//! Simple logging that allows concurrent FS system calls.
//! A log transaction contains the updates of multiple FS system calls.
//! The logging system only commits when there are no FS system calls active.
//!
//! A system call should call begin_op()/end_op() to mark its start and end.
//! The log is a physical re-do log containing disk blocks.
//!
//! On-disk log format:
//!   header block, containing block #s for block A, B, C, ...
//!   block A
//!   block B
//!   ...

#![allow(static_mut_refs)]
#![allow(unsafe_op_in_unsafe_fn)]

use core::ptr;
use core::ffi::c_void;

use param::{BSIZE, LOGSIZE, MAXOPBLOCKS};
use crate::buf::{Buf, B_DIRTY};
use crate::fsh::Superblock;
use sync::spinlockh::spinlock;

// External functions (from other crates, linked at final link time)
unsafe extern "C" {
    unsafe fn initlock(lk: *mut spinlock, name: *const u8);
    unsafe fn acquire(lk: *mut spinlock);
    unsafe fn release(lk: *mut spinlock);
    unsafe fn sleep(chan: *mut c_void, lk: *mut spinlock);
    unsafe fn wakeup(chan: *mut c_void);
}

// Intra-crate functions
use crate::bio::{bread, bwrite, brelse};
use crate::fs::readsb;

/// Contents of the header block, used for both the on-disk header block
/// and to keep track in memory of logged block #s before commit.
#[repr(C)]
struct LogHeader {
    n:     i32,
    block: [i32; LOGSIZE as usize],
}

/// In-memory log state.
#[repr(C)]
struct Log {
    lock:        spinlock,
    start:       i32,
    size:        i32,
    outstanding: i32, // how many FS sys calls are executing
    committing:  i32, // in commit(), please wait
    dev:         i32,
    lh:          LogHeader,
}

// Storage for the global log structure (zero-initialized).
#[repr(C, align(16))]
struct LogStorage([u8; core::mem::size_of::<Log>()]);
static mut LOG_STORAGE: LogStorage = LogStorage([0u8; core::mem::size_of::<Log>()]);

#[inline]
unsafe fn log_ptr() -> *mut Log {
    &raw mut LOG_STORAGE as *mut _ as *mut Log
}

/// Initialize the log. Called once at boot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn initlog(dev: i32) {
    if core::mem::size_of::<LogHeader>() >= BSIZE {
        panic!("initlog: too big logheader");
    }

    let log = &mut *log_ptr();
    let mut sb: Superblock = core::mem::zeroed();
    initlock(&raw mut log.lock, b"log\0".as_ptr());
    readsb(dev, &raw mut sb);
    log.start = sb.logstart as i32;
    log.size = sb.nlog as i32;
    log.dev = dev;
    recover_from_log();
}

/// Copy committed blocks from log to their home location.
unsafe fn install_trans() {
    let log = &mut *log_ptr();
    for tail in 0..log.lh.n {
        let lbuf = bread(log.dev as u32, (log.start + tail + 1) as u32);
        let dbuf = bread(log.dev as u32, log.lh.block[tail as usize] as u32);
        ptr::copy_nonoverlapping(
            (*lbuf).data.as_ptr(),
            (*dbuf).data.as_mut_ptr(),
            BSIZE,
        );
        bwrite(dbuf);
        brelse(lbuf);
        brelse(dbuf);
    }
}

/// Read the log header from disk into the in-memory log header.
unsafe fn read_head() {
    let log = &mut *log_ptr();
    let buf = bread(log.dev as u32, log.start as u32);
    let lh = (*buf).data.as_ptr() as *const LogHeader;
    log.lh.n = (*lh).n;
    for i in 0..log.lh.n as usize {
        log.lh.block[i] = (*lh).block[i];
    }
    brelse(buf);
}

/// Write in-memory log header to disk.
/// This is the true point at which the current transaction commits.
unsafe fn write_head() {
    let log = &mut *log_ptr();
    let buf = bread(log.dev as u32, log.start as u32);
    let hb = (*buf).data.as_mut_ptr() as *mut LogHeader;
    (*hb).n = log.lh.n;
    for i in 0..log.lh.n as usize {
        (*hb).block[i] = log.lh.block[i];
    }
    bwrite(buf);
    brelse(buf);
}

unsafe fn recover_from_log() {
    read_head();
    install_trans(); // if committed, copy from log to disk
    let log = &mut *log_ptr();
    log.lh.n = 0;
    write_head(); // clear the log
}

/// Called at the start of each FS system call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn begin_op() {
    let log = &mut *log_ptr();
    acquire(&raw mut log.lock);
    loop {
        if log.committing != 0 {
            sleep(log_ptr() as *mut c_void, &raw mut log.lock);
        } else if log.lh.n + (log.outstanding + 1) * (MAXOPBLOCKS as i32) > LOGSIZE as i32 {
            // this op might exhaust log space; wait for commit.
            sleep(log_ptr() as *mut c_void, &raw mut log.lock);
        } else {
            log.outstanding += 1;
            release(&raw mut log.lock);
            break;
        }
    }
}

/// Called at the end of each FS system call.
/// Commits if this was the last outstanding operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn end_op() {
    let log = &mut *log_ptr();
    let mut do_commit = false;

    acquire(&raw mut log.lock);
    log.outstanding -= 1;
    if log.committing != 0 {
        panic!("log.committing");
    }
    if log.outstanding == 0 {
        do_commit = true;
        log.committing = 1;
    } else {
        // begin_op() may be waiting for log space.
        wakeup(log_ptr() as *mut c_void);
    }
    release(&raw mut log.lock);

    if do_commit {
        commit();
        acquire(&raw mut log.lock);
        log.committing = 0;
        wakeup(log_ptr() as *mut c_void);
        release(&raw mut log.lock);
    }
}

/// Copy modified blocks from cache to log.
unsafe fn write_log() {
    let log = &mut *log_ptr();
    for tail in 0..log.lh.n {
        let to = bread(log.dev as u32, (log.start + tail + 1) as u32);
        let from = bread(log.dev as u32, log.lh.block[tail as usize] as u32);
        ptr::copy_nonoverlapping(
            (*from).data.as_ptr(),
            (*to).data.as_mut_ptr(),
            BSIZE,
        );
        bwrite(to);
        brelse(from);
        brelse(to);
    }
}

unsafe fn commit() {
    let log = &mut *log_ptr();
    if log.lh.n > 0 {
        write_log();     // Write modified blocks from cache to log
        write_head();    // Write header to disk -- the real commit
        install_trans(); // Now install writes to home locations
        log.lh.n = 0;
        write_head();    // Erase the transaction from the log
    }
}

/// Caller has modified b->data and is done with the buffer.
/// Record the block number and pin in the cache with B_DIRTY.
/// commit()/write_log() will do the disk write.
///
/// log_write() replaces bwrite(); a typical use is:
///   bp = bread(...)
///   modify bp->data[]
///   log_write(bp)
///   brelse(bp)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn log_write(b: *mut Buf) {
    let log = &mut *log_ptr();

    if log.lh.n >= LOGSIZE as i32 || log.lh.n >= log.size - 1 {
        panic!("too big a transaction");
    }
    if log.outstanding < 1 {
        panic!("log_write outside of trans");
    }

    acquire(&raw mut log.lock);
    let mut i = 0i32;
    while i < log.lh.n {
        if log.lh.block[i as usize] == (*b).blockno as i32 {
            // log absorption
            break;
        }
        i += 1;
    }
    log.lh.block[i as usize] = (*b).blockno as i32;
    if i == log.lh.n {
        log.lh.n += 1;
    }
    (*b).flags |= B_DIRTY; // prevent eviction
    release(&raw mut log.lock);
}
