use crate::lib::{argint, argaddr, fetcharg};
use proc::{my_proc, Procstate};
use core::ptr;

// --- System Call Implementations ---

pub unsafe fn sys_fork() -> usize {
    // We'll call the actual fork implementation from the proc crate
    // (Assuming proc::fork() exists or will be implemented)
    extern "C" { fn fork() -> i32; }
    fork() as usize
}

pub unsafe fn sys_exit() -> usize {
    let p = my_proc();
    // exit doesn't return, so we return 0 but the process vanishes
    extern "C" { fn exit() -> !; }
    exit();
}

pub unsafe fn sys_wait() -> usize {
    extern "C" { fn wait() -> i32; }
    wait() as usize
}

pub unsafe fn sys_kill() -> usize {
    let mut pid: i32 = 0;
    if argint(0, &mut pid) < 0 {
        return !0; // -1
    }
    extern "C" { fn kill(pid: i32) -> i32; }
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
    
    // growproc() handles the actual memory allocation/deallocation
    extern "C" { fn growproc(n: i32) -> i32; }
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
    
    // In xv6, sleep uses a spinlock (tickslock). 
    // For now, we'll assume a placeholder for the sleep implementation.
    extern "C" { 
        fn sleep(chan: *mut core::ffi::c_void, lock: *mut core::ffi::c_void); 
        static mut ticks: u32; // from trap.c/timer
    }
    
    // This logic is simplified; real xv6 sleep handles the lock handoff
    // but this shows the sysproc wrapper logic.
    sleep(&ticks as *const _ as *mut _, ptr::null_mut());
    0
}

pub unsafe fn sys_uptime() -> usize {
    extern "C" { static mut ticks: u32; }
    // Note: In a real kernel, you'd need a lock to read ticks safely 
    // if it's not an atomic.
    ticks as usize
}
