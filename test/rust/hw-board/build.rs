// Puts memory.x on the linker search path of the test that uses this
// crate: the one RX_MEMORY_X names (scripts/hw-build.sh sets it), else the
// board's: test/hw/rx72m-memory.x for ap-rx72m, rx74n-memory.x for the
// EK-RX74N and the EK-RX74M (the RX74M has the RX74N's memory map).
fn main() {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let memory = std::env::var("RX_MEMORY_X").unwrap_or_else(|_| {
        if std::env::var_os("CARGO_FEATURE_AP_RX72M").is_some() {
            "../../hw/rx72m-memory.x".into()
        } else {
            "../../hw/rx74n-memory.x".into()
        }
    });
    std::fs::copy(&memory, out.join("memory.x")).unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed={memory}");
    println!("cargo:rerun-if-env-changed=RX_MEMORY_X");
}
