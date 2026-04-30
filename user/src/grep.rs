#![no_std]
#![no_main]

// Simple grep. Only supports ^ . * $ operators.

use ulib::*;

fn grep(pattern: *const u8, fd: i32) {
    let mut buf = [0u8; 1024];
    let mut m: usize = 0;

    loop {
        let n = read_raw(fd, unsafe { buf.as_mut_ptr().add(m) }, buf.len() - m - 1);
        if n <= 0 {
            break;
        }
        m += n as usize;
        buf[m] = 0;

        let mut p: usize = 0;
        while p < m {
            // Find newline
            let mut q = p;
            while q < m && buf[q] != b'\n' {
                q += 1;
            }
            if q >= m {
                break;
            }
            // Temporarily null-terminate the line
            buf[q] = 0;
            if match_pattern(pattern, buf[p..].as_ptr()) {
                buf[q] = b'\n';
                write(1, &buf[p..q + 1]);
            } else {
                buf[q] = b'\n';
            }
            p = q + 1;
        }

        if p == 0 {
            m = 0;
        } else if m > 0 {
            // Move remaining data to beginning
            let remaining = m - p;
            for i in 0..remaining {
                buf[i] = buf[p + i];
            }
            m = remaining;
        }
    }
}

// Regexp matcher from Kernighan & Pike,
// The Practice of Programming, Chapter 9.

fn match_pattern(re: *const u8, text: *const u8) -> bool {
    unsafe {
        if *re == b'^' {
            return matchhere(re.add(1), text);
        }
        let mut t = text;
        loop {
            if matchhere(re, t) {
                return true;
            }
            if *t == 0 {
                break;
            }
            t = t.add(1);
        }
        false
    }
}

/// Search for re at beginning of text.
unsafe fn matchhere(re: *const u8, text: *const u8) -> bool {
    if *re == 0 {
        return true;
    }
    if *re.add(1) == b'*' {
        return matchstar(*re, re.add(2), text);
    }
    if *re == b'$' && *re.add(1) == 0 {
        return *text == 0;
    }
    if *text != 0 && (*re == b'.' || *re == *text) {
        return matchhere(re.add(1), text.add(1));
    }
    false
}

/// Search for c*re at beginning of text.
unsafe fn matchstar(c: u8, re: *const u8, text: *const u8) -> bool {
    let mut t = text;
    loop {
        if matchhere(re, t) {
            return true;
        }
        if *t == 0 || (*t != c && c != b'.') {
            break;
        }
        t = t.add(1);
    }
    false
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> ! {
    if argc <= 1 {
        printf!(2, "usage: grep pattern [file ...]\n");
        exit();
    }

    unsafe {
        let pattern = *argv.add(1);

        if argc <= 2 {
            grep(pattern, 0);
            exit();
        }

        for i in 2..argc {
            let arg = *argv.add(i as usize);
            let name = core::slice::from_raw_parts(arg, strlen(arg) + 1);
            let fd = open(name, O_RDONLY);
            if fd < 0 {
                printf!(1, "grep: cannot open file\n");
                exit();
            }
            grep(pattern, fd);
            close(fd);
        }
    }
    exit();
}
