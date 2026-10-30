mod compiler;
mod error;
mod module;
mod op;

pub use compiler::compile;
pub use error::{CompileError, DecodeError};
pub use module::{Const, FunctionProto, MAGIC, MIN_VERSION, Module, VERSION};
pub use op::Op;
