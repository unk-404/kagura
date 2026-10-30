use kagura_support::Span;

use crate::stmt::Stmt;
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
    This {
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
    Conditional {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
        span: Span,
    },
    Assign {
        target: AssignTarget,
        value: Box<Expr>,
        span: Span,
    },
    DestructureAssign {
        pattern: Box<crate::Pattern>,
        value: Box<Expr>,
        span: Span,
    },
    SuperCall {
        args: Vec<Expr>,
        span: Span,
    },
    SuperMember {
        property: String,
        span: Span,
    },
    Await {
        expr: Box<Expr>,
        span: Span,
    },
    Update {
        target: AssignTarget,
        prefix: bool,
        decrement: bool,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    New {
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
    Array {
        elements: Vec<Option<Expr>>,
        spreads: Vec<bool>,
        span: Span,
    },
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    Function {
        name: Option<String>,
        params: Vec<Param>,
        body: Vec<Stmt>,
        span: Span,
    },
    Arrow {
        params: Vec<Param>,
        body: ArrowBody,
        span: Span,
    },
    As {
        expr: Box<Expr>,
        ty: TypeAnn,
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArrowBody {
    Expr(Box<Expr>),
    Block(Vec<Stmt>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum AssignTarget {
    Ident(String),
    Member { object: Box<Expr>, property: String },
    Index { object: Box<Expr>, index: Box<Expr> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectProperty {
    pub key: String,
    pub value: Expr,
    pub spread: bool,
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
            | Self::This { span }
            | Self::Ident { span, .. }
            | Self::Unary { span, .. }
            | Self::Binary { span, .. }
            | Self::Conditional { span, .. }
            | Self::Assign { span, .. }
            | Self::DestructureAssign { span, .. }
            | Self::SuperCall { span, .. }
            | Self::SuperMember { span, .. }
            | Self::Await { span, .. }
            | Self::Update { span, .. }
            | Self::Call { span, .. }
            | Self::New { span, .. }
            | Self::Member { span, .. }
            | Self::Object { span, .. }
            | Self::Array { span, .. }
            | Self::Index { span, .. }
            | Self::Function { span, .. }
            | Self::Arrow { span, .. }
            | Self::As { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub pattern: Option<crate::Pattern>,
    pub ty: Option<TypeAnn>,
    pub default: Option<Box<Expr>>,
    pub rest: bool,
    pub span: Span,
}
