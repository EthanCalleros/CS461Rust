use crate::write; // The syscall from lib.rs
use core::fmt::{self, Write};

/// A simple wrapper around the write() syscall to support Rust's formatting
pub struct FdWriter {
    pub fd: i32,
}

impl Write for FdWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        unsafe {
            if write(self.fd, bytes.as_ptr() as *const core::ffi::c_void, bytes.len() as i32) != bytes.len() as i32 {
                return Err(fmt::Error);
            }
        }
        Ok(())
    }
}

/// The Rust version of printf. 
/// Usage: printf(1, format_args!("Hello {}, pid {}\n", "world", getpid()));
pub fn printf(fd: i32, args: fmt::Arguments) {
    let mut writer = FdWriter { fd };
    let _ = writer.write_fmt(args);
}

/// Macro to make user-space printing look like standard Rust
#[macro_export]
macro_rules! print {
    ($fd:expr, $($arg:tt)*) => {
        $crate::printf::printf($fd, format_args!($($arg)*));
    };
}

/// Helper for the common case of printing to stdout
#[macro_export]
macro_rules! fprintf {
    (1, $($arg:tt)*) => {
        $crate::printf::printf(1, format_args!($($arg)*));
    };
    (2, $($arg:tt)*) => {
        $crate::printf::printf(2, format_args!($($arg)*));
    };
}
