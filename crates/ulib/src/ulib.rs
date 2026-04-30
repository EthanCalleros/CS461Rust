use crate::{read, open, fstat, close};
use types::{stat, O_RDONLY};
use core::ptr;

/// Copy string t to s.
pub unsafe fn strcpy(s: *mut u8, t: *const u8) -> *mut u8 {
    let os = s;
    let mut s_ptr = s;
    let mut t_ptr = t;
    while {
        let val = *t_ptr;
        *s_ptr = val;
        s_ptr = s_ptr.add(1);
        t_ptr = t_ptr.add(1);
        val != 0
    } {}
    os
}

/// Compare strings p and q.
pub unsafe fn strcmp(p: *const u8, q: *const u8) -> i32 {
    let mut p_ptr = p;
    let mut q_ptr = q;
    while *p_ptr != 0 && *p_ptr == *q_ptr {
        p_ptr = p_ptr.add(1);
        q_ptr = q_ptr.add(1);
    }
    (*p_ptr as i32) - (*q_ptr as i32)
}

/// Return length of string s.
pub unsafe fn strlen(s: *const u8) -> usize {
    let mut n = 0;
    while *s.add(n) != 0 {
        n += 1;
    }
    n
}

/// Fill n bytes of dst with c.
pub unsafe fn memset(dst: *mut core::ffi::c_void, c: i32, n: u32) -> *mut core::ffi::c_void {
    // Equivalent to xv6's stosb
    ptr::write_bytes(dst as *mut u8, c as u8, n as usize);
    dst
}

/// Find first occurrence of c in s.
pub unsafe fn strchr(s: *const u8, c: u8) -> *mut u8 {
    let mut s_ptr = s;
    while *s_ptr != 0 {
        if *s_ptr == c {
            return s_ptr as *mut u8;
        }
        s_ptr = s_ptr.add(1);
    }
    ptr::null_mut()
}

/// Read a line from stdin (fd 0).
pub unsafe fn gets(buf: *mut u8, max: i32) -> *mut u8 {
    let mut i = 0;
    let mut c: u8 = 0;

    while i + 1 < max {
        let cc = read(0, &mut c as *mut u8 as *mut core::ffi::c_void, 1);
        if cc < 1 {
            break;
        }
        *buf.add(i as usize) = c;
        i += 1;
        if c == b'\n' || c == b'\r' {
            break;
        }
    }
    *buf.add(i as usize) = 0;
    buf
}

/// Get file status by name.
pub unsafe fn stat_wrapper(n: *const u8, st: *mut stat) -> i32 {
    let fd = open(n, O_RDONLY);
    if fd < 0 {
        return -1;
    }
    let r = fstat(fd, st);
    close(fd);
    r
}

/// Convert string to integer.
pub unsafe fn atoi(s: *const u8) -> i32 {
    let mut n = 0;
    let mut s_ptr = s;
    while *s_ptr >= b'0' && *s_ptr <= b'9' {
        n = n * 10 + (*s_ptr - b'0') as i32;
        s_ptr = s_ptr.add(1);
    }
    n
}

/// Copy n bytes from vsrc to vdst. 
/// In xv6 ulib, this is implemented like memcpy, but we use ptr::copy to be safe.
pub unsafe fn memmove(
    vdst: *mut core::ffi::c_void,
    vsrc: *const core::ffi::c_void,
    n: i32,
) -> *mut core::ffi::c_void {
    // ptr::copy handles overlapping regions (like C's memmove)
    ptr::copy(vsrc as *const u8, vdst as *mut u8, n as usize);
    vdst
}
