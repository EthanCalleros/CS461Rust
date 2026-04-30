//! Port of `vm.c` — virtual memory management on x86_64.
//!
//! Page-table layout: 4-level (PML4 → PDPT → PD → PT). The kernel
//! direct-maps physical memory at `KERNBASE`. User pages are mapped
//! per-process; setupkvm() builds a fresh PML4 with the kernel half
//! pre-populated.

#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use core::ptr;

use types::{addr_t, pde_t, pdpe_t, pml4e_t, uint, uint64};

use crate::mmu::{
    pte_t, APP_SEG, DPL_USER, FL_AC, FL_DF, FL_IF, FL_IOPL_3, FL_NT, FL_TF,
    KERNEL_CS, MSR_CSTAR, MSR_LSTAR, MSR_SFMASK, MSR_STAR, NPDENTRIES, NSEGS,
    PDPX, PDX, PGROUNDDOWN, PGROUNDUP, PMX, PTE_ADDR, PTE_FLAGS,
    PTE_P, PTE_PCD, PTE_PS, PTE_PWT, PTE_U, PTE_W, PTX, SEG_KCODE, SEG_KDATA,
    SEG_KCPU, SEG_TSS, SEG_UCODE, SEG_UCODE32, SEG_UDATA, STA_R, STA_W, STA_X,
    STS_T64A, USER32_CS, segdesc,
};
use crate::registers::{lcr3, lgdt, ltr};

/// `arch::mmu::PGSIZE` is `usize`. The vm code does almost all of its
/// page-aligned arithmetic in `addr_t` (= u64), so we shadow it with
/// a same-named local constant of the right type. Pointer-width APIs
/// (`<*mut u8>::add`, `core::ptr::write_bytes`) take `usize` — cast
/// explicitly at those call sites.
const PGSIZE: addr_t = crate::mmu::PGSIZE as addr_t;

// =====================================================================
// Memory layout — duplicated from `mm::memlayout` to avoid a circular
// arch ↔ mm dep. Adjust both places if you change the kernel base.
// =====================================================================

pub const KERNBASE:  addr_t = 0xFFFF_8000_0000_0000;
pub const KSTACKSIZE: addr_t = 4096;

#[inline(always)]
fn V2P(a: addr_t) -> addr_t { a.wrapping_sub(KERNBASE) }
#[inline(always)]
fn P2V<T>(a: addr_t) -> *mut T { (a + KERNBASE) as *mut T }
#[inline(always)]
fn v2p<T>(p: *const T) -> addr_t { (p as addr_t).wrapping_sub(KERNBASE) }

// =====================================================================
// External symbols — most are functions / globals from other crates
// or assembly that aren't ported yet. Each `extern` here is a
// placeholder; replace with a `use` once the real definition exists.
// =====================================================================

unsafe extern "C" {
    /// Linker-emitted symbol: start of read/write data. Used to
    /// distinguish ".rodata" mapping from ".rwdata" mapping.
    pub static data: u8;

    fn kalloc() -> *mut u8;
    fn kfree(v: *mut u8);

    fn cpunum() -> i32;

    fn pushcli();
    fn popcli();

    fn wrmsr(msr: u32, value: u64);

    fn syscall_entry();
    fn ignore_sysret();

    fn readi(ip: *mut inode, dst: *mut u8, off: u32, n: u32) -> i32;

    /// Per-CPU table from `mp.rs`.
    pub static mut cpus: [cpu_arch_local; 8];
}

/// Per-process inode for `loaduvm`. Real type lives in `fs::file`;
/// this is a placeholder opaque struct so `vm.rs` can compile in
/// isolation. Replace with a real `use fs::fs::inode;` once the fs
/// crate is wired up.
#[repr(C)]
pub struct inode {
    _opaque: [u8; 0],
}

