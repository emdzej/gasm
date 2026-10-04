# glow for gasm

[glow](https://github.com/grovesNL/glow) 0.17.0 with its native backend on
`wasm32-unknown-unknown`, where it calls the OpenGL ES 3.0 functions of the gasm
Rust SDK (`gasm::gles`, on the `gasm:gl` imports) instead of WebGL through
web-sys, which gasm guests don't have. Made by `scripts/update-glow.sh` from the
crates.io release and `sdk/glow.patch`:

- `src/lib.rs`: the native backend on every target (the web backend is removed).
- `src/gl46.rs`: function pointers are table indices on wasm32, so only null
  means "not loaded".
- `Cargo.toml`: no web dependencies.

Use it in a gasm game:

```toml
[dependencies]
glow = "0.17"
gasm-sdk = "0.8"

[patch.crates-io]
glow = { git = "https://github.com/emdzej/gasm" }
```

```rust
let gl = unsafe { glow::Context::from_loader_function_cstr(gasm::gles::get_proc_address) };
```

Everything that depends on glow 0.17 (egui_glow, ...) uses it too. glow is
MIT / Apache-2.0 / zlib licensed (LICENSE-*).
