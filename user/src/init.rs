#![no_std]
#![no_main]

// init: The initial user-level program

use ulib::*;

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> ! {
    if open(b"console\0", O_RDWR) < 0 {
        mknod(b"console\0", 1, 1);
        open(b"console\0", O_RDWR);
    }
    dup(0); // stdout
    dup(0); // stderr

    loop {
        printf!(1, "init: starting sh\n");
        let pid = fork();
        if pid < 0 {
            printf!(1, "init: fork failed\n");
            exit();
        }
        if pid == 0 {
            let argv: [*const u8; 2] = [b"sh\0".as_ptr(), core::ptr::null()];
            exec(b"sh\0", &argv);
            printf!(1, "init: exec sh failed\n");
            exit();
        }

        loop {
            let wpid = wait();
            if wpid < 0 {
                break;
            }
            if wpid == pid {
                break;
            }
            printf!(1, "zombie!\n");
        }
    }
}