/// Stand-in for the per-CPU local-storage struct. Real one is in
/// `proc::cpu`. Only the `local` field matters here.
#[repr(C)]
pub struct cpu_arch_local {
    pub apicid: u8,
    pub _pad:   [u8; 7],
    pub local:  *mut u8,
}

/// Kernel page-table root, set by `kvmalloc`.
#[unsafe(no_mangle)]
pub static mut kpgdir: *mut pde_t = ptr::null_mut();

static mut kpml4: *mut pml4e_t = ptr::null_mut();
static mut kpdpt: *mut pdpe_t = ptr::null_mut();

// Per-CPU `cpu`/`proc` are TLS via `%fs`. Rust's TLS isn't usable
// here; declare placeholders so other modules can `extern` them.
#[unsafe(no_mangle)]
pub static mut cpu: *mut core::ffi::c_void = ptr::null_mut();
#[unsafe(no_mangle)]
pub static mut proc: *mut core::ffi::c_void = ptr::null_mut();

// =====================================================================
// syscallinit — set up SYSCALL/SYSRET MSRs.
// =====================================================================

pub unsafe fn syscallinit() {
    wrmsr(
        MSR_STAR,
        ((USER32_CS as u64) << 48) | ((KERNEL_CS as u64) << 32),
    );
    wrmsr(MSR_LSTAR, syscall_entry as addr_t);
    wrmsr(MSR_CSTAR, ignore_sysret as addr_t);
    wrmsr(
        MSR_SFMASK,
        FL_TF | FL_DF | FL_IF | FL_IOPL_3 | FL_AC | FL_NT,
    );
}

// =====================================================================
// seginit — set up this CPU's GDT, TSS, and FS-base for per-CPU TLS.
// =====================================================================

pub unsafe fn seginit() {
    let local = kalloc();
    if local.is_null() {
        panic!("seginit: kalloc failed");
    }
    ptr::write_bytes(local, 0, PGSIZE as usize);

    let gdt = local as *mut segdesc;
    let tss = local.add(1024) as *mut uint;

    // IO Map Base = end of TSS.
    *tss.add(16) = 0x0068_0000;

    // FS base = middle of the per-CPU page (xv6 stores `cpu`/`proc`
    // pointers at negative offsets from FS).
    wrmsr(0xC000_0100, local as u64 + 2048);

    let cpunum = cpunum();
    cpus[cpunum as usize].local = local;

    let addr = tss as u64;

    *gdt.add(0)                   = segdesc::default();
    *gdt.add(SEG_KCODE   as usize) = segdesc::seg_new(STA_X | STA_R, 0, 0, APP_SEG as u8, 0,            1);
    *gdt.add(SEG_KDATA   as usize) = segdesc::seg_new(STA_W,         0, 0, APP_SEG as u8, 0,            0);
    *gdt.add(SEG_UCODE32 as usize) = segdesc::default();
    *gdt.add(SEG_UDATA   as usize) = segdesc::seg_new(STA_W,         0, 0, APP_SEG as u8, DPL_USER as u8, 0);
    *gdt.add(SEG_UCODE   as usize) = segdesc::seg_new(STA_X | STA_R, 0, 0, APP_SEG as u8, DPL_USER as u8, 1);
    *gdt.add(SEG_KCPU    as usize) = segdesc::default();
    *gdt.add(SEG_TSS     as usize) = segdesc::seg_new(STS_T64A, 0xB, addr as u32, 0, DPL_USER as u8, 0);
    *gdt.add(SEG_TSS as usize + 1) = segdesc::seg_new(0, (addr >> 32) as u32, (addr >> 48) as u32, 0, 0, 0);

    lgdt(gdt, ((NSEGS as i32 + 1) * core::mem::size_of::<segdesc>() as i32) as i32);
    ltr((SEG_TSS << 3) as u16);
}

// =====================================================================
// setupkvm / kvmalloc / switchkvm / switchuvm
// =====================================================================

