mod compiler;
mod error;
mod module;
mod op;

pub mod builtins;
pub use compiler::compile;
pub use error::{CompileError, DecodeError};
pub use module::{
    Const, FeedbackKind, FunctionProto, MAGIC, MIN_VERSION, Module, SAFEPOINT_DEPTH_UNKNOWN,
    Safepoint, VERSION,
};
pub use op::Op;
