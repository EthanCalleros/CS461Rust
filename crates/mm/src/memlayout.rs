// Memory layout constants

/// Start of extended memory (1MB)
pub const EXTMEM: usize = 0x100000;

/// Top physical memory (224MB)
pub const PHYSTOP: usize = 0xE000000;

/// First kernel virtual address
pub const KERNBASE: usize = 0xFFFF800000000000;

/// Address where kernel is linked
pub const KERNLINK: usize = KERNBASE + EXTMEM;

// Page constants (usually defined in mmu.h in C, but often used here)
pub const PGSIZE: usize = 4096;


/// Virtual to Physical
#[inline(always)]
pub fn V2P(a: usize) -> usize {
    a.wrapping_sub(KERNBASE)
}

/// Physical to Virtual
#[inline(always)]
pub fn P2V(a: usize) -> usize {
    a.wrapping_add(KERNBASE)
}

/// Version of V2P for raw pointers
#[inline(always)]
pub fn v2p_ptr<T>(a: *const T) -> usize {
    (a as usize).wrapping_sub(KERNBASE)
}

/// Version of P2V returning a raw pointer
#[inline(always)]
pub fn p2v_ptr<T>(a: usize) -> *mut T {
    (a.wrapping_add(KERNBASE)) as *mut T
}
