#![no_std]

//! Process table, scheduler, and exec (proc.c, exec.c).

pub mod exec;
pub mod proc;
pub mod proch;

// Re-export the names that other crates reach for at the crate root,
// so callers can write `use proc::my_proc;` instead of
// `use proc::proch::my_proc;`. Mirrors the flat namespace of the
// upstream C `proc.h`.
// NOTE: `proch::proc` (the struct) intentionally NOT re-exported here
// because it would collide with the `proc` module declared above
// (which corresponds to `proc.rs`). Consumers that need the struct
// can write `proc::proch::proc` explicitly.
pub use proch::{Context, Cpu, Procstate, cpus, my_cpu, my_proc, ncpu};