//! Recursive-descent parser core.

use aura_ast::*;
use aura_lexer::{Token, TokenKind};

use crate::error::ParseError;

mod decl;
mod expr;
mod stmt;

pub(crate) struct Parser {
    tokens: Vec<Token>,
    idx: usize,
}

impl Parser {
    pub(crate) fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, idx: 0 }
    }

    pub(crate) fn peek(&self) -> &Token {
        &self.tokens[self.idx]
    }

    pub(crate) fn bump(&mut self) -> Token {
        let t = self.tokens[self.idx].clone();
        if self.idx + 1 < self.tokens.len() {
            self.idx += 1;
        }
        t
    }

    pub(crate) fn expect(&mut self, kind: TokenKind, what: &str) -> Result<Token, ParseError> {
        if self.peek().kind == kind {
            return Ok(self.bump());
        }
        Err(ParseError {
            message: format!("expected {what}, found {:?}", self.peek().kind),
            span: self.peek().span,
        })
    }

    pub(crate) fn expect_ident(&mut self) -> Result<Ident, ParseError> {
        match &self.peek().kind {
            TokenKind::Ident(name) => {
                let span = self.peek().span;
                let name = name.clone();
                self.bump();
                Ok(Ident { name, span })
            }
            _ => Err(ParseError {
                message: format!("expected identifier, found {:?}", self.peek().kind),
                span: self.peek().span,
            }),
        }
    }

    pub(crate) fn parse_file(&mut self) -> Result<File, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::Package, "`package`")?;
        let package = self.parse_path()?;
        let mut functions = Vec::new();
        let mut classes = Vec::new();
        let mut interfaces = Vec::new();
        let mut enums = Vec::new();
        while !matches!(self.peek().kind, TokenKind::Eof) {
            let is_test = self.parse_test_attr()?;
            if matches!(self.peek().kind, TokenKind::Pub) {
                self.bump();
            }
            match self.peek().kind {
                TokenKind::Interface => {
                    if is_test {
                        return Err(ParseError {
                            message: "`@test` only applies to functions".into(),
                            span: self.peek().span,
                        });
                    }
                    interfaces.push(self.parse_interface()?);
                }
                TokenKind::Enum => {
                    if is_test {
                        return Err(ParseError {
                            message: "`@test` only applies to functions".into(),
                            span: self.peek().span,
                        });
                    }
                    enums.push(self.parse_enum()?);
                }
                TokenKind::Class => {
                    if is_test {
                        return Err(ParseError {
                            message: "`@test` only applies to functions".into(),
                            span: self.peek().span,
                        });
                    }
                    classes.push(self.parse_nominal(NominalKind::Class)?);
                }
                TokenKind::Struct => {
                    if is_test {
                        return Err(ParseError {
                            message: "`@test` only applies to functions".into(),
                            span: self.peek().span,
                        });
                    }
                    classes.push(self.parse_nominal(NominalKind::Struct)?);
                }
                TokenKind::Fun => {
                    let mut f = self.parse_fun()?;
                    f.is_test = is_test;
                    if is_test {
                        if !f.params.is_empty() {
                            return Err(ParseError {
                                message: "`@test` functions must take no parameters".into(),
                                span: f.name.span,
                            });
                        }
                        if !f.type_params.is_empty() {
                            return Err(ParseError {
                                message: "`@test` functions cannot be generic".into(),
                                span: f.name.span,
                            });
                        }
                    }
                    functions.push(f);
                }
                _ => {
                    return Err(ParseError {
                        message: format!(
                            "expected `interface`, `enum`, `class`, `struct`, or `fun`, found {:?}",
                            self.peek().kind
                        ),
                        span: self.peek().span,
                    });
                }
            }
        }
        let end = self.peek().span.end;
        Ok(File {
            package,
            interfaces,
            enums,
            classes,
            functions,
            span: Span::new(start, end),
        })
    }

    /// Optional `@test` (only attribute in C3d).
    pub(crate) fn parse_test_attr(&mut self) -> Result<bool, ParseError> {
        if !matches!(self.peek().kind, TokenKind::At) {
            return Ok(false);
        }
        let at = self.bump();
        let name = self.expect_ident()?;
        if name.name != "test" {
            return Err(ParseError {
                message: format!("unknown attribute `@{}` (only `@test` in C3d)", name.name),
                span: Span::new(at.span.start, name.span.end),
            });
        }
        Ok(true)
    }

}
