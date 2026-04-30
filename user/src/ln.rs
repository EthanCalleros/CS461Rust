#![no_std]
#![no_main]

use ulib::*;

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> ! {
    if argc != 3 {
        printf!(2, "Usage: ln old new\n");
        exit();
    }

    unsafe {
        let old = *argv.add(1);
        let new = *argv.add(2);
        let old_slice = core::slice::from_raw_parts(old, strlen(old) + 1);
        let new_slice = core::slice::from_raw_parts(new, strlen(new) + 1);
        if link(old_slice, new_slice) < 0 {
            printf!(2, "link failed\n");
        }
    }
    exit();
}
