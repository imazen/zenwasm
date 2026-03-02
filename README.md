# zenwasm

Rust-to-Rust dynamic library boundary for [wasmtime](https://wasmtime.dev/). Runs sandboxed WASM modules (compiled C/C++ codecs, image filters) without linking wasmtime into your application.

## Why

wasmtime is the right way to sandbox untrusted C/C++ code (codecs, user-submitted filters), but it adds ~10 MB to your binary. If your application already has a plugin architecture, you don't want that cost baked in.

zenwasm puts wasmtime behind a cdylib. Your application depends only on `zenwasm-api` (14 crates, 350 KB stripped) and loads the runtime at runtime.

The key optimization: **zero-copy reads.** The host gets a direct pointer into WASM linear memory via `memory_slice()`, avoiding intermediate buffers on the output path. The only copy is the write (host buffer into WASM memory), which is unavoidable.

## Crates

| Crate | Type | Description |
|-------|------|-------------|
| `zenwasm-types` | lib (`no_std`) | Shared `#[repr(C)]` types + WASM module export name conventions |
| `zenwasm-abi` | cdylib | Plugin that links wasmtime, exports `extern "C"` functions |
| `zenwasm-api` | lib | Host wrapper using `libloading` — no wasmtime dependency |

## Usage

```rust
use zenwasm_api::{WasmRuntime, WasmCodec};

// Load the plugin (once, at startup)
let runtime = WasmRuntime::load("libzenwasm_abi.so")?;

// Load a WASM module (e.g., a codec compiled from C with -msimd128)
let wasm_bytes = std::fs::read("heic_decoder.wasm")?;
let module = runtime.load_module(&wasm_bytes)?;

// Use the codec convenience wrapper
let codec = WasmCodec::new(module);
let info = codec.probe(&heic_file_bytes)?;
let (pixels, w, h, channels) = codec.decode(&heic_file_bytes)?;

// Or use the low-level API for zero-copy reads:
let module = runtime.load_module(&wasm_bytes)?;
let offset = module.write_bytes(&input_data)?;
let results = module.call_func("filter_process", &[offset as i64, w, h, 4], 1)?;
let output_offset = results[0] as u32;
let output: &[u8] = module.memory_slice(output_offset, output_len)?; // zero-copy
```

## Copy budget

```
Write: 1 memcpy  (host → WASM linear memory)
Compute: 0 copies (WASM module processes in its own memory)
Read: 0 copies   (host reads directly via memory_base + offset)
```

## Binary sizes (stripped)

| Artifact | Size |
|----------|------|
| `libzenwasm_abi.so` (wasmtime inside) | 10 MB |
| Host binary (libloading only) | 350 KB |
| Typical .wasm module | ~1 MB |

## WASM module convention

Modules compiled from C/C++ must export these symbols:

- `wasm_alloc(size: u32) -> u32` — allocate in WASM linear memory
- `wasm_dealloc(offset: u32, size: u32)` — free allocation

Codec modules additionally export `codec_probe`, `codec_decode`, `codec_encode`. Filter modules export `filter_process`. See `zenwasm-types` for the full convention.

Compile with SIMD128 for image processing workloads:

```sh
clang --target=wasm32-wasi -O3 -msimd128 -o codec.wasm codec.c
```

## Building

```sh
cargo build --release
```

Produces `target/release/libzenwasm_abi.so` (the plugin) and makes `zenwasm-api` available for downstream crates.

## License

AGPL-3.0-or-later
