use kagura_bytecode::{CompileError, DecodeError};
use kagura_diagnostics::{Diagnostic, DiagnosticKind};
use kagura_interp::{RuntimeError, RuntimeErrorKind};
use kagura_parser::ParseError;
use kagura_support::Span;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RunError {
    #[error("{0}")]
    Parse(#[from] ParseError),
    #[error("{0}")]
    Compile(#[from] CompileError),
    #[error("{0}")]
    Decode(#[from] DecodeError),
    #[error("{0}")]
    Runtime(#[from] RuntimeError),
}

impl RunError {
    pub fn diagnostic(&self) -> Diagnostic {
        match self {
            Self::Parse(error) => Diagnostic::syntax(&error.message, error.span),
            Self::Compile(error) => Diagnostic::syntax(&error.message, error.span),
            Self::Decode(error) => Diagnostic::syntax(error.to_string(), Span::default()),
            Self::Runtime(error) => {
                let kind = match error.kind {
                    RuntimeErrorKind::Syntax => DiagnosticKind::Syntax,
                    RuntimeErrorKind::Type => DiagnosticKind::Type,
                    RuntimeErrorKind::Reference => DiagnosticKind::Reference,
                    RuntimeErrorKind::Range => DiagnosticKind::Range,
                    RuntimeErrorKind::Runtime => DiagnosticKind::Runtime,
                };
                Diagnostic::new(kind, &error.message, error.span)
            }
        }
    }
}
