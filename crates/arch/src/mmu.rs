//! Port of `mmu.h` — x86_64 MMU constants and segment-descriptor
//! layout. Mirrors the upstream xv6-64 header line-for-line where
//! Rust syntax permits; C preprocessor macros become `pub const fn`
//! helpers; the segment-descriptor bit-field struct becomes a packed
//! struct with explicit byte-sized fields and a const constructor.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use types::{addr_t, pde_t};

// =====================================================================
// RFLAGS register — 64-bit on x86_64.
// =====================================================================

pub const FL_CF:        u64 = 0x0000_0001; // Carry Flag
pub const FL_PF:        u64 = 0x0000_0004; // Parity Flag
pub const FL_AF:        u64 = 0x0000_0010; // Auxiliary Carry
pub const FL_ZF:        u64 = 0x0000_0040; // Zero Flag
pub const FL_SF:        u64 = 0x0000_0080; // Sign Flag
pub const FL_TF:        u64 = 0x0000_0100; // Trap Flag
pub const FL_IF:        u64 = 0x0000_0200; // Interrupt Enable
pub const FL_DF:        u64 = 0x0000_0400; // Direction Flag
pub const FL_OF:        u64 = 0x0000_0800; // Overflow Flag
pub const FL_IOPL_MASK: u64 = 0x0000_3000; // I/O Privilege Level mask
pub const FL_IOPL_0:    u64 = 0x0000_0000;
pub const FL_IOPL_1:    u64 = 0x0000_1000;
pub const FL_IOPL_2:    u64 = 0x0000_2000;
pub const FL_IOPL_3:    u64 = 0x0000_3000;
pub const FL_NT:        u64 = 0x0000_4000; // Nested Task
pub const FL_RF:        u64 = 0x0001_0000; // Resume Flag
pub const FL_VM:        u64 = 0x0002_0000; // Virtual 8086 mode
pub const FL_AC:        u64 = 0x0004_0000; // Alignment Check
pub const FL_VIF:       u64 = 0x0008_0000; // Virtual Interrupt Flag
pub const FL_VIP:       u64 = 0x0010_0000; // Virtual Interrupt Pending
pub const FL_ID:        u64 = 0x0020_0000; // ID flag

// =====================================================================
// Control registers (CR0 / CR4) — also 64-bit on x86_64.
// Note: CR0_PG = 0x80000000 overflows i32; it must be unsigned.
// =====================================================================

pub const CR0_PE: u64 = 0x0000_0001; // Protection Enable
pub const CR0_MP: u64 = 0x0000_0002; // Monitor coProcessor
pub const CR0_EM: u64 = 0x0000_0004; // Emulation
pub const CR0_TS: u64 = 0x0000_0008; // Task Switched
pub const CR0_ET: u64 = 0x0000_0010; // Extension Type
pub const CR0_NE: u64 = 0x0000_0020; // Numeric Error
pub const CR0_WP: u64 = 0x0001_0000; // Write Protect
pub const CR0_AM: u64 = 0x0004_0000; // Alignment Mask
pub const CR0_NW: u64 = 0x2000_0000; // Not Writethrough
pub const CR0_CD: u64 = 0x4000_0000; // Cache Disable
pub const CR0_PG: u64 = 0x8000_0000; // Paging

pub const CR4_PSE:        u64 = 0x0000_0010; // Page Size Extension
pub const CR4_PAE:        u64 = 0x0000_0020; // Physical Address Extension
pub const CR4_OSXFSR:     u64 = 0x0000_0200; // OS supports FXSAVE/FXRSTOR
pub const CR4_OSXMMEXCPT: u64 = 0x0000_0400; // OS supports SSE exceptions

// =====================================================================
// Model-Specific Registers (rdmsr/wrmsr take a 32-bit ECX selector).
// =====================================================================

pub const MSR_EFER:   u32 = 0xC000_0080; // Extended Feature Enable
pub const MSR_STAR:   u32 = 0xC000_0081; // ring 0/3 segment bases
pub const MSR_LSTAR:  u32 = 0xC000_0082; // syscall entry RIP
pub const MSR_CSTAR:  u32 = 0xC000_0083; // compat-mode (unused)
pub const MSR_SFMASK: u32 = 0xC000_0084; // syscall flag mask

// =====================================================================
// Segment selectors and DPLs.
// =====================================================================

