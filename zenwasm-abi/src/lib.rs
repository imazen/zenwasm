// No forbid(unsafe_code) — this is an FFI boundary crate.

use std::cell::RefCell;

use wasmtime::*;
use zenwasm_types::*;

struct LoadedModule {
    store: Store<()>,
    instance: Instance,
    memory: Memory,
    fn_alloc: TypedFunc<u32, u32>,
    fn_dealloc: TypedFunc<(u32, u32), ()>,
}

// wasmtime Store is !Send, use thread_local
thread_local! {
    static ENGINE: Engine = Engine::default();
    static MODULES: RefCell<Vec<Option<LoadedModule>>> = const { RefCell::new(Vec::new()) };
}

#[unsafe(no_mangle)]
pub extern "C" fn zenwasm_abi_version() -> u32 {
    ABI_VERSION
}

/// Load a WASM module from bytes.
/// Returns a handle (>= 0) on success, or a negative error code.
///
/// # Safety
/// `wasm_bytes` must point to `wasm_len` valid bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn zenwasm_load(wasm_bytes: *const u8, wasm_len: usize) -> i64 {
    if wasm_bytes.is_null() {
        return ErrorCode::InvalidModule as i64;
    }

    let bytes = unsafe { std::slice::from_raw_parts(wasm_bytes, wasm_len) };

    ENGINE.with(|engine| {
        let module = match Module::new(engine, bytes) {
            Ok(m) => m,
            Err(_) => return ErrorCode::InvalidModule as i64,
        };

        let mut store = Store::new(engine, ());
        let instance = match Instance::new(&mut store, &module, &[]) {
            Ok(i) => i,
            Err(_) => return ErrorCode::InvalidModule as i64,
        };

        let memory = match instance.get_memory(&mut store, "memory") {
            Some(m) => m,
            None => return ErrorCode::InvalidModule as i64,
        };

        let fn_alloc = match instance.get_typed_func::<u32, u32>(&mut store, FN_ALLOC) {
            Ok(f) => f,
            Err(_) => return ErrorCode::FunctionNotFound as i64,
        };

        let fn_dealloc = match instance.get_typed_func::<(u32, u32), ()>(&mut store, FN_DEALLOC) {
            Ok(f) => f,
            Err(_) => return ErrorCode::FunctionNotFound as i64,
        };

        // Check ABI version if exported
        if let Ok(fn_version) = instance.get_typed_func::<(), u32>(&mut store, FN_ABI_VERSION)
            && let Ok(v) = fn_version.call(&mut store, ())
            && v != ABI_VERSION
        {
            return ErrorCode::AbiMismatch as i64;
        }

        let loaded = LoadedModule {
            store,
            instance,
            memory,
            fn_alloc,
            fn_dealloc,
        };

        MODULES.with(|modules| {
            let mut modules = modules.borrow_mut();
            for (i, slot) in modules.iter_mut().enumerate() {
                if slot.is_none() {
                    *slot = Some(loaded);
                    return i as i64;
                }
            }
            let id = modules.len();
            modules.push(Some(loaded));
            id as i64
        })
    })
}

/// Allocate memory inside the WASM module.
/// Returns the offset in WASM linear memory, or 0 on failure.
#[unsafe(no_mangle)]
pub extern "C" fn zenwasm_alloc(handle: i64, size: u32) -> u32 {
    MODULES.with(|modules| {
        let mut modules = modules.borrow_mut();
        let Some(Some(m)) = modules.get_mut(handle as usize) else {
            return 0;
        };
        m.fn_alloc.call(&mut m.store, size).unwrap_or(0)
    })
}

/// Write data from the host into WASM linear memory at `offset`.
///
/// # Safety
/// `data` must point to `len` valid bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn zenwasm_write(
    handle: i64,
    offset: u32,
    data: *const u8,
    len: usize,
) -> i32 {
    if data.is_null() && len > 0 {
        return ErrorCode::MemoryAccessError as i32;
    }
    let src = if len > 0 {
        unsafe { std::slice::from_raw_parts(data, len) }
    } else {
        &[]
    };

    MODULES.with(|modules| {
        let mut modules = modules.borrow_mut();
        let Some(Some(m)) = modules.get_mut(handle as usize) else {
            return ErrorCode::InvalidHandle as i32;
        };
        match m.memory.write(&mut m.store, offset as usize, src) {
            Ok(()) => ErrorCode::Ok as i32,
            Err(_) => ErrorCode::MemoryAccessError as i32,
        }
    })
}

