mod control;
mod functions;

use kagura_ast::{
    ArrowBody, AssignTarget, BinaryOp, Expr, ForInit, ObjectProperty, Param, Program, Stmt,
    TypeAnn, UnaryOp, VarDecl, VarKind,
};
use kagura_support::Span;
use tracing::instrument;

use crate::error::ParseError;
use crate::lexer::lex;
use crate::token::{Token, TokenKind};

#[instrument(skip(source), fields(bytes = source.len()))]
pub fn parse(source: &str) -> Result<Program, ParseError> {
    let tokens = lex(source)?;
    Parser::new(tokens).parse_program()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut body = Vec::new();
        while !self.at(&TokenKind::Eof) {
            body.push(self.parse_stmt()?);
        }
        Ok(Program { body })
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        let kind = self.peek().kind.clone();
        match kind {
            TokenKind::Let | TokenKind::Const => {
                let kind = if matches!(kind, TokenKind::Let) {
                    VarKind::Let
                } else {
                    VarKind::Const
                };
                Ok(Stmt::Var(self.parse_var_decl(kind, true)?))
            }
            TokenKind::Function => Ok(Stmt::Func(self.parse_func_decl()?)),
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::For => self.parse_for(),
            TokenKind::Return => self.parse_return(),
            TokenKind::Throw => self.parse_throw(),
            TokenKind::Try => self.parse_try(),
            TokenKind::Break => {
                let token = self.advance();
                self.expect(&TokenKind::Semi, "expected ';' after break")?;
                Ok(Stmt::Break { span: token.span })
            }
            TokenKind::Continue => {
                let token = self.advance();
                self.expect(&TokenKind::Semi, "expected ';' after continue")?;
                Ok(Stmt::Continue { span: token.span })
            }
            TokenKind::LBrace => Ok(Stmt::Block(self.parse_block()?)),
            TokenKind::Type => self.parse_type_alias(),
            TokenKind::Interface => self.parse_interface(),
            TokenKind::Semi => {
                let token = self.advance();
                Ok(Stmt::Empty { span: token.span })
            }
            _ => {
                let expr = self.parse_expr()?;
                let span = expr.span();
                self.expect(&TokenKind::Semi, "expected ';' after expression")?;
                Ok(Stmt::Expr { expr, span })
            }
        }
    }

    fn parse_var_decl(&mut self, kind: VarKind, expect_semi: bool) -> Result<VarDecl, ParseError> {
        let start = self.advance().span;
        let name_token = self.expect_ident("expected variable name")?;
        let name = match name_token.kind {
            TokenKind::Ident(name) => name,
            _ => unreachable!(),
        };
        let ty = if self.eat(&TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let init = if self.eat(&TokenKind::Assign) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        if matches!(kind, VarKind::Const) && init.is_none() {
            return Err(ParseError::new(
                "const declaration requires an initializer",
                name_token.span,
            ));
        }
        let end = init.as_ref().map(Expr::span).unwrap_or(name_token.span);
        if expect_semi {
            self.expect(&TokenKind::Semi, "expected ';' after declaration")?;
        }
        Ok(VarDecl {
            kind,
            name,
            ty,
            init,
            span: start.merge(end),
        })
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.expect(&TokenKind::LBrace, "expected '{'")?;
        let mut body = Vec::new();
        while !self.at(&TokenKind::RBrace) && !self.at(&TokenKind::Eof) {
            body.push(self.parse_stmt()?);
        }
        self.expect(&TokenKind::RBrace, "expected '}'")?;
        Ok(body)
    }

    fn parse_if(&mut self) -> Result<Stmt, ParseError> {
        let start = self.advance().span;
        self.expect(&TokenKind::LParen, "expected '(' after if")?;
        let cond = self.parse_expr()?;
        self.expect(&TokenKind::RParen, "expected ')' after if condition")?;
        let then_branch = Box::new(self.parse_sub_stmt()?);
        let else_branch = if self.eat(&TokenKind::Else) {
            Some(Box::new(self.parse_sub_stmt()?))
        } else {
            None
        };
        let end = else_branch
            .as_ref()
            .map(|stmt| stmt.span())
            .unwrap_or_else(|| then_branch.span());
        Ok(Stmt::If {
            cond,
            then_branch,
            else_branch,
            span: start.merge(end),
        })
    }

    fn parse_while(&mut self) -> Result<Stmt, ParseError> {
        let start = self.advance().span;
        self.expect(&TokenKind::LParen, "expected '(' after while")?;
        let cond = self.parse_expr()?;
        self.expect(&TokenKind::RParen, "expected ')' after while condition")?;
        let body = Box::new(self.parse_sub_stmt()?);
        let end = body.span();
        Ok(Stmt::While {
            cond,
            body,
            span: start.merge(end),
        })
    }

    fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        let start = self.advance().span;
        self.expect(&TokenKind::LParen, "expected '(' after for")?;
        let init = if self.at(&TokenKind::Let) || self.at(&TokenKind::Const) {
            let kind = if self.at(&TokenKind::Let) {
                VarKind::Let
            } else {
                VarKind::Const
            };
            let decl = self.parse_var_decl(kind, false)?;
            self.expect(&TokenKind::Semi, "expected ';' after for initializer")?;
            Some(ForInit::Var(decl))
        } else if self.at(&TokenKind::Semi) {
            self.advance();
            None
        } else {
            let expr = self.parse_expr()?;
            self.expect(&TokenKind::Semi, "expected ';' after for initializer")?;
            Some(ForInit::Expr(expr))
        };
        let cond = if self.at(&TokenKind::Semi) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect(&TokenKind::Semi, "expected ';' after for condition")?;
        let update = if self.at(&TokenKind::RParen) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect(&TokenKind::RParen, "expected ')' after for clauses")?;
        let body = Box::new(self.parse_sub_stmt()?);
        let end = body.span();
        Ok(Stmt::For {
            init,
            cond,
            update,
            body,
            span: start.merge(end),
        })
    }

    fn parse_sub_stmt(&mut self) -> Result<Stmt, ParseError> {
        if matches!(self.peek().kind, TokenKind::Let | TokenKind::Const) {
            return Err(ParseError::new(
                "lexical declaration is not allowed in single-statement position",
                self.peek().span,
            ));
        }
        self.parse_stmt()
    }

    fn parse_type_alias(&mut self) -> Result<Stmt, ParseError> {
        let start = self.advance().span;
        let name = self.expect_ident("expected type alias name")?;
        if self.eat(&TokenKind::Assign) {
            self.skip_until_semi()?;
        }
        self.expect(&TokenKind::Semi, "expected ';' after type alias")?;
        Ok(Stmt::TypeAlias {
            name: match name.kind {
                TokenKind::Ident(name) => name,
                _ => unreachable!(),
            },
            span: start.merge(name.span),
        })
    }

    fn parse_interface(&mut self) -> Result<Stmt, ParseError> {
        let start = self.advance().span;
        let name_token = self.expect_ident("expected interface name")?;
        if self.eat(&TokenKind::LBrace) {
            let mut depth = 1usize;
            while depth > 0 {
                let token = self.advance();
                match token.kind {
                    TokenKind::LBrace => depth += 1,
                    TokenKind::RBrace => depth -= 1,
                    TokenKind::Eof => {
                        return Err(ParseError::new("unterminated interface body", start));
                    }
                    _ => {}
                }
            }
            self.eat(&TokenKind::Semi);
        } else {
            self.expect(&TokenKind::Semi, "expected ';' after interface")?;
        }
        Ok(Stmt::Interface {
            name: match name_token.kind {
                TokenKind::Ident(name) => name,
                _ => unreachable!(),
            },
            span: start.merge(name_token.span),
        })
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_or()?;
        if self.eat(&TokenKind::Assign) {
            let value = self.parse_expr()?;
            let span = left.span().merge(value.span());
            return match left {
                Expr::Ident { name, .. } => Ok(Expr::Assign {
                    target: AssignTarget::Ident(name),
                    value: Box::new(value),
                    span,
                }),
                Expr::Member {
                    object, property, ..
                } => Ok(Expr::Assign {
                    target: AssignTarget::Member { object, property },
                    value: Box::new(value),
                    span,
                }),
                Expr::Index { object, index, .. } => Ok(Expr::Assign {
                    target: AssignTarget::Index { object, index },
                    value: Box::new(value),
                    span,
                }),
                _ => Err(ParseError::new("invalid assignment target", span)),
            };
        }
        if self.eat(&TokenKind::As) {
            let ty = self.parse_type()?;
            let span = left.span();
            return Ok(Expr::As {
                expr: Box::new(left),
                ty,
                span,
            });
        }
        Ok(left)
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_and()?;
        while self.eat(&TokenKind::OrOr) {
            let right = self.parse_and()?;
            let span = expr.span().merge(right.span());
            expr = Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(expr),
                right: Box::new(right),
                span,
            };
        }
        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_equality()?;
        while self.eat(&TokenKind::AndAnd) {
            let right = self.parse_equality()?;
            let span = expr.span().merge(right.span());
            expr = Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(expr),
                right: Box::new(right),
                span,
            };
        }
        Ok(expr)
    }

    fn parse_equality(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_relational()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::EqEq => BinaryOp::Eq,
                TokenKind::NotEq => BinaryOp::NotEq,
                TokenKind::StrictEq => BinaryOp::StrictEq,
                TokenKind::StrictNotEq => BinaryOp::StrictNotEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_relational()?;
            let span = expr.span().merge(right.span());
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
                span,
            };
        }
        Ok(expr)
    }

    fn parse_relational(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_additive()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Lt => BinaryOp::Lt,
                TokenKind::Le => BinaryOp::Le,
                TokenKind::Gt => BinaryOp::Gt,
                TokenKind::Ge => BinaryOp::Ge,
                TokenKind::In => BinaryOp::In,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive()?;
            let span = expr.span().merge(right.span());
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
                span,
            };
        }
        Ok(expr)
    }

    fn parse_additive(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_multiplicative()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            let span = expr.span().merge(right.span());
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
                span,
            };
        }
        Ok(expr)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_unary()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                TokenKind::Percent => BinaryOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            let span = expr.span().merge(right.span());
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
                span,
            };
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        let op = match self.peek().kind {
            TokenKind::Minus => Some(UnaryOp::Neg),
            TokenKind::Plus => Some(UnaryOp::Plus),
            TokenKind::Bang => Some(UnaryOp::Not),
            TokenKind::TypeOf => Some(UnaryOp::TypeOf),
            TokenKind::Void => Some(UnaryOp::Void),
            _ => None,
        };
        if let Some(op) = op {
            let start = self.advance().span;
            let expr = self.parse_unary()?;
            let span = start.merge(expr.span());
            return Ok(Expr::Unary {
                op,
                expr: Box::new(expr),
                span,
            });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.eat(&TokenKind::LParen) {
                let mut args = Vec::new();
                if !self.at(&TokenKind::RParen) {
                    loop {
                        args.push(self.parse_expr()?);
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                }
                let end = self.expect(&TokenKind::RParen, "expected ')' after arguments")?;
                let span = expr.span().merge(end.span);
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                    span,
                };
                continue;
            }
            if self.eat(&TokenKind::Dot) {
                let property = self.expect_ident("expected property name")?;
                let span = expr.span().merge(property.span);
                expr = Expr::Member {
                    object: Box::new(expr),
                    property: match property.kind {
                        TokenKind::Ident(name) => name,
                        _ => unreachable!(),
                    },
                    span,
                };
                continue;
            }
            if self.eat(&TokenKind::LBracket) {
                let index = self.parse_expr()?;
                let end = self.expect(&TokenKind::RBracket, "expected ']' after index")?;
                let span = expr.span().merge(end.span);
                expr = Expr::Index {
                    object: Box::new(expr),
                    index: Box::new(index),
                    span,
                };
                continue;
            }
            break;
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Number(value) => {
                self.advance();
                Ok(Expr::Number {
                    value,
                    span: token.span,
                })
            }
            TokenKind::Str(value) => {
                self.advance();
                Ok(Expr::String {
                    value,
                    span: token.span,
                })
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Boolean {
                    value: true,
                    span: token.span,
                })
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Boolean {
                    value: false,
                    span: token.span,
                })
            }
            TokenKind::Null => {
                self.advance();
                Ok(Expr::Null { span: token.span })
            }
            TokenKind::Undefined => {
                self.advance();
                Ok(Expr::Undefined { span: token.span })
            }
            TokenKind::This => {
                self.advance();
                Ok(Expr::This { span: token.span })
            }
            TokenKind::Ident(name) => {
                self.advance();
                if self.eat(&TokenKind::Arrow) {
                    let body = self.parse_arrow_body()?;
                    let end = body_span(&body);
                    return Ok(Expr::Arrow {
                        params: vec![Param {
                            name,
                            ty: None,
                            default: None,
                            span: token.span,
                        }],
                        body,
                        span: token.span.merge(end),
                    });
                }
                Ok(Expr::Ident {
                    name,
                    span: token.span,
                })
            }
            TokenKind::Function => self.parse_function_expr(),
            TokenKind::LParen => {
                if let Some(arrow) = self.try_parse_parenthesized_arrow()? {
                    return Ok(arrow);
                }
                let start = self.advance().span;
                let expr = self.parse_expr()?;
                let end = self.expect(&TokenKind::RParen, "expected ')' after expression")?;
                Ok(match expr {
                    Expr::Number { value, .. } => Expr::Number {
                        value,
                        span: start.merge(end.span),
                    },
                    other => other,
                })
            }
            TokenKind::LBrace => self.parse_object_literal(),
            TokenKind::LBracket => self.parse_array_literal(),
            _ => Err(ParseError::new("expected expression", token.span)),
        }
    }

    fn parse_array_literal(&mut self) -> Result<Expr, ParseError> {
        let start = self.advance().span;
        let mut elements = Vec::new();
        if !self.at(&TokenKind::RBracket) {
            loop {
                if self.at(&TokenKind::Comma) {
                    elements.push(None);
                    self.advance();
                    if self.at(&TokenKind::RBracket) {
                        break;
                    }
                    continue;
                }
                elements.push(Some(self.parse_expr()?));
                if !self.eat(&TokenKind::Comma) || self.at(&TokenKind::RBracket) {
                    break;
                }
            }
        }
        let end = self.expect(&TokenKind::RBracket, "expected ']' after array literal")?;
        Ok(Expr::Array {
            elements,
            span: start.merge(end.span),
        })
    }

    fn parse_object_literal(&mut self) -> Result<Expr, ParseError> {
        let start = self.advance().span;
        let mut properties = Vec::new();
        if !self.at(&TokenKind::RBrace) {
            loop {
                let key_token = self.peek().clone();
                let key = match key_token.kind {
                    TokenKind::Ident(key) | TokenKind::Str(key) => key,
                    TokenKind::Number(value) => format!("{value}"),
                    _ => {
                        return Err(ParseError::new(
                            "expected object property name",
                            key_token.span,
                        ));
                    }
                };
                self.advance();
                self.expect(&TokenKind::Colon, "expected ':' after object property")?;
                let value = self.parse_expr()?;
                let span = key_token.span.merge(value.span());
                properties.push(ObjectProperty { key, value, span });
                if !self.eat(&TokenKind::Comma) || self.at(&TokenKind::RBrace) {
                    break;
                }
            }
        }
        let end = self.expect(&TokenKind::RBrace, "expected '}' after object literal")?;
        Ok(Expr::Object {
            properties,
            span: start.merge(end.span),
        })
    }

    fn parse_type(&mut self) -> Result<TypeAnn, ParseError> {
        let token = self.expect_ident("expected type name")?;
        let TokenKind::Ident(name) = token.kind else {
            unreachable!()
        };
        Ok(match name.as_str() {
            "number" => TypeAnn::Number,
            "string" => TypeAnn::String,
            "boolean" => TypeAnn::Boolean,
            "void" => TypeAnn::Void,
            "any" => TypeAnn::Any,
            _ => TypeAnn::Named(name),
        })
    }

    fn skip_until_semi(&mut self) -> Result<(), ParseError> {
        while !self.at(&TokenKind::Semi) && !self.at(&TokenKind::Eof) {
            self.advance();
        }
        if self.at(&TokenKind::Eof) {
            return Err(ParseError::new(
                "expected ';' after type alias",
                self.peek().span,
            ));
        }
        Ok(())
    }

    fn expect_ident(&mut self, message: &str) -> Result<Token, ParseError> {
        if matches!(self.peek().kind, TokenKind::Ident(_)) {
            Ok(self.advance())
        } else {
            Err(ParseError::new(message, self.peek().span))
        }
    }

    fn expect(&mut self, kind: &TokenKind, message: &str) -> Result<Token, ParseError> {
        if self.at(kind) {
            Ok(self.advance())
        } else {
            Err(ParseError::new(message, self.peek().span))
        }
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.at(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn at(&self, kind: &TokenKind) -> bool {
        self.peek().kind == *kind
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len().saturating_sub(1))]
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.pos.min(self.tokens.len() - 1)].clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        token
    }
}

fn body_span(body: &ArrowBody) -> Span {
    match body {
        ArrowBody::Expr(expr) => expr.span(),
        ArrowBody::Block(body) => body
            .first()
            .map(Stmt::span)
            .unwrap_or_else(|| Span::empty(0)),
    }
}
