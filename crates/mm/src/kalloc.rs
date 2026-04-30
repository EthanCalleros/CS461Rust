use core::ptr;
use crate::memlayout::{PGSIZE, PGROUNDUP, PHYSTOP, V2P};
use sync::spinlock::Spinlock; // Assuming your sync crate layout

extern "C" {
    static end: [char; 0]; // First address after kernel loaded from ELF
}

struct Run {
    next: *mut Run,
}

struct KMem {
    lock: Spinlock<()>,
    use_lock: bool,
    freelist: *mut Run,
}

// In Rust, we wrap the raw state in a static Mutex-like structure
// or a static mut with unsafe access.
static mut KMEM: KMem = KMem {
    lock: Spinlock::new((), "kmem"),
    use_lock: false,
    freelist: ptr::null_mut(),
};

pub unsafe fn kinit1(vstart: *mut u8, vend: *mut u8) {
    // Initializing the lock and state
    // Note: kinit1 runs before multiprocessing, so use_lock is false
    freerange(vstart, vend);
}

pub unsafe fn kinit2() {
    KMEM.use_lock = true;
}

pub unsafe fn freerange(vstart: *mut u8, vend: *mut u8) {
    let mut p = PGROUNDUP(vstart as usize) as *mut u8;
    while p.add(PGSIZE) <= vend {
        kfree(p);
        p = p.add(PGSIZE);
    }
}

/// Free the page of physical memory pointed at by v.
pub unsafe fn kfree(v: *mut u8) {
    // Validation checks
    if v as usize % PGSIZE != 0 || (v as usize) < (end.as_ptr() as usize) || V2P(v as usize) >= PHYSTOP {
        panic!("kfree: invalid address");
    }

    // Fill with junk to catch dangling refs (0x01 in C version)
    ptr::write_bytes(v, 1, PGSIZE);

    let r = v as *mut Run;

    if KMEM.use_lock {
        let _guard = KMEM.lock.acquire();
        (*r).next = KMEM.freelist;
        KMEM.freelist = r;
    } else {
        (*r).next = KMEM.freelist;
        KMEM.freelist = r;
    }
}

/// Allocate one 4096-byte page of physical memory.
/// Returns null if memory cannot be allocated.
pub unsafe fn kalloc() -> *mut u8 {
    let r: *mut Run;

    if KMEM.use_lock {
        let _guard = KMEM.lock.acquire();
        r = KMEM.freelist;
        if !r.is_null() {
            KMEM.freelist = (*r).next;
        }
    } else {
        r = KMEM.freelist;
        if !r.is_null() {
            KMEM.freelist = (*r).next;
        }
    }

    if r.is_null() {
        // Optional: xv6 panics here in your C snippet, 
        // though usually returning null is safer.
        panic!("Out of memory!");
    }

    r as *mut u8
}
