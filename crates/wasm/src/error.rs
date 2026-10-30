//! Error types shared by the decoder, the instantiator, and the interpreter.

use std::fmt;

use thiserror::Error;
use wasmparser::WasmFeatures;

use crate::value::ValType;

/// Decode/validation error with an optional byte offset and missing capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    message: String,
    offset: Option<u64>,
    missing_feature: Option<WasmFeatures>,
}

impl DecodeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            offset: None,
            missing_feature: None,
        }
    }

    pub fn at(offset: u64, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            offset: Some(offset),
            missing_feature: None,
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn offset(&self) -> Option<u64> {
        self.offset
    }

    /// The disabled capability required by validation, if any (e.g. memory64).
    pub fn missing_feature(&self) -> Option<WasmFeatures> {
        self.missing_feature
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.offset {
            Some(offset) => write!(f, "{} (at offset 0x{offset:x})", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for DecodeError {}

impl From<wasmparser::Error> for DecodeError {
    fn from(error: wasmparser::Error) -> Self {
        let missing_feature = error.missing_wasm_feature();
        let message = match missing_feature {
            Some(features) => format!(
                "WebAssembly feature disabled: {} ({})",
                describe_missing_feature(features),
                error.message()
            ),
            None => error.message().to_string(),
        };
        Self {
            message,
            offset: Some(error.offset()),
            missing_feature,
        }
    }
}

// 为 wasmparser 报错的 capability 选一个主名称，用于面向用户的错误信息。
fn describe_missing_feature(features: WasmFeatures) -> &'static str {
    if features.memory64() {
        "memory64"
    } else if features.multi_memory() {
        "multi-memory"
    } else if features.bulk_memory() {
        "bulk-memory"
    } else if features.reference_types() {
        "reference-types"
    } else if features.simd() {
        "simd"
    } else if features.threads() {
        "threads"
    } else if features.exceptions() {
        "exceptions"
    } else if features.gc() {
        "gc"
    } else if features.component_model() {
        "component-model"
    } else {
        "post-MVP WebAssembly feature"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Trap {
    #[error("export `{name}` not found")]
    MissingExport { name: String },
    #[error("export `{name}` is not a function")]
    ExportNotFunction { name: String },
    #[error("argument count mismatch: expected {expected}, got {actual}")]
    ArgumentCount { expected: usize, actual: usize },
    #[error("argument {index} has type {actual}, expected {expected}")]
    ArgumentType {
        index: usize,
        expected: ValType,
        actual: ValType,
    },
    #[error("operand stack underflow (at offset 0x{offset:x})")]
    StackUnderflow { offset: u64 },
    #[error("type mismatch (at offset 0x{offset:x}): expected {expected}, got {actual}")]
    TypeMismatch {
        offset: u64,
        expected: ValType,
        actual: ValType,
    },
    #[error("local index {index} is out of bounds (at offset 0x{offset:x})")]
    InvalidLocal { index: u32, offset: u64 },
    #[error("function index {index} is out of bounds")]
    InvalidFunction { index: u32 },
    #[error("unsupported operator `{operator}` (at offset 0x{offset:x})")]
    UnsupportedOperator { operator: String, offset: u64 },
    #[error("unsupported execution feature: {feature}")]
    UnsupportedFeature { feature: &'static str },
    /// Malformed body after validation; defense in depth.
    #[error("malformed function body (at offset 0x{offset:x}): {message}")]
    MalformedBody { offset: u64, message: String },
    #[error("executed `unreachable` (at offset 0x{offset:x})")]
    Unreachable { offset: u64 },
}

#[derive(Debug, Error)]
pub enum InstantiateError {
    #[error("imports are not supported yet: `{module}::{name}` ({kind})")]
    UnsupportedImport {
        module: String,
        name: String,
        kind: &'static str,
    },
    #[error("start function {index} is not supported yet")]
    UnsupportedStartFunction { index: u32 },
    #[error(transparent)]
    Memory(#[from] crate::memory::MemoryError),
}
