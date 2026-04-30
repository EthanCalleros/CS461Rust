//! File-system syscall handlers (port of `sysfile.c`).
//!
//! These bodies are scaffolding — they call into hooks that live in
//! the still-unimplemented `fs` and `proc` crates. The signatures and
//! calling conventions are correct so the syscall dispatch table in
//! `lib.rs` can reference them; once the underlying machinery is up,
//! the bodies themselves will start doing real work.

#![allow(non_camel_case_types)]
#![allow(unsafe_op_in_unsafe_fn)]

use core::ptr;

use crate::{argint, argaddr, argptr, argstr, fetchstr};
use param::{MAXARG, NOFILE};
use proc::my_proc;
use types::{addr_t, stat::stat};

// =====================================================================
// External hooks — provided by the `fs` crate once it exists. Each
// `unsafe extern "C"` block must be marked `unsafe` (edition 2024)
// and each function inside must be `unsafe fn`.
// =====================================================================

unsafe extern "C" {
    unsafe fn filealloc() -> *mut file;
    unsafe fn fileclose(f: *mut file);
    unsafe fn filedup(f: *mut file);
    unsafe fn filestat(f: *mut file, st: *mut stat) -> i32;
    unsafe fn fileread(f: *mut file, addr: *mut u8, n: i32) -> i32;
    unsafe fn filewrite(f: *mut file, addr: *mut u8, n: i32) -> i32;
    unsafe fn pipealloc(rf: *mut *mut file, wf: *mut *mut file) -> i32;
    unsafe fn exec(path: *const u8, argv: *const *const u8) -> i32;
}

/// Opaque file handle — real definition lives in `fs::file`.
pub enum file {}
/// Opaque inode — real definition lives in `fs::fs`.
pub enum inode {}

// =====================================================================
// Local helper: fetch an `addr_t` from process memory.
//
// xv6 names this `fetchaddr` and provides it in `syscall.c`. Until we
// add it to `lib.rs`, define a stub here that just dereferences the
// pointer (assumes the kernel direct-map covers user addresses,
// which is the case during early porting).
// =====================================================================

#[inline(always)]
unsafe fn fetchaddr(addr: addr_t, ip: *mut addr_t) -> i32 {
    let p = my_proc();
    if addr >= p.sz || addr + (core::mem::size_of::<addr_t>() as addr_t) > p.sz {
        return -1;
    }
    *ip = *(addr as *const addr_t);
    0
}

// =====================================================================
// Helpers ported from sysfile.c.
// =====================================================================

/// Fetch the nth syscall arg as a file descriptor and return the
/// underlying `*mut file`.
unsafe fn argfd(n: i32, pfd: *mut i32, pf: *mut *mut file) -> i32 {
    let mut fd: i32 = 0;
    if argint(n, &mut fd) < 0 {
        return -1;
    }

    let p = my_proc();
    let ofile = p.ofile.as_ptr() as *mut *mut file;

    if fd < 0 || fd >= NOFILE as i32 || (*ofile.add(fd as usize)).is_null() {
        return -1;
    }

    if !pfd.is_null() {
        *pfd = fd;
    }
    if !pf.is_null() {
        *pf = *ofile.add(fd as usize);
    }
    0
}

/// Allocate a slot in the current process's open-file table.
unsafe fn fdalloc(f: *mut file) -> i32 {
    let p = my_proc();
    let ofile = p.ofile.as_ptr() as *mut *mut file;

    for fd in 0..(NOFILE as usize) {
        if (*ofile.add(fd)).is_null() {
            *ofile.add(fd) = f;
            return fd as i32;
        }
    }
    -1
}

// =====================================================================
// Syscall handlers. Every one returns `usize` (matches the
// `SyscallFn` type alias in `lib.rs`); `!0` is xv6's `-1` sentinel.
// =====================================================================