/// Get a direct pointer to the base of WASM linear memory.
///
/// The returned pointer is valid for `zenwasm_memory_len(handle)` bytes.
/// It is invalidated by ANY subsequent call to `zenwasm_alloc`,
/// `zenwasm_call`, or `zenwasm_free` on the same handle (memory may grow/relocate).
///
/// This enables zero-copy reads: the host can read output data directly
/// from `base + offset` without any intermediate buffer.
#[unsafe(no_mangle)]
pub extern "C" fn zenwasm_memory_base(handle: i64) -> *const u8 {
    MODULES.with(|modules| {
        let modules = modules.borrow();
        let Some(Some(m)) = modules.get(handle as usize) else {
            return std::ptr::null();
        };
        m.memory.data(&m.store).as_ptr()
    })
}

/// Get the current size of WASM linear memory in bytes.
#[unsafe(no_mangle)]
pub extern "C" fn zenwasm_memory_len(handle: i64) -> usize {
    MODULES.with(|modules| {
        let modules = modules.borrow();
        let Some(Some(m)) = modules.get(handle as usize) else {
            return 0;
        };
        m.memory.data(&m.store).len()
    })
}

/// Call a WASM function by name with i64 arguments.
/// Results are written to `results` (must have room for `n_results` i64 values).
///
/// # Safety
/// `func_name` must point to `name_len` valid UTF-8 bytes.
/// `args` must point to `n_args` i64 values.
/// `results` must point to `n_results` i64 values of writable memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn zenwasm_call(
    handle: i64,
    func_name: *const u8,
    name_len: usize,
    args: *const i64,
    n_args: u32,
    results: *mut i64,
    n_results: u32,
) -> i32 {
    if func_name.is_null() {
        return ErrorCode::FunctionNotFound as i32;
    }

    let name_bytes = unsafe { std::slice::from_raw_parts(func_name, name_len) };
    let name = match std::str::from_utf8(name_bytes) {
        Ok(s) => s,
        Err(_) => return ErrorCode::FunctionNotFound as i32,
    };

    let arg_slice = if n_args > 0 && !args.is_null() {
        unsafe { std::slice::from_raw_parts(args, n_args as usize) }
    } else {
        &[]
    };

    MODULES.with(|modules| {
        let mut modules = modules.borrow_mut();
        let Some(Some(m)) = modules.get_mut(handle as usize) else {
            return ErrorCode::InvalidHandle as i32;
        };

        let func = match m.instance.get_func(&mut m.store, name) {
            Some(f) => f,
            None => return ErrorCode::FunctionNotFound as i32,
        };

        // Convert i64 args to wasmtime Val based on function signature
        let ty = func.ty(&m.store);
        let wasm_args: Vec<Val> = ty
            .params()
            .zip(arg_slice.iter().chain(std::iter::repeat(&0i64)))
            .map(|(param_ty, &val)| match param_ty {
                ValType::I32 => Val::I32(val as i32),
                ValType::I64 => Val::I64(val),
                ValType::F32 => Val::F32(val as u32),
                ValType::F64 => Val::F64(val as u64),
                _ => Val::I32(val as i32),
            })
            .collect();

        let mut wasm_results: Vec<Val> = ty
            .results()
            .map(|rt| match rt {
                ValType::I32 => Val::I32(0),
                ValType::I64 => Val::I64(0),
                ValType::F32 => Val::F32(0),
                ValType::F64 => Val::F64(0),
                _ => Val::I32(0),
            })
            .collect();

        if func
            .call(&mut m.store, &wasm_args, &mut wasm_results)
            .is_err()
        {
            return ErrorCode::CallFailed as i32;
        }

        // Write results back
        if !results.is_null() && n_results > 0 {
            let out = unsafe { std::slice::from_raw_parts_mut(results, n_results as usize) };
            for (i, val) in wasm_results.iter().enumerate() {
                if i >= n_results as usize {
                    break;
                }
                out[i] = match val {
                    Val::I32(v) => *v as i64,
                    Val::I64(v) => *v,
                    Val::F32(v) => *v as i64,
                    Val::F64(v) => *v as i64,
                    _ => 0,
                };
            }
        }

        ErrorCode::Ok as i32
    })
}

/// Free WASM-side allocation (calls the module's wasm_dealloc export).
#[unsafe(no_mangle)]
pub extern "C" fn zenwasm_dealloc(handle: i64, offset: u32, size: u32) -> i32 {
    MODULES.with(|modules| {
        let mut modules = modules.borrow_mut();
        let Some(Some(m)) = modules.get_mut(handle as usize) else {
            return ErrorCode::InvalidHandle as i32;
        };
        match m.fn_dealloc.call(&mut m.store, (offset, size)) {
            Ok(()) => ErrorCode::Ok as i32,
            Err(_) => ErrorCode::CallFailed as i32,
        }
    })
}

/// Free a loaded module and all its resources.
#[unsafe(no_mangle)]
pub extern "C" fn zenwasm_free(handle: i64) {
    MODULES.with(|modules| {
        let mut modules = modules.borrow_mut();
        if let Some(slot) = modules.get_mut(handle as usize) {
            *slot = None;
        }
    });
}
