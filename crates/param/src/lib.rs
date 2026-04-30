#![no_std]

#![allow(non_camel_case_types)]
use types::uint;

pub const NPROC: uint       = 64;
pub const KSTACKSIZE: uint  = 4096;
pub const NCPU: uint        = 8;
pub const NOFILE: uint      = 16;
pub const NFILE: uint       = 100;
pub const NINODE: uint      = 50;
pub const NDEV: uint        = 10;
pub const ROOTDEV: uint     = 1;
pub const MAXARG: uint      = 32;
pub const MAXOPBLOCKS: uint = 10;
pub const LOGSIZE: uint     = (MAXOPBLOCKS*3);
pub const NBUF: uint        = (MAXOPBLOCKS*3);
pub const FSSIZE: uint      = 1000;