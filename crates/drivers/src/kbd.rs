use arch::registers::inb;

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

// C(x) macro equivalent
const fn ctrl(x: u8) -> u8 {
    x.wrapping_sub(b'@')
}

// Shift code map — which modifier each scan code activates
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

// Toggle code map — which toggle each scan code flips
static TOGGLECODE: [u32; 256] = {
    let mut m = [0u32; 256];
    m[0x3A] = CAPSLOCK;
    m[0x45] = NUMLOCK;
    m[0x46] = SCROLLLOCK;
    m
};

// Normal (unshifted) keymap
static NORMALMAP: [u8; 256] = {
    let mut m = [NO; 256];
    // Row 0x00
    m[0x01] = 0x1B; // Esc
    m[0x02] = b'1'; m[0x03] = b'2'; m[0x04] = b'3'; m[0x05] = b'4';
    m[0x06] = b'5'; m[0x07] = b'6'; m[0x08] = b'7'; m[0x09] = b'8';
    m[0x0A] = b'9'; m[0x0B] = b'0'; m[0x0C] = b'-'; m[0x0D] = b'=';
    m[0x0E] = 0x08; // Backspace
    m[0x0F] = b'\t';
    // Row 0x10
    m[0x10] = b'q'; m[0x11] = b'w'; m[0x12] = b'e'; m[0x13] = b'r';
    m[0x14] = b't'; m[0x15] = b'y'; m[0x16] = b'u'; m[0x17] = b'i';
    m[0x18] = b'o'; m[0x19] = b'p'; m[0x1A] = b'['; m[0x1B] = b']';
    m[0x1C] = b'\n';
    // Row 0x1E (skip 0x1D = Ctrl)
    m[0x1E] = b'a'; m[0x1F] = b's';
    // Row 0x20
    m[0x20] = b'd'; m[0x21] = b'f'; m[0x22] = b'g'; m[0x23] = b'h';
    m[0x24] = b'j'; m[0x25] = b'k'; m[0x26] = b'l'; m[0x27] = b';';
    m[0x28] = b'\''; m[0x29] = b'`';
    // Row 0x2B (skip 0x2A = LShift)
    m[0x2B] = b'\\';
    m[0x2C] = b'z'; m[0x2D] = b'x'; m[0x2E] = b'c'; m[0x2F] = b'v';
    // Row 0x30
    m[0x30] = b'b'; m[0x31] = b'n'; m[0x32] = b'm'; m[0x33] = b',';
    m[0x34] = b'.'; m[0x35] = b'/';
    // 0x36 = RShift (no char)
    m[0x37] = b'*'; // Keypad *
    // 0x38 = Alt (no char)
    m[0x39] = b' '; // Space
    // Keypad numbers
    m[0x47] = b'7'; m[0x48] = b'8'; m[0x49] = b'9'; m[0x4A] = b'-';
    m[0x4B] = b'4'; m[0x4C] = b'5'; m[0x4D] = b'6'; m[0x4E] = b'+';
    m[0x4F] = b'1'; m[0x50] = b'2'; m[0x51] = b'3'; m[0x52] = b'0';
    m[0x53] = b'.';
    // Extended keys (0x80 | original scancode after E0 prefix)
    m[0x9C] = b'\n'; // Keypad Enter
    m[0xB5] = b'/';  // Keypad /
    m[0xC8] = KEY_UP;
    m[0xD0] = KEY_DN;
    m[0xC9] = KEY_PGUP;
    m[0xD1] = KEY_PGDN;
    m[0xCB] = KEY_LF;
    m[0xCD] = KEY_RT;
    m[0x97] = KEY_HOME;
    m[0xCF] = KEY_END;
    m[0xD2] = KEY_INS;
    m[0xD3] = KEY_DEL;
    m
};

