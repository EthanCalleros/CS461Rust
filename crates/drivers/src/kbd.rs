use crate::lib::{inb}; // Your port I/O helpers

// PC keyboard interface constants (from kbd.h)
const KBSTATP: u16 = 0x64; // Status port
const KBDATAP: u16 = 0x60; // Data port
const KBS_DIB: u8  = 0x01; // Data in buffer bit

// Modifier bits
const SHIFT: u32     = 1 << 0;
const CTL: u32       = 1 << 1;
const ALT: u32       = 1 << 2;
const CAPSLOCK: u32  = 1 << 3;
const NUMLOCK: u32   = 1 << 4;
const SCROLLLOCK: u32 = 1 << 5;
const E0ESC: u32      = 1 << 6;

// Special keycodes
const KEY_HOME: u8 = 0xE0;
const KEY_END:  u8 = 0xE1;
const KEY_UP:   u8 = 0xE2;
const KEY_DN:   u8 = 0xE3;
const KEY_LF:   u8 = 0xE4;
const KEY_RT:   u8 = 0xE5;
const KEY_PGUP: u8 = 0xE6;
const KEY_PGDN: u8 = 0xE7;
const KEY_INS:  u8 = 0xE8;
const KEY_DEL:  u8 = 0xE9;

const NO: u8 = 0;

// C macro equivalent in Rust
const fn c(x: char) -> u8 {
    (x as u8).wrapping_sub(b'@')
}

// Maps
static SHIFTCODE: [u32; 256] = {
    let mut m = [0u32; 256];
    m[0x1D] = CTL;
    m[0x2A] = SHIFT;
    m[0x36] = SHIFT;
    m[0x38] = ALT;
    m[0x9D] = CTL;
    m[0xB8] = ALT;
    m
};

static TOGGLECODE: [u32; 256] = {
    let mut m = [0u32; 256];
    m[0x3A] = CAPSLOCK;
    m[0x45] = NUMLOCK;
    m[0x46] = SCROLLLOCK;
    m
};

static NORMALMAP: [u8; 256] = [
    NO, 0x1B, b'1', b'2', b'3', b'4', b'5', b'6', // 0x00
    b'7', b'8', b'9', b'0', b'-', b'=', b'\x08', b'\t',
    b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i', // 0x10
    b'o', b'p', b'[', b']', b'\n', NO, b'a', b's',
    b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';', // 0x20
    b'\'', b'`', NO, b'\\', b'z', b'x', b'c', b'v',
    b'b', b'n', b'm', b',', b'.', b'/', NO, b'*', // 0x30
    NO, b' ', NO, NO, NO, NO, NO, NO,
    NO, NO, NO, NO, NO, NO, NO, b'7', // 0x40
    b'8', b'9', b'-', b'4', b'5', b'6', b'+', b'1',
    b'2', b'3', b'0', b'.', NO, NO, NO, NO, // 0x50
    // Remainder filled with NO or specific entries...
    // Note: Rust arrays must be full size. 
    // You can use a build script or manual entry for the 0x9C, etc indices.
    .. [NO; 256] // Simplified for brevity
];

// State storage
static mut SHIFT_STATE: u32 = 0;

pub unsafe fn kbdgetc() -> i32 {
    let st = inb(KBSTATP);
    if (st & KBS_DIB) == 0 {
        return -1;
    }
    let mut data = inb(KBDATAP) as usize;

    if data == 0xE0 {
        SHIFT_STATE |= E0ESC;
        return 0;
    } else if (data & 0x80) != 0 {
        // Key released
        let data_idx = if (SHIFT_STATE & E0ESC) != 0 { data } else { data & 0x7F };
        SHIFT_STATE &= !(SHIFTCODE[data_idx] | E0ESC);
        return 0;
    } else if (SHIFT_STATE & E0ESC) != 0 {
        // Last char was E0 escape
        data |= 0x80;
        SHIFT_STATE &= !E0ESC;
    }

    SHIFT_STATE |= SHIFTCODE[data];
    SHIFT_STATE ^= TOGGLECODE[data];

    let maps: [&[u8; 256]; 4] = [&NORMALMAP, &SHIFT_MAP, &CTL_MAP, &CTL_MAP];
    let mut c = maps[(SHIFT_STATE & (CTL | SHIFT)) as usize][data];

    if (SHIFT_STATE & CAPSLOCK) != 0 {
        if c >= b'a' && c <= b'z' {
            c -= b'a' - b'A';
        } else if c >= b'A' && c <= b'Z' {
            c += b'a' - b'A';
        }
    }

    c as i32
}

pub unsafe fn kbdintr() {
    extern "C" {
        fn consoleintr(getc: unsafe fn() -> i32);
    }
    consoleintr(kbdgetc);
}
