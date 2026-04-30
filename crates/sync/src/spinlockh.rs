//! Port of the `struct spinlock` definition from `spinlock.h`.

#![allow(non_camel_case_types)]

use proc::Cpu;
use types::{addr_t, uint};

pub struct spinlock {
    pub locked: uint,
    pub name:   &'static str,
    pub cpu:    *mut Cpu,
    pub pcs:    [addr_t; 10],
}
