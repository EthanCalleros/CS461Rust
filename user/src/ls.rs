#![no_std]
#![no_main]

use ulib::*;
use core::mem;

/// Format a file name into a DIRSIZ-width buffer (padded with spaces).
fn fmtname(path: *const u8) -> [u8; DIRSIZ + 1] {
    let mut out = [b' '; DIRSIZ + 1];
    out[DIRSIZ] = 0;

    // Find the last '/' or use the whole path
    let len = strlen(path);
    let mut start = 0;
    unsafe {
        for i in 0..len {
            if *path.add(i) == b'/' {
                start = i + 1;
            }
        }
        let name_len = len - start;
        let copy_len = if name_len < DIRSIZ { name_len } else { DIRSIZ };
        for i in 0..copy_len {
            out[i] = *path.add(start + i);
        }
    }
    out
}

fn ls(path: &[u8]) {
    let fd = open(path, O_RDONLY);
    if fd < 0 {
        printf!(2, "ls: cannot open path\n");
        return;
    }

    let mut st: stat::stat = unsafe { mem::zeroed() };
    if fstat(fd, &mut st) < 0 {
        printf!(2, "ls: cannot stat path\n");
        close(fd);
        return;
    }

    match st.r#type as i32 {
        T_FILE => {
            let name = fmtname(path.as_ptr());
            let name_str = unsafe {
                core::str::from_utf8_unchecked(&name[..DIRSIZ])
            };
            printf!(1, "{} {} {} {}\n", name_str, st.r#type, st.ino, st.size);
        }
        T_DIR => {
            // Read directory entries
            let de_size = mem::size_of::<Dirent>();
            let mut de: Dirent = unsafe { mem::zeroed() };

            loop {
                let n = read_raw(
                    fd,
                    &mut de as *mut Dirent as *mut u8,
                    de_size,
                );
                if (n as usize) != de_size {
                    break;
                }
                if de.inum == 0 {
                    continue;
                }

                // Build full path: path + "/" + de.name
                let path_len = path.iter().position(|&b| b == 0).unwrap_or(path.len());
                let mut fullpath = [0u8; 512];
                for i in 0..path_len {
                    fullpath[i] = path[i];
                }
                fullpath[path_len] = b'/';
                let name_len = de.name.iter().position(|&b| b == 0).unwrap_or(DIRSIZ);
                for i in 0..name_len {
                    fullpath[path_len + 1 + i] = de.name[i];
                }
                fullpath[path_len + 1 + name_len] = 0;

                let mut entry_st: stat::stat = unsafe { mem::zeroed() };
                if fstat_path(&fullpath, &mut entry_st) < 0 {
                    printf!(1, "ls: cannot stat entry\n");
                    continue;
                }

                let name = fmtname(fullpath.as_ptr());
                let name_str = unsafe {
                    core::str::from_utf8_unchecked(&name[..DIRSIZ])
                };
                printf!(1, "{} {} {} {}\n",
                    name_str, entry_st.r#type, entry_st.ino, entry_st.size);
            }
        }
        _ => {}
    }
    close(fd);
}

/// Open a path, fstat it, then close.
fn fstat_path(path: &[u8], st: &mut stat::stat) -> i32 {
    let fd = open(path, O_RDONLY);
    if fd < 0 {
        return -1;
    }
    let r = fstat(fd, st);
    close(fd);
    r
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> ! {
    if argc < 2 {
        ls(b".\0");
        exit();
    }

    unsafe {
        for i in 1..argc {
            let arg = *argv.add(i as usize);
            let path = core::slice::from_raw_parts(arg, strlen(arg) + 1);
            ls(path);
        }
    }
    exit();
}