pub const DPL_USER: u16 = 0x3; // User Descriptor Privilege Level
pub const APP_SEG:  u16 = 0x1;

// GDT slot indices.
pub const SEG_KCODE:   u16 = 1; // kernel code
pub const SEG_KDATA:   u16 = 2; // kernel data + stack
pub const SEG_UCODE32: u16 = 3; // user 32-bit code
pub const SEG_UDATA:   u16 = 4; // user data + stack
pub const SEG_UCODE:   u16 = 5; // user code
pub const SEG_KCPU:    u16 = 6; // kernel per-CPU data
pub const SEG_TSS:     u16 = 7; // current task state

pub const NSEGS:     u16 = 8;
pub const CALL_GATE: u16 = 9;

// CS / DS values for user and kernel rings (selector = index<<3 | DPL).
pub const USER_CS:   u16 = (SEG_UCODE   << 3) | DPL_USER;
pub const USER_DS:   u16 = (SEG_UDATA   << 3) | DPL_USER;
pub const USER32_CS: u16 = (SEG_UCODE32 << 3) | DPL_USER;
pub const KERNEL_CS: u16 = SEG_KCODE    << 3;

// =====================================================================
// Application / system segment-type bits.
// =====================================================================

pub const STA_X: u8 = 0x8; // Executable
pub const STA_E: u8 = 0x4; // Expand-down (non-exec)
pub const STA_C: u8 = 0x4; // Conforming code (exec)
pub const STA_W: u8 = 0x2; // Writable (non-exec)
pub const STA_R: u8 = 0x2; // Readable (exec)
pub const STA_A: u8 = 0x1; // Accessed

pub const STS_T16A: u8 = 0x1; // Available 16-bit TSS
pub const STS_LDT:  u8 = 0x2; // Local Descriptor Table
pub const STS_T16B: u8 = 0x3; // Busy 16-bit TSS
pub const STS_CG16: u8 = 0x4; // 16-bit Call Gate
pub const STS_TG:   u8 = 0x5; // Task Gate
pub const STS_IG16: u8 = 0x6; // 16-bit Interrupt Gate
pub const STS_TG16: u8 = 0x7; // 16-bit Trap Gate
pub const STS_T64A: u8 = 0x9; // Available 64-bit TSS
pub const STS_T64B: u8 = 0xB; // Busy 64-bit TSS
pub const STS_CG64: u8 = 0xC; // 64-bit Call Gate
pub const STS_IG64: u8 = 0xE; // 64-bit Interrupt Gate
pub const STS_TG64: u8 = 0xF; // 64-bit Trap Gate

// =====================================================================
// 4-level paging — virtual-address split.
//
// +--16--+---9---+------9-------+-----9----+----9-------+----12-------+
// | Sign | PML4  |Page Directory| Page Dir |Page Table  | Offset Page |
// |Extend| Index | Pointer Index|  Index   |  Index     | in Page     |
// +------+-------+--------------+----------+------------+-------------+
// =====================================================================

pub const PML4XSHIFT: u32 = 39;
pub const PDPXSHIFT:  u32 = 30;
pub const PDXSHIFT:   u32 = 21;
pub const PTXSHIFT:   u32 = 12;
pub const PXMASK:     u64 = 0x1FF;

#[inline(always)]
pub const fn PMX(va: addr_t)  -> usize { ((va >> PML4XSHIFT) & PXMASK) as usize }
#[inline(always)]
pub const fn PDPX(va: addr_t) -> usize { ((va >> PDPXSHIFT)  & PXMASK) as usize }
#[inline(always)]
pub const fn PDX(va: addr_t)  -> usize { ((va >> PDXSHIFT)   & PXMASK) as usize }
#[inline(always)]
pub const fn PTX(va: addr_t)  -> usize { ((va >> PTXSHIFT)   & PXMASK) as usize }

// =====================================================================
// Page geometry.
// =====================================================================

pub const NPDENTRIES: usize = 512;  // entries per page directory
pub const NPTENTRIES: usize = 512;  // entries per page table
pub const PGSIZE:     usize = 4096; // bytes per page
pub const PGSHIFT:    u32   = 12;   // log2(PGSIZE)

#[inline(always)]
pub const fn PGROUNDUP(sz: addr_t) -> addr_t {
    (sz + (PGSIZE as addr_t - 1)) & !(PGSIZE as addr_t - 1)
}

