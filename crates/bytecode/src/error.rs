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
