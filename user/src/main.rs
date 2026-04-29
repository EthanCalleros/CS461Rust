#![no_std]
#![no_main]

//! Placeholder user binary. The actual xv6 user programs (cat, echo,
//! ls, sh, ...) will live in `src/bin/` as separate `[[bin]]` entries
//! once we add the multi-binary layout. This file exists only so the
//! `user` crate has a default binary target during early porting.

use core::panic::PanicInfo;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
