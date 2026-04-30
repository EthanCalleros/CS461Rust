//! Memory layout constants and address-translation helpers (port of
//! `memlayout.h`). Function names are kept in upper case to match the
//! upstream xv6 macros (`V2P`, `P2V`, `PGROUNDUP`, ...).

#![allow(non_snake_case)]

use types::addr_t;
use arch::mmu::{PGROUNDDOWN, PGROUNDUP};
// ---------------------------------------------------------------------
// Memory map constants
// ---------------------------------------------------------------------

/// Start of extended memory (1 MiB).
pub const EXTMEM: addr_t = 0x100000;

/// Top of physical memory (224 MiB) — anything above this is reserved.
pub const PHYSTOP: addr_t = 0xE000000;

/// First kernel virtual address.
///
/// NOTE: verify this against your specific xv6-64 source — UIC's
/// xv6-64 uses `0xFFFFFFFF80000000` (top-2GB higher half). Other ports
/// vary. Adjust if your `memlayout.h` says otherwise.
pub const KERNBASE: addr_t = 0xFFFF_8000_0000_0000;

/// Address where the kernel image is linked.
pub const KERNLINK: addr_t = KERNBASE + EXTMEM;

// ---------------------------------------------------------------------
// Page-size constants
// ---------------------------------------------------------------------
// In xv6 these live in `mmu.h`, but kalloc.c uses them so we re-export
// them from here for now. Once `arch::mmu` has them defined, switch
// the imports to use that source of truth.

/// 4 KiB page size.
pub const PGSIZE: addr_t = 4096;

// ---------------------------------------------------------------------
// Address translation
// ---------------------------------------------------------------------

/// Virtual to physical (kernel direct map).
#[inline(always)]
pub fn V2P(a: addr_t) -> addr_t {
    a.wrapping_sub(KERNBASE)
}

/// Physical to virtual (kernel direct map).
#[inline(always)]
pub fn P2V(a: addr_t) -> addr_t {
    a.wrapping_add(KERNBASE)
}

/// `V2P` for raw pointers.
#[inline(always)]
pub fn v2p_ptr<T>(a: *const T) -> addr_t {
    (a as addr_t).wrapping_sub(KERNBASE)
}

/// `P2V` returning a raw pointer.
#[inline(always)]
pub fn p2v_ptr<T>(a: addr_t) -> *mut T {
    a.wrapping_add(KERNBASE) as *mut T
}