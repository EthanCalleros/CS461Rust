//! Port of `proc.h` — process table types and per-CPU TLS accessors.
//!
//! Field order on `cpu` and `proc` matters: assembly trampolines
//! (`swtch.S`, `trapasm.S`) reach into these structs by offset, and
//! the per-CPU `cpu` / `proc` pointers live at `%fs:(-16)` / `%fs:(-8)`
//! per `vm.c`'s `seginit()`. Do not reorder fields without matching
//! changes in the assembly.

#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]

use core::arch::asm;
use core::ffi::c_void;
use core::sync::atomic::AtomicU32;

use arch::registers::trapframe;
use param::{NCPU, NOFILE};
use types::{addr_t, pde_t, uchar};

// ---------------------------------------------------------------------
// Forward declarations for types that live in other crates we don't
// depend on yet. Once `proc` gains a `fs` dependency, replace these
// opaque shells with `use fs::file::file; use fs::fs::inode;`.
// ---------------------------------------------------------------------

#[repr(C)]
pub struct file {
    _opaque: [u8; 0],
}

#[repr(C)]
pub struct inode {
    _opaque: [u8; 0],
}

// ---------------------------------------------------------------------
// Context — callee-saved registers swapped by `swtch.S`.
// Field order MUST match the push/pop order in `swtch.S`.
// ---------------------------------------------------------------------

#[repr(C)]
pub struct Context {
    pub r15: addr_t,
    pub r14: addr_t,
    pub r13: addr_t,
    pub r12: addr_t,
    pub rbx: addr_t,
    pub rbp: addr_t,
    pub rip: addr_t,
}

// ---------------------------------------------------------------------
// Per-CPU state.
// ---------------------------------------------------------------------

#[repr(C)]
pub struct Cpu {
    pub id:        uchar,
    pub apicid:    uchar,
    pub scheduler: *mut Context,
    pub started:   AtomicU32,
    pub ncli:      i32,
    pub intena:    i32,
    pub local:     *mut c_void,
}

// ---------------------------------------------------------------------
// Process states (matches `enum procstate` in the C header).
// ---------------------------------------------------------------------

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Procstate {
    UNUSED,
    EMBRYO,
    SLEEPING,
    RUNNABLE,
    RUNNING,
    ZOMBIE,
}

// ---------------------------------------------------------------------
// `struct proc` — per-process bookkeeping.
//
// `kstack` is a raw byte pointer (C: `char *kstack`), not a Rust
// `char` — Rust's `char` is a 4-byte Unicode scalar.
//
// `ofile` is a fixed-size array of file pointers, mirroring
// `struct file *ofile[NOFILE]` in C.
//
// `name` is a 16-byte buffer, not Unicode chars.
// ---------------------------------------------------------------------

#[repr(C)]
pub struct proc {
    pub sz:      addr_t,
    pub pgdir:   *mut pde_t,
    pub kstack:  *mut u8,
    pub state:   Procstate,
    pub pid:     i32,
    pub parent:  *mut proc,
    pub tf:      *mut trapframe,
    pub context: *mut Context,
    pub chan:    *mut c_void,
    pub killed:  i32,
    pub ofile:   [*mut file; NOFILE as usize],
    pub cwd:     *mut inode,
    pub name:    [u8; 16],
}

// ---------------------------------------------------------------------
// `cpus[]` and `ncpu` are populated by `mp.rs`. Re-exposed here so
// process code can read them; the storage itself lives in `arch::mp`.
// ---------------------------------------------------------------------

unsafe extern "C" {
    pub static mut cpus: [Cpu; NCPU as usize];
    pub static mut ncpu: i32;
}

// ---------------------------------------------------------------------
// Per-CPU TLS accessors.
//
// xv6-64 stashes the current `struct cpu *` and `struct proc *` at
// fs:[-16] and fs:[-8] respectively. The FS base is set per-CPU in
// `vm::seginit()` via `wrmsr(MSR_FS_BASE = 0xC0000100, ...)`.
//
// The `mov` here is Intel syntax (Rust's asm! default). `qword ptr`
// pins the operand size; without it LLVM picks based on register
// width, which is fine for `out(reg)` but spelling it out reads
// closer to objdump output.
// ---------------------------------------------------------------------

#[inline(always)]
pub unsafe fn my_cpu() -> &'static mut Cpu {
    let ptr: *mut Cpu;
    asm!(
        "mov {0}, qword ptr fs:[-16]",
        out(reg) ptr,
        options(nostack, preserves_flags),
    );
    &mut *ptr
}

#[inline(always)]
pub unsafe fn my_proc() -> &'static mut proc {
    let ptr: *mut proc;
    asm!(
        "mov {0}, qword ptr fs:[-8]",
        out(reg) ptr,
        options(nostack, preserves_flags),
    );
    &mut *ptr
}
