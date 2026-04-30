#![no_std]

use core::fmt::{self, Write};
use sync::spinlock::Spinlock;
use arch::registers::{outb, inb};
use mm::memlayout::P2V;
use drivers::uart::uartputc;

// Constants
const CRTPORT: u16 = 0x3d4;
const BACKSPACE: i32 = 0x100;
const INPUT_BUF_SIZE: usize = 128;

// CGA memory: 0xb8000
static mut CRT: *mut u16 = P2V(0xb8000) as *mut u16;

struct Cons {
    locking: bool,
}

static mut CONS: Cons = Cons { locking: true };
static CONS_LOCK: Spinlock<()> = Spinlock::new((), "console");

// --- CGA (Screen) Output ---

unsafe fn cgaputc(c: i32) {
    // Get cursor position
    outb(CRTPORT, 14);
    let mut pos = (inb(CRTPORT + 1) as usize) << 8;
    outb(CRTPORT, 15);
    pos |= inb(CRTPORT + 1) as usize;

    if c == b'\n' as i32 {
        pos += 80 - pos % 80;
    } else if c == BACKSPACE {
        if pos > 0 { pos -= 1; }
    } else {
        *CRT.add(pos) = (c as u16 & 0xff) | 0x0700; // gray on black
        pos += 1;
    }

    if (pos / 80) >= 24 { // Scroll
        core::ptr::copy(CRT.add(80), CRT, 23 * 80);
        pos -= 80;
        core::ptr::write_bytes(CRT.add(pos), 0, 24 * 80 - pos);
    }

    outb(CRTPORT, 14);
    outb(CRTPORT + 1, (pos >> 8) as u8);
    outb(CRTPORT, 15);
    outb(CRTPORT + 1, (pos & 0xff) as u8);
    *CRT.add(pos) = (b' ' as u16) | 0x0700;
}

pub unsafe fn consputc(c: i32) {
    if c == BACKSPACE {
        uartputc(b'\x08' as i32);
        uartputc(b' ' as i32);
        uartputc(b'\x08' as i32);
    } else {
        uartputc(c);
    }
    cgaputc(c);
}

// --- Rust fmt integration ---

pub struct ConsoleWriter;

impl Write for ConsoleWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            unsafe { consputc(b as i32) };
        }
        Ok(())
    }
}

/// The Rust version of cprintf
#[no_mangle]
pub unsafe fn cprintf(args: fmt::Arguments) {
    let locking = CONS.locking;
    if locking {
        let _guard = CONS_LOCK.acquire();
        let mut writer = ConsoleWriter;
        let _ = writer.write_fmt(args);
    } else {
        let mut writer = ConsoleWriter;
        let _ = writer.write_fmt(args);
    }
}

// Macro helper for print! usage
#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => {
        unsafe { $crate::cprintf(format_args!($($arg)*)) };
        unsafe { $crate::consputc(b'\n' as i32) };
    };
}

// --- Input Handling ---

struct Input {
    buf: [u8; INPUT_BUF_SIZE],
    r: usize, // Read
    w: usize, // Write
    e: usize, // Edit
}

static mut INPUT: Input = Input {
    buf: [0; INPUT_BUF_SIZE],
    r: 0, w: 0, e: 0,
};
static INPUT_LOCK: Spinlock<()> = Spinlock::new((), "input");

#[no_mangle]
pub unsafe fn consoleintr(getc: unsafe fn() -> i32) {
    let _guard = INPUT_LOCK.acquire();
    
    while let c = getc() {
        if c < 0 { break; }
        
        match c {
            // Control keys
            3 => { // Ctrl-C
                // extern "C" { fn kill(pid: i32, sig: i32); static fgpid: i32; }
                // kill(fgpid, 2);
            }
            8 | 127 => { // Backspace
                if INPUT.e != INPUT.w {
                    INPUT.e -= 1;
                    consputc(BACKSPACE);
                }
            }
            _ => {
                if c != 0 && INPUT.e.wrapping_sub(INPUT.r) < INPUT_BUF_SIZE {
                    let ch = if c == b'\r' as i32 { b'\n' } else { c as u8 };
                    INPUT.buf[INPUT.e % INPUT_BUF_SIZE] = ch;
                    INPUT.e += 1;
                    consputc(ch as i32);
                    
                    if ch == b'\n' || ch == 4 || INPUT.e == INPUT.r + INPUT_BUF_SIZE {
                        INPUT.w = INPUT.e;
                        // wakeup(&INPUT.r);
                    }
                }
            }
        }
    }
}
