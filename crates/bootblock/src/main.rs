#![no_std]
#![no_main]

//! Second-stage bootloader (port of bootmain.c). Loads the kernel
//! ELF image off the boot disk and jumps to its entry point.

use core::panic::PanicInfo;

#[unsafe(no_mangle)]
pub extern "C" fn bootmain() -> ! {
    // TODO: load kernel from sector 1, parse ELF, jump to entry.
    loop {
        core::hint::spin_loop();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    bootmain()
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
