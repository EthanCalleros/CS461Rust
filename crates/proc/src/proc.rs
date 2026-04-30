//! Port of `proc.c` — process table, scheduler, and process lifecycle.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(static_mut_refs)]

use core::ffi::c_void;
use core::mem;
use core::ptr;
use core::sync::atomic::{compiler_fence, Ordering};

use arch::mmu::{FL_IF, PGSIZE};
use arch::registers::{hlt, readeflags, sti, trapframe};
use arch::vm::{
    copyuvm, freevm, inituvm, setupkvm, switchkvm, switchuvm, KSTACKSIZE,
};
use param::{NOFILE, NPROC};
use types::{addr_t, pde_t, uint};

use crate::proch::{file, inode, Context, Cpu, Procstate, my_cpu, my_proc};

// ============================================================================
// Local spinlock definition — mirrors `sync::spinlockh::spinlock` to avoid
// a circular dependency (sync depends on proc for Cpu type).
// ============================================================================

/// C-compatible spinlock struct. Layout must match `sync::spinlockh::spinlock`.
/// Uses a raw pointer for `name` to allow zero-initialization and C FFI.
#[repr(C)]
pub struct spinlock {
    pub locked: uint,
    pub name:   *const u8,
    pub cpu:    *mut Cpu,
    pub pcs:    [addr_t; 10],
}

// ============================================================================
// External functions not yet ported to Rust. Replace with `use` imports
// once the corresponding crates are available.
// ============================================================================

unsafe extern "C" {
    fn initlock(lk: *mut spinlock, name: *const u8);
    fn acquire(lk: *mut spinlock);
    fn release(lk: *mut spinlock);
    fn holding(lk: *mut spinlock) -> i32;

    fn kalloc() -> *mut u8;
    fn kfree(v: *mut u8);

    fn iinit(dev: u32);
    fn initlog(dev: u32);
    fn namei(path: *const u8) -> *mut inode;
    fn idup(ip: *mut inode) -> *mut inode;
    fn iput(ip: *mut inode);

    fn filedup(f: *mut file) -> *mut file;
    fn fileclose(f: *mut file);

    fn begin_op();
    fn end_op();

    fn getstackpcs(rbp: *const addr_t, pcs: *mut addr_t);
}

/// Assembly-defined entry point: first scheduled instruction for a
/// new process. Jumps to `forkret`.
unsafe extern "C" {
    fn forkret_entry();
    fn syscall_trapret();
}

// ============================================================================
// Console output (cprintf equivalent)
// ============================================================================

/// Kernel cprintf (variadic in C, but we use Rust formatting).
/// For now, use a simple extern. Replace with `console::cprintf` import later.
unsafe extern "C" {
    fn cprintf_str(s: *const u8);
}

/// Minimal kernel print macro — delegates to cprintf.
macro_rules! kprint {
    ($($arg:tt)*) => {{
        // In a real kernel this would format and call cprintf.
        // Placeholder: we'll just use extern cprintf_str for string literals.
        // For full formatting, wire up to `console::cprintf(format_args!(...))`.
    }};
}

// ============================================================================
// Process table
// ============================================================================

/// The process table, protected by a spinlock.
#[repr(C)]
pub struct Ptable {
    pub lock: spinlock,
    pub proc_table: [proc_struct; NPROC as usize],
}

/// Alias for the `struct proc` from proch.rs. We re-define it here to
/// avoid name collisions with the module. In practice the type is the
/// same as `crate::proch::proc`.
pub type proc_struct = crate::proch::proc;

/// Global process table.
#[unsafe(no_mangle)]
pub static mut ptable: Ptable = unsafe { mem::zeroed() };

/// The first user process (init).
static mut INITPROC: *mut proc_struct = ptr::null_mut();

/// Monotonically increasing PID counter.
static mut NEXTPID: i32 = 1;

// ============================================================================
// pinit — initialize the process table lock.
// ============================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pinit() {
    initlock(&raw mut ptable.lock, b"ptable\0".as_ptr());
}

// ============================================================================
// allocproc — look for an UNUSED slot in the process table.
// If found, initialize it to EMBRYO state and set up the kernel stack.
// ============================================================================

