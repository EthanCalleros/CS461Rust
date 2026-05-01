//! Port of the `struct spinlock` definition from `spinlock.h`.

#![allow(non_camel_case_types)]

use proc::Cpu;
use types::{addr_t, uint};

/// C-compatible spinlock. Layout must match `proc::proc::spinlock` exactly.
#[repr(C)]
pub struct spinlock {
    pub locked: uint,
    pub name:   *const u8,
    pub cpu:    *mut Cpu,
    pub pcs:    [addr_t; 10],
}
