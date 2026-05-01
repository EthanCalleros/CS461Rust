//! Logging layer (port of log.c).
//!
//! Idiomatic Rust improvements:
//! - Uses `BufGuard` for automatic buffer release (no forgotten brelse).
//! - Transaction state tracked with clear bool fields.
//! - Log header read/write use safe slice operations where possible.

#![allow(static_mut_refs)]
#![allow(unsafe_op_in_unsafe_fn)]

use core::ptr;
use core::ffi::c_void;

use param::{BSIZE, LOGSIZE, MAXOPBLOCKS};
use crate::buf::{Buf, BufGuard, B_DIRTY};
use crate::fsh::Superblock;
use sync::spinlockh::spinlock;

// External functions (from other crates)
unsafe extern "C" {
    unsafe fn initlock(lk: *mut spinlock, name: *const u8);
    unsafe fn acquire(lk: *mut spinlock);
    unsafe fn release(lk: *mut spinlock);
    unsafe fn sleep(chan: *mut c_void, lk: *mut spinlock);
    unsafe fn wakeup(chan: *mut c_void);
}

// Intra-crate
use crate::bio;
use crate::fs::readsb;

/// Contents of the header block — maps logged block numbers.
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
    outstanding: i32,   // how many FS sys calls are executing
    committing:  bool,  // in commit(), please wait
    dev:         i32,
    lh:          LogHeader,
}

// Zero-initialized log storage.
#[repr(C, align(16))]
struct LogStorage([u8; core::mem::size_of::<Log>()]);
static mut LOG_STORAGE: LogStorage = LogStorage([0u8; core::mem::size_of::<Log>()]);

#[inline]
unsafe fn log() -> &'static mut Log {
    &mut *(&raw mut LOG_STORAGE as *mut _ as *mut Log)
}

// -----------------------------------------------------------------------
// Internal implementation using BufGuard
// -----------------------------------------------------------------------

/// Copy committed blocks from log to their home location.
unsafe fn install_trans() {
    let lg = log();
    for tail in 0..lg.lh.n as usize {
        // BufGuard ensures both buffers are released even on panic.
        let lbuf = bio::read(lg.dev as u32, (lg.start + tail as i32 + 1) as u32);
        let mut dbuf = bio::read(lg.dev as u32, lg.lh.block[tail] as u32);
        dbuf.data.copy_from_slice(&lbuf.data);
        bio::write(&mut dbuf);
        // Both lbuf and dbuf auto-released here via Drop.
    }
}

/// Read the log header from disk into the in-memory log header.
unsafe fn read_head() {
    let lg = log();
    let buf = bio::read(lg.dev as u32, lg.start as u32);
    let lh = buf.data.as_ptr() as *const LogHeader;
    lg.lh.n = (*lh).n;
    for i in 0..lg.lh.n as usize {
        lg.lh.block[i] = (*lh).block[i];
    }
    // buf auto-released via Drop.
}

/// Write in-memory log header to disk.
/// This is the true commit point of the current transaction.
unsafe fn write_head() {
    let lg = log();
    let mut buf = bio::read(lg.dev as u32, lg.start as u32);
    let hb = buf.data.as_mut_ptr() as *mut LogHeader;
    (*hb).n = lg.lh.n;
    for i in 0..lg.lh.n as usize {
        (*hb).block[i] = lg.lh.block[i];
    }
    bio::write(&mut buf);
    // buf auto-released via Drop.
}

/// Recover from a crash by replaying the log.
unsafe fn recover_from_log() {
    read_head();
    install_trans(); // if committed, copy from log to disk
    log().lh.n = 0;
    write_head(); // clear the log
}

/// Copy modified blocks from cache to log.
unsafe fn write_log() {
    let lg = log();
    for tail in 0..lg.lh.n as usize {
        let from = bio::read(lg.dev as u32, lg.lh.block[tail] as u32);
        let mut to = bio::read(lg.dev as u32, (lg.start + tail as i32 + 1) as u32);
        to.data.copy_from_slice(&from.data);
        bio::write(&mut to);
        // Both `from` and `to` auto-released via Drop.
    }
}

/// Perform the actual commit.
unsafe fn commit() {
    if log().lh.n > 0 {
        write_log();     // Write modified blocks from cache to log
        write_head();    // Write header to disk — the real commit
        install_trans(); // Install writes to home locations
        log().lh.n = 0;
        write_head();    // Erase the transaction from the log
    }
}

// -----------------------------------------------------------------------
// Public API
// -----------------------------------------------------------------------

/// Initialize the log. Called once at boot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn initlog(dev: i32) {
    if core::mem::size_of::<LogHeader>() >= BSIZE {
        panic!("initlog: too big logheader");
    }

    let lg = log();
    let mut sb: Superblock = core::mem::zeroed();
    initlock(&raw mut lg.lock, b"log\0".as_ptr());
    readsb(dev, &raw mut sb);
    lg.start = sb.logstart as i32;
    lg.size = sb.nlog as i32;
    lg.dev = dev;
    recover_from_log();
}

/// Called at the start of each FS system call.
/// May block if the log is close to full or a commit is in progress.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn begin_op() {
    let lg = log();
    acquire(&raw mut lg.lock);
    loop {
        if lg.committing {
            sleep(log() as *mut Log as *mut c_void, &raw mut lg.lock);
        } else if lg.lh.n + (lg.outstanding + 1) * (MAXOPBLOCKS as i32) > LOGSIZE as i32 {
            sleep(log() as *mut Log as *mut c_void, &raw mut lg.lock);
        } else {
            lg.outstanding += 1;
            release(&raw mut lg.lock);
            break;
        }
    }
}

/// Called at the end of each FS system call.
/// Commits if this was the last outstanding operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn end_op() {
    let lg = log();
    let do_commit;

    acquire(&raw mut lg.lock);
    lg.outstanding -= 1;
    if lg.committing {
        panic!("log.committing");
    }
    if lg.outstanding == 0 {
        do_commit = true;
        lg.committing = true;
    } else {
        do_commit = false;
        wakeup(log() as *mut Log as *mut c_void);
    }
    release(&raw mut lg.lock);

    if do_commit {
        commit();
        acquire(&raw mut lg.lock);
        lg.committing = false;
        wakeup(log() as *mut Log as *mut c_void);
        release(&raw mut lg.lock);
    }
}

/// Record a block number for write-back during commit.
/// Replaces `bwrite()` in logged operations.
///
/// Caller has modified `b->data` and is done with the buffer.
/// The actual disk write happens later during `commit()`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn log_write(b: *mut Buf) {
    let lg = log();

    if lg.lh.n >= LOGSIZE as i32 || lg.lh.n >= lg.size - 1 {
        panic!("too big a transaction");
    }
    if lg.outstanding < 1 {
        panic!("log_write outside of trans");
    }

    acquire(&raw mut lg.lock);

    // Check for log absorption (same block already logged).
    let mut i = 0i32;
    while i < lg.lh.n {
        if lg.lh.block[i as usize] == (*b).blockno as i32 {
            break;
        }
        i += 1;
    }
    lg.lh.block[i as usize] = (*b).blockno as i32;
    if i == lg.lh.n {
        lg.lh.n += 1; // new block logged
    }
    (*b).flags |= B_DIRTY; // prevent eviction from cache

    release(&raw mut lg.lock);
}