#[inline(always)]
pub const fn PGROUNDDOWN(a: addr_t) -> addr_t {
    a & !(PGSIZE as addr_t - 1)
}

// =====================================================================
// Page-table / page-directory entry flags. Width matches `pde_t`.
// =====================================================================

pub const PTE_P:   pde_t = 0x001; // Present
pub const PTE_W:   pde_t = 0x002; // Writable
pub const PTE_U:   pde_t = 0x004; // User
pub const PTE_PWT: pde_t = 0x008; // Write-Through
pub const PTE_PCD: pde_t = 0x010; // Cache-Disable
pub const PTE_A:   pde_t = 0x020; // Accessed
pub const PTE_D:   pde_t = 0x040; // Dirty
pub const PTE_PS:  pde_t = 0x080; // Page Size
pub const PTE_MBZ: pde_t = 0x180; // Must-Be-Zero

#[inline(always)]
pub const fn PTE_ADDR(pte: pde_t)  -> addr_t { pte &  !0xFFF }
#[inline(always)]
pub const fn PTE_FLAGS(pte: pde_t) -> addr_t { pte &  0xFFF }

// =====================================================================
// Trap gate flag (used when assembling an IDT entry).
// =====================================================================

pub const TRAP_GATE: u16 = 0x100;

// =====================================================================
// Segment descriptor — 8 bytes. xv6's C uses bit-fields, which Rust
// doesn't have natively; we lay out the equivalent byte-sized fields
// and provide `seg_new` / `seg16_new` constructors that pack the
// caller's logical arguments into the right bits.
//
// Layout (little-endian, low byte first):
//   bytes 0..2 : limit[15..0]
//   bytes 2..4 : base[15..0]
//   byte  4    : base[23..16]
//   byte  5    : type[3..0] s[1] dpl[2] p[1]
//   byte  6    : limit[19..16] avl[1] l[1] db[1] g[1]
//   byte  7    : base[31..24]
// =====================================================================

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default)]
pub struct segdesc {
    pub lim_15_0:   u16,
    pub base_15_0:  u16,
    pub base_23_16: u8,
    /// type:4 | s:1 | dpl:2 | p:1
    pub access:     u8,
    /// lim_19_16:4 | avl:1 | l:1 | db:1 | g:1
    pub flags:      u8,
    pub base_31_24: u8,
}

impl segdesc {
    /// Normal (32-bit) segment descriptor.
    /// C: `SEG(type, lim, base, sys, dpl, rsv)`
    #[inline(always)]
    pub const fn seg_new(typ: u8, lim: u32, base: u32, sys: u8, dpl: u8, rsv: u8) -> Self {
        Self {
            lim_15_0:   (lim & 0xFFFF) as u16,
            base_15_0:  (base & 0xFFFF) as u16,
            base_23_16: ((base >> 16) & 0xFF) as u8,
            access:     (typ & 0xF) | ((sys & 0x1) << 4) | ((dpl & 0x3) << 5) | (1 << 7),
            flags:      (((lim >> 16) & 0xF) as u8)
                        | (0 << 4)            // avl
                        | ((rsv & 0x1) << 5)  // long-mode bit
                        | (0 << 6)            // db
                        | (1 << 7),           // g (granularity)
            base_31_24: ((base >> 24) & 0xFF) as u8,
        }
    }

    /// 16-bit segment descriptor (no granularity bit).
    /// C: `SEG16(type, base, lim, dpl)`
    #[inline(always)]
    pub const fn seg16_new(typ: u8, base: u32, lim: u32, dpl: u8) -> Self {
        Self {
            lim_15_0:   (lim & 0xFFFF) as u16,
            base_15_0:  (base & 0xFFFF) as u16,
            base_23_16: ((base >> 16) & 0xFF) as u8,
            access:     (typ & 0xF) | (1 << 4) | ((dpl & 0x3) << 5) | (1 << 7),
            flags:      ((lim >> 16) & 0xF) as u8,
            base_31_24: ((base >> 24) & 0xFF) as u8,
        }
    }
}

// xv6 also uses a `pte_t` typedef; that already lives in `types::pde_t`
// (same width, same role). Re-export for parity with the C header so
// callers can write `mmu::pte_t`.
pub use types::pde_t as pte_t;
