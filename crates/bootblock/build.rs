//! Build script for the bootblock crate.
//!
//! Bootblock is the second-stage bootloader (port of `bootmain.c` +
//! `bootasm.S`). The assembly is embedded into `src/main.rs` via
//! `core::arch::global_asm!(include_str!("asm/bootasm.S"))` so LLVM's
//! built-in assembler handles it — no external `cc`, no GCC, no
//! MSVC. This script's only job is to feed the bootloader linker
//! script to rustc and arrange rerun-if-changed.

use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let linker_script = manifest_dir.join("src").join("boot.ld");

    // Only emit the link-arg when actually targeting bare metal —
    // on a hosted target (e.g. cargo check on the host), MSVC's
    // link.exe doesn't understand GNU-ld linker scripts.
    let target = std::env::var("TARGET").unwrap_or_default();
    if target == "x86_64-unknown-none" && linker_script.exists() {
        println!("cargo:rustc-link-arg=-T{}", linker_script.display());
    }

    println!("cargo:rerun-if-changed=src/boot.ld");
    println!("cargo:rerun-if-changed=src/asm/bootasm.S");
    println!("cargo:rerun-if-env-changed=TARGET");
}