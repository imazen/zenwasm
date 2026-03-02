#![no_std]
#![forbid(unsafe_code)]

/// ABI version for the zenwasm plugin boundary.
pub const ABI_VERSION: u32 = 1;

// --- WASM module export name conventions ---
// WASM modules compiled from C/C++ must export these symbols.

/// `wasm_alloc(size: u32) -> u32` — allocate in WASM linear memory.
pub const FN_ALLOC: &str = "wasm_alloc";

/// `wasm_dealloc(offset: u32, size: u32)` — free WASM memory.
pub const FN_DEALLOC: &str = "wasm_dealloc";

/// `abi_version() -> u32` — return the ABI version.
pub const FN_ABI_VERSION: &str = "abi_version";

// --- Codec module convention ---

/// `codec_probe(data_offset: u32, data_len: u32, info_offset: u32) -> i32`
pub const FN_CODEC_PROBE: &str = "codec_probe";

/// `codec_decode(data_offset: u32, data_len: u32, out_offset: u32) -> i32`
pub const FN_CODEC_DECODE: &str = "codec_decode";

/// `codec_encode(pixels_offset: u32, w: u32, h: u32, channels: u32,
///               opts_offset: u32, opts_len: u32, out_offset: u32) -> i32`
pub const FN_CODEC_ENCODE: &str = "codec_encode";

// --- Filter module convention ---

/// `filter_process(in_offset: u32, w: u32, h: u32, channels: u32,
///                 params_offset: u32, params_len: u32, out_offset: u32) -> i32`
pub const FN_FILTER_PROCESS: &str = "filter_process";

// --- Shared types (mirrored in C headers for module authors) ---

/// Image metadata returned by codec_probe.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub channels: u32,
    /// 0 = unknown, 1 = sRGB, 2 = linear, 3 = display-p3
    pub color_space: u32,
    pub has_alpha: u32,
    pub has_icc: u32,
}

/// Describes a buffer region in WASM linear memory.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct WasmBuffer {
    pub offset: u32,
    pub len: u32,
}

/// Decode output descriptor written by codec_decode.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct DecodeOutput {
    pub pixels: WasmBuffer,
    pub width: u32,
    pub height: u32,
    pub channels: u32,
}

/// Encode output descriptor written by codec_encode.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct EncodeOutput {
    pub data: WasmBuffer,
}

/// Error codes for the zenwasm FFI boundary (host ↔ plugin).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ErrorCode {
    Ok = 0,
    InvalidModule = -1,
    InvalidHandle = -2,
    MemoryAccessError = -3,
    FunctionNotFound = -4,
    CallFailed = -5,
    AllocFailed = -6,
    AbiMismatch = -7,
}
