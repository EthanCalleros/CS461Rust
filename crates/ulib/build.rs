fn main() {
    // 1. Tell Cargo to rebuild if usys.S changes
    // Assuming usys.S is located in your arch crate's asm folder
    let usys_path = "../crates/arch/src/asm/usys.S";
    println!("cargo:rerun-if-changed={}", usys_path);

    // 2. Compile usys.S into a static library named 'usys'
    // This generates the trampoline code for every syscall
    cc::Build::new()
        .file(usys_path)
        .compile("usys");
}