unsafe fn allocproc() -> *mut proc_struct {
    acquire(&raw mut ptable.lock);

    let mut p: *mut proc_struct = ptr::null_mut();
    for i in 0..NPROC as usize {
        if ptable.proc_table[i].state == Procstate::UNUSED {
            p = &raw mut ptable.proc_table[i];
            break;
        }
    }

    if p.is_null() {
        release(&raw mut ptable.lock);
        return ptr::null_mut();
    }

    (*p).state = Procstate::EMBRYO;
    (*p).pid = NEXTPID;
    NEXTPID += 1;

    release(&raw mut ptable.lock);

    // Allocate kernel stack.
    let kstack = kalloc();
    if kstack.is_null() {
        (*p).state = Procstate::UNUSED;
        return ptr::null_mut();
    }
    (*p).kstack = kstack;
    let mut sp = kstack.add(KSTACKSIZE as usize);

    // Leave room for trap frame.
    sp = sp.sub(mem::size_of::<trapframe>());
    (*p).tf = sp as *mut trapframe;

    // Set up new context to start executing at forkret,
    // which returns to trapret (syscall_trapret).
    sp = sp.sub(mem::size_of::<addr_t>());
    *(sp as *mut addr_t) = syscall_trapret as addr_t;

    sp = sp.sub(mem::size_of::<Context>());
    (*p).context = sp as *mut Context;
    ptr::write_bytes((*p).context as *mut u8, 0, mem::size_of::<Context>());
    (*(*p).context).rip = forkret as addr_t;

    p
}

// ============================================================================
// userinit — set up the first user process.
// ============================================================================

unsafe extern "C" {
    static _binary_initcode_start: u8;
    static _binary_initcode_size: u8;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn userinit() {
    let p = allocproc();

    INITPROC = p;
    let pgdir = setupkvm();
    if pgdir.is_null() {
        panic!("userinit: out of memory?");
    }
    (*p).pgdir = pgdir as *mut pde_t;

    let initcode_start = &_binary_initcode_start as *const u8;
    let initcode_size = &_binary_initcode_size as *const u8 as addr_t;
    inituvm(pgdir as *mut pde_t, initcode_start, initcode_size as uint);

    (*p).sz = (PGSIZE * 2) as addr_t;
    ptr::write_bytes((*p).tf as *mut u8, 0, mem::size_of::<trapframe>());

    (*(*p).tf).r11 = FL_IF; // with SYSRET, EFLAGS is in R11
    (*(*p).tf).rsp = (*p).sz;
    (*(*p).tf).rcx = PGSIZE as u64; // with SYSRET, RIP is in RCX

    safestrcpy((*p).name.as_mut_ptr(), b"initcode\0".as_ptr(), (*p).name.len());
    (*p).cwd = namei(b"/\0".as_ptr());

    compiler_fence(Ordering::SeqCst);
    (*p).state = Procstate::RUNNABLE;
}

// ============================================================================
// growproc — grow or shrink the current process's memory by `n` bytes.
// ============================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn growproc(n: i64) -> i32 {
    let p = my_proc();
    let mut sz = p.sz;

    if n > 0 {
        sz = arch::vm::allocuvm(p.pgdir, sz, sz + n as addr_t);
        if sz == 0 {
            return -1;
        }
    } else if n < 0 {
        sz = arch::vm::deallocuvm(p.pgdir, sz, sz.wrapping_add(n as addr_t));
        if sz == 0 {
            return -1;
        }
    }
    p.sz = sz;
    switchuvm(p as *mut _ as *mut c_void);
    0
}

// ============================================================================
// fork — create a new process copying the current process.
// ============================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fork() -> i32 {
    let p = my_proc();

    // Allocate process.
    let np = allocproc();
    if np.is_null() {
        return -1;
    }

    // Copy process state from parent.
    let new_pgdir = copyuvm(p.pgdir as *mut _, p.sz as uint);
    if new_pgdir.is_null() {
        kfree((*np).kstack);
        (*np).kstack = ptr::null_mut();
        (*np).state = Procstate::UNUSED;
        return -1;
    }
    (*np).pgdir = new_pgdir;
    (*np).sz = p.sz;
    (*np).parent = p;
    // Copy trap frame
    ptr::copy_nonoverlapping(p.tf, (*np).tf, 1);

    // Clear %rax so that fork returns 0 in the child.
    (*(*np).tf).rax = 0;

    // Duplicate open file descriptors.
    for i in 0..NOFILE as usize {
        if !p.ofile[i].is_null() {
            (*np).ofile[i] = filedup(p.ofile[i]);
        }
    }
    (*np).cwd = idup(p.cwd);

    safestrcpy((*np).name.as_mut_ptr(), p.name.as_ptr(), p.name.len());

    let pid = (*np).pid;

    compiler_fence(Ordering::SeqCst);
    (*np).state = Procstate::RUNNABLE;

    pid
}

