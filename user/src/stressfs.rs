#![no_std]
#![no_main]

// Stress test the file system by writing and reading in parallel.

use ulib::*;

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> ! {
    printf!(1, "stressfs starting\n");

    let mut data = [b'a'; 512];
    let mut path = *b"stressfs0\0";

    let mut i = 0;
    while i < 4 {
        if fork() > 0 {
            break;
        }
        i += 1;
    }

    printf!(1, "write {}\n", i);

    path[8] += i;
    let fd = open(&path, O_CREATE | O_RDWR);
    if fd < 0 {
        printf!(1, "stressfs: open failed\n");
        exit();
    }

    for _ in 0..20 {
        write(fd, &data);
    }
    close(fd);

    printf!(1, "read\n");

    let fd = open(&path, O_RDONLY);
    if fd < 0 {
        printf!(1, "stressfs: open failed\n");
        exit();
    }
    for _ in 0..20 {
        read(fd, &mut data);
    }
    close(fd);

    wait();
    exit();
}
