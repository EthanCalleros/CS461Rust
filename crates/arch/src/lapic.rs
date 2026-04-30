//! Port of `lapic.c` — Local APIC driver. Each CPU has its own local
//! APIC for timer interrupts, IPIs, and EOI signalling. References:
//! Intel SDM Vol. 3, Chapter 8 + Appendix C.

#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]

use core::ptr;

use types::{addr_t, uchar, uint};
use types::rtcdate;

use crate::registers::{inb, outb};
use crate::traps::{IRQ_ERROR, IRQ_SPURIOUS, IRQ_TIMER, T_IRQ0};

// =====================================================================
// LAPIC register indices (byte offset / 4 → uint32 array index).
// =====================================================================

const ID:    usize = 0x0020 / 4; // ID
const VER:   usize = 0x0030 / 4; // Version
const TPR:   usize = 0x0080 / 4; // Task Priority
const EOI:   usize = 0x00B0 / 4; // EOI
const SVR:   usize = 0x00F0 / 4; // Spurious Interrupt Vector
const ESR:   usize = 0x0280 / 4; // Error Status
const ICRLO: usize = 0x0300 / 4; // Interrupt Command Low
const ICRHI: usize = 0x0310 / 4; // Interrupt Command High
const TIMER: usize = 0x0320 / 4; // LVT Timer
const PCINT: usize = 0x0340 / 4; // LVT Performance Counter
const LINT0: usize = 0x0350 / 4; // LVT LINT0
const LINT1: usize = 0x0360 / 4; // LVT LINT1
const ERROR: usize = 0x0370 / 4; // LVT Error
const TICR:  usize = 0x0380 / 4; // Timer Initial Count
const TCCR:  usize = 0x0390 / 4; // Timer Current Count
const TDCR:  usize = 0x03E0 / 4; // Timer Divide Configuration

// SVR enable bit.
const ENABLE: u32 = 0x0000_0100;

// ICRLO bits.
const INIT:    u32 = 0x0000_0500;
const STARTUP: u32 = 0x0000_0600;
const DELIVS:  u32 = 0x0000_1000;
const ASSERT:  u32 = 0x0000_4000;
const LEVEL:   u32 = 0x0000_8000;
const BCAST:   u32 = 0x0008_0000;

// LVT bits.
const X1:       u32 = 0x0000_000B; // divide-by-1
const PERIODIC: u32 = 0x0002_0000;
const MASKED:   u32 = 0x0001_0000;

// =====================================================================
// LAPIC base pointer. Set by `mp.rs` after the MP table is parsed.
// =====================================================================

#[unsafe(no_mangle)]
pub static mut lapic: *mut uint = ptr::null_mut();

#[inline(always)]
unsafe fn lapicw(index: usize, value: u32) {
    let base = lapic;
    if base.is_null() { return; }
    ptr::write_volatile(base.add(index), value);
    // Read ID to wait for the write to drain.
    let _ = ptr::read_volatile(base.add(ID));
}

#[inline(always)]
unsafe fn lapicr(index: usize) -> u32 {
    let base = lapic;
    if base.is_null() { return 0; }
    ptr::read_volatile(base.add(index))
}

/// `void lapicinit(void)` — set up this CPU's local APIC.
pub unsafe fn lapicinit() {
    if lapic.is_null() { return; }

    // Enable APIC, set spurious vector.
    lapicw(SVR, ENABLE | (T_IRQ0 + IRQ_SPURIOUS));

    // Periodic timer at bus frequency / 1.
    lapicw(TDCR, X1);
    lapicw(TIMER, PERIODIC | (T_IRQ0 + IRQ_TIMER));
    lapicw(TICR, 10_000_000);

    // Mask LINT0/LINT1.
    lapicw(LINT0, MASKED);
    lapicw(LINT1, MASKED);

    // Mask perf-counter interrupt if the LVT supports it.
    if ((lapicr(VER) >> 16) & 0xFF) >= 4 {
        lapicw(PCINT, MASKED);
    }

    // Map error LVT to IRQ_ERROR.
    lapicw(ERROR, T_IRQ0 + IRQ_ERROR);

    // Clear ESR (back-to-back writes per Intel SDM).
    lapicw(ESR, 0);
    lapicw(ESR, 0);

    // Ack any outstanding interrupts.
    lapicw(EOI, 0);

    // Send INIT level-deassert to synchronise arbitration IDs.
    lapicw(ICRHI, 0);
    lapicw(ICRLO, BCAST | INIT | LEVEL);
    while lapicr(ICRLO) & DELIVS != 0 {}

    // Enable APIC interrupts (processor-side IF stays clear).
    lapicw(TPR, 0);
}

