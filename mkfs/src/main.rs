use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write, Seek, SeekFrom};
use std::mem;

// Import your xv6 structures
// Note: You may need to add #[repr(C)] to these in your crates/types
use types::{Superblock, Dinode, Dirent, stat, BSIZE, FSSIZE, IPB, LOGSIZE, ROOTINO, T_DIR, T_FILE, NDIRECT, NINDIRECT, MAXFILE, DIRSIZ};

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
        let n_inode_blocks = (200 / IPB) + 1;
        let n_bitmap = FSSIZE / (BSIZE * 8) + 1;
        let n_meta = 2 + LOGSIZE + n_inode_blocks + n_bitmap;
        let n_blocks = FSSIZE as u32 - n_meta as u32;

        let sb = Superblock {
            size: FSSIZE as u32,
            nblocks: n_blocks,
            ninodes: 200,
            nlog: LOGSIZE as u32,
            logstart: 2,
            inodestart: 2 + LOGSIZE as u32,
            bmapstart: 2 + LOGSIZE as u32 + n_inode_blocks as u32,
        };

        Ok(Mkfs {
            file,
            sb,
            free_inode: 1,
            free_block: n_meta as u32,
        })
    }

    fn write_sector(&mut self, sec: u32, data: &[u8]) -> io::Result<()> {
        assert!(data.len() <= BSIZE);
        self.file.seek(SeekFrom::Start((sec as u64) * (BSIZE as u64)))?;
        self.file.write_all(data)?;
        // Ensure the sector is exactly BSIZE
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

    // Helper to calculate which block an inode lives in
    fn iblock(&self, inum: u32) -> u32 {
        inum / IPB as u32 + self.sb.inodestart
    }

    fn read_inode(&mut self, inum: u32) -> io::Result<Dinode> {
        let mut buf = [0u8; BSIZE];
        self.read_sector(self.iblock(inum), &mut buf)?;
        let offset = (inum as usize % IPB) * mem::size_of::<Dinode>();
        
        let mut dinode = Dinode::default();
        unsafe {
            let slice = core::slice::from_raw_parts_mut(&mut dinode as *mut _ as *mut u8, mem::size_of::<Dinode>());
            slice.copy_from_slice(&buf[offset..offset + mem::size_of::<Dinode>()]);
        }
        Ok(dinode)
    }

    fn write_inode(&mut self, inum: u32, dinode: &Dinode) -> io::Result<()> {
        let mut buf = [0u8; BSIZE];
        self.read_sector(self.iblock(inum), &mut buf)?;
        let offset = (inum as usize % IPB) * mem::size_of::<Dinode>();
        
        unsafe {
            let src = core::slice::from_raw_parts(dinode as *const _ as *const u8, mem::size_of::<Dinode>());
            buf[offset..offset + mem::size_of::<Dinode>()].copy_from_slice(src);
        }
        self.write_sector(self.iblock(inum), &buf)
    }

    fn ialloc(&mut self, itype: u16) -> io::Result<u32> {
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
            assert!(fbn < MAXFILE);
            
            let target_block: u32;
            if fbn < NDIRECT {
                if din.addrs[fbn] == 0 {
                    din.addrs[fbn] = self.free_block;
                    self.free_block += 1;
                }
                target_block = din.addrs[fbn];
            } else {
                // Indirect block logic...
                // (Follow the logic from mkfs.c using read_sector/write_sector)
                unimplemented!("Indirect blocks not in this snippet");
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
        mkfs.write_sector(i as u32, &zeroes)?;
    }

    // Write superblock to sector 1
    let mut sb_buf = vec![0u8; BSIZE];
    unsafe {
        let src = core::slice::from_raw_parts(&mkfs.sb as *const _ as *const u8, mem::size_of::<Superblock>());
        sb_buf[..src.len()].copy_from_slice(src);
    }
    mkfs.write_sector(1, &sb_buf)?;

    // Create root directory
    let root_ino = mkfs.ialloc(T_DIR as u16)?;
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

        let inum = mkfs.ialloc(T_FILE as u16)?;
        
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
