// Kernel build script.
//
// Pass the kernel linker script (`kernel.ld`, at the workspace root)
// to rustc as a linker argument. This applies only to the `kernel`
// crate's final binary — library crates aren't affected.
use std::path::PathBuf;

fn main() {
    // Locate `kernel.ld` two levels up from this build script
    // (kernel/build.rs -> ../kernel.ld -> workspace root).
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().expect("kernel/ should have a parent");
    let linker_script = workspace_root.join("kernel.ld");

    // Only emit the link-arg for the bare-metal target — on a hosted
    // target (e.g. x86_64-pc-windows-msvc during cargo check on the
    // host), the script's GNU-ld syntax is incompatible with link.exe.
    let target = std::env::var("TARGET").unwrap_or_default();
    if target == "x86_64-unknown-none" {
        println!("cargo:rustc-link-arg=-T{}", linker_script.display());
    }

    println!("cargo:rerun-if-changed={}", linker_script.display());
    println!("cargo:rerun-if-env-changed=TARGET");
}
