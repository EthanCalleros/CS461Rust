#![no_std]
#![no_main]

use ulib::*;

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> ! {
    unsafe {
        for i in 1..argc {
            let arg = *argv.add(i as usize);
            write_raw(1, arg, strlen(arg));
            if i + 1 < argc {
                write(1, b" ");
            } else {
                write(1, b"\n");
            }
        }
    }
    exit();
}
