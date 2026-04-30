#![allow(non_camel_case_types)]

use crate::types::uint;

const T_DIR: i32  = 1;
const T_FILE: i32 = 2;
const T_DEV: i32  = 3;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]

pub struct stat {
    pub r#type: i16,
    pub dev:   i32,
    pub ino:   uint,
    pub nlink: i16,
    pub size:  uint,
}