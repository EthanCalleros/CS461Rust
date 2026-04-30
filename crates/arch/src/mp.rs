//! Port of `mp.c` — discover SMP topology by parsing the BIOS-
//! provided MP Floating Pointer Structure and configuration table.
//! Reference: Intel MultiProcessor Specification 1.4.

#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]

use core::ptr;
use core::slice;

use types::{addr_t, uchar, uint, uint32};

use crate::lapic;
use crate::registers::{inb, outb};

const NCPU: usize = 8; // mirrors `param::NCPU`

// Table-entry type tags.
const MPPROC:    uchar = 0x00;
const MPBUS:     uchar = 0x01;
const MPIOAPIC:  uchar = 0x02;
const MPIOINTR:  uchar = 0x03;
const MPLINTR:   uchar = 0x04;

const MPBOOT: uchar = 0x02; // mpproc.flags bit: bootstrap CPU

const KERNBASE: addr_t = 0xFFFF_8000_0000_0000;

// =====================================================================
// MP table structures (each `#[repr(C)]` and `#[repr(C, packed)]`
// where the spec doesn't pad).
// =====================================================================

/// MP Floating Pointer Structure. Located by scanning BIOS memory.
#[repr(C, packed)]
pub struct mp {
    pub signature: [uchar; 4], // "_MP_"
    pub physaddr:  uint32,
    pub length:    uchar,
    pub specrev:   uchar,
    pub checksum:  uchar,
    pub r#type:    uchar,
    pub imcrp:     uchar,
    pub reserved:  [uchar; 3],
}

/// MP Configuration Table header.
#[repr(C, packed)]
pub struct mpconf {
    pub signature:    [uchar; 4], // "PCMP"
    pub length:       u16,
    pub version:      uchar,
    pub checksum:     uchar,
    pub product:      [uchar; 20],
    pub oemtable_p:   uint32,
    pub oemlength:    u16,
    pub entry:        u16,
    pub lapicaddr_p:  uint32,
    pub xlength:      u16,
    pub xchecksum:    uchar,
    pub reserved:     uchar,
}

#[repr(C, packed)]
pub struct mpproc {
    pub r#type:    uchar,
    pub apicid:    uchar,
    pub version:   uchar,
    pub flags:     uchar,
    pub signature: [uchar; 4],
    pub feature:   uint,
    pub reserved:  [uchar; 8],
}

#[repr(C, packed)]
pub struct mpioapic {
    pub r#type:   uchar,
    pub apicno:   uchar,
    pub version:  uchar,
    pub flags:    uchar,
    pub addr_p:   uint32,
}

// =====================================================================
// Per-CPU state — cpus[] table populated as we walk MPPROC entries.
// Real `struct cpu` lives in `proc.rs`; we publish a thin stand-in
// here so `mp.rs` can compile in isolation. Replace once `proc::cpu`
// exists.
// =====================================================================

#[repr(C)]
#[derive(Clone, Copy)]
pub struct cpu_stub {
    pub apicid: uchar,
    pub _pad:   [uchar; 7],
}

#[unsafe(no_mangle)]
pub static mut cpus: [cpu_stub; NCPU] = [cpu_stub { apicid: 0, _pad: [0; 7] }; NCPU];
#[unsafe(no_mangle)]
pub static mut ncpu: i32 = 0;
#[unsafe(no_mangle)]
pub static mut ioapicid: uchar = 0;

// =====================================================================
// Helpers.
// =====================================================================

unsafe fn sum(addr: *const uchar, len: usize) -> uchar {
    let mut s: u32 = 0;
    let buf = slice::from_raw_parts(addr, len);
    for &b in buf {
        s = s.wrapping_add(b as u32);
    }
    s as uchar
}

unsafe fn memcmp(a: *const uchar, b: *const uchar, n: usize) -> i32 {
    let sa = slice::from_raw_parts(a, n);
    let sb = slice::from_raw_parts(b, n);
    if sa == sb { 0 } else { 1 }
}