// ============================================================================
// exit — exit the current process. Does not return.
// ============================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn exit() {
    let p = my_proc();

    if ptr::eq(p, INITPROC) {
        panic!("init exiting");
    }

    // Close all open files.
    for fd in 0..NOFILE as usize {
        if !p.ofile[fd].is_null() {
            fileclose(p.ofile[fd]);
            p.ofile[fd] = ptr::null_mut();
        }
    }

    begin_op();
    iput(p.cwd);
    end_op();
    p.cwd = ptr::null_mut();

    acquire(&raw mut ptable.lock);

    // Parent might be sleeping in wait().
    wakeup1(p.parent as *mut c_void);

    // Pass abandoned children to init.
    for i in 0..NPROC as usize {
        let child = &raw mut ptable.proc_table[i];
        if (*child).parent == (p as *mut _) {
            (*child).parent = INITPROC;
            if (*child).state == Procstate::ZOMBIE {
                wakeup1(INITPROC as *mut c_void);
            }
        }
    }

    // Jump into the scheduler, never to return.
    p.state = Procstate::ZOMBIE;
    sched();
    panic!("zombie exit");
}

// ============================================================================
// wait — wait for a child process to exit and return its pid.
// ============================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wait() -> i32 {
    let p = my_proc();

    acquire(&raw mut ptable.lock);
    loop {
        let mut havekids = false;
        for i in 0..NPROC as usize {
            let child = &raw mut ptable.proc_table[i];
            if (*child).parent != (p as *mut _) {
                continue;
            }
            havekids = true;
            if (*child).state == Procstate::ZOMBIE {
                // Found one.
                let pid = (*child).pid;
                kfree((*child).kstack);
                (*child).kstack = ptr::null_mut();
                freevm((*child).pgdir as *mut _);
                (*child).pid = 0;
                (*child).parent = ptr::null_mut();
                (*child).name[0] = 0;
                (*child).killed = 0;
                (*child).state = Procstate::UNUSED;
                release(&raw mut ptable.lock);
                return pid;
            }
        }

        // No point waiting if we don't have any children.
        if !havekids || p.killed != 0 {
            release(&raw mut ptable.lock);
            return -1;
        }

        // Wait for children to exit.
        sleep_proc(p as *mut _ as *mut c_void, &raw mut ptable.lock);
    }
}

// ============================================================================
// scheduler — per-CPU process scheduler. Never returns.
// ============================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn scheduler() -> ! {
    let mut skipped: u32 = 0;

    loop {
        // Enable interrupts on this processor.
        sti();

        // Loop over process table looking for process to run.
        acquire(&raw mut ptable.lock);
        for i in 0..NPROC as usize {
            let p = &raw mut ptable.proc_table[i];
            if (*p).state != Procstate::RUNNABLE {
                skipped += 1;
                continue;
            }
            skipped = 0;

            // Switch to chosen process.
            set_my_proc(p);
            switchuvm(p as *mut c_void);
            (*p).state = Procstate::RUNNING;
            arch::swtch(
                &raw mut (*my_cpu()).scheduler as *mut *mut _ as *mut *mut arch::Context,
                (*p).context as *mut arch::Context,
            );
            switchkvm();

            // Process is done running for now.
            set_my_proc(ptr::null_mut());
        }
        release(&raw mut ptable.lock);

        if skipped > NPROC {
            hlt();
            skipped = 0;
        }
    }
}

// ============================================================================
// sched — enter the scheduler. Must hold only ptable.lock and have
// changed proc->state.
// ============================================================================

pub unsafe fn sched() {
    let p = my_proc();

    if holding(&raw mut ptable.lock) == 0 {
        panic!("sched ptable.lock");
    }
    if (*my_cpu()).ncli != 1 {
        panic!("sched locks");
    }
    if p.state == Procstate::RUNNING {
        panic!("sched running");
    }
    if readeflags() & FL_IF != 0 {
        panic!("sched interruptible");
    }
    let intena = (*my_cpu()).intena;
    arch::swtch(
        &raw mut p.context as *mut *mut _ as *mut *mut arch::Context,
        (*my_cpu()).scheduler as *mut arch::Context,
    );
    (*my_cpu()).intena = intena;
}

// ============================================================================
// yield — give up the CPU for one scheduling round.
// ============================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn yield_proc() {
    acquire(&raw mut ptable.lock);
    my_proc().state = Procstate::RUNNABLE;
    sched();
    release(&raw mut ptable.lock);
}

