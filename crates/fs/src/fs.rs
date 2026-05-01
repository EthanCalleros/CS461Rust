//! File system implementation (port of fs.c).
//!
//! Idiomatic Rust improvements:
//! - `BufGuard` ensures all buffers are released (no leak on panic/early return).
//! - Methods on `Inode` for common operations.
//! - `match` expressions replace C if-chains.
//! - `copy_from_slice` / `copy_to_slice` replace memmove.
//! - Named constants and doc comments throughout.

#![allow(static_mut_refs)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(dead_code)]

use core::ptr;
use core::mem::size_of;

use types::uint;
use param::{BSIZE, NINODE, NDEV, ROOTDEV};
use sync::spinlockh::spinlock;
use sync::sleeplockh::sleeplock;
use crate::buf::{Buf, BufGuard};
use crate::fsh::{
    Superblock, Dinode, Dirent, NDIRECT, NINDIRECT, MAXFILE,
    DIRSIZ, IPB, BPB, iblock, bblock,
};
use types::{T_DIR, T_DEV};

// External functions (from other crates)
unsafe extern "C" {
    unsafe fn initlock(lk: *mut spinlock, name: *const u8);
    unsafe fn acquire(lk: *mut spinlock);
    unsafe fn release(lk: *mut spinlock);
    unsafe fn initsleeplock(lk: *mut sleeplock, name: *const u8);
    unsafe fn acquiresleep(lk: *mut sleeplock);
    unsafe fn releasesleep(lk: *mut sleeplock);
    unsafe fn holdingsleep(lk: *mut sleeplock) -> i32;
}

// Intra-crate
use crate::bio;
use crate::log::log_write;

// -----------------------------------------------------------------------
// Inode type
// -----------------------------------------------------------------------

/// In-memory copy of an inode.
#[repr(C)]
pub struct Inode {
    pub dev:   uint,
    pub inum:  uint,
    pub ref_:  i32,
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

impl Inode {
    /// Is this inode slot free (not referenced)?
    #[inline]
    pub fn is_free(&self) -> bool {
        self.ref_ == 0
    }

    /// Does this cached inode have valid data from disk?
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.flags & I_VALID != 0
    }

    /// Mark this inode as having valid data.
    #[inline]
    fn mark_valid(&mut self) {
        self.flags |= I_VALID;
    }

    /// Is this a directory inode?
    #[inline]
    pub fn is_dir(&self) -> bool {
        self.itype == T_DIR as i16
    }

    /// Is this a device inode?
    #[inline]
    pub fn is_dev(&self) -> bool {
        self.itype == T_DEV as i16
    }
}

// -----------------------------------------------------------------------
// Inode cache
// -----------------------------------------------------------------------

struct Icache {
    lock:  spinlock,
    inode: [Inode; NINODE as usize],
}

#[repr(C, align(16))]
struct IcacheStorage([u8; core::mem::size_of::<Icache>()]);
static mut ICACHE_STORAGE: IcacheStorage = IcacheStorage([0u8; core::mem::size_of::<Icache>()]);

#[inline]
unsafe fn icache() -> &'static mut Icache {
    &mut *(&raw mut ICACHE_STORAGE as *mut _ as *mut Icache)
}

/// Global superblock.
static mut SB: Superblock = Superblock {
    size: 0, nblocks: 0, ninodes: 0, nlog: 0,
    logstart: 0, inodestart: 0, bmapstart: 0,
};

// External: process cwd
unsafe extern "C" {
    unsafe fn my_proc_cwd() -> *mut Inode;
}

// -----------------------------------------------------------------------
// Superblock
// -----------------------------------------------------------------------

/// Read the super block from disk.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn readsb(dev: i32, sb: *mut Superblock) {
    let buf = bio::read(dev as u32, 1);
    ptr::copy_nonoverlapping(
        buf.data.as_ptr(),
        sb as *mut u8,
        size_of::<Superblock>(),
    );
    // buf auto-released via Drop
}

// -----------------------------------------------------------------------
// Block allocator
// -----------------------------------------------------------------------

/// Zero a block on disk.
unsafe fn bzero(dev: u32, bno: u32) {
    let mut buf = bio::read(dev, bno);
    buf.data.fill(0);
    log_write(buf.as_raw());
    // buf auto-released via Drop
}

