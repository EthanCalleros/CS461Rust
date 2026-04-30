#![no_std]
#![no_main]

use ulib::*;

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> ! {
    if argc < 2 {
        printf!(2, "usage: kill pid...\n");
        exit();
    }

    unsafe {
        for i in 1..argc {
            let arg = *argv.add(i as usize);
            kill(atoi_cstr(arg));
        }
    }
    exit();
}
