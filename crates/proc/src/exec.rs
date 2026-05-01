//! Port of `exec.c` — load and execute an ELF binary.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use core::mem;
use core::ptr;

use arch::elf::{elfhdr, proghdr, ELF_MAGIC, ELF_PROG_LOAD};
use arch::mmu::{PGROUNDUP, PGSIZE};
use arch::vm::{self, allocuvm, clearpteu, copyout, freevm, loaduvm, setupkvm, switchuvm};
use param::MAXARG;
use types::{addr_t, pde_t};

use crate::proch::{inode, my_proc};

// ============================================================================
// External functions from other crates (filesystem, etc.) not yet ported.
// These will become `use fs::...` imports once those crates are wired up.
// ============================================================================

unsafe extern "C" {
    fn begin_op();
    fn end_op();
    fn namei(path: *const u8) -> *mut inode;
    fn ilock(ip: *mut inode);
    fn iunlockput(ip: *mut inode);
    fn readi(ip: *mut inode, dst: *mut u8, off: u32, n: u32) -> i32;
}

// ============================================================================
// safestrcpy — copy at most `n - 1` bytes from `src` into `dst`, always
// NUL-terminating.
// ============================================================================

unsafe fn safestrcpy(dst: *mut u8, src: *const u8, n: usize) {
    if n == 0 {
        return;
    }
    let mut i = 0;
    while i < n - 1 {
        let c = *src.add(i);
        *dst.add(i) = c;
        if c == 0 {
            return;
        }
        i += 1;
    }
    *dst.add(i) = 0;
}

