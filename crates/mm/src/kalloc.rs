//! Physical memory allocator (port of `kalloc.c`).
//!
//! Maintains a singly-linked free list of 4 KiB physical pages. Used
//! to allocate user pages, kernel stacks, page-table pages, and pipe
//! buffers.
//!
//! Initialisation runs in two phases, matching xv6:
//! * `kinit1(vstart, vend)` — single-CPU, before SMP. `use_lock = false`.
//! * `kinit2()`             — flips `use_lock = true` once other CPUs
//!                            are alive.

// `kmem` and `run` are kept lowercase to mirror the C source and let
// the port read line-for-line against `kalloc.c`. The static name
// `kmem` similarly matches the C global.
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
// Edition 2024 fires `static_mut_refs` whenever you reach into a
// `static mut`. The lock + freelist layout below mirrors xv6's global
// `struct kmem` exactly, which depends on field access on the static.
// Allow the lint here; if/when we refactor the allocator to wrap the
// state in a `Spinlock<kmem_inner>` we can drop the allow.
#![allow(static_mut_refs)]

use core::ptr;

use crate::memlayout::{PGSIZE, PHYSTOP, V2P};
use arch::mmu::PGROUNDUP;
use sync::spinlock::Spinlock;
use types::addr_t;

unsafe extern "C" {
    /// First address past the kernel image, emitted by the linker
    /// script. The free pool starts at the next page boundary.
    unsafe static end: u8;
}

struct run {
    next: *mut run,
}

struct kmem_t {
    lock:     Spinlock<()>,
    use_lock: bool,
    freelist: *mut run,
}

// SAFETY: the freelist pointer chain is only ever traversed under
// `lock` (after `use_lock` is set), or single-threaded during early
// boot. Send + Sync are required to put the value in a `static`.
unsafe impl Send for kmem_t {}
unsafe impl Sync for kmem_t {}

static mut kmem: kmem_t = kmem_t {
    lock:     Spinlock::new((), "kmem"),
    use_lock: false,
    freelist: ptr::null_mut(),
};

// ---------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------

/// Build the free list across `[vstart, vend)`. Runs single-CPU
/// before SMP bring-up, so we leave `use_lock = false`.
///
/// C: `void kinit1(void *vstart, void *vend)`
pub unsafe fn kinit1(vstart: *mut u8, vend: *mut u8) {
    // C calls `initlock(&kmem.lock, "kmem")` here. Our Spinlock is
    // already initialised by its `const fn new` in the static
    // declaration, so no runtime initlock is needed.
    kmem.use_lock = false;
    kmem.freelist = ptr::null_mut();
    freerange(vstart, vend);
}

pub unsafe fn kinit2() {
    kmem.use_lock = true;
}

pub unsafe fn freerange(vstart: *mut u8, vend: *mut u8) {
    // `PGSIZE` is `addr_t` (u64) in `memlayout`; `<*mut u8>::add` takes
    // `usize`, so cast at the pointer-arithmetic sites.
    let mut p = PGROUNDUP(vstart as addr_t) as *mut u8;
    while p.add(PGSIZE as usize) <= vend {
        kfree(p);
        p = p.add(PGSIZE as usize);
    }
}


pub unsafe fn kfree(v: *mut u8) {
    // `PGSIZE`, `PHYSTOP`, and `V2P` are all `addr_t` in `memlayout`,
    // so the validation triple lives on the `addr_t` side.
    let kernel_end = &end as *const u8 as addr_t;

    if (v as addr_t) % PGSIZE != 0
        || (v as addr_t) < kernel_end
        || V2P(v as addr_t) >= PHYSTOP
    {
        panic!("kfree");
    }

    // Fill with junk (0x01) to catch dangling references — same as
    // C's `memset(v, 1, PGSIZE)`. `write_bytes`'s `count` is `usize`,
    // so narrow `PGSIZE` at the call site.
    ptr::write_bytes(v, 1, PGSIZE as usize);

    let r = v as *mut run;

    // The lock guard from `acquire()` releases on scope exit, which
    // mirrors the explicit `release(&kmem.lock)` in the C version.
    if kmem.use_lock {
        let _guard = kmem.lock.acquire();
        (*r).next = kmem.freelist;
        kmem.freelist = r;
    } else {
        (*r).next = kmem.freelist;
        kmem.freelist = r;
    }
}


pub unsafe fn kalloc() -> *mut u8 {
    let r: *mut run;

    if kmem.use_lock {
        let _guard = kmem.lock.acquire();
        r = kmem.freelist;
        if !r.is_null() {
            kmem.freelist = (*r).next;
        } else {
            panic!("Out of memory!");
        }
    } else {
        r = kmem.freelist;
        if !r.is_null() {
            kmem.freelist = (*r).next;
        } else {
            panic!("Out of memory!");
        }
    }

    r as *mut u8
}
