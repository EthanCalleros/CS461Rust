//! File system implementation (port of fs.c).
//!
//! Five layers:
//!   + Blocks: allocator for raw disk blocks.
//!   + Log: crash recovery for multi-step updates.
//!   + Files: inode allocator, reading, writing, metadata.
//!   + Directories: inode with special contents (list of other inodes!)
//!   + Names: paths like /usr/rtm/xv6/fs.c for convenient naming.

#![allow(static_mut_refs)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(dead_code)]

use core::ptr;
use core::ffi::c_void;
use core::mem::size_of;

use types::uint;
use param::{BSIZE, NINODE, NDEV, ROOTDEV};
use sync::spinlockh::spinlock;
use sync::sleeplockh::sleeplock;
use crate::buf::Buf;
use crate::fsh::{
    Superblock, Dinode, Dirent, NDIRECT, NINDIRECT, MAXFILE,
    DIRSIZ, IPB, BPB, iblock, bblock,
};
use types::{T_DIR, T_FILE, T_DEV};

// External functions (from other crates, linked at final link time)
unsafe extern "C" {
    unsafe fn initlock(lk: *mut spinlock, name: *const u8);
    unsafe fn acquire(lk: *mut spinlock);
    unsafe fn release(lk: *mut spinlock);
    unsafe fn initsleeplock(lk: *mut sleeplock, name: *const u8);
    unsafe fn acquiresleep(lk: *mut sleeplock);
    unsafe fn releasesleep(lk: *mut sleeplock);
    unsafe fn holdingsleep(lk: *mut sleeplock) -> i32;
}

// Intra-crate functions
use crate::bio::{bread, brelse};
use crate::log::log_write;

/// In-memory copy of an inode.
#[repr(C)]
pub struct Inode {
    pub dev:   uint,          // Device number
    pub inum:  uint,          // Inode number
    pub ref_:  i32,           // Reference count
    pub lock:  sleeplock,
    pub flags: i32,           // I_VALID

    pub itype: i16,           // copy of disk inode
    pub major: i16,
    pub minor: i16,
    pub nlink: i16,
    pub size:  uint,
    pub addrs: [uint; NDIRECT + 1],
}

const I_VALID: i32 = 0x2;

/// Inode cache.
struct Icache {
    lock:  spinlock,
    inode: [Inode; NINODE as usize],
}

#[repr(C, align(16))]
struct IcacheStorage([u8; core::mem::size_of::<Icache>()]);
static mut ICACHE_STORAGE: IcacheStorage = IcacheStorage([0u8; core::mem::size_of::<Icache>()]);

#[inline]
unsafe fn icache() -> *mut Icache {
    &raw mut ICACHE_STORAGE as *mut _ as *mut Icache
}

/// Global superblock (one per disk device, but we only run with one device).
static mut SB: Superblock = Superblock {
    size: 0, nblocks: 0, ninodes: 0, nlog: 0,
    logstart: 0, inodestart: 0, bmapstart: 0,
};

// External: process cwd
unsafe extern "C" {
    unsafe fn my_proc_cwd() -> *mut Inode;
}

/// Read the super block.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn readsb(dev: i32, sb: *mut Superblock) {
    let bp = bread(dev as u32, 1);
    ptr::copy_nonoverlapping(
        (*bp).data.as_ptr(),
        sb as *mut u8,
        size_of::<Superblock>(),
    );
    brelse(bp);
}

/// Zero a block.
unsafe fn bzero(dev: u32, bno: u32) {
    let bp = bread(dev, bno);
    ptr::write_bytes((*bp).data.as_mut_ptr(), 0, BSIZE);
    log_write(bp);
    brelse(bp);
}

// -----------------------------------------------------------------------
// Blocks
// -----------------------------------------------------------------------

