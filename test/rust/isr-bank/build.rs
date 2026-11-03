// Puts memory.x next to rx-rt's link.x on the linker search path. RX_MEMORY_X
// names another one (scripts/hw-build.sh uses test/hw/rx72m-memory.x).
fn main() {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let memory = std::env::var("RX_MEMORY_X").unwrap_or_else(|_| "memory.x".into());
    std::fs::copy(&memory, out.join("memory.x")).unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed={memory}");
    println!("cargo:rerun-if-env-changed=RX_MEMORY_X");
}
