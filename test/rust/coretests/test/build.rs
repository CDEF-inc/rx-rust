// The linker scripts (coretests.x for QEMU, coretests-rx74n.x for the
// board) are found through this directory.
fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-search={dir}");
    println!("cargo:rerun-if-changed=coretests.x");
    println!("cargo:rerun-if-changed=coretests-rx74n.x");
}