/// Allocate a zeroed disk block.
unsafe fn balloc(dev: uint) -> uint {
    let mut b: u32 = 0;
    while b < SB.size {
        let bp = bread(dev, bblock(b, &SB));
        let mut bi: u32 = 0;
        while bi < BPB && b + bi < SB.size {
            let m: u8 = 1 << (bi % 8);
            if ((*bp).data[(bi / 8) as usize] & m) == 0 {
                // Is block free? Mark in use.
                (*bp).data[(bi / 8) as usize] |= m;
                log_write(bp);
                brelse(bp);
                bzero(dev, b + bi);
                return b + bi;
            }
            bi += 1;
        }
        brelse(bp);
        b += BPB;
    }
    panic!("balloc: out of blocks");
}

/// Free a disk block.
unsafe fn bfree(dev: uint, b: uint) {
    readsb(dev as i32, &raw mut SB);
    let bp = bread(dev, bblock(b, &SB));
    let bi = b % BPB;
    let m: u8 = 1 << (bi % 8);
    if ((*bp).data[(bi / 8) as usize] & m) == 0 {
        panic!("freeing free block");
    }
    (*bp).data[(bi / 8) as usize] &= !m;
    log_write(bp);
    brelse(bp);
}

// -----------------------------------------------------------------------
// Inodes
// -----------------------------------------------------------------------

/// Initialize the inode cache.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iinit(dev: i32) {
    let ic = &mut *icache();
    initlock(&raw mut ic.lock, b"icache\0".as_ptr());
    for i in 0..NINODE as usize {
        initsleeplock(&raw mut ic.inode[i].lock, b"inode\0".as_ptr());
    }
    readsb(dev, &raw mut SB);
}

/// Find the inode with number inum on device dev
/// and return the in-memory copy. Does not lock
/// the inode and does not read it from disk.
unsafe fn iget(dev: uint, inum: uint) -> *mut Inode {
    let ic = &mut *icache();
    acquire(&raw mut ic.lock);

    // Is the inode already cached?
    let mut empty: *mut Inode = ptr::null_mut();
    for i in 0..NINODE as usize {
        let ip = &raw mut ic.inode[i];
        if (*ip).ref_ > 0 && (*ip).dev == dev && (*ip).inum == inum {
            (*ip).ref_ += 1;
            release(&raw mut ic.lock);
            return ip;
        }
        if empty.is_null() && (*ip).ref_ == 0 {
            empty = ip;
        }
    }

    // Recycle an inode cache entry.
    if empty.is_null() {
        panic!("iget: no inodes");
    }

    (*empty).dev = dev;
    (*empty).inum = inum;
    (*empty).ref_ = 1;
    (*empty).flags = 0;
    release(&raw mut ic.lock);

    empty
}

/// Allocate a new inode with the given type on device dev.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ialloc(dev: uint, itype: i16) -> *mut Inode {
    for inum in 1..SB.ninodes {
        let bp = bread(dev, iblock(inum, &SB));
        let dip = ((*bp).data.as_mut_ptr() as *mut Dinode)
            .add((inum as usize) % IPB);
        if (*dip).itype == 0 {
            // a free inode
            ptr::write_bytes(dip as *mut u8, 0, size_of::<Dinode>());
            (*dip).itype = itype;
            log_write(bp);
            brelse(bp);
            return iget(dev, inum);
        }
        brelse(bp);
    }
    panic!("ialloc: no inodes");
}

/// Copy a modified in-memory inode to disk.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iupdate(ip: *mut Inode) {
    let bp = bread((*ip).dev, iblock((*ip).inum, &SB));
    let dip = ((*bp).data.as_mut_ptr() as *mut Dinode)
        .add(((*ip).inum as usize) % IPB);
    (*dip).itype = (*ip).itype;
    (*dip).major = (*ip).major;
    (*dip).minor = (*ip).minor;
    (*dip).nlink = (*ip).nlink;
    (*dip).size  = (*ip).size;
    ptr::copy_nonoverlapping(
        (*ip).addrs.as_ptr(),
        (*dip).addrs.as_mut_ptr(),
        NDIRECT + 1,
    );
    log_write(bp);
    brelse(bp);
}

/// Increment reference count for ip.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idup(ip: *mut Inode) -> *mut Inode {
    let ic = &mut *icache();
    acquire(&raw mut ic.lock);
    (*ip).ref_ += 1;
    release(&raw mut ic.lock);
    ip
}

