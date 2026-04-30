//! Port of `ioapic.c` — I/O APIC driver for SMP interrupt routing.
//!
//! Reference: Intel IOAPIC datasheet
//! <http://www.intel.com/design/chipsets/datashts/29056601.pdf>

#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]

use core::ptr;

use types::{addr_t, uchar, uint};

use crate::traps::T_IRQ0;

/// Default physical address of the I/O APIC.
pub const IOAPIC: addr_t = 0xFEC0_0000;

// Register indices (selector values written to the `reg` window).
const REG_ID:    u32 = 0x00;
const REG_VER:   u32 = 0x01;
const REG_TABLE: u32 = 0x10;

// Redirection-table entry bit positions.
const INT_DISABLED:  u32 = 0x0001_0000; // mask out
const INT_LEVEL:     u32 = 0x0000_8000; // level-triggered
const INT_ACTIVELOW: u32 = 0x0000_2000; // active low
const INT_LOGICAL:   u32 = 0x0000_0800; // destination is logical APIC ID

/// MMIO layout of the IOAPIC: write `reg`, then read/write `data`.
#[repr(C)]
pub struct ioapic {
    pub reg:  uint,
    pub pad:  [uint; 3],
    pub data: uint,
}

/// Pointer to the mapped IOAPIC; populated by `ioapicinit`.
static mut IOAPIC_PTR: *mut ioapic = ptr::null_mut();

// IOAPIC ID published by `mp.rs` after parsing the MP configuration
// table. Declared `extern` here so we don't pull `mp.rs` symbols
// into a cyclic include.
unsafe extern "C" {
    static ioapicid: uchar;
}

#[inline(always)]
unsafe fn ioapicread(reg: u32) -> uint {
    let p = IOAPIC_PTR;
    ptr::write_volatile(&mut (*p).reg, reg);
    ptr::read_volatile(&(*p).data)
}

#[inline(always)]
unsafe fn ioapicwrite(reg: u32, data: uint) {
    let p = IOAPIC_PTR;
    ptr::write_volatile(&mut (*p).reg, reg);
    ptr::write_volatile(&mut (*p).data, data);
}

/// `void ioapicinit(void)` — discover and initialize the I/O APIC.
pub unsafe fn ioapicinit() {
    // P2V(IOAPIC) — the kernel direct-map exposes physical IO at the
    // KERNBASE+phys offset. Use the published `mm::memlayout::P2V`
    // helper if you've added a cross-crate dep; otherwise compute
    // inline. Hardcoded KERNBASE here for arch-self-containment.
    const KERNBASE: addr_t = 0xFFFF_8000_0000_0000;
    IOAPIC_PTR = (IOAPIC + KERNBASE) as *mut ioapic;

    let maxintr = (ioapicread(REG_VER) >> 16) & 0xFF;
    let id = ioapicread(REG_ID) >> 24;
    if id as uchar != ioapicid {
        // cprintf isn't ported yet; once it is, swap this for a real
        // diagnostic. For now we silently continue — non-fatal.
    }

    // Mark every interrupt edge-triggered, active-high, masked, and
    // unrouted.
    for i in 0..=maxintr {
        ioapicwrite(REG_TABLE + 2 * i, INT_DISABLED | (T_IRQ0 + i));
        ioapicwrite(REG_TABLE + 2 * i + 1, 0);
    }
}

/// `void ioapicenable(int irq, int cpunum)` — route `irq` to CPU
/// `cpunum`'s APIC ID, edge-triggered, active high, unmasked.
pub unsafe fn ioapicenable(irq: i32, cpunum: i32) {
    ioapicwrite(REG_TABLE + 2 * irq as u32, T_IRQ0 + irq as u32);
    ioapicwrite(REG_TABLE + 2 * irq as u32 + 1, (cpunum as u32) << 24);
}
