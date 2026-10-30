use kagura_support::Span;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TokenKind {
    Ident(String),
    Number(f64),
    Str(String),
    Let,
    Const,
    Function,
    If,
    Else,
    While,
    For,
    Return,
    Throw,
    Try,
    Catch,
    Finally,
    Break,
    Continue,
    True,
    False,
    Null,
    Undefined,
    This,
    Type,
    Interface,
    As,
    In,
    TypeOf,
    Void,
    Delete,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Semi,
    Comma,
    Colon,
    Assign,
    Arrow,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Lt,
    Le,
    Gt,
    Ge,
    EqEq,
    NotEq,
    StrictEq,
    StrictNotEq,
    AndAnd,
    OrOr,
    Bang,
    Dot,
    DotDotDot,
    Eof,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub(crate) fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}
