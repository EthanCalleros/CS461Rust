//! Buffer cache types — port of `struct buf` from buf.h.

#![allow(non_upper_case_globals)]
#![allow(dead_code)]

use param::BSIZE;
use sync::sleeplockh::sleeplock;

pub const B_VALID: u32 = 0x2; // buffer has been read from disk
pub const B_DIRTY: u32 = 0x4; // buffer needs to be written to disk

/// Buffer structure — holds a cached copy of a disk block.
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
