// Puts memory.x next to rx-rt's link.x on the linker search path. RX_MEMORY_X
// names another one (scripts/hw-build.sh uses the board's, in test/hw). The
// default is not called memory.x: the linker would find it in the current
// directory first, whatever RX_MEMORY_X says.
fn main() {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let memory = std::env::var("RX_MEMORY_X").unwrap_or_else(|_| "qemu-memory.x".into());
    std::fs::copy(&memory, out.join("memory.x")).unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed={memory}");
    println!("cargo:rerun-if-env-changed=RX_MEMORY_X");
}
