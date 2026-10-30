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
    Var,
    Let,
    Const,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Pow,
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
    InstanceOf,
    And,
    Or,
    BitOr,
    BitXor,
    BitAnd,
    Shl,
    Shr,
    UShr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
    Plus,
    TypeOf,
    Void,
    Delete,
    BitNot,
}
