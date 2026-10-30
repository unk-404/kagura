use kagura_support::Span;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("{message}")]
pub struct CompileError {
    pub message: String,
    pub span: Span,
}

impl CompileError {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }
}

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("bytecode is truncated")]
    Truncated,
    #[error("invalid bytecode magic")]
    BadMagic,
    #[error("unsupported bytecode version {0}")]
    UnsupportedVersion(u16),
    #[error("invalid constant tag {0}")]
    InvalidConstant(u8),
    #[error("invalid constant encoding")]
    InvalidConstantEncoding,
    #[error("invalid UTF-8 in bytecode")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("entry function index out of range")]
    InvalidEntry,
    #[error("trailing bytes after bytecode module")]
    TrailingBytes,
}