/// Lock the given inode. Reads the inode from disk if necessary.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ilock(ip: *mut Inode) {
    if ip.is_null() || (*ip).ref_ < 1 {
        panic!("ilock");
    }

    acquiresleep(&raw mut (*ip).lock);

    if ((*ip).flags & I_VALID) == 0 {
        let bp = bread((*ip).dev, iblock((*ip).inum, &SB));
        let dip = ((*bp).data.as_ptr() as *const Dinode)
            .add(((*ip).inum as usize) % IPB);
        (*ip).itype = (*dip).itype;
        (*ip).major = (*dip).major;
        (*ip).minor = (*dip).minor;
        (*ip).nlink = (*dip).nlink;
        (*ip).size  = (*dip).size;
        ptr::copy_nonoverlapping(
            (*dip).addrs.as_ptr(),
            (*ip).addrs.as_mut_ptr(),
            NDIRECT + 1,
        );
        brelse(bp);
        (*ip).flags |= I_VALID;
        if (*ip).itype == 0 {
            panic!("ilock: no type");
        }
    }
}

/// Unlock the given inode.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iunlock(ip: *mut Inode) {
    if ip.is_null() || holdingsleep(&raw mut (*ip).lock) == 0 || (*ip).ref_ < 1 {
        panic!("iunlock");
    }
    releasesleep(&raw mut (*ip).lock);
}

/// Drop a reference to an in-memory inode.
/// If that was the last reference, the inode cache entry can be recycled.
/// If that was the last reference and the inode has no links to it,
/// free the inode (and its content) on disk.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iput(ip: *mut Inode) {
    let ic = &mut *icache();
    acquire(&raw mut ic.lock);
    if (*ip).ref_ == 1 && ((*ip).flags & I_VALID) != 0 && (*ip).nlink == 0 {
        // inode has no links and no other references: truncate and free.
        release(&raw mut ic.lock);
        itrunc(ip);
        (*ip).itype = 0;
        iupdate(ip);
        acquire(&raw mut ic.lock);
        (*ip).flags = 0;
    }
    (*ip).ref_ -= 1;
    release(&raw mut ic.lock);
}

/// Common idiom: unlock, then put.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iunlockput(ip: *mut Inode) {
    iunlock(ip);
    iput(ip);
}

// -----------------------------------------------------------------------
// Inode content
// -----------------------------------------------------------------------

/// Return the disk block address of the nth block in inode ip.
/// If there is no such block, bmap allocates one.
unsafe fn bmap(ip: *mut Inode, bn: uint) -> uint {
    if (bn as usize) < NDIRECT {
        let mut addr = (*ip).addrs[bn as usize];
        if addr == 0 {
            addr = balloc((*ip).dev);
            (*ip).addrs[bn as usize] = addr;
        }
        return addr;
    }

    let bn = bn - NDIRECT as u32;
    if (bn as usize) < NINDIRECT {
        // Load indirect block, allocating if necessary.
        let mut addr = (*ip).addrs[NDIRECT];
        if addr == 0 {
            addr = balloc((*ip).dev);
            (*ip).addrs[NDIRECT] = addr;
        }
        let bp = bread((*ip).dev, addr);
        let a = (*bp).data.as_mut_ptr() as *mut uint;
        addr = *a.add(bn as usize);
        if addr == 0 {
            addr = balloc((*ip).dev);
            *a.add(bn as usize) = addr;
            log_write(bp);
        }
        brelse(bp);
        return addr;
    }

    panic!("bmap: out of range");
}

/// Truncate inode (discard contents).
unsafe fn itrunc(ip: *mut Inode) {
    for i in 0..NDIRECT {
        if (*ip).addrs[i] != 0 {
            bfree((*ip).dev, (*ip).addrs[i]);
            (*ip).addrs[i] = 0;
        }
    }

    if (*ip).addrs[NDIRECT] != 0 {
        let bp = bread((*ip).dev, (*ip).addrs[NDIRECT]);
        let a = (*bp).data.as_ptr() as *const uint;
        for j in 0..NINDIRECT {
            let block = *a.add(j);
            if block != 0 {
                bfree((*ip).dev, block);
            }
        }
        brelse(bp);
        bfree((*ip).dev, (*ip).addrs[NDIRECT]);
        (*ip).addrs[NDIRECT] = 0;
    }

    (*ip).size = 0;
    iupdate(ip);
}