/// Allocate a zeroed disk block. Panics if disk is full.
unsafe fn balloc(dev: uint) -> uint {
    let mut b: u32 = 0;
    while b < SB.size {
        let mut buf = bio::read(dev, bblock(b, &SB));
        for bi in 0..BPB {
            if b + bi >= SB.size {
                break;
            }
            let m: u8 = 1 << (bi % 8);
            if (buf.data[(bi / 8) as usize] & m) == 0 {
                // Block is free — mark it as used.
                buf.data[(bi / 8) as usize] |= m;
                log_write(buf.as_raw());
                drop(buf); // explicit release before bzero
                bzero(dev, b + bi);
                return b + bi;
            }
        }
        // buf auto-released here
        drop(buf);
        b += BPB;
    }
    panic!("balloc: out of blocks");
}

/// Free a disk block.
unsafe fn bfree(dev: uint, b: uint) {
    readsb(dev as i32, &raw mut SB);
    let mut buf = bio::read(dev, bblock(b, &SB));
    let bi = b % BPB;
    let m: u8 = 1 << (bi % 8);
    if (buf.data[(bi / 8) as usize] & m) == 0 {
        panic!("freeing free block");
    }
    buf.data[(bi / 8) as usize] &= !m;
    log_write(buf.as_raw());
    // buf auto-released via Drop
}

// -----------------------------------------------------------------------
// Inode operations
// -----------------------------------------------------------------------

/// Initialize the inode cache.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iinit(dev: i32) {
    let ic = icache();
    initlock(&raw mut ic.lock, b"icache\0".as_ptr());
    for i in 0..NINODE as usize {
        initsleeplock(&raw mut ic.inode[i].lock, b"inode\0".as_ptr());
    }
    readsb(dev, &raw mut SB);
}

/// Find or allocate a cache slot for (dev, inum).
/// Does not lock or read from disk.
unsafe fn iget(dev: uint, inum: uint) -> *mut Inode {
    let ic = icache();
    acquire(&raw mut ic.lock);

    let mut empty: *mut Inode = ptr::null_mut();
    for inode in ic.inode.iter_mut() {
        if inode.ref_ > 0 && inode.dev == dev && inode.inum == inum {
            inode.ref_ += 1;
            release(&raw mut ic.lock);
            return inode as *mut Inode;
        }
        if empty.is_null() && inode.is_free() {
            empty = inode as *mut Inode;
        }
    }

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
        let mut buf = bio::read(dev, iblock(inum, &SB));
        let dip = (buf.data.as_mut_ptr() as *mut Dinode)
            .add((inum as usize) % IPB);
        if (*dip).itype == 0 {
            // Found a free inode — zero it and set the type.
            ptr::write_bytes(dip as *mut u8, 0, size_of::<Dinode>());
            (*dip).itype = itype;
            log_write(buf.as_raw());
            drop(buf);
            return iget(dev, inum);
        }
        // buf auto-released each iteration
    }
    panic!("ialloc: no inodes");
}

/// Copy a modified in-memory inode to disk.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iupdate(ip: *mut Inode) {
    let mut buf = bio::read((*ip).dev, iblock((*ip).inum, &SB));
    let dip = (buf.data.as_mut_ptr() as *mut Dinode)
        .add(((*ip).inum as usize) % IPB);
    (*dip).itype = (*ip).itype;
    (*dip).major = (*ip).major;
    (*dip).minor = (*ip).minor;
    (*dip).nlink = (*ip).nlink;
    (*dip).size  = (*ip).size;
    (*dip).addrs.copy_from_slice(&(*ip).addrs);
    log_write(buf.as_raw());
    // buf auto-released via Drop
}