/// `int cpunum(void)` — return the index in `cpus[]` of the calling
/// CPU. Must be called with interrupts disabled.
///
/// Stub implementation: depends on `cpus[]` and `ncpu` published by
/// `mp.rs`. Once the proc crate's `cpu` table is wired up, replace
/// this with the loop from the C source.
pub unsafe fn cpunum() -> i32 {
    if lapic.is_null() {
        return 0;
    }
    // TODO: walk `cpus[0..ncpu]` to map apicid → index. For now,
    // return the raw APIC ID, which is correct on systems where the
    // BIOS hands out 0..ncpu-1 sequentially.
    let apicid = lapicr(ID) >> 24;
    apicid as i32
}

/// `void lapiceoi(void)` — acknowledge the current interrupt.
pub unsafe fn lapiceoi() {
    if !lapic.is_null() {
        lapicw(EOI, 0);
    }
}

/// `void microdelay(int us)` — busy-wait for `us` microseconds.
/// Empty in upstream xv6 (timing depends on bus speed).
pub unsafe fn microdelay(_us: i32) {
    // TODO: calibrate against the LAPIC timer.
}

// =====================================================================
// CMOS access — RTC / shutdown vector / time of day.
// =====================================================================

const CMOS_PORT:   u16 = 0x70;
const CMOS_RETURN: u16 = 0x71;

const CMOS_STATA: u32 = 0x0A;
const CMOS_STATB: u32 = 0x0B;
const CMOS_UIP:   u32 = 1 << 7;

const SECS:  u32 = 0x00;
const MINS:  u32 = 0x02;
const HOURS: u32 = 0x04;
const DAY:   u32 = 0x07;
const MONTH: u32 = 0x08;
const YEAR:  u32 = 0x09;

unsafe fn cmos_read(reg: u32) -> uint {
    outb(CMOS_PORT, reg as u8);
    microdelay(200);
    inb(CMOS_RETURN) as uint
}

unsafe fn fill_rtcdate(r: *mut rtcdate) {
    (*r).second = cmos_read(SECS);
    (*r).minute = cmos_read(MINS);
    (*r).hour   = cmos_read(HOURS);
    (*r).day    = cmos_read(DAY);
    (*r).month  = cmos_read(MONTH);
    (*r).year   = cmos_read(YEAR);
}

/// `void lapicstartap(uchar apicid, uint addr)` — boot another
/// processor by sending it INIT-SIPI-SIPI. `addr` is the AP's start
/// vector (must be page-aligned in the low 1 MiB).
pub unsafe fn lapicstartap(apicid: uchar, addr: u32) {
    const KERNBASE: addr_t = 0xFFFF_8000_0000_0000;

    // Set CMOS shutdown code to 0x0A (warm reset).
    outb(CMOS_PORT, 0xF);
    outb(CMOS_PORT + 1, 0x0A);

    // Warm reset vector at 40:67 (segment:offset → linear 0x467).
    let wrv = (KERNBASE + (0x40 << 4 | 0x67)) as *mut u16;
    ptr::write_volatile(wrv, 0);
    ptr::write_volatile(wrv.add(1), (addr >> 4) as u16);

    // Universal startup: INIT (assert), then INIT (deassert), then
    // STARTUP twice.
    lapicw(ICRHI, (apicid as u32) << 24);
    lapicw(ICRLO, INIT | LEVEL | ASSERT);
    microdelay(200);
    lapicw(ICRLO, INIT | LEVEL);
    microdelay(100);

    for _ in 0..2 {
        lapicw(ICRHI, (apicid as u32) << 24);
        lapicw(ICRLO, STARTUP | (addr >> 12));
        microdelay(200);
    }
}

/// `void cmostime(struct rtcdate *r)` — read wall-clock time from
/// CMOS. QEMU returns 24-hour BCD by default.
pub unsafe fn cmostime(r: *mut rtcdate) {
    let mut t1: rtcdate = rtcdate::default();
    let mut t2: rtcdate = rtcdate::default();

    let sb = cmos_read(CMOS_STATB);
    let bcd = (sb & (1 << 2)) == 0;

    // Read twice across an update boundary to make sure CMOS didn't
    // tick during our read.
    loop {
        fill_rtcdate(&mut t1);
        if cmos_read(CMOS_STATA) & CMOS_UIP != 0 {
            continue;
        }
        fill_rtcdate(&mut t2);
        // memcmp equivalent — compare every field.
        if t1.second == t2.second
            && t1.minute == t2.minute
            && t1.hour == t2.hour
            && t1.day == t2.day
            && t1.month == t2.month
            && t1.year == t2.year
        {
            break;
        }
    }

    if bcd {
        // BCD → binary: high nibble * 10 + low nibble.
        fn conv(x: uint) -> uint { ((x >> 4) * 10) + (x & 0xF) }
        t1.second = conv(t1.second);
        t1.minute = conv(t1.minute);
        t1.hour   = conv(t1.hour);
        t1.day    = conv(t1.day);
        t1.month  = conv(t1.month);
        t1.year   = conv(t1.year);
    }

    *r = t1;
    (*r).year += 2000;
}
