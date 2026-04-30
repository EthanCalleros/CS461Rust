use types::{addr_t, uint, ushort};
use core::mem::size_of;

const ROOTINO: i32   = 1;
const BSIZE: i32     = 512;

const NDIRECT: i32   = 12;
const NINDIRECT: uint = (BSIZE / size_of::<uint>() as i32) as uint;
const MAXFILE: i32   = NDIRECT + (NINDIRECT as i32);

pub struct superblock {
    pub size:       uint,
    pub nblocks:    uint,
    pub ninodes:    uint,
    pub nlog:       uint,
    pub logstart:   uint,
    pub inodestart: uint,
    pub bmapstart:  uint,
}

pub struct dinode {
    pub r#type:      i16,
    pub major:       i16,
    pub minor:       i16,
    pub nlink:       i16,
    pub size:        uint,
    pub addrs:       [uint; (NDIRECT+1) as usize],
}

const IPB:uint      = (BSIZE / size_of::<dinode>() as i32) as uint;
pub const fn IBLOCK(i: i32, sb: superblock) -> i32{
    (i/(IPB as i32)) + sb.inodestart as i32
}
const BPB: uint     = (BSIZE*8) as uint;
pub const fn BBLOCK(b: i32, sb: superblock) -> i32{
    (b/(BPB as i32)) + sb.bmapstart as i32
}
const DIRSIZE: uint = 14;

pub struct dirent {
    inum: ushort,
    name: [char; DIRSIZE as usize]
}

