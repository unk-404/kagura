use kagura_ast::{ArrowBody, Expr, FuncDecl, Param, Stmt};
use kagura_support::Span;

use crate::error::ParseError;
use crate::token::TokenKind;

use super::{Parser, body_span};

impl Parser {
    fn parse_params(&mut self, opener: &str) -> Result<Vec<Param>, ParseError> {
        self.expect(&TokenKind::LParen, opener)?;
        let mut params = Vec::new();
        if !self.at(&TokenKind::RParen) {
            loop {
                let rest = self.eat(&TokenKind::DotDotDot);
                let param_token = self.expect_ident("expected parameter name")?;
                let param_name = match param_token.kind {
                    TokenKind::Ident(name) => name,
                    _ => unreachable!(),
                };
                let ty = if self.eat(&TokenKind::Colon) {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                let default = if !rest && self.eat(&TokenKind::Assign) {
                    Some(Box::new(self.parse_expr()?))
                } else {
                    None
                };
                params.push(Param {
                    name: param_name,
                    ty,
                    default,
                    rest,
                    span: param_token.span,
                });
                if rest || !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RParen, "expected ')' after parameters")?;
        Ok(params)
    }

    pub(super) fn parse_function_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.advance().span;
        let name = if let TokenKind::Ident(name) = self.peek().kind.clone() {
            self.advance();
            Some(name)
        } else {
            None
        };
        let params = self.parse_params("expected '(' after function")?;
        if self.eat(&TokenKind::Colon) {
            self.parse_type()?;
        }
        let body_start = self.peek().span;
        let body = self.parse_block()?;
        let end = body_start.merge(
            body.last()
                .map(Stmt::span)
                .unwrap_or_else(|| Span::empty(body_start.start)),
        );
        Ok(Expr::Function {
            name,
            params,
            body,
            span: start.merge(end),
        })
    }

    pub(super) fn parse_arrow_body(&mut self) -> Result<ArrowBody, ParseError> {
        if self.at(&TokenKind::LBrace) {
            Ok(ArrowBody::Block(self.parse_block()?))
        } else {
            Ok(ArrowBody::Expr(Box::new(self.parse_expr()?)))
        }
    }

    pub(super) fn try_parse_parenthesized_arrow(&mut self) -> Result<Option<Expr>, ParseError> {
        let save = self.pos;
        let start = self.peek().span;
        let params = match self.parse_params("expected '(' after arrow parameters") {
            Ok(params) => params,
            Err(_) => {
                self.pos = save;
                return Ok(None);
            }
        };
        if !self.eat(&TokenKind::Arrow) {
            self.pos = save;
            return Ok(None);
        }
        let body = self.parse_arrow_body()?;
        let end = body_span(&body);
        Ok(Some(Expr::Arrow {
            params,
            body,
            span: start.merge(end),
        }))
    }

    pub(super) fn parse_func_decl(&mut self) -> Result<FuncDecl, ParseError> {
        let start = self.advance().span;
        let name_token = self.expect_ident("expected function name")?;
        let name = match name_token.kind {
            TokenKind::Ident(name) => name,
            _ => unreachable!(),
        };
        self.expect(&TokenKind::LParen, "expected '(' after function name")?;
        let mut params = Vec::new();
        if !self.at(&TokenKind::RParen) {
            loop {
                let rest = self.eat(&TokenKind::DotDotDot);
                let param_token = self.expect_ident("expected parameter name")?;
                let param_name = match param_token.kind {
                    TokenKind::Ident(name) => name,
                    _ => unreachable!(),
                };
                let ty = if self.eat(&TokenKind::Colon) {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                let default = if !rest && self.eat(&TokenKind::Assign) {
                    Some(Box::new(self.parse_expr()?))
                } else {
                    None
                };
                params.push(Param {
                    name: param_name,
                    ty,
                    default,
                    rest,
                    span: param_token.span,
                });
                if rest || !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RParen, "expected ')' after parameters")?;
        let ret = if self.eat(&TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let body_start = self.peek().span;
        let body = self.parse_block()?;
        let end = body_start.merge(
            body.last()
                .map(Stmt::span)
                .unwrap_or_else(|| Span::empty(body_start.start)),
        );
        Ok(FuncDecl {
            name,
            params,
            ret,
            body,
            span: start.merge(end),
        })
    }
}