pub unsafe fn setupkvm() -> *mut pml4e_t {
    let pml4 = kalloc() as *mut pml4e_t;
    if pml4.is_null() { return ptr::null_mut(); }
    ptr::write_bytes(pml4 as *mut u8, 0, PGSIZE as usize);
    *pml4.add(256) = v2p(kpdpt) | PTE_P | PTE_W;
    pml4
}

pub unsafe fn kvmalloc() {
    kpml4 = kalloc() as *mut pml4e_t;
    ptr::write_bytes(kpml4 as *mut u8, 0, PGSIZE as usize);

    kpdpt = kalloc() as *mut pdpe_t;
    ptr::write_bytes(kpdpt as *mut u8, 0, PGSIZE as usize);
    *kpml4.add(PMX(KERNBASE)) = v2p(kpdpt) | PTE_P | PTE_W;

    // Direct-map first 1 GiB.
    *kpdpt.add(0) = 0 | PTE_PS | PTE_P | PTE_W;
    // Direct-map 4th GiB for memory-mapped I/O (LAPIC, IOAPIC).
    *kpdpt.add(3) = 0xC000_0000 | PTE_PS | PTE_P | PTE_W | PTE_PWT | PTE_PCD;

    switchkvm();
}

pub unsafe fn switchkvm() {
    lcr3(v2p(kpml4));
}

/// `void switchuvm(struct proc *p)` — load the user pgdir for `p`.
/// Stub: the real version reads p->pgdir, p->kstack, etc.
pub unsafe fn switchuvm(_p: *mut core::ffi::c_void) {
    pushcli();
    // TODO: reach into proc, set TSS rsp0 to kstack + KSTACKSIZE,
    // then `lcr3(v2p(p->pgdir))`. Stubbed to keep the type-check pass.
    popcli();
}

// =====================================================================
// walkpgdir — walk PML4→PDPT→PD→PT to the PTE for `va`. Allocates
// missing tables when `alloc` is true.
// =====================================================================

unsafe fn walkpgdir(pml4: *mut pml4e_t, va: addr_t, alloc: bool) -> *mut pte_t {
    // PML4 → PDPT
    let pml4e = pml4.add(PMX(va));
    let pdp: *mut pdpe_t = if *pml4e & PTE_P != 0 {
        P2V::<pdpe_t>(PTE_ADDR(*pml4e))
    } else {
        if !alloc { return ptr::null_mut(); }
        let p = kalloc() as *mut pdpe_t;
        if p.is_null() { return ptr::null_mut(); }
        ptr::write_bytes(p as *mut u8, 0, PGSIZE as usize);
        *pml4e = V2P(p as addr_t) | PTE_P | PTE_W | PTE_U;
        p
    };

    // PDPT → PD
    let pdpe = pdp.add(PDPX(va));
    let pd: *mut pde_t = if *pdpe & PTE_P != 0 {
        P2V::<pde_t>(PTE_ADDR(*pdpe))
    } else {
        if !alloc { return ptr::null_mut(); }
        let p = kalloc() as *mut pde_t;
        if p.is_null() { return ptr::null_mut(); }
        ptr::write_bytes(p as *mut u8, 0, PGSIZE as usize);
        *pdpe = V2P(p as addr_t) | PTE_P | PTE_W | PTE_U;
        p
    };

    // PD → PT
    let pde = pd.add(PDX(va));
    let pgtab: *mut pte_t = if *pde & PTE_P != 0 {
        P2V::<pte_t>(PTE_ADDR(*pde))
    } else {
        if !alloc { return ptr::null_mut(); }
        let p = kalloc() as *mut pte_t;
        if p.is_null() { return ptr::null_mut(); }
        ptr::write_bytes(p as *mut u8, 0, PGSIZE as usize);
        *pde = V2P(p as addr_t) | PTE_P | PTE_W | PTE_U;
        p
    };

    pgtab.add(PTX(va))
}

