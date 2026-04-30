#![no_std]
#![no_main]
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(static_mut_refs)]

//! Port of xv6's `main.c`.
//!
//! Two entry points:
//!
//! * `main()`     — bootstrap CPU. Called from `entry.S` once long
//!                  mode is up. Initialises every subsystem in
//!                  dependency order, kicks the other CPUs into
//!                  life, then drops into the scheduler.
//! * `mpenter()`  — non-boot (AP) CPUs jump here from
//!                  `entryother.S` after they finish their own
//!                  long-mode bring-up.
//!
//! Both finish by calling `mpmain()`, which loads the IDT, signals
//! `cpu->started = 1`, and never returns (it enters the scheduler).

use core::panic::PanicInfo;
use core::sync::atomic::Ordering;

use arch::ioapic::ioapicinit;
use arch::lapic::{lapicinit, lapicstartap};
use arch::mp::mpinit;
use arch::traps::{idtinit, tvinit};
use arch::vm::{kvmalloc, seginit, switchkvm, syscallinit};
use mm::kalloc::{freerange, kalloc, kinit1, kinit2};
use mm::memlayout::{p2v_ptr, PHYSTOP};
use proc::proch::my_cpu;

const KSTACKSIZE: u64 = 4096;

// ---------------------------------------------------------------------
// External hooks. Everything in this block is something `main.c`
// calls but we haven't ported yet. Replace each `extern` with a real
// `use` once the corresponding module exists.
// ---------------------------------------------------------------------

unsafe extern "C" {
    /// First address past the kernel image. Emitted by `kernel.ld`.
    unsafe static end: u8;

    /// Page-table root used by `entry.S` and `entryother.S` before
    /// kvmalloc() runs. Allocated in assembly.
    unsafe static mut entrypgdir: u8;

    /// `entryother.S` blob — the linker exposes its start and size
    /// symbols when you embed a binary via `objcopy --redefine-sym`.
    unsafe static _binary_entryother_start: u8;
    unsafe static _binary_entryother_size:  u8;

    // Per-CPU table. Defined in `arch::mp` (or `mp.c`). We declare it
    // here with the `proc::proch::Cpu` layout because that's what the
    // kernel reads from; the actual storage in `arch::mp` uses a
    // smaller stub struct that needs to be reconciled before this
    // ever boots.
    unsafe static mut cpus: [proc::proch::Cpu; param::NCPU as usize];
    unsafe static mut ncpu: i32;

    // Subsystems not yet ported. The order matches xv6's main().
    unsafe fn picinit();
    unsafe fn consoleinit();
    unsafe fn uartinit();
    unsafe fn pinit();
    unsafe fn binit();
    unsafe fn fileinit();
    unsafe fn ideinit();
    unsafe fn userinit();
    unsafe fn scheduler() -> !;

    // Memory move primitive. Will move into a `string` crate eventually.
    unsafe fn memmove(dst: *mut u8, src: *const u8, n: u32) -> *mut u8;
}

// =====================================================================
// Bootstrap CPU entry. The C version is `int main(void)` but never
// returns — Rust gets `-> !` to match.
// =====================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn main() -> ! {
    // Phase 1: build the page-list across [end, P2V(4MB)). The
    // address of the linker symbol `end` is the first byte past the
    // loaded kernel image; the range stops at 4 MiB so `mpinit` can
    // still touch BIOS memory below that line.
    let kernel_end = &end as *const u8 as *mut u8;
    kinit1(kernel_end, p2v_ptr::<u8>(4 * 1024 * 1024));

    kvmalloc();        // build the kernel page table
    mpinit();          // discover SMP topology
    lapicinit();       // local APIC on this CPU
    seginit();         // GDT + per-CPU TLS
    picinit();         // mask the legacy 8259 PIC
    ioapicinit();      // I/O APIC
    consoleinit();     // console (CRT + keyboard)
    uartinit();        // serial port
    pinit();           // process table
    tvinit();          // trap vectors / IDT
    binit();           // buffer cache
    fileinit();        // file table
    ideinit();         // disk

    startothers();     // boot the other CPUs

    // Phase 2: extend the allocator across the rest of physical RAM.
    // MUST come after startothers() — the AP startup blob lives in low
    // memory and we mustn't free it until the APs have copied off.
    //
    // xv6-master folds this `freerange` into `kinit2(vstart, vend)`;
    // the kalloc.rs you ported keeps `kinit2()` signature-free, so we
    // do the freerange explicitly and then flip `use_lock`.
    freerange(p2v_ptr::<u8>(4 * 1024 * 1024), p2v_ptr::<u8>(PHYSTOP));
    kinit2();

    syscallinit();     // SYSCALL/SYSRET MSRs
    userinit();        // first user process (init)

    mpmain();          // never returns
}

// =====================================================================
// AP entry. Called from entryother.S once the AP is in long mode.
// =====================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mpenter() -> ! {
    switchkvm();
    seginit();
    lapicinit();
    syscallinit();
    mpmain();
}

// =====================================================================
// Per-CPU finish. Loads the IDT, signals startup, enters the scheduler.
// =====================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mpmain() -> ! {
    // TODO: cprintf("cpu%d: starting %d\n", cpunum(), cpunum());
    idtinit();

    // Atomic store of 1 into mycpu()->started — what the C version
    // does with `xchg`. Using AtomicU32::store with SeqCst gets us the
    // memory barrier we want.
    let cpu = my_cpu();
    cpu.started.store(1, Ordering::SeqCst);

    scheduler();
}

// =====================================================================
// startothers — copy entryother.S into low memory and IPI each
// non-boot CPU into its bring-up sequence.
// =====================================================================

unsafe fn startothers() {
    // entryother.S relocates itself to physical 0x7000.
    let code: *mut u8 = p2v_ptr::<u8>(0x7000);
    let size = &_binary_entryother_size as *const u8 as u32;
    memmove(code, &_binary_entryother_start as *const u8, size);

    let me = my_cpu() as *const proc::proch::Cpu;

    for i in 0..(ncpu as usize) {
        let c: *mut proc::proch::Cpu = &raw mut cpus[i];

        // Skip ourselves — we're already running.
        if c as *const _ == me {
            continue;
        }

        // Hand the AP its kernel stack, entry point, and pgdir at
        // the negative offsets entryother.S reads:
        //   [code-8]  : top of the AP's kernel stack
        //   [code-16] : function to jump to (mpenter)
        //   [code-24] : V2P(pgdir) — entrypgdir while we're still in
        //               low memory
        let stack = kalloc();
        *(code.offset(-8)  as *mut u64) = stack as u64 + KSTACKSIZE;
        *(code.offset(-16) as *mut u64) = mpenter as u64;
        *(code.offset(-24) as *mut u64) =
            (&raw const entrypgdir as u64).wrapping_sub(mm::memlayout::KERNBASE);

        lapicstartap((*c).apicid as u8, code as u32);

        // Wait for the AP to flip its `started` to 1.
        while (*c).started.load(Ordering::SeqCst) == 0 {
            core::hint::spin_loop();
        }
    }
}

// =====================================================================
// Linker still needs `_start` until entry.S is wired up. Once the
// boot path calls `main` directly, this can be removed.
// =====================================================================

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    unsafe { main() }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
