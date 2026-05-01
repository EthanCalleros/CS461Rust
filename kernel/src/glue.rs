//! C-compatible symbol shims ("glue").
//!
//! Several crates in this workspace declare external functions via
//! `extern "C" { fn foo(); }`. This module provides the `#[no_mangle]`
//! definitions those declarations resolve to at link time.
//!
//! As individual crates are converted to use Rust-native cross-crate
//! calls, these shims can be removed one at a time.

#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(static_mut_refs)]

use sync::spinlockh::spinlock;

// =====================================================================
// Spinlock shims (sync crate has Rust API, but proc/fs use extern "C")
// =====================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn initlock(lk: *mut spinlock, name: *const u8) {
    if !lk.is_null() {
        (*lk).locked = 0;
        (*lk).name = name;
        (*lk).cpu = core::ptr::null_mut();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn acquire(lk: *mut spinlock) {
    // TODO: real xchg-based spinlock + pushcli
    // For single-CPU bring-up, this is a no-op.
    pushcli();
    if !lk.is_null() {
        // Spin until locked == 0, then set to 1 (atomically)
        // For now, just set it:
        (*lk).locked = 1;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn release(lk: *mut spinlock) {
    // TODO: real release + popcli
    if !lk.is_null() {
        (*lk).locked = 0;
    }
    popcli();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn holding(lk: *mut spinlock) -> i32 {
    if !lk.is_null() && (*lk).locked != 0 {
        1
    } else {
        0
    }
}

// =====================================================================
// CLI/STI helpers (sync)
// =====================================================================

// Per-CPU interrupt disable nesting counter (single CPU for now)
static mut CLI_DEPTH: i32 = 0;
static mut INT_ENABLED: bool = false;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pushcli() {
    let eflags: u64;
    core::arch::asm!("pushfq; pop {}", out(reg) eflags);
    core::arch::asm!("cli");
    if CLI_DEPTH == 0 {
        INT_ENABLED = (eflags & 0x200) != 0; // IF flag
    }
    CLI_DEPTH += 1;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn popcli() {
    CLI_DEPTH -= 1;
    if CLI_DEPTH == 0 && INT_ENABLED {
        core::arch::asm!("sti");
    }
}

// =====================================================================
// Memory allocator shims (mm crate uses Rust paths, extern "C" callers
// need flat symbol names)
// =====================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kalloc() -> *mut u8 {
    mm::kalloc::kalloc()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kfree(v: *mut u8) {
    mm::kalloc::kfree(v)
}

// =====================================================================
// cpunum (from LAPIC — provides current CPU ID)
// =====================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cpunum() -> i32 {
    // Read LAPIC ID register. For single-CPU bring-up, return 0.
    // TODO: implement properly via LAPIC MMIO
    0
}

// =====================================================================
// Subsystem init stubs (not yet fully ported)
// =====================================================================

/// Legacy 8259 PIC: mask all interrupts (we use APIC instead).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn picinit() {
    // Mask all IRQs on both PICs
    arch::registers::outb(0x21, 0xFF);
    arch::registers::outb(0xA1, 0xFF);
}

/// UART serial port init.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn uartinit() {
    drivers::uart::uartinit();
}

/// Console init — register console read/write in DEVSW and init the lock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn consoleinit() {
    // CONSOLE = 1 (from fs::file)
    fs::file::DEVSW[1].read  = Some(consoleread);
    fs::file::DEVSW[1].write = Some(consolewrite);
    // The console spinlock is managed by the console crate's Spinlock<()>.
}

/// consoleread — read from the console input buffer.
/// Matches the Devsw read signature: fn(ip, off, dst, n) -> i32
#[unsafe(no_mangle)]
pub unsafe extern "C" fn consoleread(
    _ip: *mut fs::fs::Inode, _off: types::uint, dst: *mut u8, n: i32,
) -> i32 {
    // Minimal implementation: read from UART.
    // A full implementation would read from the console input buffer
    // with sleep/wakeup. For initial bring-up, poll UART.
    let mut i = 0i32;
    while i < n {
        let c = drivers::uart::uartgetc();
        if c < 0 {
            break;
        }
        *dst.add(i as usize) = c as u8;
        i += 1;
        if c == b'\n' as i32 {
            break;
        }
    }
    i
}

/// consolewrite — write to the console (CGA + UART).
/// Matches the Devsw write signature: fn(ip, off, src, n) -> i32
#[unsafe(no_mangle)]
pub unsafe extern "C" fn consolewrite(
    _ip: *mut fs::fs::Inode, _off: types::uint, src: *mut u8, n: i32,
) -> i32 {
    for i in 0..n as usize {
        console::consputc(*src.add(i) as i32);
    }
    n
}

// =====================================================================
// Binary blobs (initcode.S, entryother.S).
//
// These are assembled by arch/build.rs into libxv6asm.a, which
// provides the _binary_initcode_start/size and
// _binary_entryother_start/size symbols. The arch crate links
// libxv6asm.a via `cargo:rustc-link-lib=static=xv6asm`.
//
// If build.rs can't find gcc, the build will fail with a clear
// warning — no placeholder symbols are provided here because they
// would conflict with the real ones when gcc IS available.
// =====================================================================

// =====================================================================
// Additional missing symbols
// =====================================================================

/// sleep(chan, lock) — put process to sleep on channel.
/// Delegates to proc::proc::sleep_proc which is the real implementation.
/// Cast through raw pointer because proc has its own spinlock type with
/// identical layout to sync::spinlockh::spinlock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sleep(chan: *const core::ffi::c_void, lk: *mut spinlock) {
    proc::proc::sleep_proc(
        chan as *mut core::ffi::c_void,
        lk as *mut _ as *mut proc::proc::spinlock,
    );
}