#[inline(always)]
unsafe fn p2v<T>(pa: addr_t) -> *mut T {
    (pa + KERNBASE) as *mut T
}

// =====================================================================
// Scan helpers.
// =====================================================================

/// Look for an MP signature in `len` bytes starting at physical `a`.
unsafe fn mpsearch1(a: addr_t, len: usize) -> *mut mp {
    let addr: *mut uchar = p2v(a);
    let end = addr.add(len);
    let mut p = addr;
    while p < end {
        if memcmp(p, b"_MP_".as_ptr(), 4) == 0
            && sum(p, core::mem::size_of::<mp>()) == 0
        {
            return p as *mut mp;
        }
        p = p.add(core::mem::size_of::<mp>());
    }
    ptr::null_mut()
}

/// Search the BIOS-known locations for the MP Floating Pointer.
unsafe fn mpsearch() -> *mut mp {
    let bda: *const uchar = p2v(0x400);
    let ebda_seg = ((*bda.add(0x0F) as u32) << 8) | (*bda.add(0x0E) as u32);
    let p = ebda_seg << 4;
    if p != 0 {
        let m = mpsearch1(p as addr_t, 1024);
        if !m.is_null() { return m; }
    } else {
        let basemem_kb = ((*bda.add(0x14) as u32) << 8) | (*bda.add(0x13) as u32);
        let p = basemem_kb * 1024;
        let m = mpsearch1((p - 1024) as addr_t, 1024);
        if !m.is_null() { return m; }
    }
    mpsearch1(0xF_0000, 0x1_0000)
}

/// Find and validate the MP Configuration Table.
unsafe fn mpconfig(pmp: *mut *mut mp) -> *mut mpconf {
    let m = mpsearch();
    if m.is_null() || (*m).physaddr == 0 {
        return ptr::null_mut();
    }
    let conf: *mut mpconf = p2v((*m).physaddr as addr_t);
    if memcmp((*conf).signature.as_ptr(), b"PCMP".as_ptr(), 4) != 0 {
        return ptr::null_mut();
    }
    if (*conf).version != 1 && (*conf).version != 4 {
        return ptr::null_mut();
    }
    let len = (*conf).length as usize;
    if sum(conf as *const uchar, len) != 0 {
        return ptr::null_mut();
    }
    *pmp = m;
    conf
}

/// `void mpinit(void)` — discover SMP topology and publish into
/// `cpus[]`, `ncpu`, `ioapicid`, and `lapic`.
pub unsafe fn mpinit() {
    let mut m: *mut mp = ptr::null_mut();
    let conf = mpconfig(&mut m);
    if conf.is_null() {
        // No additional CPUs found.
        return;
    }

    lapic::lapic = p2v((*conf).lapicaddr_p as addr_t);

    let conf_end = (conf as *mut uchar).add((*conf).length as usize);
    let mut p = (conf as *mut uchar).add(core::mem::size_of::<mpconf>());

    while p < conf_end {
        match *p {
            t if t == MPPROC => {
                let proc_entry = p as *mut mpproc;
                if (ncpu as usize) < NCPU {
                    cpus[ncpu as usize].apicid = (*proc_entry).apicid;
                    ncpu += 1;
                }
                p = p.add(core::mem::size_of::<mpproc>());
            }
            t if t == MPIOAPIC => {
                let io = p as *mut mpioapic;
                ioapicid = (*io).apicno;
                p = p.add(core::mem::size_of::<mpioapic>());
            }
            t if t == MPBUS || t == MPIOINTR || t == MPLINTR => {
                p = p.add(8);
            }
            _ => {
                // Unknown entry type — would panic in xv6.
                break;
            }
        }
    }

    if (*m).imcrp != 0 {
        // Mask external interrupts via IMCR.
        outb(0x22, 0x70);
        let v = inb(0x23) | 1;
        outb(0x23, v);
    }
}
