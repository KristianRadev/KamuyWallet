//! # Kamuy Wallet WASM
//!
//! WASM bindings for Kamuy Wallet.

use wasm_bindgen::prelude::*;

// Use wee_alloc for smaller binary size
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

/// Initialize the WASM module
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// WASM-compatible wallet
#[wasm_bindgen]
pub struct WasmWallet {
    // TODO: Implement WASM wallet
}

#[wasm_bindgen]
impl WasmWallet {
    /// Create a new wallet
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {}
    }

    /// Get wallet version
    #[wasm_bindgen(js_name = "version")]
    pub fn version(&self) -> String {
        kamuy_wallet_core::VERSION.to_string()
    }
}

/// Generate a new session ID
#[wasm_bindgen(js_name = "generateSessionId")]
pub fn generate_session_id() -> Vec<u8> {
    let mut id = [0u8; 32];
    getrandom::getrandom(&mut id).unwrap();
    id.to_vec()
}

/// Test function
#[wasm_bindgen]
pub fn hello() -> String {
    "Hello from Kamuy Wallet WASM!".to_string()
}

#[cfg(test)]
mod tests {
    use wasm_bindgen_test::*;
    use super::hello;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn test_hello() {
        assert_eq!(hello(), "Hello from Kamuy Wallet WASM!");
    }
}
