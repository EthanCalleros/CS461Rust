//! Port of `trap.c` + `traps.h` — IDT setup and trap dispatcher.

#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]

use core::ptr;

use types::{addr_t, uint};

use crate::mmu::{DPL_USER, KERNEL_CS, PGSIZE};
use crate::registers::{lidt, rcr2, trapframe};

// =====================================================================
// CPU-defined trap numbers.
// =====================================================================

pub const T_DIVIDE:  u32 = 0;
pub const T_DEBUG:   u32 = 1;
pub const T_NMI:     u32 = 2;
pub const T_BRKPT:   u32 = 3;
pub const T_OFLOW:   u32 = 4;
pub const T_BOUND:   u32 = 5;
pub const T_ILLOP:   u32 = 6;
pub const T_DEVICE:  u32 = 7;
pub const T_DBLFLT:  u32 = 8;
// 9 reserved
pub const T_TSS:     u32 = 10;
pub const T_SEGNP:   u32 = 11;
pub const T_STACK:   u32 = 12;
pub const T_GPFLT:   u32 = 13;
pub const T_PGFLT:   u32 = 14;
// 15 reserved
pub const T_FPERR:   u32 = 16;
pub const T_ALIGN:   u32 = 17;
pub const T_MCHK:    u32 = 18;
pub const T_SIMDERR: u32 = 19;

/// Vector base for IRQ delivery (so IRQ N → trap T_IRQ0 + N).
pub const T_IRQ0: u32 = 32;

// IRQ line numbers.
pub const IRQ_TIMER:    u32 = 0;
pub const IRQ_KBD:      u32 = 1;
pub const IRQ_COM1:     u32 = 4;
pub const IRQ_IDE:      u32 = 14;
pub const IRQ_ERROR:    u32 = 19;
pub const IRQ_SPURIOUS: u32 = 31;

// =====================================================================
// IDT state.
// =====================================================================

/// IDT pointer, allocated in `tvinit()`. 256 gates × 16 bytes = 4 KiB.
#[unsafe(no_mangle)]
pub static mut idtt: *mut uint = ptr::null_mut();

/// Tick counter, advanced by the timer interrupt.
#[unsafe(no_mangle)]
pub static mut ticks: uint = 0;

// Vector entry-point table emitted by `vectors.S`.
unsafe extern "C" {
    pub static vectors: [addr_t; 256];
}

// =====================================================================
// Functions provided by other crates that don't exist yet — declared
// here as `extern "C"` placeholders so the trap dispatcher compiles.
// Replace each with a real `use` once the corresponding crate is up.
// =====================================================================

unsafe extern "C" {
    fn kalloc() -> *mut u8;

    // Driver interrupt handlers.
    fn ideintr();
    fn kbdintr();
    fn uartintr();

    // Process / scheduler hooks.
    fn exit_process();   // xv6's `exit()` — renamed because `exit` collides.
    fn yield_proc();     // xv6's `yield()` — `yield` is a Rust keyword.
    fn wakeup(chan: *const core::ffi::c_void);

    // Spinlock hooks.
    fn acquire(lk: *mut core::ffi::c_void);
    fn release(lk: *mut core::ffi::c_void);
}

/// Stand-in for the global tick spinlock. Replace with the real
/// `proc::tickslock` once that crate is up.
#[unsafe(no_mangle)]
pub static mut tickslock: u64 = 0; // placeholder; sync::spinlock::Spinlock<()> in real impl

// =====================================================================
// Build a 64-bit interrupt gate at `idtt[n]`.
//
// Layout: 4 × uint per gate (16 bytes total on x86_64).
//   [n+0] : offset[15:0]  | (KERNEL_CS << 16)
//   [n+1] : offset[31:16] | flags                ; flags = 0x8E00 | (DPL<<13)
//   [n+2] : offset[63:32]
//   [n+3] : reserved (zero)
// =====================================================================

unsafe fn mkgate(idt_ptr: *mut uint, n: u32, kva: addr_t, pl: u32) {
    let n = (n * 4) as usize;
    let addr = kva;
    *idt_ptr.add(n + 0) = (addr & 0xFFFF) as uint | ((KERNEL_CS as uint) << 16);
    *idt_ptr.add(n + 1) = ((addr & 0xFFFF_0000) as uint) | 0x8E00 | ((pl & 3) << 13);
    *idt_ptr.add(n + 2) = (addr >> 32) as uint;
    *idt_ptr.add(n + 3) = 0;
}

/// `void tvinit(void)` — allocate the IDT and populate every gate.
pub unsafe fn tvinit() {
    idtt = kalloc() as *mut uint;
    if idtt.is_null() {
        panic!("tvinit: kalloc failed");
    }
    ptr::write_bytes(idtt as *mut u8, 0, PGSIZE as usize);

    for n in 0u32..256 {
        mkgate(idtt, n, vectors[n as usize], 0);
    }
}

/// `void idtinit(void)` — load the IDT register on this CPU.
pub unsafe fn idtinit() {
    lidt(idtt as *const _, PGSIZE as i32);
}

// =====================================================================
// Main trap dispatcher. Called from trapasm.S after the trapframe is
// built on the kernel stack.
// =====================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn trap(tf: *mut trapframe) {
    let trapno = (*tf).trapno as u32;

    match trapno {
        // Timer.
        n if n == T_IRQ0 + IRQ_TIMER => {
            // Only CPU 0 advances the global tick counter.
            // TODO: gate on `cpunum() == 0` once that's wired up.
            acquire(&raw mut tickslock as *mut _);
            ticks = ticks.wrapping_add(1);
            wakeup(&raw const ticks as *const _);
            release(&raw mut tickslock as *mut _);
            crate::lapic::lapiceoi();
        }

        // IDE primary.
        n if n == T_IRQ0 + IRQ_IDE => {
            ideintr();
            crate::lapic::lapiceoi();
        }

        // IDE secondary — Bochs spurious.
        n if n == T_IRQ0 + IRQ_IDE + 1 => {}

        // Keyboard.
        n if n == T_IRQ0 + IRQ_KBD => {
            kbdintr();
            crate::lapic::lapiceoi();
        }

        // Serial.
        n if n == T_IRQ0 + IRQ_COM1 => {
            uartintr();
            crate::lapic::lapiceoi();
        }

        // Spurious / IRQ7.
        n if n == T_IRQ0 + 7 || n == T_IRQ0 + IRQ_SPURIOUS => {
            crate::lapic::lapiceoi();
        }

        // Default: kernel-mode → panic, user-mode → kill the process.
        _ => {
            let cs = (*tf).cs;
            if cs & 3 == 0 {
                // Kernel fault — shouldn't happen.
                let _cr2 = rcr2();
                panic!("trap");
            }
            // User-mode fault: kill the current process.
            // TODO: set proc->killed = 1 once `proc` is up.
        }
    }

    // TODO: forced-exit / yield / re-check-killed logic from C trap()
    // once `proc` is wired up. The structure is:
    //
    //   if proc && proc->killed && (tf->cs & 3) == DPL_USER { exit(); }
    //   if proc && proc->state == RUNNING && trapno == T_IRQ0+IRQ_TIMER { yield(); }
    //   if proc && proc->killed && (tf->cs & 3) == DPL_USER { exit(); }
    let _ = DPL_USER;
    let _ = exit_process;
    let _ = yield_proc;
}
