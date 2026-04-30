#![no_std]
#![allow(dead_code)]

//! User-mode runtime library for xv6: syscall wrappers, string utilities,
//! printf, and a simple heap allocator.
//! This is the Rust equivalent of xv6's ulib.c + printf.c + umalloc.c.

use core::arch::asm;
use core::fmt;

// Re-export types that user programs need
pub use types::stat;
pub use types::{O_RDONLY, O_WRONLY, O_RDWR, O_CREATE};
pub use types::{T_DIR, T_FILE, T_DEV};

// ============================================================================
// Syscall layer
// ============================================================================
// xv6 on x86-64 uses `int 0x40` for syscalls.
// Syscall number in rax, args in rdi, rsi, rdx, r10, r8, r9.

#[inline(always)]
unsafe fn syscall0(n: i64) -> i64 {
    let ret: i64;
    asm!(
        "int 0x40",
        inout("rax") n => ret,
        out("rcx") _,
        out("r11") _,
    );
    ret
}

#[inline(always)]
unsafe fn syscall1(n: i64, a1: i64) -> i64 {
    let ret: i64;
    asm!(
        "int 0x40",
        inout("rax") n => ret,
        in("rdi") a1,
        out("rcx") _,
        out("r11") _,
    );
    ret
}

#[inline(always)]
unsafe fn syscall2(n: i64, a1: i64, a2: i64) -> i64 {
    let ret: i64;
    asm!(
        "int 0x40",
        inout("rax") n => ret,
        in("rdi") a1,
        in("rsi") a2,
        out("rcx") _,
        out("r11") _,
    );
    ret
}

#[inline(always)]
unsafe fn syscall3(n: i64, a1: i64, a2: i64, a3: i64) -> i64 {
    let ret: i64;
    asm!(
        "int 0x40",
        inout("rax") n => ret,
        in("rdi") a1,
        in("rsi") a2,
        in("rdx") a3,
        out("rcx") _,
        out("r11") _,
    );
    ret
}

// Syscall numbers
const SYS_FORK: i64 = 1;
const SYS_EXIT: i64 = 2;
const SYS_WAIT: i64 = 3;
const SYS_PIPE: i64 = 4;
const SYS_READ: i64 = 5;
const SYS_KILL: i64 = 6;
const SYS_EXEC: i64 = 7;
const SYS_FSTAT: i64 = 8;
const SYS_CHDIR: i64 = 9;
const SYS_DUP: i64 = 10;
const SYS_GETPID: i64 = 11;
const SYS_SBRK: i64 = 12;
const SYS_SLEEP: i64 = 13;
const SYS_UPTIME: i64 = 14;
const SYS_OPEN: i64 = 15;
const SYS_WRITE: i64 = 16;
const SYS_MKNOD: i64 = 17;
const SYS_UNLINK: i64 = 18;
const SYS_LINK: i64 = 19;
const SYS_MKDIR: i64 = 20;
const SYS_CLOSE: i64 = 21;

// ============================================================================
// Public syscall wrappers
// ============================================================================

pub fn fork() -> i32 {
    unsafe { syscall0(SYS_FORK) as i32 }
}

/// xv6 exit takes no arguments.
pub fn exit() -> ! {
    unsafe { syscall0(SYS_EXIT); }
    loop {}
}

/// xv6 wait takes no arguments, returns child pid (or -1).
pub fn wait() -> i32 {
    unsafe { syscall0(SYS_WAIT) as i32 }
}

pub fn pipe(fd: &mut [i32; 2]) -> i32 {
    unsafe { syscall1(SYS_PIPE, fd.as_mut_ptr() as i64) as i32 }
}

pub fn read(fd: i32, buf: &mut [u8]) -> i32 {
    unsafe { syscall3(SYS_READ, fd as i64, buf.as_mut_ptr() as i64, buf.len() as i64) as i32 }
}

/// Raw read with pointer and length (for use with raw buffers).
pub fn read_raw(fd: i32, buf: *mut u8, n: usize) -> i32 {
    unsafe { syscall3(SYS_READ, fd as i64, buf as i64, n as i64) as i32 }
}

pub fn write(fd: i32, buf: &[u8]) -> i32 {
    unsafe { syscall3(SYS_WRITE, fd as i64, buf.as_ptr() as i64, buf.len() as i64) as i32 }
}

/// Raw write with pointer and length.
pub fn write_raw(fd: i32, buf: *const u8, n: usize) -> i32 {
    unsafe { syscall3(SYS_WRITE, fd as i64, buf as i64, n as i64) as i32 }
}

