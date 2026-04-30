#![no_std]
#![no_main]

//! Second-stage bootloader. Reads the kernel from disk into memory and
//! transfers control to its entry point.
//!
//! NOTE: this scaffolding currently scans for a Multiboot 1 magic
//! header. xv6's stock `bootmain.c` is ELF-based, not Multiboot-based;
//! decide which boot protocol you're targeting and align the
//! discovery loop with that choice. Both cannot be in flight at once.

use core::panic::PanicInfo;

const SECTSIZE: u32 = 512;

// ---------------------------------------------------------------------
// I/O port helpers — wrap x86 `in` / `out` instructions.
// ---------------------------------------------------------------------

#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    core::arch::asm!(
        "in al, dx",
        out("al") value,
        in("dx") port,
        options(nomem, nostack, preserves_flags),
    );
    value
}

#[inline(always)]
unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!(
        "out dx, al",
        in("dx") port,
        in("al") value,
        options(nomem, nostack, preserves_flags),
    );
}

/// Read `count` 32-bit words from `port` into `dst`.
#[inline(always)]
unsafe fn insl(port: u16, dst: *mut u32, count: usize) {
    core::arch::asm!(
        "rep insd",
        in("dx") port,
        inout("rdi") dst => _,
        inout("rcx") count => _,
        options(nostack, preserves_flags),
    );
}

// ---------------------------------------------------------------------
// Multiboot 1 header layout (used by the discovery loop below).
// Mirrors the spec; field widths are all `u32`.
// ---------------------------------------------------------------------

#[repr(C)]
struct MultibootHeader {
    magic:         u32,
    flags:         u32,
    checksum:      u32,
    header_addr:   u32,
    load_addr:     u32,
    load_end_addr: u32,
    bss_end_addr:  u32,
    entry_addr:    u32,
}

// ---------------------------------------------------------------------
// IDE PIO sector reads.
// ---------------------------------------------------------------------

unsafe fn waitdisk() {
    // 0x40 = ready, 0x80 = busy. Spin until the drive is idle.
    while (inb(0x1F7) & 0xC0) != 0x40 {}
}

unsafe fn readsect(dst: *mut u32, offset: u32) {
    waitdisk();
    outb(0x1F2, 1); // count = 1 sector
    outb(0x1F3, offset as u8);
    outb(0x1F4, (offset >> 8) as u8);
    outb(0x1F5, (offset >> 16) as u8);
    outb(0x1F6, ((offset >> 24) | 0xE0) as u8); // LBA top + drive select
    outb(0x1F7, 0x20); // command 0x20 = READ SECTORS

    waitdisk();
    insl(0x1F0, dst, (SECTSIZE / 4) as usize);
}

unsafe fn readseg(mut pa: *mut u8, count: u32, offset: u32) {
    let epa = pa.add(count as usize);

    // Round destination down to a sector boundary.
    pa = pa.sub((offset % SECTSIZE) as usize);

    // Sector 0 is the bootblock itself, so the kernel image starts at
    // sector 1.
    let mut sector = (offset / SECTSIZE) + 1;
    while pa < epa {
        readsect(pa as *mut u32, sector);
        pa = pa.add(SECTSIZE as usize);
        sector += 1;
    }
}

// ---------------------------------------------------------------------
// Boot entry — called from bootasm.S after long-mode setup.
// ---------------------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bootmain() -> ! {
    let scratch_space = 0x10000 as *mut u32;

    // Read the first 8 KiB of the kernel image to scan for the
    // Multiboot magic.
    readseg(scratch_space as *mut u8, 8192, 0);

    for n in 0..(8192 / 4) {
        let current_ptr = scratch_space.add(n);

        if *current_ptr == 0x1BADB002 {
            // checksum: magic + flags + checksum must total zero.
            let checksum_valid = (*current_ptr)
                .wrapping_add(*current_ptr.add(1))
                .wrapping_add(*current_ptr.add(2))
                == 0;

            if checksum_valid {
                let hdr = &*(current_ptr as *const MultibootHeader);

                // bit 16 of `flags` indicates that the address fields
                // (load_addr, load_end_addr, ...) are valid.
                if hdr.flags & 0x10000 != 0 {
                    readseg(
                        hdr.load_addr as *mut u8,
                        hdr.load_end_addr - hdr.load_addr,
                        (n as u32 * 4) - (hdr.header_addr - hdr.load_addr),
                    );

                    if hdr.bss_end_addr > hdr.load_end_addr {
                        let bss_start = hdr.load_end_addr as *mut u8;
                        let bss_size =
                            (hdr.bss_end_addr - hdr.load_end_addr) as usize;
                        core::ptr::write_bytes(bss_start, 0, bss_size);
                    }

                    let entry: extern "C" fn() -> ! =
                        core::mem::transmute(hdr.entry_addr as usize);
                    entry();
                }
            }
        }
    }

    // No kernel found — hang.
    loop {
        core::hint::spin_loop();
    }
}

/// Real entry symbol expected by the linker. Just defers to bootmain.
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    unsafe { bootmain() }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
