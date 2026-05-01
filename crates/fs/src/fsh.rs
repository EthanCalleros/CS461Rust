//! On-disk file system format (port of fs.h).
//!
//! Both the kernel and user programs use these definitions.
//! All types are `#[repr(C)]` for ABI compatibility with the on-disk format.

#![allow(dead_code)]

use types::uint;
use param::BSIZE;
use core::mem::size_of;

pub const ROOTINO: u32 = 1;

pub const NDIRECT: usize = 12;
pub const NINDIRECT: usize = BSIZE / size_of::<uint>();
pub const MAXFILE: usize = NDIRECT + NINDIRECT;

pub const DIRSIZ: usize = 14;

// -----------------------------------------------------------------------
// On-disk structures
// -----------------------------------------------------------------------

/// Superblock — describes the disk layout.
///
/// Disk layout:
/// `[ boot block | super block | log | inode blocks | free bit map | data blocks ]`
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
    pub itype:  i16,                    // File type (0 = free)
    pub major:  i16,                    // Major device number (T_DEV only)
    pub minor:  i16,                    // Minor device number (T_DEV only)
    pub nlink:  i16,                    // Number of directory links
    pub size:   uint,                   // Size of file (bytes)
    pub addrs:  [uint; NDIRECT + 1],    // Data block addresses
}

/// Directory entry — a (name, inode number) pair.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Dirent {
    pub inum: u16,
    pub name: [u8; DIRSIZ],
}

impl Dirent {
    /// A zeroed (empty) directory entry.
    pub const EMPTY: Self = Self {
        inum: 0,
        name: [0u8; DIRSIZ],
    };

    /// Is this slot unused?
    #[inline]
    pub fn is_free(&self) -> bool {
        self.inum == 0
    }
}

// -----------------------------------------------------------------------
// Layout constants (computed at compile time)
// -----------------------------------------------------------------------

/// Inodes per block.
pub const IPB: usize = BSIZE / size_of::<Dinode>();

/// Block containing inode `i`.
#[inline]
pub const fn iblock(i: u32, sb: &Superblock) -> u32 {
    i / (IPB as u32) + sb.inodestart
}

/// Bitmap bits per block.
pub const BPB: u32 = (BSIZE * 8) as u32;

/// Block of free map containing bit for block `b`.
#[inline]
pub const fn bblock(b: u32, sb: &Superblock) -> u32 {
    b / BPB + sb.bmapstart
}
