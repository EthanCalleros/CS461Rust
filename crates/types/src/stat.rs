//! Port of `stat.h` — file metadata returned by the `stat`/`fstat`
//! syscalls, plus inode-type constants.

#![allow(non_camel_case_types)]

use crate::types::uint;

// File-type constants for `stat::r#type`.
pub const T_DIR:  i32 = 1;
pub const T_FILE: i32 = 2;
pub const T_DEV:  i32 = 3;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct stat {
    pub r#type: i16,
    pub dev:    i32,
    pub ino:    uint,
    pub nlink:  i16,
    pub size:   uint,
}