pub fn close(fd: i32) -> i32 {
    unsafe { syscall1(SYS_CLOSE, fd as i64) as i32 }
}

pub fn kill(pid: i32) -> i32 {
    unsafe { syscall1(SYS_KILL, pid as i64) as i32 }
}

pub fn exec(path: &[u8], argv: &[*const u8]) -> i32 {
    unsafe { syscall2(SYS_EXEC, path.as_ptr() as i64, argv.as_ptr() as i64) as i32 }
}

pub fn open(path: &[u8], omode: i32) -> i32 {
    unsafe { syscall2(SYS_OPEN, path.as_ptr() as i64, omode as i64) as i32 }
}

pub fn fstat(fd: i32, st: &mut stat::stat) -> i32 {
    unsafe { syscall2(SYS_FSTAT, fd as i64, st as *mut stat::stat as i64) as i32 }
}

pub fn dup(fd: i32) -> i32 {
    unsafe { syscall1(SYS_DUP, fd as i64) as i32 }
}

pub fn getpid() -> i32 {
    unsafe { syscall0(SYS_GETPID) as i32 }
}

pub fn sbrk(n: i32) -> *mut u8 {
    unsafe { syscall1(SYS_SBRK, n as i64) as *mut u8 }
}

pub fn sleep(n: i32) -> i32 {
    unsafe { syscall1(SYS_SLEEP, n as i64) as i32 }
}

pub fn uptime() -> i32 {
    unsafe { syscall0(SYS_UPTIME) as i32 }
}

pub fn chdir(path: &[u8]) -> i32 {
    unsafe { syscall1(SYS_CHDIR, path.as_ptr() as i64) as i32 }
}

pub fn mknod(path: &[u8], major: i16, minor: i16) -> i32 {
    unsafe { syscall3(SYS_MKNOD, path.as_ptr() as i64, major as i64, minor as i64) as i32 }
}

pub fn unlink(path: &[u8]) -> i32 {
    unsafe { syscall1(SYS_UNLINK, path.as_ptr() as i64) as i32 }
}

pub fn link(old: &[u8], new: &[u8]) -> i32 {
    unsafe { syscall2(SYS_LINK, old.as_ptr() as i64, new.as_ptr() as i64) as i32 }
}

pub fn mkdir(path: &[u8]) -> i32 {
    unsafe { syscall1(SYS_MKDIR, path.as_ptr() as i64) as i32 }
}

// ============================================================================
// String / Memory Utilities
// ============================================================================

/// Returns length of a null-terminated C string.
pub fn strlen(s: *const u8) -> usize {
    unsafe {
        let mut n = 0;
        while *s.add(n) != 0 {
            n += 1;
        }
        n
    }
}

pub fn strcmp(s1: *const u8, s2: *const u8) -> i32 {
    unsafe {
        let mut i = 0;
        loop {
            let a = *s1.add(i);
            let b = *s2.add(i);
            if a != b {
                return (a as i32) - (b as i32);
            }
            if a == 0 {
                return 0;
            }
            i += 1;
        }
    }
}

pub fn strchr(s: *const u8, c: u8) -> *const u8 {
    unsafe {
        let mut p = s;
        loop {
            if *p == c {
                return p;
            }
            if *p == 0 {
                return core::ptr::null();
            }
            p = p.add(1);
        }
    }
}

pub fn memset(dst: *mut u8, c: u8, n: usize) {
    unsafe {
        for i in 0..n {
            *dst.add(i) = c;
        }
    }
}

pub fn memmove(dst: *mut u8, src: *const u8, n: usize) {
    unsafe {
        if (dst as usize) < (src as usize) {
            for i in 0..n {
                *dst.add(i) = *src.add(i);
            }
        } else {
            for i in (0..n).rev() {
                *dst.add(i) = *src.add(i);
            }
        }
    }
}

pub fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    unsafe {
        for i in 0..n {
            let a = *s1.add(i);
            let b = *s2.add(i);
            if a != b {
                return (a as i32) - (b as i32);
            }
        }
        0
    }
}

pub fn atoi(s: &[u8]) -> i32 {
    let mut n: i32 = 0;
    let mut neg = false;
    let mut i = 0;
    if i < s.len() && s[i] == b'-' {
        neg = true;
        i += 1;
    }
    while i < s.len() && s[i] >= b'0' && s[i] <= b'9' {
        n = n * 10 + (s[i] - b'0') as i32;
        i += 1;
    }
    if neg { -n } else { n }
}

/// atoi from a null-terminated C string pointer.
pub fn atoi_cstr(s: *const u8) -> i32 {
    unsafe {
        let len = strlen(s);
        let slice = core::slice::from_raw_parts(s, len);
        atoi(slice)
    }
}

