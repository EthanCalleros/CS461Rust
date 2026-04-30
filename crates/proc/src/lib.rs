#![no_std]

//! Process table, scheduler, and exec (proc.c, exec.c).

pub mod exec;
pub mod proc;
pub mod proch;

// Re-export the names that other crates reach for at the crate root,
// so callers can write `use proc::my_proc;` instead of
// `use proc::proch::my_proc;`. Mirrors the flat namespace of the
// upstream C `proc.h`.
// Re-export only the *types* and *accessor functions* — the
// `static mut cpus[]` / `static mut ncpu` globals stay reachable as
// `proc::proch::cpus` and `proc::proch::ncpu`, because re-exporting
// `unsafe extern "C"` statics through `pub use` runs into edition
// 2024's `unsafe static` requirement on extern items.
//
// `proch::proc` (the struct) is intentionally NOT re-exported either,
// because it would collide with the `proc` module declared above
// (corresponding to `proc.rs`). Consumers that need the struct can
// write `proc::proch::proc` explicitly.
pub use proch::{Context, Cpu, Procstate, my_cpu, my_proc};
