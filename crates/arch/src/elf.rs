#![allow(non_camel_case_types)]

use types::{addr_t, uchar, uint, uint32, uint64, ushort};

pub const ELF_MAGIC: uint = 0x464C457F;

pub const ELF_PROG_LOAD:       i32 = 1;

pub const ELF_PROG_FLAG_EXEC:  i32 = 1;
pub const ELF_PROG_FLAG_WRITE: i32 = 2;
pub const ELF_PROG_FLAG_READ:  i32 = 4;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct elfhdr {
    pub magic:     uint,
    pub elf:       uchar,
    pub r#type:    ushort,
    pub machine:   ushort,
    pub version:   uint,
    pub entry:     addr_t,
    pub phoff:     addr_t,
    pub shoff:     addr_t,
    pub flags:     uint,
    pub ehsize:    ushort,
    pub phentsize: ushort,
    pub phnum:     ushort,
    pub shentsize: ushort,
    pub shnum:     ushort,
    pub shstrndx:  ushort,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct proghdr {
    pub r#type: uint32,
    pub flags:  uint64,
    pub off:    uint64,
    pub vaddr:  uint64,
    pub paddr:  uint64,
    pub filesz: uint64,
    pub memsz:  uint64,
    pub align:  uint64,
}