/// Copy stat information from inode.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stati(ip: *mut Inode, st: *mut types::stat) {
    (*st).dev = (*ip).dev as i32;
    (*st).ino = (*ip).inum;
    (*st).r#type = (*ip).itype;
    (*st).nlink = (*ip).nlink;
    (*st).size = (*ip).size;
}

/// Read data from inode.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn readi(ip: *mut Inode, dst: *mut u8, off: uint, n: uint) -> i32 {
    use crate::file::DEVSW;

    if (*ip).itype == T_DEV as i16 {
        let major = (*ip).major as usize;
        if major >= NDEV as usize {
            return -1;
        }
        match DEVSW[major].read {
            Some(read_fn) => return read_fn(ip, off, dst, n as i32),
            None => return -1,
        }
    }

    if off > (*ip).size || off.wrapping_add(n) < off {
        return -1;
    }
    let mut n = n;
    if off + n > (*ip).size {
        n = (*ip).size - off;
    }

    let mut tot: uint = 0;
    let mut cur_off = off;
    while tot < n {
        let bp = bread((*ip).dev, bmap(ip, cur_off / BSIZE as u32));
        let m = core::cmp::min(n - tot, BSIZE as u32 - cur_off % BSIZE as u32);
        ptr::copy_nonoverlapping(
            (*bp).data.as_ptr().add((cur_off % BSIZE as u32) as usize),
            dst.add(tot as usize),
            m as usize,
        );
        brelse(bp);
        tot += m;
        cur_off += m;
    }
    n as i32
}

/// Write data to inode.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn writei(ip: *mut Inode, src: *mut u8, off: uint, n: uint) -> i32 {
    use crate::file::DEVSW;

    if (*ip).itype == T_DEV as i16 {
        let major = (*ip).major as usize;
        if major >= NDEV as usize {
            return -1;
        }
        match DEVSW[major].write {
            Some(write_fn) => return write_fn(ip, off, src, n as i32),
            None => return -1,
        }
    }

    if off > (*ip).size || off.wrapping_add(n) < off {
        return -1;
    }
    if off + n > (MAXFILE * BSIZE) as u32 {
        return -1;
    }

    let mut tot: uint = 0;
    let mut cur_off = off;
    while tot < n {
        let bp = bread((*ip).dev, bmap(ip, cur_off / BSIZE as u32));
        let m = core::cmp::min(n - tot, BSIZE as u32 - cur_off % BSIZE as u32);
        ptr::copy_nonoverlapping(
            src.add(tot as usize),
            (*bp).data.as_mut_ptr().add((cur_off % BSIZE as u32) as usize),
            m as usize,
        );
        log_write(bp);
        brelse(bp);
        tot += m;
        cur_off += m;
    }

    if n > 0 && cur_off > (*ip).size {
        (*ip).size = cur_off;
        iupdate(ip);
    }
    n as i32
}

// -----------------------------------------------------------------------
// Directories
// -----------------------------------------------------------------------

/// Compare directory entry names.
unsafe fn namecmp(s: *const u8, t: *const u8) -> i32 {
    let mut i = 0usize;
    while i < DIRSIZ {
        let sc = *s.add(i);
        let tc = *t.add(i);
        if sc != tc {
            return (sc as i32) - (tc as i32);
        }
        if sc == 0 {
            break;
        }
        i += 1;
    }
    0
}

/// Look for a directory entry in a directory.
/// If found, set *poff to byte offset of entry.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dirlookup(dp: *mut Inode, name: *const u8, poff: *mut uint) -> *mut Inode {
    if (*dp).itype != T_DIR as i16 {
        panic!("dirlookup not DIR");
    }

    let de_size = size_of::<Dirent>() as uint;
    let mut off: uint = 0;
    while off < (*dp).size {
        let mut de: Dirent = core::mem::zeroed();
        if readi(dp, &raw mut de as *mut u8, off, de_size) != de_size as i32 {
            panic!("dirlookup read");
        }
        if de.inum == 0 {
            off += de_size;
            continue;
        }
        if namecmp(name, de.name.as_ptr()) == 0 {
            if !poff.is_null() {
                *poff = off;
            }
            return iget((*dp).dev, de.inum as uint);
        }
        off += de_size;
    }

    ptr::null_mut()
}

