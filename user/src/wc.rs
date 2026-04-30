#![no_std]
#![no_main]

use ulib::*;

fn wc(fd: i32, name: &[u8]) {
    let mut buf = [0u8; 512];
    let mut l: u32 = 0;
    let mut w: u32 = 0;
    let mut c: u32 = 0;
    let mut inword = false;

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            if n < 0 {
                printf!(1, "wc: read error\n");
                exit();
            }
            break;
        }
        for i in 0..n as usize {
            c += 1;
            if buf[i] == b'\n' {
                l += 1;
            }
            if buf[i] == b' ' || buf[i] == b'\r' || buf[i] == b'\t'
                || buf[i] == b'\n' || buf[i] == 0x0B
            {
                inword = false;
            } else if !inword {
                w += 1;
                inword = true;
            }
        }
    }
    // Print name as a C string
    let name_len = name.iter().position(|&b| b == 0).unwrap_or(name.len());
    let name_str = unsafe { core::str::from_utf8_unchecked(&name[..name_len]) };
    printf!(1, "{} {} {} {}\n", l, w, c, name_str);
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> ! {
    if argc <= 1 {
        wc(0, b"\0");
        exit();
    }

    unsafe {
        for i in 1..argc {
            let arg = *argv.add(i as usize);
            let name = core::slice::from_raw_parts(arg, strlen(arg) + 1);
            let fd = open(name, O_RDONLY);
            if fd < 0 {
                printf!(1, "wc: cannot open file\n");
                exit();
            }
            wc(fd, name);
            close(fd);
        }
    }
    exit();
}
