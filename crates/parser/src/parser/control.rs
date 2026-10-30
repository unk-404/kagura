use kagura_ast::Stmt;

use crate::error::ParseError;
use crate::token::TokenKind;

use super::Parser;

impl Parser {
    pub(super) fn parse_throw(&mut self) -> Result<Stmt, ParseError> {
        let start = self.advance().span;
        let value = self.parse_expr()?;
        self.expect(&TokenKind::Semi, "expected ';' after throw")?;
        let span = start.merge(value.span());
        Ok(Stmt::Throw { value, span })
    }

    pub(super) fn parse_try(&mut self) -> Result<Stmt, ParseError> {
        let start = self.advance().span;
        let block = self.parse_block()?;
        let mut catch_param = None;
        let mut catch_block = None;
        if self.eat(&TokenKind::Catch) {
            if self.eat(&TokenKind::LParen) {
                let token = self.expect_ident("expected catch parameter")?;
                let TokenKind::Ident(name) = token.kind else {
                    unreachable!()
                };
                catch_param = Some(name);
                self.expect(&TokenKind::RParen, "expected ')' after catch parameter")?;
            }
            catch_block = Some(self.parse_block()?);
        }
        let finally_block = if self.eat(&TokenKind::Finally) {
            Some(self.parse_block()?)
        } else {
            None
        };
        if catch_block.is_none() {
            return Err(ParseError::new(
                "try requires a catch or finally block",
                start,
            ));
        }
        Ok(Stmt::Try {
            block,
            catch_param,
            catch_block,
            finally_block,
            span: start,
        })
    }

    pub(super) fn parse_return(&mut self) -> Result<Stmt, ParseError> {
        let start = self.advance().span;
        let value = if self.at(&TokenKind::Semi) || self.at(&TokenKind::RBrace) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect(&TokenKind::Semi, "expected ';' after return")?;
        Ok(Stmt::Return { value, span: start })
    }
}
