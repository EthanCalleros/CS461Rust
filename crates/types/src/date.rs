#![allow(non_camel_case_types)]

use crate::types::uint;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct rtcdate {
    pub second: uint,
    pub minute: uint,
    pub hour:   uint,
    pub day:    uint,
    pub month:  uint,
    pub year:   uint,
}
