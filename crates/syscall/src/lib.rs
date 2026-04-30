#![no_std]

//! Syscall dispatch (syscall.c) and handlers (sysfile.c, sysproc.c).

#![no_std]

pub mod sysfile;
pub mod sysproc;

use proc::my_proc;
use types::{addr_t, PGSIZE};
use core::ptr;

// The Table type: Array of unsafe functions returning addr_t (usize)
type SyscallFn = unsafe fn() -> addr_t;

/// Fetch the nth 64-bit system call argument.
pub unsafe fn fetcharg(n: i32) -> addr_t {
    let p = my_proc();
    let tf = &*p.tf;
    match n {
        0 => tf.rdi,
        1 => tf.rsi,
        2 => tf.rdx,
        3 => tf.r10,
        4 => tf.r8,
        5 => tf.r9,
        _ => panic!("fetcharg: invalid index"),
    }
}

pub unsafe fn argint(n: i32, ip: &mut i32) -> i32 {
    *ip = fetcharg(n) as i32;
    0
}

pub unsafe fn argaddr(n: i32, ip: &mut addr_t) -> i32 {
    *ip = fetcharg(n);
    0
}

/// Fetch the nth word-sized syscall argument as a pointer to a block of memory.
/// Checks that the pointer lies within the process address space.
pub unsafe fn argptr(n: i32, pp: &mut *mut u8, size: i32) -> i32 {
    let mut i: addr_t = 0;
    if argaddr(n, &mut i) < 0 {
        return -1;
    }
    let p = my_proc();
    if size < 0 || i >= p.sz || i + (size as addr_t) > p.sz {
        return -1;
    }
    *pp = i as *mut u8;
    0
}

/// Fetch the nth syscall argument as a string pointer.
pub unsafe fn argstr(n: i32, pp: &mut *mut u8) -> i32 {
    let mut addr: addr_t = 0;
    if argaddr(n, &mut addr) < 0 {
        return -1;
    }
    fetchstr(addr, pp)
}

/// Helper to check string bounds and find null terminator.
pub unsafe fn fetchstr(addr: addr_t, pp: &mut *mut u8) -> i32 {
    let p = my_proc();
    if addr < (PGSIZE as addr_t) || addr >= p.sz {
        return -1;
    }
    *pp = addr as *mut u8;
    let max = p.sz - addr;
    
    for i in 0..max {
        if *((*pp).add(i as usize)) == 0 {
            return i as i32;
        }
    }
    -1
}

#[no_mangle]
pub unsafe extern "C" fn syscall() {
    let p = my_proc();
    let num = (*p.tf).rax as usize;

    if num > 0 && num < SYSCALLS.len() && SYSCALLS[num].is_some() {
        let func = SYSCALLS[num].unwrap();
        (*p.tf).rax = func();
    } else {
        // You'll eventually want a println! macro for your console driver here
        (*p.tf).rax = !0; // -1 in 64-bit
    }
}

// Map IDs to the functions in sysproc.rs and sysfile.rs
static SYSCALLS: [Option<SyscallFn>; 26] = {
    let mut table: [Option<SyscallFn>; 26] = [None; 26];
    
    // Process syscalls
    table[1] = Some(sysproc::sys_fork);
    table[2] = Some(sysproc::sys_exit);
    table[3] = Some(sysproc::sys_wait);
    table[6] = Some(sysproc::sys_kill);
    table[11] = Some(sysproc::sys_getpid);
    table[12] = Some(sysproc::sys_sbrk);
    table[13] = Some(sysproc::sys_sleep);
    table[14] = Some(sysproc::sys_uptime);
    
    // File syscalls
    table[4] = Some(sysfile::sys_pipe);
    table[5] = Some(sysfile::sys_read);
    table[7] = Some(sysfile::sys_exec);
    table[8] = Some(sysfile::sys_fstat);
    table[9] = Some(sysfile::sys_chdir);
    table[10] = Some(sysfile::sys_dup);
    table[15] = Some(sysfile::sys_open);
    table[16] = Some(sysfile::sys_write);
    table[17] = Some(sysfile::sys_mknod);
    table[18] = Some(sysfile::sys_unlink);
    table[19] = Some(sysfile::sys_link);
    table[20] = Some(sysfile::sys_mkdir);
    table[21] = Some(sysfile::sys_close);

    table
};
