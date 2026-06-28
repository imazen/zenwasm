<!-- GENERATED FROM README.md by zenutils gen-readme-crates.sh — DO NOT EDIT. -->

# zenwasm [![CI](https://img.shields.io/github/actions/workflow/status/imazen/zenwasm/ci.yml?style=flat-square&label=CI)](https://github.com/imazen/zenwasm/actions/workflows/ci.yml)

zenwasm runs sandboxed WASM modules — compiled C/C++ codecs, image filters, anything untrusted — from a Rust host **without linking [wasmtime](https://wasmtime.dev/) into your application binary**. wasmtime lives behind a `cdylib` plugin you load at runtime; your app depends only on a small host loader (`libloading` + `thiserror`). Reads out of WASM linear memory are zero-copy. The shared-types crate is `no_std` + `#![forbid(unsafe_code)]`; the host loader and plugin confine `unsafe` to the FFI boundary.

## Quick start

```toml
[dependencies]
zenwasm-api = "0.1.0"   # host loader — no wasmtime in your dependency tree
```

Build `zenwasm-abi` into a plugin (`libzenwasm_abi.so` / `.dll` / `.dylib`) and ship it next to your binary, then load it and run a module:

```rust
use zenwasm_api::{WasmRuntime, WasmCodec};

// Load the plugin once at startup. wasmtime lives inside the cdylib, not your app.
let runtime = WasmRuntime::load("libzenwasm_abi.so")?;

// Load a sandboxed module — e.g. a codec compiled from C with -msimd128.
let wasm = std::fs::read("heic_decoder.wasm")?;
let module = runtime.load_module(&wasm)?;

// Decode through the codec convenience wrapper.
let codec = WasmCodec::new(module);
let info = codec.probe(&heic_bytes)?;                  // ImageInfo { width, height, channels, .. }
let (pixels, w, h, channels) = codec.decode(&heic_bytes)?;
```

The loader checks the plugin's `ABI_VERSION` on `load()` and refuses a mismatched one, so a stale `.so` fails loudly instead of corrupting memory.

For a custom module ABI, drop to the low-level path — one host→WASM copy on the way in, zero-copy on the way out:

```rust
let module = runtime.load_module(&wasm)?;
let in_off = module.write_bytes(&input)?;                    // the only copy: host → WASM memory
let out = module.call_func("filter_process", &[in_off as i64, w as i64, h as i64, 4], 1)?;
let out_off = out[0] as u32;
let pixels: &[u8] = module.memory_slice(out_off, out_len)?;  // zero-copy read from WASM memory
```

## Why

[wasmtime](https://wasmtime.dev/) is a solid way to sandbox untrusted C/C++ (codecs, user-submitted filters), but it adds several megabytes to whatever links it. If your application already has a plugin architecture, you don't want that weight compiled into the main binary.

zenwasm puts wasmtime behind a `cdylib`. Your application links only `zenwasm-api` (just `libloading` + `thiserror`); the wasmtime-bearing plugin is loaded at runtime via `dlopen`. The boundary is a small, versioned C ABI — the host checks `ABI_VERSION` on load and refuses a mismatched plugin.

The output path is **zero-copy**: the host gets a direct pointer into WASM linear memory via `memory_slice()`, so decoded pixels are read in place. The only copy is the input write (host buffer → WASM memory), which is unavoidable.

```
Write:   1 memcpy   (host → WASM linear memory)
Compute: 0 copies   (the module works in its own memory)
Read:    0 copies   (host reads in place via memory_base + offset)
```

> **Lifetime note:** a zero-copy slice borrows the module and is valid only until
> the next `alloc`, `write_bytes`, `call_func`, or drop — any of which may grow and
> relocate WASM memory. Copy it out (`.to_vec()`) if you need to keep the data.

## Crates

| Crate | Type | Role |
|-------|------|------|
| [`zenwasm-api`](https://crates.io/crates/zenwasm-api) | lib | **Host loader — depend on this.** `libloading` + `thiserror`, no wasmtime. |
| [`zenwasm-abi`](https://crates.io/crates/zenwasm-abi) | `cdylib` | **The plugin.** Wraps wasmtime behind a C ABI; build and ship the `.so` / `.dll` / `.dylib`. |
| [`zenwasm-types`](https://crates.io/crates/zenwasm-types) | lib (`no_std`) | Shared `#[repr(C)]` types + WASM export-name conventions. `#![forbid(unsafe_code)]`. |

All three are versioned together (currently `0.1.0`) and built on wasmtime 43.

## WASM module convention

A module the plugin can host must export a small allocator so the host can place input bytes:

- `wasm_alloc(size: u32) -> u32` — allocate in WASM linear memory
- `wasm_dealloc(offset: u32, size: u32)` — free an allocation
- `abi_version() -> u32` — *optional*; if present, it's checked against the host's `ABI_VERSION`

On top of that, codec modules export `codec_probe`, `codec_decode`, and `codec_encode`; filter modules export `filter_process`. The exact signatures and the shared `#[repr(C)]` structs (`ImageInfo`, `WasmBuffer`, `DecodeOutput`, `EncodeOutput`, `ErrorCode`) live in [`zenwasm-types`](https://github.com/imazen/zenwasm/blob/main/zenwasm-types/src/lib.rs) — mirror them in your C header.

Compile image-processing modules with SIMD128:

```sh
clang --target=wasm32-wasi -O3 -msimd128 -o codec.wasm codec.c
```

## Building

```sh
cargo build --release
```

produces `target/release/libzenwasm_abi.so` (the plugin to ship) and makes `zenwasm-api` available to downstream crates. MSRV is 1.91 (required by wasmtime 43).

## Binary sizes

Approximate, release build, stripped:

| Artifact | Size |
|----------|------|
| `libzenwasm_abi.so` (wasmtime inside) | ~10 MB |
| Host application (with `zenwasm-api` only) | ~350 KB |
| Typical `.wasm` module | ~1 MB |

wasmtime's footprint is paid once, in a plugin loaded on demand, instead of being compiled into every build of your application.

## License

Dual-licensed: [AGPL-3.0](https://github.com/imazen/zenwasm/blob/main/LICENSE-AGPL3) or [commercial](https://github.com/imazen/zenwasm/blob/main/LICENSE-COMMERCIAL).

I've maintained and developed open-source image server software — and the 40+
library ecosystem it depends on — full-time since 2011. Fifteen years of
continual maintenance, backwards compatibility, support, and the (very rare)
security patch. That kind of stability requires sustainable funding, and
dual-licensing is how we make it work without venture capital or rug-pulls.
Support sustainable and secure software; swap patch tuesday for patch leap-year.

[Our open-source products](https://www.imazen.io/open-source)

**Your options:**

- **Startup license** — $1 if your company has under $1M revenue and fewer
  than 5 employees. [Get a key →](https://www.imazen.io/pricing)
- **Commercial subscription** — Governed by the Imazen Site-wide Subscription
  License v1.1 or later. Apache 2.0-like terms, no source-sharing requirement.
  Sliding scale by company size.
  [Pricing & 60-day free trial →](https://www.imazen.io/pricing)
- **AGPL v3** — Free and open. Share your source if you distribute.

See [LICENSE-COMMERCIAL](https://github.com/imazen/zenwasm/blob/main/LICENSE-COMMERCIAL) for details.

## Image tech I maintain

| | |
|:--|:--|
| **Codecs** ¹ | [zenjpeg] · [zenpng] · [zenwebp] · [zengif] · [zenavif] · [zenjxl] · [zenbitmaps] · [heic] · [zentiff] · [zenpdf] · [zensvg] · [zenjp2] · [zenraw] · [ultrahdr] |
| Codec internals | [zenjxl-decoder] · [jxl-encoder] · [zenrav1e] · [rav1d-safe] · [zenavif-parse] · [zenavif-serialize] |
| Compression | [zenflate] · [zenzop] · [zenzstd] |
| Processing | [zenresize] · [zenquant] · [zenblend] · [zenfilters] · [zensally] · [zentone] |
| Pixels & color | [zenpixels] · [zenpixels-convert] · [linear-srgb] · [garb] |
| Pipeline & framework | [zenpipe] · [zencodec] · [zencodecs] · [zenlayout] · [zennode] · **zenwasm** · [zentract] |
| Metrics | [zensim] · [fast-ssim2] · [butteraugli] · [zenmetrics] · [resamplescope-rs] |
| Pickers & ML | [zenanalyze] · [zenpredict] · [zenpicker] |
| Products | [Imageflow] image engine ([.NET][imageflow-dotnet] · [Node][imageflow-node] · [Go][imageflow-go]) · [Imageflow Server] · [ImageResizer] (C#) |

<sub>¹ pure-Rust, `#![forbid(unsafe_code)]` codecs, as of 2026</sub>

### General Rust awesomeness

[zenbench] · [archmage] · [magetypes] · [enough] · [whereat] · [cargo-copter]

[Open source](https://www.imazen.io/open-source) · [@imazen](https://github.com/imazen) · [@lilith](https://github.com/lilith) · [lib.rs/~lilith](https://lib.rs/~lilith)

[zenjpeg]: https://github.com/imazen/zenjpeg
[zenpng]: https://github.com/imazen/zenpng
[zenwebp]: https://github.com/imazen/zenwebp
[zengif]: https://github.com/imazen/zengif
[zenavif]: https://github.com/imazen/zenavif
[zenjxl]: https://github.com/imazen/zenjxl
[zenbitmaps]: https://github.com/imazen/zenbitmaps
[heic]: https://github.com/imazen/heic
[zentiff]: https://github.com/imazen/zentiff
[zenpdf]: https://github.com/imazen/zenpdf
[zensvg]: https://github.com/imazen/zenextras
[zenjp2]: https://github.com/imazen/zenextras
[zenraw]: https://github.com/imazen/zenraw
[ultrahdr]: https://github.com/imazen/ultrahdr
[zenjxl-decoder]: https://github.com/imazen/zenjxl-decoder
[jxl-encoder]: https://github.com/imazen/jxl-encoder
[zenrav1e]: https://github.com/imazen/zenrav1e
[rav1d-safe]: https://github.com/imazen/rav1d-safe
[zenavif-parse]: https://github.com/imazen/zenavif-parse
[zenavif-serialize]: https://github.com/imazen/zenavif-serialize
[zenflate]: https://github.com/imazen/zenflate
[zenzop]: https://github.com/imazen/zenzop
[zenzstd]: https://github.com/imazen/zenzstd
[zenresize]: https://github.com/imazen/zenresize
[zenquant]: https://github.com/imazen/zenquant
[zenblend]: https://github.com/imazen/zenblend
[zenfilters]: https://github.com/imazen/zenfilters
[zensally]: https://github.com/imazen/zensally
[zentone]: https://github.com/imazen/zentone
[zenpixels]: https://github.com/imazen/zenpixels
[zenpixels-convert]: https://github.com/imazen/zenpixels
[linear-srgb]: https://github.com/imazen/linear-srgb
[garb]: https://github.com/imazen/garb
[zenpipe]: https://github.com/imazen/zenpipe
[zencodec]: https://github.com/imazen/zencodec
[zencodecs]: https://github.com/imazen/zencodecs
[zenlayout]: https://github.com/imazen/zenlayout
[zennode]: https://github.com/imazen/zennode
[zentract]: https://github.com/imazen/zentract
[zensim]: https://github.com/imazen/zensim
[fast-ssim2]: https://github.com/imazen/fast-ssim2
[butteraugli]: https://github.com/imazen/butteraugli
[zenmetrics]: https://github.com/imazen/zenmetrics
[resamplescope-rs]: https://github.com/imazen/resamplescope-rs
[zenanalyze]: https://github.com/imazen/zenanalyze
[zenpredict]: https://github.com/imazen/zenanalyze
[zenpicker]: https://github.com/imazen/zenanalyze
[zenbench]: https://github.com/imazen/zenbench
[archmage]: https://github.com/imazen/archmage
[magetypes]: https://github.com/imazen/archmage
[enough]: https://github.com/imazen/enough
[whereat]: https://github.com/lilith/whereat
[cargo-copter]: https://github.com/imazen/cargo-copter
[Imageflow]: https://github.com/imazen/imageflow
[Imageflow Server]: https://github.com/imazen/imageflow-dotnet-server
[ImageResizer]: https://github.com/imazen/resizer
[imageflow-dotnet]: https://github.com/imazen/imageflow-dotnet
[imageflow-node]: https://github.com/imazen/imageflow-node
[imageflow-go]: https://github.com/imazen/imageflow-go
