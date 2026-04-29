#![no_std]
#![no_main]

//! xv6-Rust kernel binary. Entry point lives here once early boot
//! (bootblock + entry.S) hands control over.

use core::panic::PanicInfo;

/// Kernel entry, called from `entry.S` after long-mode setup.
#[unsafe(no_mangle)]
pub extern "C" fn kmain() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

/// Placeholder `_start` so the linker is satisfied during early porting.
/// Real boot flow will jump to `kmain` from assembly; this can be
/// removed once `entry.S` is wired up.
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    kmain()
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