// NOTE: wakeup is provided by proc::proc::wakeup (#[no_mangle])
// Do not duplicate it here.

/// iderw — IDE read/write (called from bio.rs)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iderw(b: *mut u8) {
    drivers::ide::iderw(b as *mut _);
}

/// ideintr — IDE interrupt handler
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ideintr() {
    drivers::ide::ideintr();
}

/// kbdintr — keyboard interrupt handler
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kbdintr() {
    drivers::kbd::kbdintr();
}

/// uartintr — UART interrupt handler
#[unsafe(no_mangle)]
pub unsafe extern "C" fn uartintr() {
    drivers::uart::uartintr();
}

/// my_proc_killed — check if current process is killed
#[unsafe(no_mangle)]
pub unsafe extern "C" fn my_proc_killed() -> i32 {
    proc::proch::my_proc().killed
}

/// my_proc_cwd — get current process's working directory inode
#[unsafe(no_mangle)]
pub unsafe extern "C" fn my_proc_cwd() -> *mut u8 {
    proc::proch::my_proc().cwd as *mut u8
}

// =====================================================================
// Subsystem init wrappers (called from main.rs via glue::)
// =====================================================================

/// IDE disk init.
pub unsafe fn ideinit() {
    drivers::ide::ideinit();
}

/// Buffer cache init.
pub unsafe fn binit() {
    // Already exported by fs crate with #[no_mangle]
    unsafe extern "C" { unsafe fn binit(); }
    binit();
}

/// File table init.
pub unsafe fn fileinit() {
    fs::file::fileinit();
}

/// Process table init.
pub unsafe fn pinit() {
    proc::proc::pinit();
}

/// Create first user process.
pub unsafe fn userinit() {
    proc::proc::userinit();
}

/// Scheduler — never returns.
pub unsafe fn scheduler() -> ! {
    proc::proc::scheduler();
}

// =====================================================================
// Memory helpers
// =====================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(dst: *mut u8, src: *const u8, n: u32) -> *mut u8 {
    core::ptr::copy(src, dst, n as usize);
    dst
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(dst: *mut u8, c: i32, n: u32) -> *mut u8 {
    core::ptr::write_bytes(dst, c as u8, n as usize);
    dst
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(s1: *const u8, s2: *const u8, n: u32) -> i32 {
    for i in 0..n as usize {
        let a = *s1.add(i);
        let b = *s2.add(i);
        if a != b {
            return a as i32 - b as i32;
        }
    }
    0
}

// =====================================================================
// Process-related shims
// =====================================================================

/// exit_process — xv6's exit(), renamed because `exit` collides with
/// Rust keywords / other symbols. Called from trap handler.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn exit_process() {
    proc::proc::exit();
}

/// forkret_entry — declared in proc.rs but unused (proc.rs uses
/// `forkret` directly). Provide a trampoline to satisfy the linker.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn forkret_entry() {
    proc::proc::forkret();
}

/// getstackpcs — walk the frame-pointer chain to collect return PCs.
/// Used by procdump() for debugging stack traces.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getstackpcs(rbp: *const u64, pcs: *mut u64) {
    let mut bp = rbp;
    for i in 0..10usize {
        if bp.is_null() || (bp as usize) < 0x1000 || (bp as usize) >= 0xFFFF_FFFF_FFFF_0000 {
            *pcs.add(i) = 0;
            return;
        }
        // pc = return address = *(rbp + 1)
        *pcs.add(i) = *bp.add(1);
        // walk to caller's frame
        bp = *bp as *const u64;
    }
}

/// cprintf_str — print a NUL-terminated string to the console.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cprintf_str(s: *const u8) {
    if s.is_null() {
        return;
    }
    let mut p = s;
    while *p != 0 {
        drivers::uart::uartputc(*p as i32);
        p = p.add(1);
    }
}

