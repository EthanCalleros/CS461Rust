#![no_std]
#![no_main]

use ulib::*;

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> ! {
    if argc < 2 {
        printf!(2, "Usage: mkdir files...\n");
        exit();
    }

    unsafe {
        for i in 1..argc {
            let arg = *argv.add(i as usize);
            let name = core::slice::from_raw_parts(arg, strlen(arg) + 1);
            if mkdir(name) < 0 {
                printf!(2, "mkdir: failed to create directory\n");
                break;
            }
        }
    }
    exit();
}
