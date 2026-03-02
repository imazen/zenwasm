use zenwasm_api::WasmRuntime;

fn main() {
    println!("zenwasm-api size check binary");
    println!("This binary does NOT link wasmtime — it loads the plugin at runtime.");

    // Try to load the plugin (will fail if not present)
    match WasmRuntime::load("libzenwasm_abi.so") {
        Ok(runtime) => {
            println!("Plugin loaded!");
            // Try loading an invalid module
            match runtime.load_module(&[0, 0, 0, 0]) {
                Ok(_) => println!("Module loaded (unexpected)"),
                Err(e) => println!("Module load failed as expected: {e}"),
            }
        }
        Err(e) => println!("Plugin not found (expected): {e}"),
    }
}