// =====================================================================
// mappages — install PTEs for [va, va+size) → [pa, pa+size).
// =====================================================================

pub unsafe fn mappages(
    pgdir: *mut pde_t,
    va: addr_t,
    size: addr_t,
    mut pa: addr_t,
    perm: pde_t,
) -> i32 {
    let mut a = PGROUNDDOWN(va);
    let last = PGROUNDDOWN(va + size - 1);
    loop {
        let pte = walkpgdir(pgdir as *mut pml4e_t, a, true);
        if pte.is_null() { return -1; }
        if *pte & PTE_P != 0 {
            panic!("remap");
        }
        *pte = pa | perm | PTE_P;
        if a == last { break; }
        a += PGSIZE;
        pa += PGSIZE;
    }
    0
}

// =====================================================================
// inituvm — load initcode into a fresh process at virtual address
// PGSIZE (= 4 KiB).
// =====================================================================

pub unsafe fn inituvm(pgdir: *mut pde_t, init: *const u8, sz: uint) {
    if sz as addr_t >= PGSIZE {
        panic!("inituvm: more than a page");
    }
    let mem = kalloc();
    ptr::write_bytes(mem, 0, PGSIZE as usize);
    mappages(pgdir, PGSIZE, PGSIZE, V2P(mem as addr_t), PTE_W | PTE_U);
    ptr::copy_nonoverlapping(init, mem, sz as usize);
}

// =====================================================================
// loaduvm — load `sz` bytes from inode `ip` (at file offset `offset`)
// into already-mapped pages starting at `addr`. `addr` must be page-
// aligned.
// =====================================================================

pub unsafe fn loaduvm(
    pgdir: *mut pde_t,
    addr: addr_t,
    ip: *mut inode,
    offset: uint,
    sz: uint,
) -> i32 {
    if addr % PGSIZE != 0 {
        panic!("loaduvm: addr must be page aligned");
    }
    let mut i: uint = 0;
    while (i as addr_t) < sz as addr_t {
        let pte = walkpgdir(pgdir as *mut pml4e_t, addr + i as addr_t, false);
        if pte.is_null() {
            panic!("loaduvm: address should exist");
        }
        let pa = PTE_ADDR(*pte);
        let n = if (sz - i) < PGSIZE as uint { sz - i } else { PGSIZE as uint };
        if readi(ip, P2V::<u8>(pa), offset + i, n) != n as i32 {
            return -1;
        }
        i += PGSIZE as uint;
    }
    0
}

// =====================================================================
// allocuvm / deallocuvm — grow/shrink a process's address space.
// =====================================================================

pub unsafe fn allocuvm(pgdir: *mut pde_t, oldsz: uint64, newsz: uint64) -> uint64 {
    if newsz >= KERNBASE { return 0; }
    if newsz < oldsz { return oldsz; }

    let mut a = PGROUNDUP(oldsz);
    while a < newsz {
        let mem = kalloc();
        if mem.is_null() {
            deallocuvm(pgdir, newsz, oldsz);
            return 0;
        }
        ptr::write_bytes(mem, 0, PGSIZE as usize);
        if mappages(pgdir, a, PGSIZE, V2P(mem as addr_t), PTE_W | PTE_U) < 0 {
            deallocuvm(pgdir, newsz, oldsz);
            kfree(mem);
            return 0;
        }
        a += PGSIZE;
    }
    newsz
}

pub unsafe fn deallocuvm(pgdir: *mut pde_t, oldsz: uint64, newsz: uint64) -> uint64 {
    if newsz >= oldsz { return oldsz; }

    let mut a = PGROUNDUP(newsz);
    while a < oldsz {
        let pte = walkpgdir(pgdir as *mut pml4e_t, a, false);
        if !pte.is_null() && *pte & PTE_P != 0 {
            let pa = PTE_ADDR(*pte);
            if pa == 0 {
                panic!("kfree");
            }
            let v: *mut u8 = P2V(pa);
            kfree(v);
            *pte = 0;
        }
        a += PGSIZE;
    }
    newsz
}

