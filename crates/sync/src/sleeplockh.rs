use crate::spinlockh::spinlock;
use types::uint;

pub struct sleeplock {
    pub locked: uint,
    pub lk: spinlock,
    pub name: *const u8,
    pub pid: i32,
}