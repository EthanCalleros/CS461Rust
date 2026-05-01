#![no_std]
#![no_main]
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(static_mut_refs)]
#![allow(dead_code)]
#![allow(unused_imports)]

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

// Pull in glue module — provides #[no_mangle] shims for extern "C" symbols.
mod glue;

// Force the linker to include all crate code (needed for #[no_mangle] exports).
extern crate fs;
extern crate drivers;
extern crate console;
extern crate syscall;

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
}

// =====================================================================
// Bootstrap CPU entry. The C version is `int main(void)` but never
// returns — Rust gets `-> !` to match.
// =====================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn main() -> ! {
    // Debug: write stage markers to UART (0x3F8) so we can see how
    // far boot gets before a crash. QEMU echoes these even without
    // UART initialisation.
    dbg_uart(b'A'); // A = entered main

    // Phase 1: build the page-list across [end, P2V(4MB)). The
    // address of the linker symbol `end` is the first byte past the
    // loaded kernel image; the range stops at 4 MiB so `mpinit` can
    // still touch BIOS memory below that line.
    let kernel_end = &end as *const u8 as *mut u8;
    kinit1(kernel_end, p2v_ptr::<u8>(4 * 1024 * 1024));
    dbg_uart(b'B'); // B = kinit1 done

    kvmalloc();        // build the kernel page table
    dbg_uart(b'C'); // C = kvmalloc done

    mpinit();          // discover SMP topology
    dbg_uart(b'D'); // D = mpinit done

    lapicinit();       // local APIC on this CPU
    dbg_uart(b'E'); // E = lapicinit done

    seginit();         // GDT + per-CPU TLS
    dbg_uart(b'F'); // F = seginit done

    glue::picinit();   // mask the legacy 8259 PIC
    ioapicinit();      // I/O APIC
    glue::consoleinit(); // console (CRT + keyboard)
    glue::uartinit();  // serial port
    dbg_uart(b'G'); // G = pic/ioapic/console/uart done

    glue::pinit();     // process table
    tvinit();          // trap vectors / IDT
    dbg_uart(b'H'); // H = pinit/tvinit done

    glue::binit();     // buffer cache
    glue::fileinit();  // file table
    glue::ideinit();   // disk
    dbg_uart(b'I'); // I = binit/fileinit/ideinit done

    startothers();     // boot the other CPUs
    dbg_uart(b'J'); // J = startothers done

    // Phase 2: extend the allocator across the rest of physical RAM.
    // MUST come after startothers() — the AP startup blob lives in low
    // memory and we mustn't free it until the APs have copied off.
    freerange(p2v_ptr::<u8>(4 * 1024 * 1024), p2v_ptr::<u8>(PHYSTOP));
    kinit2();
    dbg_uart(b'K'); // K = kinit2 done

    syscallinit();     // SYSCALL/SYSRET MSRs
    glue::userinit();  // first user process (init)
    dbg_uart(b'L'); // L = userinit done

    mpmain();          // never returns
}

/// Write a single byte to COM1 (0x3F8) for debug tracing.
/// Works even before uartinit() on QEMU.
#[inline(always)]
unsafe fn dbg_uart(c: u8) {
    core::arch::asm!(
        "out dx, al",
        in("dx") 0x3F8u16,
        in("al") c,
        options(nomem, nostack, preserves_flags),
    );
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

    glue::scheduler();
}

// =====================================================================
// startothers — copy entryother.S into low memory and IPI each
// non-boot CPU into its bring-up sequence.
// =====================================================================

unsafe fn startothers() {
    // entryother.S relocates itself to physical 0x7000.
    let code: *mut u8 = p2v_ptr::<u8>(0x7000);
    let size = &_binary_entryother_size as *const u8 as u32;
    glue::memmove(code, &_binary_entryother_start as *const u8, size);

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
        //   [code-24] : V2P(pgdir) — the bootstrap PML4 at phys 0x1000
        let stack = mm::kalloc::kalloc();
        *(code.offset(-8)  as *mut u64) = stack as u64 + KSTACKSIZE;
        *(code.offset(-16) as *mut u64) = mpenter as u64;
        // entry.S creates bootstrap page tables at physical 0x1000
        *(code.offset(-24) as *mut u64) = 0x1000;

        lapicstartap((*c).apicid as u8, code as u32);

        // Wait for the AP to flip its `started` to 1.
        while (*c).started.load(Ordering::SeqCst) == 0 {
            core::hint::spin_loop();
        }
    }
}

// NOTE: `_start` is now provided by entry.S (via global_asm! in the
// arch crate). entry.S jumps to `main` after setting up long mode and
// the initial stack. No Rust `_start` shim is needed.

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // Write "PANIC\n" to UART so we see it even without console init.
    unsafe {
        for &b in b"PANIC\n" {
            core::arch::asm!(
                "out dx, al",
                in("dx") 0x3F8u16,
                in("al") b,
                options(nomem, nostack, preserves_flags),
            );
        }
    }
    loop {
        core::hint::spin_loop();
    }
}