// =====================================================================
// freevm — tear down a complete page-table tree.
// =====================================================================

pub unsafe fn freevm(pml4: *mut pml4e_t) {
    if pml4.is_null() {
        panic!("freevm: no pgdir");
    }

    for i in 0..(NPDENTRIES / 2) {
        let pml4e = *pml4.add(i);
        if pml4e & PTE_P == 0 { continue; }
        let pdp: *mut pdpe_t = P2V(PTE_ADDR(pml4e));

        for j in 0..NPDENTRIES {
            let pdpe = *pdp.add(j);
            if pdpe & PTE_P == 0 { continue; }
            let pd: *mut pde_t = P2V(PTE_ADDR(pdpe));

            for k in 0..NPDENTRIES {
                let pde = *pd.add(k);
                if pde & PTE_P == 0 { continue; }
                let pt: *mut pte_t = P2V(PTE_ADDR(pde));

                for l in 0..NPDENTRIES {
                    let pte = *pt.add(l);
                    if pte & PTE_P != 0 {
                        let v: *mut u8 = P2V(PTE_ADDR(pte));
                        kfree(v);
                    }
                }
                kfree(pt as *mut u8);
            }
            kfree(pd as *mut u8);
        }
        kfree(pdp as *mut u8);
    }
    kfree(pml4 as *mut u8);
}

// =====================================================================
// clearpteu / copyuvm / uva2ka / copyout
// =====================================================================

pub unsafe fn clearpteu(pgdir: *mut pml4e_t, uva: addr_t) {
    let pte = walkpgdir(pgdir, uva, false);
    if pte.is_null() {
        panic!("clearpteu");
    }
    *pte &= !PTE_U;
}

pub unsafe fn copyuvm(pgdir: *mut pml4e_t, sz: uint) -> *mut pde_t {
    let d = setupkvm();
    if d.is_null() { return ptr::null_mut(); }

    let mut i: addr_t = PGSIZE;
    while i < sz as addr_t {
        let pte = walkpgdir(pgdir, i, false);
        if pte.is_null() {
            panic!("copyuvm: pte should exist");
        }
        if *pte & PTE_P == 0 {
            panic!("copyuvm: page not present");
        }
        let pa = PTE_ADDR(*pte);
        let flags = PTE_FLAGS(*pte);
        let mem = kalloc();
        if mem.is_null() {
            freevm(d);
            return ptr::null_mut();
        }
        ptr::copy_nonoverlapping(P2V::<u8>(pa), mem, PGSIZE as usize);
        if mappages(d as *mut pde_t, i, PGSIZE, V2P(mem as addr_t), flags) < 0 {
            freevm(d);
            return ptr::null_mut();
        }
        i += PGSIZE;
    }
    d as *mut pde_t
}

pub unsafe fn uva2ka(pgdir: *mut pml4e_t, uva: addr_t) -> *mut u8 {
    let pte = walkpgdir(pgdir, uva, false);
    if pte.is_null() || *pte & PTE_P == 0 || *pte & PTE_U == 0 {
        return ptr::null_mut();
    }
    P2V(PTE_ADDR(*pte))
}

pub unsafe fn copyout(pgdir: *mut pml4e_t, mut va: addr_t, p: *const u8, mut len: uint64) -> i32 {
    let mut buf = p;
    while len > 0 {
        let va0 = PGROUNDDOWN(va);
        let pa0 = uva2ka(pgdir, va0);
        if pa0.is_null() { return -1; }
        let mut n = PGSIZE - (va - va0);
        if n > len { n = len; }
        ptr::copy_nonoverlapping(buf, pa0.add((va - va0) as usize), n as usize);
        len -= n;
        buf = buf.add(n as usize);
        va = va0 + PGSIZE;
    }
    0
}