// Shifted keymap
static SHIFTMAP: [u8; 256] = {
    let mut m = [NO; 256];
    m[0x01] = 0x1B; // Esc
    m[0x02] = b'!'; m[0x03] = b'@'; m[0x04] = b'#'; m[0x05] = b'$';
    m[0x06] = b'%'; m[0x07] = b'^'; m[0x08] = b'&'; m[0x09] = b'*';
    m[0x0A] = b'('; m[0x0B] = b')'; m[0x0C] = b'_'; m[0x0D] = b'+';
    m[0x0E] = 0x08; // Backspace
    m[0x0F] = b'\t';
    m[0x10] = b'Q'; m[0x11] = b'W'; m[0x12] = b'E'; m[0x13] = b'R';
    m[0x14] = b'T'; m[0x15] = b'Y'; m[0x16] = b'U'; m[0x17] = b'I';
    m[0x18] = b'O'; m[0x19] = b'P'; m[0x1A] = b'{'; m[0x1B] = b'}';
    m[0x1C] = b'\n';
    m[0x1E] = b'A'; m[0x1F] = b'S';
    m[0x20] = b'D'; m[0x21] = b'F'; m[0x22] = b'G'; m[0x23] = b'H';
    m[0x24] = b'J'; m[0x25] = b'K'; m[0x26] = b'L'; m[0x27] = b':';
    m[0x28] = b'"'; m[0x29] = b'~';
    m[0x2B] = b'|';
    m[0x2C] = b'Z'; m[0x2D] = b'X'; m[0x2E] = b'C'; m[0x2F] = b'V';
    m[0x30] = b'B'; m[0x31] = b'N'; m[0x32] = b'M'; m[0x33] = b'<';
    m[0x34] = b'>'; m[0x35] = b'?';
    m[0x37] = b'*';
    m[0x39] = b' ';
    m[0x47] = b'7'; m[0x48] = b'8'; m[0x49] = b'9'; m[0x4A] = b'-';
    m[0x4B] = b'4'; m[0x4C] = b'5'; m[0x4D] = b'6'; m[0x4E] = b'+';
    m[0x4F] = b'1'; m[0x50] = b'2'; m[0x51] = b'3'; m[0x52] = b'0';
    m[0x53] = b'.';
    m[0x9C] = b'\n';
    m[0xB5] = b'/';
    m[0xC8] = KEY_UP;
    m[0xD0] = KEY_DN;
    m[0xC9] = KEY_PGUP;
    m[0xD1] = KEY_PGDN;
    m[0xCB] = KEY_LF;
    m[0xCD] = KEY_RT;
    m[0x97] = KEY_HOME;
    m[0xCF] = KEY_END;
    m[0xD2] = KEY_INS;
    m[0xD3] = KEY_DEL;
    m
};

// Control keymap (Ctrl held)
static CTLMAP: [u8; 256] = {
    let mut m = [NO; 256];
    m[0x1C] = b'\n'; // Ctrl+Enter
    m[0x1E] = ctrl(b'A'); m[0x1F] = ctrl(b'S');
    m[0x20] = ctrl(b'D'); m[0x21] = ctrl(b'F'); m[0x22] = ctrl(b'G');
    m[0x23] = ctrl(b'H'); m[0x24] = ctrl(b'J'); m[0x25] = ctrl(b'K');
    m[0x26] = ctrl(b'L');
    m[0x2C] = ctrl(b'Z'); m[0x2D] = ctrl(b'X'); m[0x2E] = ctrl(b'C');
    m[0x2F] = ctrl(b'V');
    m[0x30] = ctrl(b'B'); m[0x31] = ctrl(b'N'); m[0x32] = ctrl(b'M');
    m[0x10] = ctrl(b'Q'); m[0x11] = ctrl(b'W'); m[0x12] = ctrl(b'E');
    m[0x13] = ctrl(b'R'); m[0x14] = ctrl(b'T'); m[0x15] = ctrl(b'Y');
    m[0x16] = ctrl(b'U'); m[0x17] = ctrl(b'I'); m[0x18] = ctrl(b'O');
    m[0x19] = ctrl(b'P');
    m[0x39] = b' ';
    m[0x9C] = b'\n';
    m[0xB5] = ctrl(b'/');
    m[0xC8] = KEY_UP;
    m[0xD0] = KEY_DN;
    m[0xC9] = KEY_PGUP;
    m[0xD1] = KEY_PGDN;
    m[0xCB] = KEY_LF;
    m[0xCD] = KEY_RT;
    m[0x97] = KEY_HOME;
    m[0xCF] = KEY_END;
    m[0xD2] = KEY_INS;
    m[0xD3] = KEY_DEL;
    m
};

// State storage
static mut SHIFT_STATE: u32 = 0;

#[allow(static_mut_refs)]
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

    let maps: [&[u8; 256]; 4] = [&NORMALMAP, &SHIFTMAP, &CTLMAP, &CTLMAP];
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
    unsafe extern "C" {
        unsafe fn consoleintr(getc: unsafe fn() -> i32);
    }
    consoleintr(kbdgetc);
}
