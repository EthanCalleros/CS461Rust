#![no_std]
#![no_main]

// Test that fork fails gracefully.
// Tiny executable so that the limit can be filling the proc table.

use ulib::*;

const N: i32 = 1000;

fn forktest() {
    printf!(1, "fork test\n");

    let mut n = 0;
    while n < N {
        let pid = fork();
        if pid < 0 {
            break;
        }
        if pid == 0 {
            exit();
        }
        n += 1;
    }

    if n == N {
        printf!(1, "fork claimed to work N times!\n");
        exit();
    }

    for _ in (0..n).rev() {
        if wait() < 0 {
            printf!(1, "wait stopped early\n");
            exit();
        }
    }

    if wait() != -1 {
        printf!(1, "wait got too many\n");
        exit();
    }

    printf!(1, "fork test OK\n");
}

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> ! {
    forktest();
    exit();
}
