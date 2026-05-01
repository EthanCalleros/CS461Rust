#![no_std]

use core::arch::global_asm;

// All xv6 assembly is GAS / AT&T syntax. Pass `options(att_syntax)`
// on every `global_asm!` invocation rather than emitting an
// `.att_syntax` directive inside each `.S` file (which trips rustc's
// `bad_asm_style` lint).
//
// IMPORTANT: Only include assembly that belongs in the kernel binary.
// These are separate binaries and must NOT be included here:
//   - bootasm.S   → belongs in the bootblock crate (separate 512-byte binary at 0x7c00)
//   - initcode.S  → assembled separately, embedded as a byte array in the kernel
//   - entryother.S → assembled separately, copied to 0x7000 at runtime for AP boot
//   - usys.S      → user-space syscall stubs, linked into user programs only
//
// Kernel-side assembly:
global_asm!(include_str!("asm/entry.S"),       options(att_syntax));
global_asm!(include_str!("asm/swtch.S"),       options(att_syntax));
global_asm!(include_str!("asm/trapasm.S"),     options(att_syntax));
global_asm!(include_str!("asm/vectors.S"),     options(att_syntax));

pub mod elf;
pub mod ioapic;
pub mod lapic;
pub mod mmu;
pub mod mp;
pub mod registers;
pub mod traps;
pub mod vm;

extern "C" {
    /// Save the callee-saved registers in `*old`, then load the
    /// callee-saved registers from `new`. Defined in `asm/swtch.S`.
    pub fn swtch(old: *mut *mut Context, new: *mut Context);
}

#[repr(C)]
#[derive(Debug, Default)]
pub struct Context {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub rip: u64,
}

pub use registers::*;