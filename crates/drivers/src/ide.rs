use arch::registers::{inb, outb, insl, outsl};
use sync::spinlock::Spinlock;
use fs::buf::{Buf, B_DIRTY, B_VALID};
use param::{BSIZE, FSSIZE};
use core::ptr;

// IDE Constants
const SECTOR_SIZE: usize = 512;
const IDE_BSY: u8        = 0x80;
const IDE_DRDY: u8       = 0x40;
const IDE_DF: u8         = 0x20;
const IDE_ERR: u8        = 0x01;

const IDE_CMD_READ: u8   = 0x20;
const IDE_CMD_WRITE: u8  = 0x30;
const IDE_CMD_RDMUL: u8  = 0xC4;
const IDE_CMD_WRMUL: u8  = 0xC5;

// Global State
static IDE_LOCK: Spinlock<()> = Spinlock::new((), "ide");
static mut IDE_QUEUE: *mut Buf = ptr::null_mut();
static mut HAVE_DISK1: bool = false;

/// Wait for IDE disk to become ready.
unsafe fn idewait(checkerr: bool) -> i32 {
    let mut r;
    loop {
        r = inb(0x1F7);
        if (r & (IDE_BSY | IDE_DRDY)) == IDE_DRDY {
            break;
        }
    }
    if checkerr && (r & (IDE_DF | IDE_ERR)) != 0 {
        return -1;
    }
    0
}

pub unsafe fn ideinit() {
    // Note: Spinlock init is usually static in Rust
    // ioapic::enable(IRQ_IDE, ncpu - 1);
    
    idewait(false);

    // Check if disk 1 is present
    outb(0x1F6, 0xE0 | (1 << 4));
    for _ in 0..1000 {
        if inb(0x1F7) != 0 {
            HAVE_DISK1 = true;
            break;
        }
    }

    // Switch back to disk 0
    outb(0x1F6, 0xE0 | (0 << 4));
}

/// Start the request for b. Caller must hold idelock.
unsafe fn idestart(b: *mut Buf) {
    if b.is_null() { panic!("idestart"); }
    if (*b).blockno >= FSSIZE as u32 { panic!("incorrect blockno"); }

    let sector_per_block = BSIZE / SECTOR_SIZE;
    let sector = (*b).blockno * sector_per_block as u32;
    
    let read_cmd = if sector_per_block == 1 { IDE_CMD_READ } else { IDE_CMD_RDMUL };
    let write_cmd = if sector_per_block == 1 { IDE_CMD_WRITE } else { IDE_CMD_WRMUL };

    if sector_per_block > 7 { panic!("idestart"); }

    idewait(false);
    outb(0x3F6, 0); // generate interrupt
    outb(0x1F2, sector_per_block as u8);
    outb(0x1F3, (sector & 0xFF) as u8);
    outb(0x1F4, ((sector >> 8) & 0xFF) as u8);
    outb(0x1F5, ((sector >> 16) & 0xFF) as u8);
    outb(0x1F6, 0xE0 | (((*b).dev & 1) << 4) as u8 | ((sector >> 24) & 0x0F) as u8);

    if ((*b).flags & B_DIRTY) != 0 {
        outb(0x1F7, write_cmd);
        outsl(0x1F0, (*b).data.as_ptr() as *const u32, BSIZE / 4);
    } else {
        outb(0x1F7, read_cmd);
    }
}

/// Interrupt handler.
pub unsafe fn ideintr() {
    let _guard = IDE_LOCK.acquire();

    let b = IDE_QUEUE;
    if b.is_null() {
        return;
    }
    
    // Advance queue
    IDE_QUEUE = (*b).qnext;

    // Read data if this was a read request
    if ((*b).flags & B_DIRTY) == 0 && idewait(true) >= 0 {
        insl(0x1F0, (*b).data.as_mut_ptr() as *mut u32, BSIZE / 4);
    }

    // Mark buffer as valid and wake waiters
    (*b).flags |= B_VALID;
    (*b).flags &= !B_DIRTY;
    
    extern "C" { fn wakeup(chan: *const core::ffi::c_void); }
    wakeup(b as *const _);

    // Start disk on next buf in queue
    if !IDE_QUEUE.is_null() {
        idestart(IDE_QUEUE);
    }
}

/// Sync buf with disk.
pub unsafe fn iderw(b: *mut Buf) {
    // Check sleep lock (assuming buf has a sleep lock implementation)
    // if !(*b).lock.holding() { panic!("iderw: buf not locked"); }
    
    if ((*b).flags & (B_VALID | B_DIRTY)) == B_VALID {
        panic!("iderw: nothing to do");
    }
    if (*b).dev != 0 && !HAVE_DISK1 {
        panic!("iderw: disk 1 not present");
    }

    let _guard = IDE_LOCK.acquire();

    // Append b to idequeue
    (*b).qnext = ptr::null_mut();
    let mut pp = &mut IDE_QUEUE;
    while !(*pp).is_null() {
        pp = &mut (**pp).qnext;
    }
    *pp = b;

    // Start disk if the queue was empty
    if IDE_QUEUE == b {
        idestart(b);
    }

    // Wait for request to finish
    extern "C" { 
        fn sleep(chan: *const core::ffi::c_void, lock: *mut Spinlock<()>); 
    }
    while ((*b).flags & (B_VALID | B_DIRTY)) != B_VALID {
        // We have to pass the raw lock inside our Spinlock wrapper to sleep
        sleep(b as *const _, &IDE_LOCK as *const _ as *mut _);
    }
}
