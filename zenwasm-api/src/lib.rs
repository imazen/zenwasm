// deny rather than forbid: FFI wrapper needs unsafe for libloading + raw pointer reads
#![deny(unsafe_code)]

pub use zenwasm_types::{self, DecodeOutput, EncodeOutput, ImageInfo, WasmBuffer};

use libloading::Library;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to load plugin library: {0}")]
    LoadLibrary(#[from] libloading::Error),
    #[error("ABI version mismatch: host expects {expected}, plugin has {actual}")]
    AbiMismatch { expected: u32, actual: u32 },
    #[error("module load failed (error code {0})")]
    ModuleLoad(i64),
    #[error("call failed (error code {0})")]
    CallFailed(i32),
    #[error("memory access out of bounds")]
    MemoryAccess,
    #[error("allocation failed in WASM module")]
    AllocFailed,
    #[error("invalid handle")]
    InvalidHandle,
}

// Function pointer types matching zenwasm-abi exports.
type FnAbiVersion = unsafe extern "C" fn() -> u32;
type FnLoad = unsafe extern "C" fn(*const u8, usize) -> i64;
type FnAlloc = unsafe extern "C" fn(i64, u32) -> u32;
type FnWrite = unsafe extern "C" fn(i64, u32, *const u8, usize) -> i32;
type FnMemoryBase = unsafe extern "C" fn(i64) -> *const u8;
type FnMemoryLen = unsafe extern "C" fn(i64) -> usize;
type FnCall = unsafe extern "C" fn(i64, *const u8, usize, *const i64, u32, *mut i64, u32) -> i32;
type FnDealloc = unsafe extern "C" fn(i64, u32, u32) -> i32;
type FnFree = unsafe extern "C" fn(i64);

/// A loaded zenwasm plugin. Holds the dylib open.
pub struct WasmRuntime {
    _lib: Library,
    fn_load: FnLoad,
    fn_alloc: FnAlloc,
    fn_write: FnWrite,
    fn_memory_base: FnMemoryBase,
    fn_memory_len: FnMemoryLen,
    fn_call: FnCall,
    fn_dealloc: FnDealloc,
    fn_free: FnFree,
}

#[allow(unsafe_code)]
unsafe impl Send for WasmRuntime {}
#[allow(unsafe_code)]
unsafe impl Sync for WasmRuntime {}

impl WasmRuntime {
    /// Load the zenwasm plugin from a shared library path.
    #[allow(unsafe_code)]
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        let lib = unsafe { Library::new(path.as_ref()) }?;

        let fn_abi_version: FnAbiVersion =
            unsafe { *lib.get::<FnAbiVersion>(b"zenwasm_abi_version\0")? };
        let actual = unsafe { fn_abi_version() };
        if actual != zenwasm_types::ABI_VERSION {
            return Err(Error::AbiMismatch {
                expected: zenwasm_types::ABI_VERSION,
                actual,
            });
        }

        let fn_load: FnLoad = unsafe { *lib.get::<FnLoad>(b"zenwasm_load\0")? };
        let fn_alloc: FnAlloc = unsafe { *lib.get::<FnAlloc>(b"zenwasm_alloc\0")? };
        let fn_write: FnWrite = unsafe { *lib.get::<FnWrite>(b"zenwasm_write\0")? };
        let fn_memory_base: FnMemoryBase =
            unsafe { *lib.get::<FnMemoryBase>(b"zenwasm_memory_base\0")? };
        let fn_memory_len: FnMemoryLen =
            unsafe { *lib.get::<FnMemoryLen>(b"zenwasm_memory_len\0")? };
        let fn_call: FnCall = unsafe { *lib.get::<FnCall>(b"zenwasm_call\0")? };
        let fn_dealloc: FnDealloc = unsafe { *lib.get::<FnDealloc>(b"zenwasm_dealloc\0")? };
        let fn_free: FnFree = unsafe { *lib.get::<FnFree>(b"zenwasm_free\0")? };

        Ok(Self {
            _lib: lib,
            fn_load,
            fn_alloc,
            fn_write,
            fn_memory_base,
            fn_memory_len,
            fn_call,
            fn_dealloc,
            fn_free,
        })
    }

    /// Load a WASM module from bytes.
    #[allow(unsafe_code)]
    pub fn load_module(&self, wasm_bytes: &[u8]) -> Result<ModuleHandle<'_>, Error> {
        let handle =
            unsafe { (self.fn_load)(wasm_bytes.as_ptr(), wasm_bytes.len()) };
        if handle < 0 {
            return Err(Error::ModuleLoad(handle));
        }
        Ok(ModuleHandle {
            runtime: self,
            handle,
        })
    }
}

/// A loaded WASM module. Freed on drop.
pub struct ModuleHandle<'r> {
    runtime: &'r WasmRuntime,
    handle: i64,
}

impl<'r> ModuleHandle<'r> {
    /// Allocate memory in the WASM module's linear memory.
    #[allow(unsafe_code)]
    pub fn alloc(&self, size: u32) -> Result<u32, Error> {
        let offset = unsafe { (self.runtime.fn_alloc)(self.handle, size) };
        if offset == 0 && size > 0 {
            return Err(Error::AllocFailed);
        }
        Ok(offset)
    }

    /// Write data into WASM linear memory at `offset`.
    /// This is the only copy in the pipeline — host buffer → WASM memory.
    #[allow(unsafe_code)]
    pub fn write(&self, offset: u32, data: &[u8]) -> Result<(), Error> {
        let rc = unsafe {
            (self.runtime.fn_write)(self.handle, offset, data.as_ptr(), data.len())
        };
        if rc != 0 {
            return Err(Error::CallFailed(rc));
        }
        Ok(())
    }

