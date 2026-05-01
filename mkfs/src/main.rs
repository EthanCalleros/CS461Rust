use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write, Seek, SeekFrom};
use std::mem;

// FS constants (mirrored from param and fs.h — mkfs is a standalone host tool)
const BSIZE: usize = 512;
const FSSIZE: u32 = 2000;
const LOGSIZE: u32 = 30; // MAXOPBLOCKS(10) * 3
const ROOTINO: u32 = 1;
const NDIRECT: usize = 12;
const NINDIRECT: usize = BSIZE / mem::size_of::<u32>();
const MAXFILE: usize = NDIRECT + NINDIRECT;
const DIRSIZ: usize = 14;
const IPB: usize = BSIZE / mem::size_of::<Dinode>();
const T_DIR: i16 = 1;
const T_FILE: i16 = 2;

// On-disk structures (must match kernel layout exactly)

#[repr(C)]
#[derive(Clone, Copy)]
struct Superblock {
    size:       u32,
    nblocks:    u32,
    ninodes:    u32,
    nlog:       u32,
    logstart:   u32,
    inodestart: u32,
    bmapstart:  u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Dinode {
    itype:  i16,
    major:  i16,
    minor:  i16,
    nlink:  i16,
    size:   u32,
    addrs:  [u32; NDIRECT + 1],
}

impl Default for Dinode {
    fn default() -> Self {
        Self {
            itype: 0, major: 0, minor: 0, nlink: 0, size: 0,
            addrs: [0; NDIRECT + 1],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Dirent {
    inum: u16,
    name: [u8; DIRSIZ],
}

impl Default for Dirent {
    fn default() -> Self {
        Self { inum: 0, name: [0u8; DIRSIZ] }
    }
}

impl Dirent {
    fn set_name(&mut self, s: &str) {
        self.name = [0u8; DIRSIZ];
        let bytes = s.as_bytes();
        let len = bytes.len().min(DIRSIZ);
        self.name[..len].copy_from_slice(&bytes[..len]);
    }

    fn as_bytes(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(self as *const _ as *const u8, mem::size_of::<Dirent>())
        }
    }
}

struct Mkfs {
    file: File,
    sb: Superblock,
    free_inode: u32,
    free_block: u32,
}

impl Mkfs {
    fn new(img_path: &str) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(img_path)?;

        // Standard xv6 layout calculation
        let n_inode_blocks = (200 / IPB as u32) + 1;
        let n_bitmap = FSSIZE / (BSIZE as u32 * 8) + 1;
        let n_meta = 2 + LOGSIZE + n_inode_blocks + n_bitmap;
        let n_blocks = FSSIZE - n_meta;

        let sb = Superblock {
            size: FSSIZE,
            nblocks: n_blocks,
            ninodes: 200,
            nlog: LOGSIZE,
            logstart: 2,
            inodestart: 2 + LOGSIZE,
            bmapstart: 2 + LOGSIZE + n_inode_blocks,
        };

        Ok(Mkfs {
            file,
            sb,
            free_inode: 1,
            free_block: n_meta,
        })
    }

    fn write_sector(&mut self, sec: u32, data: &[u8]) -> io::Result<()> {
        assert!(data.len() <= BSIZE);
        self.file.seek(SeekFrom::Start((sec as u64) * (BSIZE as u64)))?;
        self.file.write_all(data)?;
        if data.len() < BSIZE {
            let padding = vec![0u8; BSIZE - data.len()];
            self.file.write_all(&padding)?;
        }
        Ok(())
    }

    fn read_sector(&mut self, sec: u32, buf: &mut [u8]) -> io::Result<()> {
        self.file.seek(SeekFrom::Start((sec as u64) * (BSIZE as u64)))?;
        self.file.read_exact(buf)
    }

    fn iblock(&self, inum: u32) -> u32 {
        inum / IPB as u32 + self.sb.inodestart
    }

    fn read_inode(&mut self, inum: u32) -> io::Result<Dinode> {
        let mut buf = [0u8; BSIZE];
        self.read_sector(self.iblock(inum), &mut buf)?;
        let offset = (inum as usize % IPB) * mem::size_of::<Dinode>();

        let mut dinode = Dinode::default();
        unsafe {
            let slice = core::slice::from_raw_parts_mut(
                &mut dinode as *mut _ as *mut u8,
                mem::size_of::<Dinode>(),
            );
            slice.copy_from_slice(&buf[offset..offset + mem::size_of::<Dinode>()]);
        }
        Ok(dinode)
    }

    fn write_inode(&mut self, inum: u32, dinode: &Dinode) -> io::Result<()> {
        let mut buf = [0u8; BSIZE];
        self.read_sector(self.iblock(inum), &mut buf)?;
        let offset = (inum as usize % IPB) * mem::size_of::<Dinode>();

        unsafe {
            let src = core::slice::from_raw_parts(
                dinode as *const _ as *const u8,
                mem::size_of::<Dinode>(),
            );
            buf[offset..offset + mem::size_of::<Dinode>()].copy_from_slice(src);
        }
        self.write_sector(self.iblock(inum), &buf)
    }

    fn ialloc(&mut self, itype: i16) -> io::Result<u32> {
        let inum = self.free_inode;
        self.free_inode += 1;

        let mut din = Dinode::default();
        din.itype = itype;
        din.nlink = 1;
        din.size = 0;
        self.write_inode(inum, &din)?;
        Ok(inum)
    }

    fn iappend(&mut self, inum: u32, data: &[u8]) -> io::Result<()> {
        let mut din = self.read_inode(inum)?;
        let mut off = din.size as usize;
        let mut p_offset = 0;
        let n = data.len();

        while p_offset < n {
            let fbn = off / BSIZE;
            assert!(fbn < MAXFILE, "file too large");

            let target_block: u32;
            if fbn < NDIRECT {
                if din.addrs[fbn] == 0 {
                    din.addrs[fbn] = self.free_block;
                    self.free_block += 1;
                }
                target_block = din.addrs[fbn];
            } else {
                // Indirect block
                if din.addrs[NDIRECT] == 0 {
                    din.addrs[NDIRECT] = self.free_block;
                    self.free_block += 1;
                }
                let mut indirect = [0u8; BSIZE];
                self.read_sector(din.addrs[NDIRECT], &mut indirect)?;
                let idx = fbn - NDIRECT;
                let entry_off = idx * mem::size_of::<u32>();
                let mut blk = u32::from_ne_bytes(
                    indirect[entry_off..entry_off + 4].try_into().unwrap(),
                );
                if blk == 0 {
                    blk = self.free_block;
                    self.free_block += 1;
                    indirect[entry_off..entry_off + 4]
                        .copy_from_slice(&blk.to_ne_bytes());
                    self.write_sector(din.addrs[NDIRECT], &indirect)?;
                }
                target_block = blk;
            }

            let n1 = std::cmp::min(n - p_offset, (fbn + 1) * BSIZE - off);
            let mut buf = [0u8; BSIZE];
            self.read_sector(target_block, &mut buf)?;

            let buf_off = off - (fbn * BSIZE);
            buf[buf_off..buf_off + n1].copy_from_slice(&data[p_offset..p_offset + n1]);

            self.write_sector(target_block, &buf)?;

            off += n1;
            p_offset += n1;
        }

        din.size = off as u32;
        self.write_inode(inum, &din)
    }
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: mkfs fs.img files...");
        std::process::exit(1);
    }

