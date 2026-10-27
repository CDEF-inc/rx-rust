// Puts the RX72M memory.x (test/hw/rx72m-memory.x) next to rx-rt's link.x
// on the linker search path. Board only: the PAC is the RX72M's.
fn main() {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let memory = "../../hw/rx72m-memory.x";
    std::fs::copy(memory, out.join("memory.x")).unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed={memory}");
}
