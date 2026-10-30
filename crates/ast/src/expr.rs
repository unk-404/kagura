use kagura_support::Span;

use crate::types::{BinaryOp, TypeAnn, UnaryOp};

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Number {
        value: f64,
        span: Span,
    },
    String {
        value: String,
        span: Span,
    },
    Boolean {
        value: bool,
        span: Span,
    },
    Null {
        span: Span,
    },
    Undefined {
        span: Span,
    },
    Ident {
        name: String,
        span: Span,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
        span: Span,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    Assign {
        target: AssignTarget,
        value: Box<Expr>,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    Member {
        object: Box<Expr>,
        property: String,
        span: Span,
    },
    Object {
        properties: Vec<ObjectProperty>,
        span: Span,
    },
    As {
        expr: Box<Expr>,
        ty: TypeAnn,
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum AssignTarget {
    Ident(String),
    Member { object: Box<Expr>, property: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectProperty {
    pub key: String,
    pub value: Expr,
    pub span: Span,
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Self::Number { span, .. }
            | Self::String { span, .. }
            | Self::Boolean { span, .. }
            | Self::Null { span }
            | Self::Undefined { span }
            | Self::Ident { span, .. }
            | Self::Unary { span, .. }
            | Self::Binary { span, .. }
            | Self::Assign { span, .. }
            | Self::Call { span, .. }
            | Self::Member { span, .. }
            | Self::Object { span, .. }
            | Self::As { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Option<TypeAnn>,
    pub span: Span,
}