    /// Write data into WASM memory, allocating space first.
    /// Returns the offset where data was written.
    pub fn write_bytes(&self, data: &[u8]) -> Result<u32, Error> {
        let offset = self.alloc(data.len() as u32)?;
        self.write(offset, data)?;
        Ok(offset)
    }

    /// Get a direct read-only view into WASM linear memory. **Zero-copy.**
    ///
    /// The returned slice borrows `self` — it is valid until the next
    /// call to `alloc`, `write_bytes`, `call_func`, or `drop`.
    /// These operations may grow WASM memory, relocating the backing store.
    #[allow(unsafe_code)]
    pub fn memory_slice(&self, offset: u32, len: u32) -> Result<&[u8], Error> {
        let base = unsafe { (self.runtime.fn_memory_base)(self.handle) };
        let mem_len = unsafe { (self.runtime.fn_memory_len)(self.handle) };

        if base.is_null() {
            return Err(Error::InvalidHandle);
        }

        let end = offset as usize + len as usize;
        if end > mem_len {
            return Err(Error::MemoryAccess);
        }

        let slice = unsafe {
            std::slice::from_raw_parts(base.add(offset as usize), len as usize)
        };
        Ok(slice)
    }

    /// Read a `#[repr(C)]` struct from WASM linear memory. **Zero-copy.**
    #[allow(unsafe_code)]
    pub fn read_struct<T: Copy>(&self, offset: u32) -> Result<T, Error> {
        let size = std::mem::size_of::<T>();
        let slice = self.memory_slice(offset, size as u32)?;
        let val = unsafe { std::ptr::read(slice.as_ptr().cast::<T>()) };
        Ok(val)
    }

    /// Call a WASM function by name with i64 arguments, returning i64 results.
    #[allow(unsafe_code)]
    pub fn call_func(
        &self,
        name: &str,
        args: &[i64],
        n_results: u32,
    ) -> Result<Vec<i64>, Error> {
        let mut results = vec![0i64; n_results as usize];

        let rc = unsafe {
            (self.runtime.fn_call)(
                self.handle,
                name.as_ptr(),
                name.len(),
                args.as_ptr(),
                args.len() as u32,
                results.as_mut_ptr(),
                n_results,
            )
        };

        if rc != 0 {
            return Err(Error::CallFailed(rc));
        }
        Ok(results)
    }

    /// Free a WASM-side allocation.
    #[allow(unsafe_code)]
    pub fn dealloc(&self, offset: u32, size: u32) -> Result<(), Error> {
        let rc = unsafe { (self.runtime.fn_dealloc)(self.handle, offset, size) };
        if rc != 0 {
            return Err(Error::CallFailed(rc));
        }
        Ok(())
    }
}

#[allow(unsafe_code)]
impl Drop for ModuleHandle<'_> {
    fn drop(&mut self) {
        unsafe { (self.runtime.fn_free)(self.handle) };
    }
}

/// Convenience wrapper for codec WASM modules.
pub struct WasmCodec<'r> {
    module: ModuleHandle<'r>,
}

impl<'r> WasmCodec<'r> {
    pub fn new(module: ModuleHandle<'r>) -> Self {
        Self { module }
    }

    /// Probe image metadata without full decode.
    pub fn probe(&self, data: &[u8]) -> Result<ImageInfo, Error> {
        let data_offset = self.module.write_bytes(data)?;
        let info_size = std::mem::size_of::<ImageInfo>() as u32;
        let info_offset = self.module.alloc(info_size)?;

        let results = self.module.call_func(
            zenwasm_types::FN_CODEC_PROBE,
            &[data_offset as i64, data.len() as i64, info_offset as i64],
            1,
        )?;

        let rc = results.first().copied().unwrap_or(-1) as i32;
        if rc != 0 {
            return Err(Error::CallFailed(rc));
        }

        // Zero-copy read of the struct from WASM memory
        let info: ImageInfo = self.module.read_struct(info_offset)?;

        self.module.dealloc(data_offset, data.len() as u32)?;
        self.module.dealloc(info_offset, info_size)?;

        Ok(info)
    }

    /// Decode an image. Returns pixel data, width, height, channels.
    ///
    /// The pixel data is copied out of WASM memory. For zero-copy access,
    /// use `module.call_func()` + `module.memory_slice()` directly.
    pub fn decode(&self, data: &[u8]) -> Result<(Vec<u8>, u32, u32, u32), Error> {
        let data_offset = self.module.write_bytes(data)?;
        let out_desc_size = std::mem::size_of::<DecodeOutput>() as u32;
        let out_offset = self.module.alloc(out_desc_size)?;

        let results = self.module.call_func(
            zenwasm_types::FN_CODEC_DECODE,
            &[data_offset as i64, data.len() as i64, out_offset as i64],
            1,
        )?;

        let rc = results.first().copied().unwrap_or(-1) as i32;
        if rc != 0 {
            return Err(Error::CallFailed(rc));
        }

        // Zero-copy read of the descriptor
        let desc: DecodeOutput = self.module.read_struct(out_offset)?;

        // Read pixels — this IS a copy, but callers who want zero-copy
        // can use memory_slice() directly with the descriptor offsets.
        let pixel_slice = self
            .module
            .memory_slice(desc.pixels.offset, desc.pixels.len)?;
        let pixels = pixel_slice.to_vec();

        self.module.dealloc(data_offset, data.len() as u32)?;
        self.module.dealloc(desc.pixels.offset, desc.pixels.len)?;
        self.module.dealloc(out_offset, out_desc_size)?;

        Ok((pixels, desc.width, desc.height, desc.channels))
    }
}
