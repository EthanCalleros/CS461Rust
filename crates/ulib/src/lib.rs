#![no_std]

use types::{stat, addr_t, uint64};

// Re-export modules
pub mod string;
pub mod printf;
pub mod umalloc;
pub mod ulib;

// System Calls
// These are defined in usys.S (linked via build.rs)
extern "C" {
    pub fn fork() -> i32;
    pub fn exit() -> !;
    pub fn wait() -> i32;
    pub fn pipe(p: *mut i32) -> i32;
    pub fn write(fd: i32, buf: *const core::ffi::c_void, n: i32) -> i32;
    pub fn read(fd: i32, buf: *mut core::ffi::c_void, n: i32) -> i32;
    pub fn close(fd: i32) -> i32;
    pub fn kill(pid: i32, sig: i32) -> i32;
    pub fn exec(path: *const u8, argv: *const *const u8) -> i32;
    pub fn open(path: *const u8, mode: i32) -> i32;
    pub fn mknod(path: *const u8, major: i16, minor: i16) -> i32;
    pub fn unlink(path: *const u8) -> i32;
    pub fn fstat(fd: i32, st: *mut stat) -> i32;
    pub fn link(old_path: *const u8, new_path: *const u8) -> i32;
    pub fn mkdir(path: *const u8) -> i32;
    pub fn chdir(path: *const u8) -> i32;
    pub fn dup(fd: i32) -> i32;
    pub fn getpid() -> i32;
    pub fn sbrk(n: uint64) -> *mut u8;
    pub fn sleep(ticks: i32) -> i32;
    pub fn uptime() -> i32;

    pub fn alarm(ticks: i32);
    pub fn signal(sig: i32, handler: extern "C" fn(i32));
    pub fn sigret();
    pub fn fgproc(pid: i32);
}

// Re-export common ulib functions for convenience
pub use string::{strcpy, strlen, strcmp, strchr, memset, memmove, atoi};
pub use umalloc::{malloc, free};
pub use printf::printf;
pub use ulib::{stat_wrapper as stat, gets};
