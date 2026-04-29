// Arch build script.
//
// xv6's bootloader and trampolines are GNU-assembler (GAS) syntax `.S`
// files and target i386/x86_64 ELF. They cannot be assembled by MSVC's
// `cl.exe`, which is what the `cc` crate auto-selects on a stock
// Windows host. Until a cross-compilation toolchain (e.g. an
// `i686-elf-gcc` / `x86_64-elf-gcc` from MSYS2 or a Linux/WSL build
// environment) is wired up, this script is a no-op and the kernel will
// not link.
//
// Two viable paths forward:
//
//   1. Embed the assembly directly into Rust source via the
//      `core::arch::global_asm!` macro. This is the most idiomatic
//      Rust-kernel approach and removes the need for `cc` entirely.
//
//   2. Detect a cross GCC at configure time (e.g. via `CC` env var or
//      probing `x86_64-elf-gcc` on PATH) and invoke it explicitly
//      instead of letting `cc` pick MSVC.
//
// For now we just track the asm files so a future change re-runs this
// script.
fn main() {
    for f in [
        "src/asm/bootasm.S",
        "src/asm/entryother.S",
        "src/asm/entry.S",
        "src/asm/swtch.S",
        "src/asm/trapasm.S",
        "src/asm/initcode.S",
        "src/asm/vectors.S",
        "src/asm/usys.S",
    ] {
        println!("cargo:rerun-if-changed={f}");
    }
}
