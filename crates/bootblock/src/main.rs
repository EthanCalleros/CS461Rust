#![no_std]
#![no_main]

const SECTSIZE: u32 = 512;

unsafe fn waitdisk() {
    // Wait for disk to be ready (0x40 is ready, 0x80 is busy)
    while (inb(0x1F7) & 0xC0) != 0x40 {}
}

unsafe fn readsect(dst: *mut u32, offset: u32) {
    waitdisk();
    outb(0x1F2, 1);                         // count = 1 sector
    outb(0x1F3, offset as u8);              // LBA low
    outb(0x1F4, (offset >> 8) as u8);       // LBA mid
    outb(0x1F5, (offset >> 16) as u8);      // LBA high
    outb(0x1F6, ((offset >> 24) | 0xE0) as u8); // LBA top + drive select
    outb(0x1F7, 0x20);                      // command 0x20 = read sectors

    waitdisk();
    insl(0x1F0, dst, (SECTSIZE / 4) as usize);
}

unsafe fn readseg(mut pa: *mut u8, count: u32, offset: u32) {
    let epa = pa.add(count as usize);
    // Round down to sector boundary
    pa = pa.sub((offset % SECTSIZE) as usize);
    
    // Kernel starts at sector 1 (sector 0 is the bootblock)
    let mut sector = (offset / SECTSIZE) + 1;
    while pa < epa {
        readsect(pa as *mut u32, sector);
        pa = pa.add(SECTSIZE as usize);
        sector += 1;
    }
}


#[no_mangle]
pub unsafe extern "C" fn bootmain() -> ! {
    let scratch_space = 0x10000 as *mut u32;

    // 1. Read first 8KB to find the multiboot header
    readseg(scratch_space as *mut u8, 8192, 0);

    // 2. Search for the Multiboot magic number
    for n in 0..(8192 / 4) {
        let current_ptr = scratch_space.add(n);
        
        // Multiboot 1 Magic: 0x1BADB002
        if *current_ptr == 0x1BADB002 {
            // Checksum check: magic + flags + checksum == 0
            let checksum_valid = (*current_ptr)
                .wrapping_add(*current_ptr.add(1))
                .wrapping_add(*current_ptr.add(2)) == 0;

            if checksum_valid {
                let hdr = &*(current_ptr as *const MultibootHeader);
                
                // Validate flags (bit 16 must be set for address fields)
                if hdr.flags & 0x10000 != 0 {
                    // 3. Load the kernel into memory
                    readseg(
                        hdr.load_addr as *mut u8,
                        hdr.load_end_addr - hdr.load_addr,
                        (n as u32 * 4) - (hdr.header_addr - hdr.load_addr)
                    );

                    // 4. Zero out BSS
                    if hdr.bss_end_addr > hdr.load_end_addr {
                        let bss_start = hdr.load_end_addr as *mut u8;
                        let bss_size = (hdr.bss_end_addr - hdr.load_end_addr) as usize;
                        core::ptr::write_bytes(bss_start, 0, bss_size);
                    }

                    // 5. Transfer control to the kernel
                    let entry: extern "C" fn() -> ! = core::mem::transmute(hdr.entry_addr);
                    entry();
                }
            }
        }
    }

    // If we fail to find the kernel, just hang
    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
