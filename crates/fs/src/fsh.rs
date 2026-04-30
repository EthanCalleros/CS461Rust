//! On-disk file system format (port of fs.h).
//! Both the kernel and user programs use these definitions.

#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(dead_code)]

use types::uint;
use param::BSIZE;
use core::mem::size_of;

pub const ROOTINO: u32 = 1;

pub const NDIRECT: usize = 12;
pub const NINDIRECT: usize = BSIZE / size_of::<uint>();
pub const MAXFILE: usize = NDIRECT + NINDIRECT;

pub const DIRSIZ: usize = 14;

/// Disk layout:
/// [ boot block | super block | log | inode blocks |
///                                          free bit map | data blocks]
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Superblock {
    pub size:       uint, // Size of file system image (blocks)
    pub nblocks:    uint, // Number of data blocks
    pub ninodes:    uint, // Number of inodes
    pub nlog:       uint, // Number of log blocks
    pub logstart:   uint, // Block number of first log block
    pub inodestart: uint, // Block number of first inode block
    pub bmapstart:  uint, // Block number of first free map block
}

/// On-disk inode structure.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Dinode {
    pub itype:  i16,                        // File type
    pub major:  i16,                        // Major device number (T_DEV only)
    pub minor:  i16,                        // Minor device number (T_DEV only)
    pub nlink:  i16,                        // Number of links to inode
    pub size:   uint,                       // Size of file (bytes)
    pub addrs:  [uint; NDIRECT + 1],        // Data block addresses
}

/// Directory entry.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Dirent {
    pub inum: u16,
    pub name: [u8; DIRSIZ],
}

/// Inodes per block.
pub const IPB: usize = BSIZE / size_of::<Dinode>();

/// Block containing inode i.
#[inline]
pub const fn iblock(i: u32, sb: &Superblock) -> u32 {
    i / (IPB as u32) + sb.inodestart
}

/// Bitmap bits per block.
pub const BPB: u32 = (BSIZE * 8) as u32;

/// Block of free map containing bit for block b.
#[inline]
pub const fn bblock(b: u32, sb: &Superblock) -> u32 {
    b / BPB + sb.bmapstart
}
