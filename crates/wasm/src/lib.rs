//! WebAssembly v1 foundation: `wasmparser` validation, a self-written MVP
//! interpreter, and 32/64-bit-ready linear memory. No external executor is
//! embedded.

mod error;
mod instance;
mod memory;
mod module;
mod value;

pub use error::{DecodeError, InstantiateError, Trap};
pub use instance::Instance;
pub use memory::{
    AddressWidth, DEFAULT_MAX_PAGES_32, DEFAULT_MAX_PAGES_64, MAX_PAGES_32, MAX_PAGES_64, Memory,
    MemoryCap, MemoryError, MemoryReservation, MemoryType, WASM_PAGE_SIZE,
};
pub use module::{
    Export, ExportKind, Function, Import, ImportKind, Module, ModuleStats, default_features,
};
pub use value::{FuncType, ValType, Value};
pub use wasmparser::WasmFeatures;
