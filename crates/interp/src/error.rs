use kagura_diagnostics::{Diagnostic, DiagnosticKind};
use kagura_support::Span;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeErrorKind {
    Type,
    Reference,
    Range,
    Runtime,
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct RuntimeError {
    pub kind: RuntimeErrorKind,
    pub message: String,
    pub span: Span,
}

impl RuntimeError {
    pub fn new(kind: RuntimeErrorKind, message: impl Into<String>, span: Span) -> Self {
        Self {
            kind,
            message: message.into(),
            span,
        }
    }

    pub fn type_error(message: impl Into<String>, span: Span) -> Self {
        Self::new(RuntimeErrorKind::Type, message, span)
    }

    pub fn reference(message: impl Into<String>, span: Span) -> Self {
        Self::new(RuntimeErrorKind::Reference, message, span)
    }

    pub fn range(message: impl Into<String>, span: Span) -> Self {
        Self::new(RuntimeErrorKind::Range, message, span)
    }

    pub fn diagnostic(&self) -> Diagnostic {
        let kind = match self.kind {
            RuntimeErrorKind::Type => DiagnosticKind::Type,
            RuntimeErrorKind::Reference => DiagnosticKind::Reference,
            RuntimeErrorKind::Range => DiagnosticKind::Range,
            RuntimeErrorKind::Runtime => DiagnosticKind::Runtime,
        };
        Diagnostic::new(kind, &self.message, self.span)
    }
}
