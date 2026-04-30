//! Port of `x86.h` — thin wrappers around special x86_64 instructions
//! that Rust can't express in safe code. Each function is `unsafe`
//! because every one of them either talks to hardware, modifies a
//! control register, or assumes a contract the type system can't
//! verify.

#![allow(non_camel_case_types)]

use core::arch::asm;

use crate::mmu::segdesc;
use types::{addr_t, uchar, uint, ushort};

// =====================================================================
// Programmed I/O — `in` / `out` and their string variants.
// =====================================================================

/// Read one byte from I/O port `port`.
#[inline(always)]
pub unsafe fn inb(port: ushort) -> uchar {
    let data: u8;
    asm!(
        "in al, dx",
        out("al") data,
        in("dx") port,
        options(nomem, nostack, preserves_flags),
    );
    data
}

/// `rep insl` — read `cnt` 32-bit words from `port` into `addr`.
#[inline(always)]
pub unsafe fn insl(port: uint, addr: *mut u32, cnt: usize) {
    asm!(
        "cld",
        "rep insd",
        in("dx")    port as u16,
        inout("rdi") addr => _,
        inout("rcx") cnt  => _,
        options(nostack, preserves_flags),
    );
}

/// Write one byte to I/O port.
#[inline(always)]
pub unsafe fn outb(port: ushort, data: uchar) {
    asm!(
        "out dx, al",
        in("dx") port,
        in("al") data,
        options(nomem, nostack, preserves_flags),
    );
}

/// Write one 16-bit word to I/O port.
#[inline(always)]
pub unsafe fn outw(port: ushort, data: ushort) {
    asm!(
        "out dx, ax",
        in("dx") port,
        in("ax") data,
        options(nomem, nostack, preserves_flags),
    );
}

/// `rep outsl` — write `cnt` 32-bit words from `addr` to `port`.
#[inline(always)]
pub unsafe fn outsl(port: uint, addr: *const u32, cnt: usize) {
    asm!(
        "cld",
        "rep outsd",
        in("dx")     port as u16,
        inout("rsi") addr => _,
        inout("rcx") cnt  => _,
        options(nostack, preserves_flags),
    );
}

// =====================================================================
// String stores — `rep stosb` / `rep stosd`.
// =====================================================================

/// `rep stosb` — fill `cnt` bytes at `addr` with the low byte of `data`.
#[inline(always)]
pub unsafe fn stosb(addr: *mut u8, data: u32, cnt: usize) {
    asm!(
        "cld",
        "rep stosb",
        inout("rdi") addr => _,
        inout("rcx") cnt  => _,
        in("al")     data as u8,
        options(nostack, preserves_flags),
    );
}

/// `rep stosd` — fill `cnt` 32-bit words at `addr` with `data`.
#[inline(always)]
pub unsafe fn stosl(addr: *mut u32, data: u32, cnt: usize) {
    asm!(
        "cld",
        "rep stosd",
        inout("rdi") addr => _,
        inout("rcx") cnt  => _,
        in("eax")    data,
        options(nostack, preserves_flags),
    );
}

// =====================================================================
// GDT / IDT loads.
//
// On x86_64 the LGDT/LIDT operand is a 10-byte pseudo-descriptor:
//   bytes 0..2  : limit (size - 1)
//   bytes 2..10 : 64-bit base address
// Built here as five contiguous u16's for layout simplicity.
// =====================================================================

#[inline(always)]
pub unsafe fn lgdt(p: *const segdesc, size: i32) {
    let addr = p as addr_t;
    let pd: [u16; 5] = [
        (size - 1) as u16,
        addr as u16,
        (addr >> 16) as u16,
        (addr >> 32) as u16,
        (addr >> 48) as u16,
    ];
    asm!(
        "lgdt [{}]",
        in(reg) &pd,
        options(readonly, nostack, preserves_flags),
    );
}

/// Placeholder gate-descriptor type — replace with a proper definition
/// once `arch::traps` ports `struct gatedesc`.
#[repr(C, packed)]
#[derive(Clone, Copy, Default)]
pub struct gatedesc {
    pub raw: [u64; 2], // 16 bytes per IDT entry on x86_64
}

#[inline(always)]
pub unsafe fn lidt(p: *const gatedesc, size: i32) {
    let addr = p as addr_t;
    let pd: [u16; 5] = [
        (size - 1) as u16,
        addr as u16,
        (addr >> 16) as u16,
        (addr >> 32) as u16,
        (addr >> 48) as u16,
    ];
    asm!(
        "lidt [{}]",
        in(reg) &pd,
        options(readonly, nostack, preserves_flags),
    );
}

/// Load Task Register with the given segment selector.
#[inline(always)]
pub unsafe fn ltr(sel: ushort) {
    asm!(
        "ltr {0:x}",
        in(reg) sel,
        options(nomem, nostack, preserves_flags),
    );
}

// =====================================================================
// RFLAGS / interrupt control / halt.
// =====================================================================

/// Read the RFLAGS register.
#[inline(always)]
pub unsafe fn readeflags() -> addr_t {
    let eflags: u64;
    asm!(
        "pushfq",
        "pop {}",
        out(reg) eflags,
        options(nomem),
    );
    eflags
}

/// Disable maskable interrupts.
#[inline(always)]
pub unsafe fn cli() {
    asm!("cli", options(nomem, nostack));
}

/// Enable maskable interrupts.
#[inline(always)]
pub unsafe fn sti() {
    asm!("sti", options(nomem, nostack));
}

/// Halt the CPU until the next interrupt.
#[inline(always)]
pub unsafe fn hlt() {
    asm!("hlt", options(nomem, nostack, preserves_flags));
}

// =====================================================================
// Atomics & control registers.
// =====================================================================

/// Atomic 32-bit exchange — used by the spinlock acquire path.
///
/// C: `static inline uint xchg(volatile uint *addr, addr_t newval)`
#[inline(always)]
pub unsafe fn xchg(addr: *mut uint, newval: uint) -> uint {
    let result: u32;
    asm!(
        "lock xchg [{addr}], {val:e}",
        addr = in(reg) addr,
        val  = inout(reg) newval => result,
        options(nostack, preserves_flags),
    );
    result
}

/// Read the CR2 register (faulting linear address from a page fault).
#[inline(always)]
pub unsafe fn rcr2() -> addr_t {
    let val: u64;
    asm!(
        "mov {}, cr2",
        out(reg) val,
        options(nomem, nostack, preserves_flags),
    );
    val
}

/// Load the CR3 register (page-table-base pointer + flags).
#[inline(always)]
pub unsafe fn lcr3(val: addr_t) {
    asm!(
        "mov cr3, {}",
        in(reg) val,
        options(nostack, preserves_flags),
    );
}

// =====================================================================
// Trap frame — layout the hardware + trapasm.S build on the kernel
// stack before calling `trap()`. Field order MUST match the push order
// in trapasm.S; if you reorder fields here, also update the assembly.
// =====================================================================

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct trapframe {
    // Caller-saved + callee-saved general-purpose registers, in the
    // order trapasm.S pushes them.
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rbp: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub r8:  u64,
    pub r9:  u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,

    /// Trap vector number (0-255).
    pub trapno: u64,
    /// Hardware-pushed error code (0 if the CPU didn't push one).
    pub err: u64,

    // Hardware-pushed iret frame.
    pub rip:    u64,
    pub cs:     u64,
    pub rflags: u64,
    pub rsp:    u64,
    pub ss:     u64,
}
