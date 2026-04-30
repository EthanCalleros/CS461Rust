use crate::sbrk; // The syscall from lib.rs
use core::ptr;

#[repr(C, align(16))]
pub struct Header {
    ptr: *mut Header,
    size: u32,
}

static mut BASE: Header = Header {
    ptr: ptr::null_mut(),
    size: 0,
};

static mut FREEP: *mut Header = ptr::null_mut();

/// Free the memory block pointed to by ap.
pub unsafe fn free(ap: *mut core::ffi::c_void) {
    if ap.is_null() {
        return;
    }

    let mut bp = (ap as *mut Header).offset(-1);
    let mut p = FREEP;

    // Search the free list for the right place to insert
    while !(bp > p && bp < (*p).ptr) {
        if p >= (*p).ptr && (bp > p || bp < (*p).ptr) {
            break; // Freed block at start or end of arena
        }
        p = (*p).ptr;
    }

    // Join to upper neighbor
    if bp.add((*bp).size as usize) == (*p).ptr {
        (*bp).size += (*(*p).ptr).size;
        (*bp).ptr = (*(*p).ptr).ptr;
    } else {
        (*bp).ptr = (*p).ptr;
    }

    // Join to lower neighbor
    if p.add((*p).size as usize) == bp {
        (*p).size += (*bp).size;
        (*p).ptr = (*bp).ptr;
    } else {
        (*p).ptr = bp;
    }

    FREEP = p;
}

/// Ask the OS for more memory.
unsafe fn morecore(nu: u32) -> *mut Header {
    let units = if nu < 4096 { 4096 } else { nu };
    
    let p = sbrk((units as usize) * core::mem::size_of::<Header>());
    if p == !0 as *mut u8 { // sbrk returns -1 on error
        return ptr::null_mut();
    }

    let hp = p as *mut Header;
    (*hp).size = units;
    
    // Use the free logic to put this new block into the list
    free(hp.offset(1) as *mut core::ffi::c_void);
    FREEP
}

/// Allocate nbytes of memory.
pub unsafe fn malloc(nbytes: u32) -> *mut core::ffi::c_void {
    let nunits = (nbytes as usize + core::mem::size_of::<Header>() - 1) 
                 / core::mem::size_of::<Header>() + 1;

    let mut prevp = FREEP;
    if prevp.is_null() {
        BASE.ptr = &mut BASE;
        BASE.size = 0;
        FREEP = &mut BASE;
        prevp = FREEP;
    }

    let mut p = (*prevp).ptr;
    loop {
        if (*p).size >= nunits as u32 {
            if (*p).size == nunits as u32 {
                (*prevp).ptr = (*p).ptr;
            } else {
                (*p).size -= nunits as u32;
                p = p.add((*p).size as usize);
                (*p).size = nunits as u32;
            }
            FREEP = prevp;
            return p.offset(1) as *mut core::ffi::c_void;
        }

        if p == FREEP {
            p = morecore(nunits as u32);
            if p.is_null() {
                return ptr::null_mut();
            }
        }

        prevp = p;
        p = (*p).ptr;
    }
}