/// Write a new directory entry (name, inum) into the directory dp.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dirlink(dp: *mut Inode, name: *const u8, inum: uint) -> i32 {
    let de_size = size_of::<Dirent>() as uint;

    // Check that name is not present.
    let ip = dirlookup(dp, name, ptr::null_mut());
    if !ip.is_null() {
        iput(ip);
        return -1;
    }

    // Look for an empty dirent.
    let mut off: uint = 0;
    let mut de: Dirent = core::mem::zeroed();
    while off < (*dp).size {
        if readi(dp, &raw mut de as *mut u8, off, de_size) != de_size as i32 {
            panic!("dirlink read");
        }
        if de.inum == 0 {
            break;
        }
        off += de_size;
    }

    // Fill in the entry.
    let name_len = {
        let mut len = 0usize;
        while len < DIRSIZ && *name.add(len) != 0 {
            len += 1;
        }
        len
    };
    ptr::copy_nonoverlapping(name, de.name.as_mut_ptr(), name_len);
    if name_len < DIRSIZ {
        ptr::write_bytes(de.name.as_mut_ptr().add(name_len), 0, DIRSIZ - name_len);
    }
    de.inum = inum as u16;
    if writei(dp, &raw mut de as *mut u8, off, de_size) != de_size as i32 {
        panic!("dirlink");
    }

    0
}

// -----------------------------------------------------------------------
// Paths
// -----------------------------------------------------------------------

/// Copy the next path element from path into name.
/// Return a pointer to the element following the copied one.
/// If no name to remove, return null.
unsafe fn skipelem(path: *const u8, name: *mut u8) -> *const u8 {
    let mut p = path;

    // Skip leading slashes
    while *p == b'/' {
        p = p.add(1);
    }
    if *p == 0 {
        return ptr::null();
    }

    let s = p;
    while *p != b'/' && *p != 0 {
        p = p.add(1);
    }
    let len = p.offset_from(s) as usize;

    if len >= DIRSIZ {
        ptr::copy_nonoverlapping(s, name, DIRSIZ);
    } else {
        ptr::copy_nonoverlapping(s, name, len);
        *name.add(len) = 0;
    }

    // Skip trailing slashes
    while *p == b'/' {
        p = p.add(1);
    }
    p
}

/// Look up and return the inode for a path name.
/// If nameiparent != 0, return the inode for the parent and copy the final
/// path element into name, which must have room for DIRSIZ bytes.
unsafe fn namex(path: *const u8, nameiparent: i32, name: *mut u8) -> *mut Inode {
    let mut ip: *mut Inode;

    if *path == b'/' {
        ip = iget(ROOTDEV, 1); // ROOTINO = 1
    } else {
        ip = idup(my_proc_cwd());
    }

    let mut cur_path = path;
    loop {
        cur_path = skipelem(cur_path, name);
        if cur_path.is_null() {
            break;
        }
        ilock(ip);
        if (*ip).itype != T_DIR as i16 {
            iunlockput(ip);
            return ptr::null_mut();
        }
        if nameiparent != 0 && *cur_path == 0 {
            // Stop one level early.
            iunlock(ip);
            return ip;
        }
        let next = dirlookup(ip, name, ptr::null_mut());
        iunlockput(ip);
        if next.is_null() {
            return ptr::null_mut();
        }
        ip = next;
    }

    if nameiparent != 0 {
        iput(ip);
        return ptr::null_mut();
    }
    ip
}

/// Look up the inode for a path name.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn namei(path: *const u8) -> *mut Inode {
    let mut name = [0u8; DIRSIZ];
    namex(path, 0, name.as_mut_ptr())
}

/// Look up the parent inode for a path name.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nameiparent(path: *const u8, name: *mut u8) -> *mut Inode {
    namex(path, 1, name)
}