// ============================================================================
// Printf (using Rust's fmt infrastructure)
// ============================================================================

/// A writer that outputs to a file descriptor.
pub struct FdWriter {
    pub fd: i32,
}

impl fmt::Write for FdWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        write(self.fd, s.as_bytes());
        Ok(())
    }
}

/// Print formatted text to a file descriptor.
/// Usage: `printf!(fd, "hello {}\n", name);`
#[macro_export]
macro_rules! printf {
    ($fd:expr, $($arg:tt)*) => {{
        use core::fmt::Write;
        let mut w = $crate::FdWriter { fd: $fd };
        let _ = write!(w, $($arg)*);
    }};
}

// ============================================================================
// gets: read a line from fd 0
// ============================================================================

/// Reads characters from stdin into `buf` until newline or EOF.
/// Returns the number of bytes read (including the newline if present).
pub fn gets(buf: &mut [u8]) -> usize {
    let mut i = 0;
    while i + 1 < buf.len() {
        let n = read_raw(0, unsafe { buf.as_mut_ptr().add(i) }, 1);
        if n <= 0 {
            break;
        }
        i += 1;
        if buf[i - 1] == b'\n' {
            break;
        }
    }
    buf[i] = 0;
    i
}

// ============================================================================
// Simple heap allocator using sbrk
// ============================================================================

/// Header for each allocated block.
#[repr(C)]
struct Header {
    next: *mut Header,
    size: usize, // size in units of Header
}

// Union-like alignment (Header is 16 bytes on 64-bit)
const HEADER_SIZE: usize = core::mem::size_of::<Header>();

static mut FREEP: *mut Header = core::ptr::null_mut();
static mut BASE: Header = Header {
    next: core::ptr::null_mut(),
    size: 0,
};

/// Allocate `nbytes` from the heap.
pub fn malloc(nbytes: usize) -> *mut u8 {
    unsafe {
        let nunits = (nbytes + HEADER_SIZE - 1) / HEADER_SIZE + 1;

        let mut prevp = FREEP;
        if prevp.is_null() {
            // First call: initialize the free list
            BASE.next = &raw mut BASE;
            BASE.size = 0;
            FREEP = &raw mut BASE;
            prevp = FREEP;
        }

        let mut p = (*prevp).next;
        loop {
            if (*p).size >= nunits {
                // Found a fit
                if (*p).size == nunits {
                    // Exact fit: unlink
                    (*prevp).next = (*p).next;
                } else {
                    // Allocate tail end
                    (*p).size -= nunits;
                    p = p.add((*p).size);
                    (*p).size = nunits;
                }
                FREEP = prevp;
                return p.add(1) as *mut u8;
            }

            if p == FREEP {
                // Wrapped around: need more memory
                p = morecore(nunits);
                if p.is_null() {
                    return core::ptr::null_mut();
                }
            }

            prevp = p;
            p = (*p).next;
        }
    }
}

/// Request more memory from the kernel.
unsafe fn morecore(nu: usize) -> *mut Header {
    let units = if nu < 4096 { 4096 } else { nu };
    let bytes = units * HEADER_SIZE;

    let p = sbrk(bytes as i32);
    if (p as i64) == -1 {
        return core::ptr::null_mut();
    }

    let hp = p as *mut Header;
    (*hp).size = units;
    free(hp.add(1) as *mut u8);
    FREEP
}

/// Free memory previously allocated by malloc.
pub fn free(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        let bp = (ptr as *mut Header).sub(1);

        // Find where to insert in the ordered free list
        let mut p = FREEP;
        while !(bp > p && bp < (*p).next) {
            if p >= (*p).next && (bp > p || bp < (*p).next) {
                break;
            }
            p = (*p).next;
        }

        // Coalesce with upper neighbor
        if bp.add((*bp).size) == (*p).next {
            (*bp).size += (*(*p).next).size;
            (*bp).next = (*(*p).next).next;
        } else {
            (*bp).next = (*p).next;
        }

        // Coalesce with lower neighbor
        if p.add((*p).size) == bp {
            (*p).size += (*bp).size;
            (*p).next = (*bp).next;
        } else {
            (*p).next = bp;
        }

        FREEP = p;
    }
}

// ============================================================================
// Dirent (needed by ls)
// ============================================================================

pub const DIRSIZ: usize = 14;

#[repr(C)]
pub struct Dirent {
    pub inum: u16,
    pub name: [u8; DIRSIZ],
}

// ============================================================================
// Panic handler — required for #![no_std] binaries.
// ============================================================================

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    write(2, b"panic in user program\n");
    exit();
}
