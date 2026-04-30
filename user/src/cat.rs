#![no_std]
#![no_main]

use ulib::*;

fn cat(fd: i32) {
    let mut buf = [0u8; 512];
    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            if n < 0 {
                printf!(1, "cat: read error\n");
                exit();
            }
            break;
        }
        if write(1, &buf[..n as usize]) != n {
            printf!(1, "cat: write error\n");
            exit();
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> ! {
    if argc <= 1 {
        cat(0);
        exit();
    }

    unsafe {
        for i in 1..argc {
            let arg = *argv.add(i as usize);
            let name = core::slice::from_raw_parts(arg, strlen(arg) + 1);
            let fd = open(name, O_RDONLY);
            if fd < 0 {
                printf!(1, "cat: cannot open file\n");
                exit();
            }
            cat(fd);
            close(fd);
        }
    }
    exit();
}
