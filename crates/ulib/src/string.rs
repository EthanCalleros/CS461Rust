use core::ptr;
use arch::registers::{stosb, stosl}; // Assuming these exist in your arch crate
use types::addr_t;

/// Fill memory with a constant byte.
pub unsafe fn memset(dst: *mut core::ffi::c_void, c: i32, n: u64) -> *mut core::ffi::c_void {
    // xv6 optimization: if aligned to 4 bytes, use stosl
    if (dst as addr_t) % 4 == 0 && n % 4 == 0 {
        let c8 = (c & 0xFF) as u32;
        let c32 = (c8 << 24) | (c8 << 16) | (c8 << 8) | c8;
        stosl(dst, c32, (n / 4) as usize);
    } else {
        stosb(dst, c as u8, n as usize);
    }
    dst
}

/// Compare memory areas.
pub unsafe fn memcmp(v1: *const core::ffi::c_void, v2: *const core::ffi::c_void, n: usize) -> i32 {
    let s1 = v1 as *const u8;
    let s2 = v2 as *const u8;
    for i in 0..n {
        let val1 = *s1.add(i);
        let val2 = *s2.add(i);
        if val1 != val2 {
            return (val1 as i32) - (val2 as i32);
        }
    }
    0
}

/// Copy memory area, handling overlaps.
pub unsafe fn memmove(dst: *mut core::ffi::c_void, src: *const core::ffi::c_void, n: usize) -> *mut core::ffi::c_void {
    // In Rust, ptr::copy is equivalent to C's memmove (handles overlap)
    ptr::copy(src as *const u8, dst as *mut u8, n);
    dst
}

/// Copy memory area (alias for memmove to satisfy compiler expectations).
pub unsafe fn memcpy(dst: *mut core::ffi::c_void, src: *const core::ffi::c_void, n: usize) -> *mut core::ffi::c_void {
    memmove(dst, src, n)
}

/// Compare two strings up to n characters.
pub unsafe fn strncmp(p: *const u8, q: *const u8, mut n: usize) -> i32 {
    let mut p_ptr = p;
    let mut q_ptr = q;

    while n > 0 && *p_ptr != 0 && *p_ptr == *q_ptr {
        n -= 1;
        p_ptr = p_ptr.add(1);
        q_ptr = q_ptr.add(1);
    }
    
    if n == 0 {
        return 0;
    }
    (*p_ptr as i32) - (*q_ptr as i32)
}

/// Copy a string up to n characters.
pub unsafe fn strncpy(s: *mut u8, t: *const u8, mut n: i32) -> *mut u8 {
    let os = s;
    let mut s_ptr = s;
    let mut t_ptr = t;

    while n > 0 {
        let val = *t_ptr;
        *s_ptr = val;
        s_ptr = s_ptr.add(1);
        n -= 1;
        if val == 0 {
            break;
        }
        t_ptr = t_ptr.add(1);
    }
    
    while n > 0 {
        *s_ptr = 0;
        s_ptr = s_ptr.add(1);
        n -= 1;
    }
    os
}

/// Like strncpy but guaranteed to be NUL-terminated.
pub unsafe fn safestrcpy(s: *mut u8, t: *const u8, n: i32) -> *mut u8 {
    let os = s;
    if n <= 0 {
        return os;
    }
    
    let mut s_ptr = s;
    let mut t_ptr = t;
    let mut count = n;

    while count > 1 {
        let val = *t_ptr;
        *s_ptr = val;
        s_ptr = s_ptr.add(1);
        t_ptr = t_ptr.add(1);
        count -= 1;
        if val == 0 {
            return os;
        }
    }
    
    *s_ptr = 0;
    os
}

/// Calculate the length of a string.
pub unsafe fn strlen(s: *const u8) -> usize {
    let mut n = 0;
    while *s.add(n) != 0 {
        n += 1;
    }
    n
}
