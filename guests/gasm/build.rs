// On wasm32, link libgasm_sp.a: two functions that read and set the wasm stack
// pointer (sp/gasm_sp.s), which stable Rust can't. Only the cooperative thread
// scheduler (gasm::thread) uses them; games without threads don't link them.
fn main() {
    println!("cargo:rerun-if-changed=sp/libgasm_sp.a");
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        let dir = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("sp");
        println!("cargo:rustc-link-search=native={}", dir.display());
        println!("cargo:rustc-link-lib=static=gasm_sp");
    }
}
