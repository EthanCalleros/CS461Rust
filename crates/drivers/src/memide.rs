use core::ptr;
use fs::buf::{Buf, B_DIRTY, B_VALID};
use param::BSIZE;

// These symbols are typically provided by objcopy or the linker 
// when embedding a binary file (like fs.img).
unsafe extern "C" {
    unsafe static _binary_fs_img_start: u8;
    unsafe static _binary_fs_img_size: usize;
}

static mut DISKSIZE: u32 = 0;
static mut MEMDISK: *const u8 = ptr::null();

/// Initialize the memory disk by pointing to the embedded image.
pub unsafe fn ideinit() {
    MEMDISK = &_binary_fs_img_start as *const u8;
    // The size symbol's address is actually the value of the size
    DISKSIZE = (&_binary_fs_img_size as *const _ as usize / BSIZE) as u32;
}

/// The memory disk doesn't trigger hardware interrupts.
pub unsafe fn ideintr() {
    // no-op
}

/// Sync buf with the memory "disk".
/// Mimics iderw but uses memmove instead of port I/O.
pub unsafe fn iderw(b: *mut Buf) {
    // In a real xv6-rust, you'd check the sleep lock here.
    // if !(*b).lock.holding() { panic!("iderw: buf not locked"); }

    if ((*b).flags & (B_VALID | B_DIRTY)) == B_VALID {
        panic!("iderw: nothing to do");
    }
    
    // In memide, we usually only handle one device
    if (*b).dev != 1 {
        panic!("iderw: request not for disk 1");
    }
    
    if (*b).blockno >= DISKSIZE {
        panic!("iderw: block out of range");
    }

    // Calculate the offset into the memory disk
    let p = MEMDISK.add((*b).blockno as usize * BSIZE) as *mut u8;

    if ((*b).flags & B_DIRTY) != 0 {
        (*b).flags &= !B_DIRTY;
        // Write: Buf -> Memory Disk
        ptr::copy_nonoverlapping((*b).data.as_ptr(), p, BSIZE);
    } else {
        // Read: Memory Disk -> Buf
        ptr::copy_nonoverlapping(p, (*b).data.as_mut_ptr(), BSIZE);
    }

    (*b).flags |= B_VALID;
}
