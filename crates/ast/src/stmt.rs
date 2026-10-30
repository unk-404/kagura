use kagura_support::Span;

use crate::expr::{Expr, Param};
use crate::types::{TypeAnn, VarKind};

#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VarDecl {
    pub kind: VarKind,
    pub name: String,
    pub ty: Option<TypeAnn>,
    pub init: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FuncDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Option<TypeAnn>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SwitchCase {
    pub test: Option<Expr>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ForInit {
    Var(VarDecl),
    Expr(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    Ident(String),
    Array(Vec<Option<Pattern>>),
    Object(Vec<(String, Pattern)>),
    Default {
        pattern: Box<Pattern>,
        default: Expr,
    },
    Rest(Box<Pattern>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct DestructureDecl {
    pub kind: VarKind,
    pub pattern: Pattern,
    pub init: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    Var(VarDecl),
    Destructure(DestructureDecl),
    Func(FuncDecl),
    Block(Vec<Stmt>),
    If {
        cond: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
        span: Span,
    },
    While {
        cond: Expr,
        body: Box<Stmt>,
        span: Span,
    },
    For {
        init: Option<ForInit>,
        cond: Option<Expr>,
        update: Option<Expr>,
        body: Box<Stmt>,
        span: Span,
    },
    ForIn {
        declaration: Option<VarDecl>,
        target: Option<crate::expr::AssignTarget>,
        object: Expr,
        body: Box<Stmt>,
        span: Span,
    },
    Switch {
        discriminant: Expr,
        cases: Vec<SwitchCase>,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    Throw {
        value: Expr,
        span: Span,
    },
    Try {
        block: Vec<Stmt>,
        catch_param: Option<String>,
        catch_block: Option<Vec<Stmt>>,
        finally_block: Option<Vec<Stmt>>,
        span: Span,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    Expr {
        expr: Expr,
        span: Span,
    },
    TypeAlias {
        name: String,
        span: Span,
    },
    Interface {
        name: String,
        span: Span,
    },
    Empty {
        span: Span,
    },
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Self::Var(decl) => decl.span,
            Self::Destructure(decl) => decl.span,
            Self::Func(decl) => decl.span,
            Self::Block(body) => body
                .first()
                .map(Stmt::span)
                .unwrap_or_else(|| Span::empty(0)),
            Self::If { span, .. }
            | Self::While { span, .. }
            | Self::For { span, .. }
            | Self::ForIn { span, .. }
            | Self::Switch { span, .. }
            | Self::Return { span, .. }
            | Self::Throw { span, .. }
            | Self::Try { span, .. }
            | Self::Break { span }
            | Self::Continue { span }
            | Self::Expr { span, .. }
            | Self::TypeAlias { span, .. }
            | Self::Interface { span, .. }
            | Self::Empty { span } => *span,
        }
    }
}