// ============================================================================
// forkret — a new process's very first scheduling by scheduler() will
// swtch here. "Return" to user space.
// ============================================================================

static mut FORKRET_FIRST: bool = true;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn forkret() {
    // Still holding ptable.lock from scheduler.
    release(&raw mut ptable.lock);

    if FORKRET_FIRST {
        // Some initialization functions must be run in the context
        // of a regular process (they call sleep), and thus cannot
        // be run from main().
        FORKRET_FIRST = false;
        iinit(param::ROOTDEV);
        initlog(param::ROOTDEV);
    }

    // Return to "caller", actually trapret (see allocproc).
}

// ============================================================================
// sleep — atomically release lock and sleep on channel.
// Reacquires lock when awakened.
// ============================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sleep_proc(chan: *mut c_void, lk: *mut spinlock) {
    let p = my_proc();

    if lk.is_null() {
        panic!("sleep without lk");
    }

    // Must acquire ptable.lock in order to change p->state and then
    // call sched. Once we hold ptable.lock, we can be guaranteed
    // that we won't miss any wakeup.
    if lk != &raw mut ptable.lock {
        acquire(&raw mut ptable.lock);
        release(lk);
    }

    // Go to sleep.
    p.chan = chan;
    p.state = Procstate::SLEEPING;
    sched();

    // Tidy up.
    p.chan = ptr::null_mut();

    // Reacquire original lock.
    if lk != &raw mut ptable.lock {
        release(&raw mut ptable.lock);
        acquire(lk);
    }
}

// ============================================================================
// wakeup1 — wake up all processes sleeping on chan.
// The ptable lock must be held.
// ============================================================================

unsafe fn wakeup1(chan: *mut c_void) {
    for i in 0..NPROC as usize {
        let p = &raw mut ptable.proc_table[i];
        if (*p).state == Procstate::SLEEPING && (*p).chan == chan {
            (*p).state = Procstate::RUNNABLE;
        }
    }
}

// ============================================================================
// wakeup — wake up all processes sleeping on chan (acquires ptable.lock).
// ============================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wakeup(chan: *mut c_void) {
    acquire(&raw mut ptable.lock);
    wakeup1(chan);
    release(&raw mut ptable.lock);
}

// ============================================================================
// kill — kill the process with the given pid.
// ============================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kill(pid: i32) -> i32 {
    acquire(&raw mut ptable.lock);
    for i in 0..NPROC as usize {
        let p = &raw mut ptable.proc_table[i];
        if (*p).pid == pid {
            (*p).killed = 1;
            // Wake process from sleep if necessary.
            if (*p).state == Procstate::SLEEPING {
                (*p).state = Procstate::RUNNABLE;
            }
            release(&raw mut ptable.lock);
            return 0;
        }
    }
    release(&raw mut ptable.lock);
    -1
}

// ============================================================================
// procdump — print a process listing to the console. For debugging.
// ============================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn procdump() {
    let states: [&[u8]; 6] = [
        b"unused\0",
        b"embryo\0",
        b"sleep \0",
        b"runble\0",
        b"run   \0",
        b"zombie\0",
    ];

    for i in 0..NPROC as usize {
        let p = &ptable.proc_table[i];
        if p.state == Procstate::UNUSED {
            continue;
        }
        let state_idx = p.state as usize;
        let _state = if state_idx < 6 {
            states[state_idx]
        } else {
            b"???\0"
        };

        // In a full port this would call:
        // cprintf("%d %s %s", p.pid, state, p.name);
        // For now, we leave a TODO — the cprintf infrastructure
        // will be wired up when the console crate is integrated.

        if p.state == Procstate::SLEEPING {
            let mut pcs: [addr_t; 10] = [0; 10];
            getstackpcs(
                ((*p.context).rbp as *const addr_t).add(2),
                pcs.as_mut_ptr(),
            );
            // Would print PCs here.
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Copy at most `n - 1` bytes from `src` into `dst`, NUL-terminating.
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

/// Set the per-CPU current process pointer.
/// In the C version this is `proc = p;` which writes to the TLS slot.
/// Here we write to fs:[-8].
#[inline(always)]
unsafe fn set_my_proc(p: *mut proc_struct) {
    core::arch::asm!(
        "mov qword ptr fs:[-8], {0}",
        in(reg) p,
        options(nostack, preserves_flags),
    );
}
