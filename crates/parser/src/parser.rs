mod control;
mod functions;

use kagura_ast::{
    ArrowBody, AssignTarget, BinaryOp, DestructureDecl, Expr, ForInit, ObjectProperty, Param,
    Pattern, Program, Stmt, SwitchCase, TypeAnn, UnaryOp, VarDecl, VarKind,
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
            TokenKind::Var | TokenKind::Let | TokenKind::Const => {
                let kind = match kind {
                    TokenKind::Var => VarKind::Var,
                    TokenKind::Let => VarKind::Let,
                    _ => VarKind::Const,
                };
                if self.tokens.get(self.pos + 1).is_some_and(|token| {
                    matches!(token.kind, TokenKind::LBracket | TokenKind::LBrace)
                }) {
                    let start = self.advance().span;
                    let pattern = self.parse_binding_pattern()?;
                    self.expect(
                        &TokenKind::Assign,
                        "expected '=' after destructuring pattern",
                    )?;
                    let init = self.parse_expr()?;
                    let end = init.span();
                    self.expect(&TokenKind::Semi, "expected ';' after declaration")?;
                    Ok(Stmt::Destructure(DestructureDecl {
                        kind,
                        pattern,
                        init,
                        span: start.merge(end),
                    }))
                } else {
                    let start = self.advance().span;
                    let mut declarations = Vec::new();
                    loop {
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
                        declarations.push(VarDecl {
                            kind,
                            name,
                            ty,
                            init,
                            span: start.merge(end),
                        });
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(&TokenKind::Semi, "expected ';' after declaration")?;
                    if declarations.len() == 1 {
                        Ok(Stmt::Var(declarations.remove(0)))
                    } else {
                        Ok(Stmt::VarList(declarations))
                    }
                }
            }
            TokenKind::Class => self.parse_class_decl(),
            TokenKind::Function => Ok(Stmt::Func(self.parse_func_decl()?)),
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::For => self.parse_for(),
            TokenKind::Switch => self.parse_switch(),
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
            TokenKind::Async => {
                if matches!(
                    self.tokens.get(self.pos + 1).map(|token| &token.kind),
                    Some(TokenKind::Function)
                ) {
                    self.advance();
                    Ok(Stmt::Func(self.parse_func_decl()?))
                } else {
                    let expr = self.parse_expr()?;
                    let span = expr.span();
                    self.expect(&TokenKind::Semi, "expected ';' after expression")?;
                    Ok(Stmt::Expr { expr, span })
                }
            }
            _ => {
                let expr = self.parse_expr()?;
                let span = expr.span();
                self.expect(&TokenKind::Semi, "expected ';' after expression")?;
                Ok(Stmt::Expr { expr, span })
            }
        }
    }

    fn parse_binding_pattern(&mut self) -> Result<Pattern, ParseError> {
        if self.eat(&TokenKind::LBracket) {
            let mut elements = Vec::new();
            while !self.at(&TokenKind::RBracket) {
                if self.eat(&TokenKind::Comma) {
                    elements.push(None);
                    continue;
                }
                if self.eat(&TokenKind::DotDotDot) {
                    elements.push(Some(Pattern::Rest(Box::new(self.parse_binding_pattern()?))));
                } else {
                    elements.push(Some(self.parse_binding_element()?));
                }
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::RBracket, "expected ']' after array pattern")?;
            return Ok(Pattern::Array(elements));
        }
        if self.eat(&TokenKind::LBrace) {
            let mut properties = Vec::new();
            let mut rest = None;
            while !self.at(&TokenKind::RBrace) {
                if self.eat(&TokenKind::DotDotDot) {
                    rest = Some(Box::new(self.parse_binding_pattern()?));
                    break;
                }
                let key_token = self.advance();
                let key = match key_token.kind {
                    TokenKind::Ident(key) | TokenKind::Str(key) => key,
                    TokenKind::Number(value) => format!("{value}"),
                    _ => {
                        return Err(ParseError::new(
                            "expected object pattern property",
                            key_token.span,
                        ));
                    }
                };
                let pattern = if self.eat(&TokenKind::Colon) {
                    self.parse_binding_element()?
                } else {
                    let pattern = Pattern::Ident(key.clone());
                    self.parse_pattern_default(pattern)?
                };
                properties.push((key, pattern));
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::RBrace, "expected '}' after object pattern")?;
            return Ok(Pattern::Object { properties, rest });
        }
        let name = self.expect_ident("expected binding name")?;
        let TokenKind::Ident(name) = name.kind else {
            unreachable!()
        };
        Ok(Pattern::Ident(name))
    }

    fn parse_binding_element(&mut self) -> Result<Pattern, ParseError> {
        let pattern = self.parse_binding_pattern()?;
        self.parse_pattern_default(pattern)
    }

    fn parse_pattern_default(&mut self, pattern: Pattern) -> Result<Pattern, ParseError> {
        if self.eat(&TokenKind::Assign) {
            let default = self.parse_expr()?;
            return Ok(Pattern::Default {
                pattern: Box::new(pattern),
                default,
            });
        }
        Ok(pattern)
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
        if matches!(kind, VarKind::Const) && init.is_none() && expect_semi {
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
        if matches!(
            self.peek().kind,
            TokenKind::Var | TokenKind::Let | TokenKind::Const
        ) {
            let kind = match self.peek().kind {
                TokenKind::Var => VarKind::Var,
                TokenKind::Let => VarKind::Let,
                _ => VarKind::Const,
            };
            let declaration = self.parse_var_decl(kind, false)?;
            if self.eat(&TokenKind::In) {
                let object = self.parse_expr()?;
                self.expect(&TokenKind::RParen, "expected ')' after for-in object")?;
                let body = Box::new(self.parse_sub_stmt()?);
                let span = start.merge(body.span());
                return Ok(Stmt::ForIn {
                    declaration: Some(declaration),
                    target: None,
                    object,
                    body,
                    span,
                });
            }
            if self.at_of() {
                self.advance();
                let iterable = self.parse_expr()?;
                self.expect(&TokenKind::RParen, "expected ')' after for-of iterable")?;
                let body = Box::new(self.parse_sub_stmt()?);
                let span = start.merge(body.span());
                return Ok(Stmt::ForOf {
                    declaration: Some(declaration),
                    target: None,
                    iterable,
                    body,
                    span,
                });
            }
            self.expect(&TokenKind::Semi, "expected ';' after for initializer")?;
            return self.parse_for_tail(start, Some(ForInit::Var(declaration)));
        }
        if self.at(&TokenKind::Semi) {
            self.advance();
            return self.parse_for_tail(start, None);
        }
        let expr = self.parse_expr()?;
        if self.at_of() {
            self.advance();
            let target = self.expr_to_target(expr, start)?;
            let iterable = self.parse_expr()?;
            self.expect(&TokenKind::RParen, "expected ')' after for-of iterable")?;
            let body = Box::new(self.parse_sub_stmt()?);
            let span = start.merge(body.span());
            return Ok(Stmt::ForOf {
                declaration: None,
                target: Some(target),
                iterable,
                body,
                span,
            });
        }
        if let Expr::Binary {
            op: BinaryOp::In,
            left,
            right,
            span,
        } = expr
        {
            let target = self.expr_to_target(*left, span)?;
            self.expect(&TokenKind::RParen, "expected ')' after for-in object")?;
            let body = Box::new(self.parse_sub_stmt()?);
            let span = start.merge(body.span());
            return Ok(Stmt::ForIn {
                declaration: None,
                target: Some(target),
                object: *right,
                body,
                span,
            });
        }
        self.expect(&TokenKind::Semi, "expected ';' after for initializer")?;
        self.parse_for_tail(start, Some(ForInit::Expr(expr)))
    }

    fn parse_for_tail(&mut self, start: Span, init: Option<ForInit>) -> Result<Stmt, ParseError> {
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

    fn parse_switch(&mut self) -> Result<Stmt, ParseError> {
        let start = self.advance().span;
        self.expect(&TokenKind::LParen, "expected '(' after switch")?;
        let discriminant = self.parse_expr()?;
        self.expect(&TokenKind::RParen, "expected ')' after switch expression")?;
        self.expect(&TokenKind::LBrace, "expected '{' after switch")?;
        let mut cases = Vec::new();
        let mut seen_default = false;
        while !self.at(&TokenKind::RBrace) {
            let case_start = self.peek().span;
            let test = if self.eat(&TokenKind::Case) {
                Some(self.parse_expr()?)
            } else if self.eat(&TokenKind::Default) {
                if seen_default {
                    return Err(ParseError::new("duplicate default clause", case_start));
                }
                seen_default = true;
                None
            } else {
                return Err(ParseError::new("expected case or default", case_start));
            };
            self.expect(&TokenKind::Colon, "expected ':' after switch clause")?;
            let mut body = Vec::new();
            while !self.at(&TokenKind::Case)
                && !self.at(&TokenKind::Default)
                && !self.at(&TokenKind::RBrace)
            {
                body.push(self.parse_stmt()?);
            }
            let span = body
                .last()
                .map(|stmt| case_start.merge(stmt.span()))
                .unwrap_or(case_start);
            cases.push(SwitchCase { test, body, span });
        }
        let end = self.expect(&TokenKind::RBrace, "expected '}' after switch")?;
        Ok(Stmt::Switch {
            discriminant,
            cases,
            span: start.merge(end.span),
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
        let mut left = self.parse_or()?;
        while self.eat(&TokenKind::Nullish) {
            let right = self.parse_or()?;
            let span = left.span().merge(right.span());
            let read = left.clone();
            left = Expr::Conditional {
                condition: Box::new(Expr::Binary {
                    op: BinaryOp::Eq,
                    left: Box::new(read.clone()),
                    right: Box::new(Expr::Null { span }),
                    span,
                }),
                then_branch: Box::new(right),
                else_branch: Box::new(read),
                span,
            };
        }
        let left = if self.eat(&TokenKind::Question) {
            let then_branch = self.parse_expr()?;
            self.expect(&TokenKind::Colon, "expected ':' in conditional expression")?;
            let else_branch = self.parse_or()?;
            let span = left.span().merge(else_branch.span());
            Expr::Conditional {
                condition: Box::new(left),
                then_branch: Box::new(then_branch),
                else_branch: Box::new(else_branch),
                span,
            }
        } else {
            left
        };
        if let Some(op) = match self.peek().kind {
            TokenKind::AndEq => Some(BinaryOp::And),
            TokenKind::OrEq => Some(BinaryOp::Or),
            TokenKind::NullishEq => Some(BinaryOp::Eq),
            _ => None,
        } {
            self.advance();
            let right = self.parse_expr()?;
            let span = left.span().merge(right.span());
            let read = left.clone();
            let target = match left {
                Expr::Ident { name, .. } => AssignTarget::Ident(name),
                Expr::Member {
                    object, property, ..
                } => AssignTarget::Member { object, property },
                Expr::Index { object, index, .. } => AssignTarget::Index { object, index },
                _ => return Err(ParseError::new("invalid assignment target", span)),
            };
            let assignment = Expr::Assign {
                target,
                value: Box::new(right),
                span,
            };
            return Ok(match op {
                BinaryOp::And | BinaryOp::Or => Expr::Binary {
                    op,
                    left: Box::new(read),
                    right: Box::new(assignment),
                    span,
                },
                _ => Expr::Conditional {
                    condition: Box::new(Expr::Binary {
                        op: BinaryOp::Eq,
                        left: Box::new(read.clone()),
                        right: Box::new(Expr::Null { span }),
                        span,
                    }),
                    then_branch: Box::new(assignment),
                    else_branch: Box::new(read),
                    span,
                },
            });
        }
        if let Some(op) = match self.peek().kind {
            TokenKind::PlusEq => Some(BinaryOp::Add),
            TokenKind::MinusEq => Some(BinaryOp::Sub),
            TokenKind::StarEq => Some(BinaryOp::Mul),
            TokenKind::SlashEq => Some(BinaryOp::Div),
            TokenKind::PercentEq => Some(BinaryOp::Mod),
            _ => None,
        } {
            self.advance();
            let right = self.parse_expr()?;
            let span = left.span().merge(right.span());
            let value = Expr::Binary {
                op,
                left: Box::new(left.clone()),
                right: Box::new(right),
                span,
            };
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
        if self.eat(&TokenKind::Assign) {
            let value = self.parse_expr()?;
            let span = left.span().merge(value.span());
            if matches!(&left, Expr::Array { .. } | Expr::Object { .. }) {
                let pattern = self.expr_to_pattern(left)?;
                return Ok(Expr::DestructureAssign {
                    pattern: Box::new(pattern),
                    value: Box::new(value),
                    span,
                });
            }
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

    fn expr_to_pattern(&mut self, expr: Expr) -> Result<Pattern, ParseError> {
        match expr {
            Expr::Ident { name, .. } => Ok(Pattern::Ident(name)),
            Expr::Assign {
                target: AssignTarget::Ident(name),
                value,
                ..
            } => Ok(Pattern::Default {
                pattern: Box::new(Pattern::Ident(name)),
                default: *value,
            }),
            Expr::Array {
                elements, spreads, ..
            } => {
                let mut patterns = Vec::with_capacity(elements.len());
                for (index, element) in elements.into_iter().enumerate() {
                    if spreads.get(index).copied().unwrap_or(false) {
                        let Some(element) = element else {
                            continue;
                        };
                        patterns.push(Some(Pattern::Rest(Box::new(
                            self.expr_to_pattern(element)?,
                        ))));
                    } else if let Some(element) = element {
                        patterns.push(Some(self.expr_to_pattern(element)?));
                    } else {
                        patterns.push(None);
                    }
                }
                Ok(Pattern::Array(patterns))
            }
            Expr::Object { properties, .. } => {
                let mut result = Vec::new();
                for property in properties {
                    if property.spread {
                        return Err(ParseError::new(
                            "object spread is not valid in an assignment pattern",
                            property.span,
                        ));
                    }
                    result.push((property.key, self.expr_to_pattern(property.value)?));
                }
                Ok(Pattern::Object {
                    properties: result,
                    rest: None,
                })
            }
            other => Err(ParseError::new(
                "invalid destructuring assignment target",
                other.span(),
            )),
        }
    }

    fn at_of(&self) -> bool {
        matches!(&self.peek().kind, TokenKind::Ident(name) if name == "of")
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
                TokenKind::InstanceOf => BinaryOp::InstanceOf,
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
        let mut expr = self.parse_exponentiation()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                TokenKind::Percent => BinaryOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_exponentiation()?;
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

    fn parse_exponentiation(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_unary()?;
        if self.eat(&TokenKind::StarStar) {
            let right = self.parse_exponentiation()?;
            let span = left.span().merge(right.span());
            return Ok(Expr::Binary {
                op: BinaryOp::Pow,
                left: Box::new(left),
                right: Box::new(right),
                span,
            });
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        if self.at(&TokenKind::New) {
            return self.parse_new();
        }
        if matches!(self.peek().kind, TokenKind::Plus | TokenKind::Minus)
            && self
                .tokens
                .get(self.pos + 1)
                .is_some_and(|token| token.kind == self.peek().kind)
        {
            let decrement = self.peek().kind == TokenKind::Minus;
            let start = self.advance().span;
            self.advance();
            let target = self.parse_postfix()?;
            let span = start.merge(target.span());
            let target = self.expr_to_target(target, span)?;
            return Ok(Expr::Update {
                target,
                prefix: true,
                decrement,
                span,
            });
        }
        let op = match self.peek().kind {
            TokenKind::Minus => Some(UnaryOp::Neg),
            TokenKind::Plus => Some(UnaryOp::Plus),
            TokenKind::Bang => Some(UnaryOp::Not),
            TokenKind::TypeOf => Some(UnaryOp::TypeOf),
            TokenKind::Void => Some(UnaryOp::Void),
            TokenKind::Delete => Some(UnaryOp::Delete),
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
        let expr = self.parse_primary()?;
        self.parse_suffix(expr)
    }

    fn parse_suffix(&mut self, mut expr: Expr) -> Result<Expr, ParseError> {
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
                let (property, property_span) = self.expect_property_name()?;
                let span = expr.span().merge(property_span);
                expr = Expr::Member {
                    object: Box::new(expr),
                    property,
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
            if matches!(self.peek().kind, TokenKind::Plus | TokenKind::Minus)
                && self
                    .tokens
                    .get(self.pos + 1)
                    .is_some_and(|token| token.kind == self.peek().kind)
            {
                let decrement = self.peek().kind == TokenKind::Minus;
                self.advance();
                let end = self.advance().span;
                let span = expr.span().merge(end);
                let target = self.expr_to_target(expr, span)?;
                return Ok(Expr::Update {
                    target,
                    prefix: false,
                    decrement,
                    span,
                });
            }
            break;
        }
        Ok(expr)
    }

    fn parse_new(&mut self) -> Result<Expr, ParseError> {
        let start = self.advance().span;
        let mut callee = self.parse_primary()?;
        loop {
            if self.eat(&TokenKind::Dot) {
                let (property, property_span) = self.expect_property_name()?;
                let span = callee.span().merge(property_span);
                callee = Expr::Member {
                    object: Box::new(callee),
                    property,
                    span,
                };
                continue;
            }
            if self.eat(&TokenKind::LBracket) {
                let index = self.parse_expr()?;
                let end = self.expect(&TokenKind::RBracket, "expected ']' after index")?;
                let span = callee.span().merge(end.span);
                callee = Expr::Index {
                    object: Box::new(callee),
                    index: Box::new(index),
                    span,
                };
                continue;
            }
            break;
        }
        let mut args = Vec::new();
        if self.eat(&TokenKind::LParen) {
            if !self.at(&TokenKind::RParen) {
                loop {
                    args.push(self.parse_expr()?);
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
            }
            let end = self.expect(&TokenKind::RParen, "expected ')' after arguments")?;
            let span = start.merge(end.span);
            let expr = Expr::New {
                callee: Box::new(callee),
                args,
                span,
            };
            return self.parse_suffix(expr);
        }
        let span = start.merge(callee.span());
        let expr = Expr::New {
            callee: Box::new(callee),
            args,
            span,
        };
        self.parse_suffix(expr)
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
                            pattern: None,
                            ty: None,
                            default: None,
                            rest: false,
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
            TokenKind::Await => {
                let start = self.advance().span;
                let expr = self.parse_unary()?;
                let span = start.merge(expr.span());
                Ok(Expr::Await {
                    expr: Box::new(expr),
                    span,
                })
            }
            TokenKind::Async => {
                let start = self.advance().span;
                if self.at(&TokenKind::Function) {
                    return self.parse_function_expr();
                }
                if let TokenKind::Ident(name) = self.peek().kind.clone() {
                    let param_span = self.advance().span;
                    let params = vec![Param {
                        name,
                        pattern: None,
                        ty: None,
                        default: None,
                        rest: false,
                        span: param_span,
                    }];
                    self.expect(&TokenKind::Arrow, "expected '=>' after async parameter")?;
                    let body = self.parse_arrow_body()?;
                    let end = body_span(&body);
                    return Ok(Expr::Arrow {
                        params,
                        body,
                        span: start.merge(end),
                    });
                }
                let params = self.parse_params("expected '(' after async")?;
                self.expect(&TokenKind::Arrow, "expected '=>' after async parameters")?;
                let body = self.parse_arrow_body()?;
                let end = body_span(&body);
                Ok(Expr::Arrow {
                    params,
                    body,
                    span: start.merge(end),
                })
            }
            TokenKind::Super => {
                let start = self.advance().span;
                if self.eat(&TokenKind::Dot) {
                    let (property, property_span) = self.expect_property_name()?;
                    return Ok(Expr::SuperMember {
                        property,
                        span: start.merge(property_span),
                    });
                }
                self.expect(&TokenKind::LParen, "expected '(' after super")?;
                let mut args = Vec::new();
                if !self.at(&TokenKind::RParen) {
                    loop {
                        args.push(self.parse_expr()?);
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                }
                let end = self.expect(&TokenKind::RParen, "expected ')' after super arguments")?;
                Ok(Expr::SuperCall {
                    args,
                    span: start.merge(end.span),
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
        let mut spreads = Vec::new();
        if !self.at(&TokenKind::RBracket) {
            loop {
                if self.at(&TokenKind::Comma) {
                    elements.push(None);
                    spreads.push(false);
                    self.advance();
                    if self.at(&TokenKind::RBracket) {
                        break;
                    }
                    continue;
                }
                let spread = self.eat(&TokenKind::DotDotDot);
                elements.push(Some(self.parse_expr()?));
                spreads.push(spread);
                if !self.eat(&TokenKind::Comma) || self.at(&TokenKind::RBracket) {
                    break;
                }
            }
        }
        let end = self.expect(&TokenKind::RBracket, "expected ']' after array literal")?;
        Ok(Expr::Array {
            elements,
            spreads,
            span: start.merge(end.span),
        })
    }

    fn parse_object_literal(&mut self) -> Result<Expr, ParseError> {
        let start = self.advance().span;
        let mut properties = Vec::new();
        if !self.at(&TokenKind::RBrace) {
            loop {
                let key_token = self.peek().clone();
                if self.eat(&TokenKind::DotDotDot) {
                    let value = self.parse_expr()?;
                    let span = key_token.span.merge(value.span());
                    properties.push(ObjectProperty {
                        key: String::new(),
                        value,
                        spread: true,
                        span,
                    });
                    if !self.eat(&TokenKind::Comma) || self.at(&TokenKind::RBrace) {
                        break;
                    }
                    continue;
                }
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
                properties.push(ObjectProperty {
                    key,
                    value,
                    spread: false,
                    span,
                });
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

    fn expr_to_target(&self, expr: Expr, span: Span) -> Result<AssignTarget, ParseError> {
        match expr {
            Expr::Ident { name, .. } => Ok(AssignTarget::Ident(name)),
            Expr::Member {
                object, property, ..
            } => Ok(AssignTarget::Member { object, property }),
            Expr::Index { object, index, .. } => Ok(AssignTarget::Index { object, index }),
            _ => Err(ParseError::new("invalid update target", span)),
        }
    }

    fn expect_property_name(&mut self) -> Result<(String, Span), ParseError> {
        let token = self.advance();
        let name = match token.kind {
            TokenKind::Ident(name) => name,
            TokenKind::Undefined => "undefined".into(),
            TokenKind::True => "true".into(),
            TokenKind::False => "false".into(),
            TokenKind::Null => "null".into(),
            _ => {
                return Err(ParseError::new("expected property name", token.span));
            }
        };
        Ok((name, token.span))
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