pub unsafe fn sys_dup() -> usize {
    let mut f: *mut file = ptr::null_mut();
    if argfd(0, ptr::null_mut(), &mut f) < 0 {
        return !0;
    }

    let fd = fdalloc(f);
    if fd < 0 {
        return !0;
    }

    filedup(f);
    fd as usize
}

pub unsafe fn sys_read() -> usize {
    let mut f: *mut file = ptr::null_mut();
    let mut n: i32 = 0;
    let mut p: *mut u8 = ptr::null_mut();

    if argfd(0, ptr::null_mut(), &mut f) < 0
        || argint(2, &mut n) < 0
        || argptr(1, &mut p, n) < 0
    {
        return !0;
    }
    fileread(f, p, n) as usize
}

pub unsafe fn sys_write() -> usize {
    let mut f: *mut file = ptr::null_mut();
    let mut n: i32 = 0;
    let mut p: *mut u8 = ptr::null_mut();

    if argfd(0, ptr::null_mut(), &mut f) < 0
        || argint(2, &mut n) < 0
        || argptr(1, &mut p, n) < 0
    {
        return !0;
    }
    filewrite(f, p, n) as usize
}

pub unsafe fn sys_close() -> usize {
    let mut fd: i32 = 0;
    let mut f: *mut file = ptr::null_mut();

    if argfd(0, &mut fd, &mut f) < 0 {
        return !0;
    }

    let p = my_proc();
    let ofile = p.ofile.as_ptr() as *mut *mut file;
    *ofile.add(fd as usize) = ptr::null_mut();

    fileclose(f);
    0
}

pub unsafe fn sys_fstat() -> usize {
    let mut f: *mut file = ptr::null_mut();
    let mut st_ptr: *mut u8 = ptr::null_mut();

    if argfd(0, ptr::null_mut(), &mut f) < 0
        || argptr(1, &mut st_ptr, core::mem::size_of::<stat>() as i32) < 0
    {
        return !0;
    }
    filestat(f, st_ptr as *mut stat) as usize
}

pub unsafe fn sys_exec() -> usize {
    let mut path: *mut u8 = ptr::null_mut();
    let mut uargv: addr_t = 0;
    let mut argv: [*const u8; MAXARG as usize] = [ptr::null(); MAXARG as usize];

    if argstr(0, &mut path) < 0 || argaddr(1, &mut uargv) < 0 {
        return !0;
    }

    for i in 0..(MAXARG as usize) {
        let mut uarg: addr_t = 0;
        let off = (core::mem::size_of::<addr_t>() * i) as addr_t;
        if fetchaddr(uargv + off, &mut uarg) < 0 {
            return !0;
        }
        if uarg == 0 {
            argv[i] = ptr::null();
            break;
        }
        let mut s: *mut u8 = ptr::null_mut();
        if fetchstr(uarg, &mut s) < 0 {
            return !0;
        }
        argv[i] = s as *const u8;
    }

    exec(path, argv.as_ptr()) as usize
}

pub unsafe fn sys_pipe() -> usize {
    let mut fd_array: *mut u8 = ptr::null_mut();
    let mut rf: *mut file = ptr::null_mut();
    let mut wf: *mut file = ptr::null_mut();

    if argptr(0, &mut fd_array, 2 * core::mem::size_of::<i32>() as i32) < 0 {
        return !0;
    }
    let fds = fd_array as *mut i32;

    if pipealloc(&mut rf, &mut wf) < 0 {
        return !0;
    }

    let fd0 = fdalloc(rf);
    if fd0 < 0 {
        fileclose(rf);
        fileclose(wf);
        return !0;
    }

    let fd1 = fdalloc(wf);
    if fd1 < 0 {
        let p = my_proc();
        let ofile = p.ofile.as_ptr() as *mut *mut file;
        *ofile.add(fd0 as usize) = ptr::null_mut();
        fileclose(rf);
        fileclose(wf);
        return !0;
    }

    *fds.add(0) = fd0;
    *fds.add(1) = fd1;
    0
}
