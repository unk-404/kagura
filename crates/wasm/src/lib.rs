//! Kagura WebAssembly v1 foundation.
//!
//! This crate owns the WebAssembly side of the runtime: validation-aware
//! decoding, the runtime value model, linear memory, instance creation, and a
//! small self-written interpreter for an MVP instruction subset.
//!
//! `wasmparser` is used for binary decoding and specification validation only.
//! The executor is implemented in this crate; no external execution engine
//! (wasmtime, wasmer, ...) is embedded.
//!
//! # Implemented
//!
//! - [`Module::decode`]: MVP validation with byte-offset errors, then metadata
//!   extraction for types, functions, locals, memories, imports, exports, the
//!   start function, and aggregate statistics.
//! - [`Module::decode_with_features`]: the same decoder behind an explicit
//!   [`WasmFeatures`] capability gate. The v1 default is MVP plus floats;
//!   memory64 is opt-in through this gate.
//! - [`Value`], [`ValType`], [`FuncType`], [`Instance`], [`Memory`], and
//!   [`MemoryCap`] for the runtime side. Values keep `i64` and memory sizes use
//!   canonical `u64`, so the 64-bit path does not start from a 32-bit model.
//! - Linear memory over a committed byte buffer with an explicit address width,
//!   per-memory cap, bounds-checked `u64` accessors, and a `grow`
//!   implementation.
//! - An MVP interpreter subset: `i32.const`, `i32.add`, `i32.sub`, `i32.mul`,
//!   `local.get`, `local.set`, `local.tee`, `nop`, `unreachable`, `return`, and
//!   the function-level `end`.
//!
//! # Not implemented yet
//!
//! - Host imports, start-function execution, data/element segment application,
//!   tables, globals, calls, structured control flow, floats, and any post-MVP
//!   proposal beyond the memory64 architecture hook. Unsupported operators and
//!   instantiations fail with explicit errors rather than being silently
//!   accepted.
//! - KMem-backed reservation/commit. [`MemoryReservation`] documents the
//!   intended interface while the storage is still a byte buffer.

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