    let mut mkfs = Mkfs::new(&args[1])?;

    // Zero out the whole file first
    let zeroes = vec![0u8; BSIZE];
    for i in 0..FSSIZE {
        mkfs.write_sector(i, &zeroes)?;
    }

    // Write superblock to sector 1
    let mut sb_buf = vec![0u8; BSIZE];
    unsafe {
        let src = core::slice::from_raw_parts(
            &mkfs.sb as *const _ as *const u8,
            mem::size_of::<Superblock>(),
        );
        sb_buf[..src.len()].copy_from_slice(src);
    }
    mkfs.write_sector(1, &sb_buf)?;

    // Create root directory
    let root_ino = mkfs.ialloc(T_DIR)?;
    assert_eq!(root_ino, ROOTINO);

    // Add . and ..
    let mut de = Dirent::default();
    de.inum = root_ino as u16;
    de.set_name(".");
    mkfs.iappend(root_ino, de.as_bytes())?;

    de.set_name("..");
    mkfs.iappend(root_ino, de.as_bytes())?;

    // Process files from command line
    for filename in &args[2..] {
        let mut host_file = File::open(filename)?;
        let mut display_name = filename.as_str();
        if display_name.starts_with('_') {
            display_name = &display_name[1..];
        }

        let inum = mkfs.ialloc(T_FILE)?;

        let mut de = Dirent::default();
        de.inum = inum as u16;
        de.set_name(display_name);
        mkfs.iappend(root_ino, de.as_bytes())?;

        let mut file_buf = Vec::new();
        host_file.read_to_end(&mut file_buf)?;
        mkfs.iappend(inum, &file_buf)?;
    }

    // Finalize bitmap (balloc)
    let mut bitmap = vec![0u8; BSIZE];
    for i in 0..mkfs.free_block {
        bitmap[(i as usize) / 8] |= 1 << (i % 8);
    }
    mkfs.write_sector(mkfs.sb.bmapstart, &bitmap)?;

    Ok(())
}
