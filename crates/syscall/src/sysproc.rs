//! Process-control syscall handlers (port of `sysproc.c`).
//!
//! Each handler is the kernel's view of a single syscall — pulls
//! arguments off the trapframe, calls into proc/scheduler primitives
//! (declared `extern "C"` here as placeholders until the proc crate
//! provides them), and returns a `usize` for the dispatcher to write
//! back into RAX.

#![allow(unsafe_op_in_unsafe_fn)]

use core::ptr;

use crate::{argint, argaddr, fetcharg};
use proc::my_proc;

// =====================================================================
// External proc/scheduler hooks. Each `extern "C"` block needs to be
// `unsafe extern "C"` in edition 2024, and each item inside needs its
// own `unsafe` qualifier.
//
// These are declared per-handler rather than in a single top-level
// block to keep the C ↔ Rust line-by-line mapping. Promote them to a
// shared `extern "C"` block at the top of the file once the calling
// conventions stabilise.
// =====================================================================

pub unsafe fn sys_fork() -> usize {
    unsafe extern "C" {
        unsafe fn fork() -> i32;
    }
    fork() as usize
}

pub unsafe fn sys_exit() -> usize {
    unsafe extern "C" {
        unsafe fn exit() -> !;
    }
    exit();
}

pub unsafe fn sys_wait() -> usize {
    unsafe extern "C" {
        unsafe fn wait() -> i32;
    }
    wait() as usize
}

pub unsafe fn sys_kill() -> usize {
    let mut pid: i32 = 0;
    if argint(0, &mut pid) < 0 {
        return !0;
    }
    unsafe extern "C" {
        unsafe fn kill(pid: i32) -> i32;
    }
    kill(pid) as usize
}

pub unsafe fn sys_getpid() -> usize {
    let p = my_proc();
    p.pid as usize
}

pub unsafe fn sys_sbrk() -> usize {
    let mut n: i32 = 0;
    if argint(0, &mut n) < 0 {
        return !0;
    }

    let p = my_proc();
    let addr = p.sz;

    unsafe extern "C" {
        unsafe fn growproc(n: i32) -> i32;
    }
    if growproc(n) < 0 {
        return !0;
    }

    addr as usize
}

pub unsafe fn sys_sleep() -> usize {
    let mut n: i32 = 0;
    if argint(0, &mut n) < 0 {
        return !0;
    }

    unsafe extern "C" {
        unsafe fn sleep(chan: *mut core::ffi::c_void, lock: *mut core::ffi::c_void);
        unsafe static mut ticks: u32;
    }

    // Simplified — real xv6 acquires `tickslock` and sleeps until
    // `ticks - start >= n`. Replace with the full version once the
    // tick/scheduler glue is in.
    sleep(&raw mut ticks as *mut _, ptr::null_mut());
    let _ = n;
    0
}

pub unsafe fn sys_uptime() -> usize {
    unsafe extern "C" {
        unsafe static mut ticks: u32;
    }
    ticks as usize
}

// Suppress unused-import warning from the dispatcher passing args we
// don't yet consume in every handler.
#[allow(dead_code)]
fn _unused_imports_keepalive() {
    let _ = (argaddr, fetcharg);
}
