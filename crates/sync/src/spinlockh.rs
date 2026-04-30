use types::{addr_t, uint};
use crate::spinlock;

pub struct spinlock {
    locked: uint,
    name: &'static str,
    cpu: *mut Cpu,
    pcs: [addr_t; 10],
}