#![no_std]
#![no_main]

// Create a zombie process that must be reparented at exit.

use ulib::*;

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> ! {
    if fork() > 0 {
        sleep(5); // Let child exit before parent.
    }
    exit();
}
