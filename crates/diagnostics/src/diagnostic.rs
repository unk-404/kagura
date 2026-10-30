use kagura_support::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticKind {
    Syntax,
    Type,
    Reference,
    Range,
    Runtime,
}

impl DiagnosticKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Syntax => "SyntaxError",
            Self::Type => "TypeError",
            Self::Reference => "ReferenceError",
            Self::Range => "RangeError",
            Self::Runtime => "RuntimeError",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn new(kind: DiagnosticKind, message: impl Into<String>, span: Span) -> Self {
        Self {
            kind,
            message: message.into(),
            span,
        }
    }

    pub fn syntax(message: impl Into<String>, span: Span) -> Self {
        Self::new(DiagnosticKind::Syntax, message, span)
    }
}
