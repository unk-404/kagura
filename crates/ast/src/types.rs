#[derive(Clone, Debug, PartialEq)]
pub enum TypeAnn {
    Number,
    String,
    Boolean,
    Void,
    Any,
    Named(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarKind {
    Let,
    Const,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    StrictEq,
    NotEq,
    StrictNotEq,
    In,
    And,
    Or,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
    Plus,
}
