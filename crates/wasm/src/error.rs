//! Error types shared by the decoder, the instantiator, and the interpreter.

use std::fmt;

use thiserror::Error;
use wasmparser::WasmFeatures;

use crate::value::ValType;

/// Error returned when a WebAssembly binary cannot be decoded or validated.
///
/// Decoding errors carry the byte offset of the offending construct whenever
/// the underlying decoder can report one. When validation failed because a
/// disabled WebAssembly feature was required, [`DecodeError::missing_feature`]
/// reports the capability that would have to be enabled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    message: String,
    offset: Option<u64>,
    missing_feature: Option<WasmFeatures>,
}

impl DecodeError {
    /// Creates an error that is not tied to a binary offset.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            offset: None,
            missing_feature: None,
        }
    }

    /// Creates an error at a specific byte offset in the module.
    pub fn at(offset: u64, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            offset: Some(offset),
            missing_feature: None,
        }
    }

    /// Returns the human-readable error message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the byte offset in the module, when known.
    pub fn offset(&self) -> Option<u64> {
        self.offset
    }

    /// Returns the disabled capability that validation required, when known.
    ///
    /// For example, decoding a memory64 module with the default v1 feature gate
    /// yields `Some(WasmFeatures::MEMORY64)`. Re-run
    /// [`crate::Module::decode_with_features`] with that flag enabled to accept
    /// the module.
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

/// A WebAssembly runtime failure observed while calling an exported function.
///
/// The MVP interpreter uses this type for ABI mismatches, operand-stack
/// failures, and constructs outside the implemented subset. Later milestones
/// will separate spec traps from "unsupported in this interpreter" errors.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Trap {
    /// The requested export does not exist.
    #[error("export `{name}` not found")]
    MissingExport {
        /// Export name that was requested.
        name: String,
    },
    /// The requested export exists but is not a function.
    #[error("export `{name}` is not a function")]
    ExportNotFunction {
        /// Export name that was requested.
        name: String,
    },
    /// The caller passed an unexpected number of arguments.
    #[error("argument count mismatch: expected {expected}, got {actual}")]
    ArgumentCount {
        /// Number of parameters declared by the function type.
        expected: usize,
        /// Number of arguments supplied by the caller.
        actual: usize,
    },
    /// An argument has the wrong WebAssembly value type.
    #[error("argument {index} has type {actual}, expected {expected}")]
    ArgumentType {
        /// Zero-based argument position.
        index: usize,
        /// Declared parameter type.
        expected: ValType,
        /// Runtime argument type.
        actual: ValType,
    },
    /// The operand stack was popped while empty.
    #[error("operand stack underflow (at offset 0x{offset:x})")]
    StackUnderflow {
        /// Byte offset of the instruction that underflowed the stack.
        offset: u64,
    },
    /// An operand or local had the wrong value type.
    #[error("type mismatch (at offset 0x{offset:x}): expected {expected}, got {actual}")]
    TypeMismatch {
        /// Byte offset of the instruction that observed the mismatch.
        offset: u64,
        /// Type required by the instruction.
        expected: ValType,
        /// Type actually found at runtime.
        actual: ValType,
    },
    /// A local index is outside the function's local index space.
    #[error("local index {index} is out of bounds (at offset 0x{offset:x})")]
    InvalidLocal {
        /// Local index encoded in the instruction.
        index: u32,
        /// Byte offset of the instruction.
        offset: u64,
    },
    /// A function index is outside the module's defined-function index space.
    #[error("function index {index} is out of bounds")]
    InvalidFunction {
        /// Function index requested by the caller.
        index: u32,
    },
    /// The interpreter reached an operator outside its implemented subset.
    #[error("unsupported operator `{operator}` (at offset 0x{offset:x})")]
    UnsupportedOperator {
        /// Debug rendering of the decoded operator.
        operator: String,
        /// Byte offset of the operator.
        offset: u64,
    },
    /// The interpreter reached a feature outside its implemented subset.
    #[error("unsupported execution feature: {feature}")]
    UnsupportedFeature {
        /// Name of the missing execution feature.
        feature: &'static str,
    },
    /// The function body could not be decoded while executing.
    ///
    /// A valid module validated by [`crate::Module::decode`] should never hit
    /// this path; it exists for defense in depth.
    #[error("malformed function body (at offset 0x{offset:x}): {message}")]
    MalformedBody {
        /// Byte offset where decoding failed.
        offset: u64,
        /// Decoder error message.
        message: String,
    },
    /// The function executed the `unreachable` instruction.
    #[error("executed `unreachable` (at offset 0x{offset:x})")]
    Unreachable {
        /// Byte offset of the `unreachable` instruction.
        offset: u64,
    },
}

/// Error returned when an instance cannot be created from a decoded module.
#[derive(Debug, Error)]
pub enum InstantiateError {
    /// Host imports are not wired up in this foundation.
    #[error("imports are not supported yet: `{module}::{name}` ({kind})")]
    UnsupportedImport {
        /// Import module name.
        module: String,
        /// Import field name.
        name: String,
        /// Import kind, for diagnostics.
        kind: &'static str,
    },
    /// The start section is validated but not executed in this foundation.
    #[error("start function {index} is not supported yet")]
    UnsupportedStartFunction {
        /// Start function index.
        index: u32,
    },
    /// Linear memory could not be created under the configured cap.
    #[error(transparent)]
    Memory(#[from] crate::memory::MemoryError),
}
