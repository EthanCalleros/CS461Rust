#![allow(non_camel_case_types)]

pub type uint   = u32;
pub type ushort = u16;
pub type uchar  = u8;

pub type int64  = i64;
pub type uint32 = u32;
pub type uint64 = u64;

pub type addr_t = u64;

pub type pde_t   = addr_t; // Page Directory Entry    
pub type pml4e_t = addr_t; // Page-Map Level-4 Entry 
pub type pdpe_t  = addr_t; // Page-Directory Pointer Entry 
