#![no_std]
#![allow(non_camel_case_types)]

use core::arch::asm;
use core::sync::atomic::AtomicU32;
use types::{uchar, addr_t, pde_t};
use param::{NCPU, NOFILE};


#[repr(C)]
pub struct Cpu {
    id: uchar,
    apicid: uchar,
    scheduler: *mut Context,
    started: AtomicU32,
    ncli: i32,
    intena: i32,
    local: *mut core::ffi::c_void,
}

#[repr(C)]
pub struct Context {
    pub r15: addr_t,
    pub r14: addr_t,
    pub r13: addr_t,
    pub r12: addr_t,
    pub rbx: addr_t,
    pub rbp: addr_t,
    pub rip: addr_t,
}

pub struct proc {
    sz: addr_t,
    pgdir: *mut pde_t,
    kstack: *mut char,
    state: Procstate,
    pid: i32,
    parent: *mut proc,
    tf: *mut trapframe,
    context: *mut Context,
    chan: *mut core::ffi::c_void,
    killed: i32,
    ofile: *mut [file, NOFILE],
    cwd: *mut inode,
    name: [char, 16],
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Procstate {
    UNUSED,
    EMBRYO,
    SLEEPING,
    RUNNABLE,
    RUNNING,
    ZOMBIE,
}

unsafe extern "C" {
    pub static mut cpus: [Cpu; NCPU];
    pub static mut ncpu: i32;
}

pub unsafe fn my_cpu() -> &'static mut Cpu {
    let ptr: *mut Cpu;
    asm!("mov {}, gs:[-16]", out(reg) ptr);
    &mut *ptr
}

pub unsafe fn my_proc() -> &'static mut Proc {
    let ptr: *mut Proc;
    asm!("mov {}, gs:[-8]", out(reg) ptr);
    &mut *ptr
}