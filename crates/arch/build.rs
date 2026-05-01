// Arch build script.
//
// Assembles initcode.S and entryother.S into flat binaries (via objcopy)
// and wraps them into linkable .o files that provide:
//   _binary_initcode_start / _binary_initcode_size
//   _binary_entryother_start / _binary_entryother_size
//
// These symbols are referenced by proc.rs (initcode) and main.rs (entryother).
//
// REQUIRES: a cross GCC on PATH (x86_64-elf-gcc / x86_64-elf-objcopy)
//           OR running on Linux/WSL with native GNU binutils.
//
// If the cross tools aren't found, the build prints a warning and skips
// assembly — the kernel will fail to link (unresolved symbols) but
// `cargo check` will still pass.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // Track all assembly source files for rebuild detection.
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

    // Only attempt assembly for the bare-metal target.
    let target = env::var("TARGET").unwrap_or_default();
    if target != "x86_64-unknown-none" {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // Try to find cross tools. Prefer x86_64-elf-* (typical cross prefix),
    // fall back to plain gcc/objcopy (works on native Linux x86_64).
    let (gcc, objcopy) = find_tools();
    let (Some(gcc), Some(objcopy)) = (gcc, objcopy) else {
        println!(
            "cargo:warning=Cross assembler not found (tried x86_64-elf-gcc, gcc). \
             initcode.S and entryother.S will NOT be assembled. \
             The kernel will fail to link."
        );
        return;
    };

    // Assemble and embed initcode.S
    assemble_flat_binary(
        &gcc,
        &objcopy,
        &manifest_dir.join("src/asm/initcode.S"),
        "initcode",
        &out_dir,
    );

    // Assemble and embed entryother.S
    assemble_flat_binary(
        &gcc,
        &objcopy,
        &manifest_dir.join("src/asm/entryother.S"),
        "entryother",
        &out_dir,
    );

    // Tell rustc where to find the resulting archive.
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=xv6asm");
}

/// Assemble `src` into a flat binary, then wrap it via objcopy into a
/// relocatable .o that exports `_binary_{name}_start` and `_binary_{name}_size`.
fn assemble_flat_binary(
    gcc: &str,
    objcopy: &str,
    src: &Path,
    name: &str,
    out_dir: &Path,
) {
    let obj = out_dir.join(format!("{name}.o"));
    // Use the bare name (no extension) so objcopy generates
    // _binary_{name}_start / _binary_{name}_end / _binary_{name}_size
    let bin = out_dir.join(name);
    let wrap_obj = out_dir.join(format!("{name}_blob.o"));

    // Step 1: assemble .S -> .o (ELF relocatable)
    let status = Command::new(gcc)
        .args([
            "-m64",
            "-nostdinc",
            "-fno-pic",
            "-c",
            "-o",
        ])
        .arg(&obj)
        .arg(src)
        .status()
        .expect("failed to run gcc");
    assert!(status.success(), "gcc failed to assemble {}", src.display());

    // Step 2: extract flat binary from .text section
    let status = Command::new(objcopy)
        .args(["-S", "-O", "binary"])
        .arg(&obj)
        .arg(&bin)
        .status()
        .expect("failed to run objcopy (binary)");
    assert!(status.success(), "objcopy binary extraction failed for {name}");

    // Step 3: wrap flat binary back into a .o with _binary_* symbols
    // objcopy -I binary generates symbols from the INPUT filename:
    //   _binary_<name>_start, _binary_<name>_end, _binary_<name>_size
    // IMPORTANT: run from out_dir so the input is just the bare name
    // (e.g. "initcode"), not the full path. Otherwise the symbols
    // include the entire filesystem path and won't match.
    let status = Command::new(objcopy)
        .current_dir(out_dir)
        .args([
            "-I", "binary",
            "-O", "elf64-x86-64",
            "-B", "i386:x86-64",
            "--rename-section", ".data=.rodata,alloc,load,readonly,data,contents",
        ])
        .arg(name)
        .arg(&wrap_obj)
        .status()
        .expect("failed to run objcopy (wrap)");
    assert!(status.success(), "objcopy wrap failed for {name}");

    // Step 4: bundle into a static library so rustc can find it
    let archive = out_dir.join("libxv6asm.a");
    // Use `ar` to append (if archive doesn't exist yet, create it).
    let ar = find_ar().unwrap_or_else(|| "ar".to_string());
    let status = Command::new(&ar)
        .args(["rcs"])
        .arg(&archive)
        .arg(&wrap_obj)
        .status()
        .expect("failed to run ar");
    assert!(status.success(), "ar failed for {name}");
}

fn find_tools() -> (Option<String>, Option<String>) {
    // Check for x86_64-elf-gcc first (cross-compiler on macOS/Windows)
    if which("x86_64-elf-gcc") {
        return (
            Some("x86_64-elf-gcc".to_string()),
            Some("x86_64-elf-objcopy".to_string()),
        );
    }
    // Check for plain gcc (Linux native)
    if which("gcc") {
        return (Some("gcc".to_string()), Some("objcopy".to_string()));
    }
    (None, None)
}

fn find_ar() -> Option<String> {
    if which("x86_64-elf-ar") {
        Some("x86_64-elf-ar".to_string())
    } else if which("ar") {
        Some("ar".to_string())
    } else {
        None
    }
}

fn which(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