/// Increment reference count for ip.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idup(ip: *mut Inode) -> *mut Inode {
    let ic = icache();
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

    if !(*ip).is_valid() {
        let buf = bio::read((*ip).dev, iblock((*ip).inum, &SB));
        let dip = (buf.data.as_ptr() as *const Dinode)
            .add(((*ip).inum as usize) % IPB);
        (*ip).itype = (*dip).itype;
        (*ip).major = (*dip).major;
        (*ip).minor = (*dip).minor;
        (*ip).nlink = (*dip).nlink;
        (*ip).size  = (*dip).size;
        (*ip).addrs.copy_from_slice(&(*dip).addrs);
        // buf auto-released via Drop
        (*ip).mark_valid();
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
/// If last reference and no links, truncates and frees on disk.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iput(ip: *mut Inode) {
    let ic = icache();
    acquire(&raw mut ic.lock);

    if (*ip).ref_ == 1 && (*ip).is_valid() && (*ip).nlink == 0 {
        // Last ref, no links — truncate and free.
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
// Inode content (block mapping)
// -----------------------------------------------------------------------

/// Return the disk block address of the nth block in inode ip.
/// Allocates new blocks as needed.
unsafe fn bmap(ip: *mut Inode, bn: uint) -> uint {
    if (bn as usize) < NDIRECT {
        let addr = &mut (*ip).addrs[bn as usize];
        if *addr == 0 {
            *addr = balloc((*ip).dev);
        }
        return *addr;
    }

    let bn = bn - NDIRECT as u32;
    if (bn as usize) < NINDIRECT {
        // Load indirect block, allocating if necessary.
        let indirect = &mut (*ip).addrs[NDIRECT];
        if *indirect == 0 {
            *indirect = balloc((*ip).dev);
        }
        let mut buf = bio::read((*ip).dev, *indirect);
        let a = buf.data.as_mut_ptr() as *mut uint;
        let addr = a.add(bn as usize);
        if *addr == 0 {
            *addr = balloc((*ip).dev);
            log_write(buf.as_raw());
        }
        let result = *addr;
        // buf auto-released via Drop
        return result;
    }

    panic!("bmap: out of range");
}

/// Truncate inode (discard all content).
unsafe fn itrunc(ip: *mut Inode) {
    // Free direct blocks.
    for i in 0..NDIRECT {
        if (*ip).addrs[i] != 0 {
            bfree((*ip).dev, (*ip).addrs[i]);
            (*ip).addrs[i] = 0;
        }
    }

    // Free indirect blocks.
    if (*ip).addrs[NDIRECT] != 0 {
        let buf = bio::read((*ip).dev, (*ip).addrs[NDIRECT]);
        let a = buf.data.as_ptr() as *const uint;
        for j in 0..NINDIRECT {
            let block = *a.add(j);
            if block != 0 {
                bfree((*ip).dev, block);
            }
        }
        drop(buf); // release before freeing the indirect block itself
        bfree((*ip).dev, (*ip).addrs[NDIRECT]);
        (*ip).addrs[NDIRECT] = 0;
    }

    (*ip).size = 0;
    iupdate(ip);
}

/// Copy stat information from inode.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stati(ip: *mut Inode, st: *mut types::stat::stat) {
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

    if (*ip).is_dev() {
        let major = (*ip).major as usize;
        if major >= NDEV as usize {
            return -1;
        }
        return match DEVSW[major].read {
            Some(read_fn) => read_fn(ip, off, dst, n as i32),
            None => -1,
        };
    }

    if off > (*ip).size || off.wrapping_add(n) < off {
        return -1;
    }
    let n = core::cmp::min(n, (*ip).size - off);

    let mut tot: uint = 0;
    let mut cur_off = off;
    while tot < n {
        let buf = bio::read((*ip).dev, bmap(ip, cur_off / BSIZE as u32));
        let m = core::cmp::min(n - tot, BSIZE as u32 - cur_off % BSIZE as u32);
        // Safe slice copy from buffer into user destination.
        let src_start = (cur_off % BSIZE as u32) as usize;
        ptr::copy_nonoverlapping(
            buf.data[src_start..].as_ptr(),
            dst.add(tot as usize),
            m as usize,
        );
        // buf auto-released via Drop
        tot += m;
        cur_off += m;
    }
    n as i32
}

/// Write data to inode.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn writei(ip: *mut Inode, src: *mut u8, off: uint, n: uint) -> i32 {
    use crate::file::DEVSW;

    if (*ip).is_dev() {
        let major = (*ip).major as usize;
        if major >= NDEV as usize {
            return -1;
        }
        return match DEVSW[major].write {
            Some(write_fn) => write_fn(ip, off, src, n as i32),
            None => -1,
        };
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
        let mut buf = bio::read((*ip).dev, bmap(ip, cur_off / BSIZE as u32));
        let m = core::cmp::min(n - tot, BSIZE as u32 - cur_off % BSIZE as u32);
        let dst_start = (cur_off % BSIZE as u32) as usize;
        ptr::copy_nonoverlapping(
            src.add(tot as usize),
            buf.data[dst_start..].as_mut_ptr(),
            m as usize,
        );
        log_write(buf.as_raw());
        // buf auto-released via Drop
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

/// Compare directory entry names (up to DIRSIZ bytes).
fn namecmp(s: &[u8], t: &[u8]) -> bool {
    let len = core::cmp::min(s.len(), DIRSIZ);
    for i in 0..len {
        let sc = if i < s.len() { s[i] } else { 0 };
        let tc = if i < t.len() { t[i] } else { 0 };
        if sc != tc {
            return false;
        }
        if sc == 0 {
            break;
        }
    }
    true
}

/// Look for a directory entry in a directory.
/// If found, set *poff to byte offset of entry.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dirlookup(dp: *mut Inode, name: *const u8, poff: *mut uint) -> *mut Inode {
    if !(*dp).is_dir() {
        panic!("dirlookup not DIR");
    }

    let de_size = size_of::<Dirent>() as uint;
    let mut off: uint = 0;
    while off < (*dp).size {
        let mut de: Dirent = core::mem::zeroed();
        if readi(dp, &raw mut de as *mut u8, off, de_size) != de_size as i32 {
            panic!("dirlookup read");
        }
        if de.inum != 0 {
            // Compare name using safe slice operations.
            let name_slice = core::slice::from_raw_parts(name, name_len(name));
            if namecmp(name_slice, &de.name) {
                if !poff.is_null() {
                    *poff = off;
                }
                return iget((*dp).dev, de.inum as uint);
            }
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

    // Look for an empty dirent slot.
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

    // Fill in the entry using safe slice copy.
    let nlen = name_len(name);
    let copy_len = core::cmp::min(nlen, DIRSIZ);
    de.name[..copy_len].copy_from_slice(core::slice::from_raw_parts(name, copy_len));
    if copy_len < DIRSIZ {
        de.name[copy_len..].fill(0);
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

/// Get length of a null-terminated C string (up to DIRSIZ).
unsafe fn name_len(s: *const u8) -> usize {
    let mut len = 0;
    while len < DIRSIZ && *s.add(len) != 0 {
        len += 1;
    }
    len
}

/// Copy the next path element from path into name.
/// Returns the remaining path, or None if no element left.
unsafe fn skipelem(path: *const u8, name: &mut [u8; DIRSIZ]) -> Option<*const u8> {
    let mut p = path;

    // Skip leading slashes.
    while *p == b'/' {
        p = p.add(1);
    }
    if *p == 0 {
        return None;
    }

    let s = p;
    while *p != b'/' && *p != 0 {
        p = p.add(1);
    }
    let len = p.offset_from(s) as usize;

    // Copy element into name buffer.
    let copy_len = core::cmp::min(len, DIRSIZ);
    ptr::copy_nonoverlapping(s, name.as_mut_ptr(), copy_len);
    if copy_len < DIRSIZ {
        name[copy_len] = 0;
    }

    // Skip trailing slashes.
    while *p == b'/' {
        p = p.add(1);
    }
    Some(p)
}

/// Look up and return the inode for a path name.
/// If `nameiparent`, return the parent inode and set name to final element.
unsafe fn namex(path: *const u8, find_parent: bool, name: &mut [u8; DIRSIZ]) -> *mut Inode {
    let mut ip = if *path == b'/' {
        iget(ROOTDEV, 1)
    } else {
        idup(my_proc_cwd())
    };

    let mut cur_path = path;
    loop {
        match skipelem(cur_path, name) {
            None => break,
            Some(remaining) => {
                cur_path = remaining;
                ilock(ip);
                if !(*ip).is_dir() {
                    iunlockput(ip);
                    return ptr::null_mut();
                }
                if find_parent && *cur_path == 0 {
                    // Stop one level early.
                    iunlock(ip);
                    return ip;
                }
                let next = dirlookup(ip, name.as_ptr(), ptr::null_mut());
                iunlockput(ip);
                if next.is_null() {
                    return ptr::null_mut();
                }
                ip = next;
            }
        }
    }

    if find_parent {
        iput(ip);
        return ptr::null_mut();
    }
    ip
}

/// Look up the inode for a path name.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn namei(path: *const u8) -> *mut Inode {
    let mut name = [0u8; DIRSIZ];
    namex(path, false, &mut name)
}

/// Look up the parent inode for a path name.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nameiparent(path: *const u8, name: *mut u8) -> *mut Inode {
    let mut name_buf = [0u8; DIRSIZ];
    let result = namex(path, true, &mut name_buf);
    if !result.is_null() {
        ptr::copy_nonoverlapping(name_buf.as_ptr(), name, DIRSIZ);
    }
    result
}
