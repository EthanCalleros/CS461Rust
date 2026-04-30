use crate::lib::{argint, argaddr, argptr, argstr, fetchaddr, fetchstr};
use proc::my_proc;
use param::{NOFILE, MAXARG};
use types::{addr_t, stat};
use core::ptr;

// External functions from the fs and file crates
extern "C" {
    fn filealloc() -> *mut file;
    fn fileclose(f: *mut file);
    fn filedup(f: *mut file);
    fn filestat(f: *mut file, st: *mut stat) -> i32;
    fn fileread(f: *mut file, addr: *mut u8, n: i32) -> i32;
    fn filewrite(f: *mut file, addr: *mut u8, n: i32) -> i32;
    fn pipealloc(rf: *mut *mut file, wf: *mut *mut file) -> i32;
    fn exec(path: *const u8, argv: *const *const u8) -> i32;
    // ... add fs/inode functions as needed (namei, begin_op, etc)
}

// Opaque types for now
pub enum file {}
pub enum inode {}

/// Fetch the nth syscall arg as a file descriptor and return the struct file.
unsafe fn argfd(n: i32, pfd: *mut i32, pf: *mut *mut file) -> i32 {
    let mut fd: i32 = 0;
    if argint(n, &mut fd) < 0 { return -1; }
    
    let p = my_proc();
    // In Rust, we treat the ofile pointer as a slice/array
    let ofile = p.ofile as *mut *mut file;
    
    if fd < 0 || fd >= NOFILE as i32 || (*ofile.add(fd as usize)).is_null() {
        return -1;
    }
    
    if !pfd.is_null() { *pfd = fd; }
    if !pf.is_null() { *pf = *ofile.add(fd as usize); }
    0
}

/// Allocate a file descriptor for the given file.
unsafe fn fdalloc(f: *mut file) -> i32 {
    let p = my_proc();
    let ofile = p.ofile as *mut *mut file;
    
    for fd in 0..NOFILE {
        if (*ofile.add(fd)).is_null() {
            *ofile.add(fd) = f;
            return fd as i32;
        }
    }
    -1
}

pub unsafe fn sys_dup() -> usize {
    let mut f: *mut file = ptr::null_mut();
    if argfd(0, ptr::null_mut(), &mut f) < 0 { return !0; }
    
    let fd = fdalloc(f);
    if fd < 0 { return !0; }
    
    filedup(f);
    fd as usize
}

pub unsafe fn sys_read() -> usize {
    let mut f: *mut file = ptr::null_mut();
    let mut n: i32 = 0;
    let mut p: *mut u8 = ptr::null_mut();
    
    if argfd(0, ptr::null_mut(), &mut f) < 0 || 
       argint(2, &mut n) < 0 || 
       argptr(1, &mut p, n) < 0 {
        return !0;
    }
    fileread(f, p, n) as usize
}

pub unsafe fn sys_write() -> usize {
    let mut f: *mut file = ptr::null_mut();
    let mut n: i32 = 0;
    let mut p: *mut u8 = ptr::null_mut();
    
    if argfd(0, ptr::null_mut(), &mut f) < 0 || 
       argint(2, &mut n) < 0 || 
       argptr(1, &mut p, n) < 0 {
        return !0;
    }
    filewrite(f, p, n) as usize
}

pub unsafe fn sys_close() -> usize {
    let mut fd: i32 = 0;
    let mut f: *mut file = ptr::null_mut();
    
    if argfd(0, &mut fd, &mut f) < 0 { return !0; }
    
    let p = my_proc();
    let ofile = p.ofile as *mut *mut file;
    *ofile.add(fd as usize) = ptr::null_mut();
    
    fileclose(f);
    0
}

pub unsafe fn sys_fstat() -> usize {
    let mut f: *mut file = ptr::null_mut();
    let mut st: *mut stat = ptr::null_mut();
    
    if argfd(0, ptr::null_mut(), &mut f) < 0 || 
       argptr(1, &mut (st as *mut u8), core::mem::size_of::<stat>() as i32) < 0 {
        return !0;
    }
    filestat(f, st) as usize
}

pub unsafe fn sys_exec() -> usize {
    let mut path: *mut u8 = ptr::null_mut();
    let mut uargv: addr_t = 0;
    let mut argv: [*const u8; MAXARG] = [ptr::null(); MAXARG];
    
    if argstr(0, &mut path) < 0 || argaddr(1, &mut uargv) < 0 {
        return !0;
    }
    
    for i in 0..MAXARG {
        let mut uarg: addr_t = 0;
        if fetchaddr(uargv + (core::mem::size_of::<addr_t>() * i) as addr_t, &mut uarg) < 0 {
            return !0;
        }
        if uarg == 0 {
            argv[i] = ptr::null();
            break;
        }
        if fetchstr(uarg, &mut (argv[i] as *mut u8)) < 0 {
            return !0;
        }
    }
    
    exec(path, argv.as_ptr()) as usize
}

pub unsafe fn sys_pipe() -> usize {
    let mut fd_array: *mut i32 = ptr::null_mut();
    let mut rf: *mut file = ptr::null_mut();
    let mut wf: *mut file = ptr::null_mut();
    
    if argptr(0, &mut (fd_array as *mut u8), 2 * core::mem::size_of::<i32>() as i32) < 0 {
        return !0;
    }
    
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
        let ofile = p.ofile as *mut *mut file;
        *ofile.add(fd0 as usize) = ptr::null_mut();
        fileclose(rf);
        fileclose(wf);
        return !0;
    }
    
    *fd_array.add(0) = fd0;
    *fd_array.add(1) = fd1;
    0
}
