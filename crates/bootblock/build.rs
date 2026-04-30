use std::process::Command;
use std::env;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // 1. Compile the assembly entry point (bootasm.S)
    // We use the 'cc' crate to handle cross-compilation flags for x86
    cc::Build::new()
        .file("src/asm/bootasm.S")
        .compiler("gcc") // Or your cross-compiler like i686-linux-gnu-gcc
        .flag("-m32")    // Ensure 32-bit for the bootloader
        .flag("-gdwarf-2")
        .compile("bootasm");

    // 2. Tell Cargo where the linker script is
    let linker_script = "src/boot.ld";
    println!("cargo:rustc-link-arg=-T{}", linker_script);
    
    // 3. Rebuild if these files change
    println!("cargo:rerun-if-changed=src/boot.ld");
    println!("cargo:rerun-if-changed=src/asm/bootasm.S");
}
