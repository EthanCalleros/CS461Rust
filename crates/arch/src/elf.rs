#![allow(non_camel_case_types)]

use types::{addr_t, uchar, uint, uint32, uint64, ushort};

const ELF_MAGIC: uint = 0x464C457F;

const ELF_PROG_LOAD:       i32 = 1;

const ELF_PROG_FLAG_EXEC:  i32 = 1;
const ELF_PROG_FLAG_WRITE: i32 = 2;
const ELF_PROG_FLAG_READ:  i32 = 4;

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
    r#type: uint32,
    flags:  uint64,
    off:    uint64,
    vaddr:  uint64,
    paddr:  uint64,
    filesz: uint64,
    memsz:  uint64,
    align:  uint64,
}