// ============================================================================
// exec — replace the current process image with a new ELF binary.
//
// `path` is a NUL-terminated pathname. `argv` is a NULL-terminated
// array of NUL-terminated argument strings.
//
// On success, does not return (jumps to the new program's entry point
// via the modified trapframe). On failure, returns -1 without changing
// the current process.
// ============================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn exec(path: *const u8, argv: *const *const u8) -> i32 {
    let mut elf: elfhdr = mem::zeroed();
    let mut ph: proghdr = mem::zeroed();
    let mut ip: *mut inode = ptr::null_mut();
    let mut pgdir: *mut pde_t = ptr::null_mut();
    let mut sz: addr_t;
    let mut sp: addr_t;
    let mut ustack: [addr_t; 3 + MAXARG as usize + 1] = [0; 3 + MAXARG as usize + 1];

    let p = my_proc();
    let oldpgdir = p.pgdir;

    begin_op();

    ip = namei(path);
    if ip.is_null() {
        end_op();
        return -1;
    }
    ilock(ip);

    // Check ELF header
    let elf_size = mem::size_of::<elfhdr>() as u32;
    if readi(ip, &mut elf as *mut elfhdr as *mut u8, 0, elf_size) != elf_size as i32 {
        goto_bad(&mut pgdir, &mut ip);
        return -1;
    }
    if elf.magic != ELF_MAGIC {
        goto_bad(&mut pgdir, &mut ip);
        return -1;
    }

    pgdir = setupkvm();
    if pgdir.is_null() {
        goto_bad(&mut pgdir, &mut ip);
        return -1;
    }

    // Load program into memory.
    sz = PGSIZE as addr_t; // skip the first page (guard page)
    let mut i: u16 = 0;
    let mut off = elf.phoff as u32;
    let ph_size = mem::size_of::<proghdr>() as u32;
    while i < elf.phnum {
        if readi(ip, &mut ph as *mut proghdr as *mut u8, off, ph_size) != ph_size as i32 {
            goto_bad(&mut pgdir, &mut ip);
            return -1;
        }
        if ph.r#type as i32 != ELF_PROG_LOAD {
            i += 1;
            off += ph_size;
            continue;
        }
        if ph.memsz < ph.filesz {
            goto_bad(&mut pgdir, &mut ip);
            return -1;
        }
        if ph.vaddr.wrapping_add(ph.memsz) < ph.vaddr {
            goto_bad(&mut pgdir, &mut ip);
            return -1;
        }
        sz = allocuvm(pgdir, sz, ph.vaddr + ph.memsz);
        if sz == 0 {
            goto_bad(&mut pgdir, &mut ip);
            return -1;
        }
        if ph.vaddr % PGSIZE as u64 != 0 {
            goto_bad(&mut pgdir, &mut ip);
            return -1;
        }
        if loaduvm(pgdir, ph.vaddr, ip as *mut _ as *mut vm::inode, ph.off as u32, ph.filesz as u32) < 0 {
            goto_bad(&mut pgdir, &mut ip);
            return -1;
        }

        i += 1;
        off += ph_size;
    }
    iunlockput(ip);
    end_op();
    ip = ptr::null_mut();

    // Allocate two pages at the next page boundary.
    // Make the first inaccessible (guard page). Use the second as the user stack.
    sz = PGROUNDUP(sz);
    sz = allocuvm(pgdir, sz, sz + 2 * PGSIZE as addr_t);
    if sz == 0 {
        goto_bad(&mut pgdir, &mut ip);
        return -1;
    }
    clearpteu(pgdir as *mut _, sz - 2 * PGSIZE as addr_t);
    sp = sz;

    // Push argument strings onto the stack, recording their addresses
    // in ustack[].
    let mut argc: usize = 0;
    if !argv.is_null() {
        while !(*argv.add(argc)).is_null() {
            if argc >= MAXARG as usize {
                goto_bad(&mut pgdir, &mut ip);
                return -1;
            }
            // Align sp down to addr_t boundary
            let arg = *argv.add(argc);
            let arglen = c_strlen(arg) + 1;
            sp = (sp - arglen as addr_t) & !(mem::size_of::<addr_t>() as addr_t - 1);
            if copyout(pgdir as *mut _, sp, arg, arglen as u64) < 0 {
                goto_bad(&mut pgdir, &mut ip);
                return -1;
            }
            ustack[1 + argc] = sp;
            argc += 1;
        }
    }
    ustack[1 + argc] = 0;

    ustack[0] = 0xffffffffffffffff; // fake return PC

    // argc and argv for main() entry point
    (*p.tf).rdi = argc as u64;
    (*p.tf).rsi = sp - (argc as addr_t + 1) * mem::size_of::<addr_t>() as addr_t;

    sp -= (1 + argc + 1) as addr_t * mem::size_of::<addr_t>() as addr_t;
    if copyout(
        pgdir as *mut _,
        sp,
        ustack.as_ptr() as *const u8,
        ((1 + argc + 1) * mem::size_of::<addr_t>()) as u64,
    ) < 0
    {
        goto_bad(&mut pgdir, &mut ip);
        return -1;
    }

    // Save program name for debugging.
    let last = find_last_component(path);
    safestrcpy(p.name.as_mut_ptr(), last, p.name.len());

    // Commit to the user image.
    p.pgdir = pgdir;
    p.sz = sz;
    (*p.tf).rip = elf.entry;
    (*p.tf).rcx = elf.entry;
    (*p.tf).rsp = sp;
    switchuvm(p as *mut _ as *mut core::ffi::c_void);
    freevm(oldpgdir as *mut _);
    0
}

// ============================================================================
// Helpers
// ============================================================================

/// Clean up on failure: free page table if allocated, unlock inode if held.
unsafe fn goto_bad(pgdir: &mut *mut pde_t, ip: &mut *mut inode) {
    if !(*pgdir).is_null() {
        freevm(*pgdir as *mut _);
        *pgdir = ptr::null_mut();
    }
    if !(*ip).is_null() {
        iunlockput(*ip);
        end_op();
        *ip = ptr::null_mut();
    }
}

/// Find the last path component (after the final '/').
unsafe fn find_last_component(path: *const u8) -> *const u8 {
    let mut last = path;
    let mut s = path;
    while *s != 0 {
        if *s == b'/' {
            last = s.add(1);
        }
        s = s.add(1);
    }
    last
}

/// Length of a NUL-terminated C string (not counting the NUL).
unsafe fn c_strlen(s: *const u8) -> usize {
    let mut n = 0;
    while *s.add(n) != 0 {
        n += 1;
    }
    n
}
