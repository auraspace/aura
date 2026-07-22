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
            // `join` is also an async operation keyword. In declaration and
            // path positions it is unambiguous as an identifier; expression
            // parsing still handles `join(...)` through `parse_prefix`.
            TokenKind::Join => {
                let span = self.peek().span;
                self.bump();
                Ok(Ident {
                    name: "join".into(),
                    span,
                })
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
        let mut imports = Vec::new();
        while matches!(self.peek().kind, TokenKind::Import) {
            imports.push(self.parse_import()?);
        }
        let mut functions = Vec::new();
        let mut async_functions = Vec::new();
        let mut classes = Vec::new();
        let mut interfaces = Vec::new();
        let mut enums = Vec::new();
        let mut type_aliases = Vec::new();
        let mut consts = Vec::new();
        while !matches!(self.peek().kind, TokenKind::Eof) {
            let is_test = self.parse_test_attr()?;
            let is_pub = if matches!(self.peek().kind, TokenKind::Pub) {
                self.bump();
                true
            } else {
                false
            };
            match self.peek().kind {
                TokenKind::Type => {
                    if is_test {
                        return Err(ParseError {
                            message: "`@test` only applies to functions".into(),
                            span: self.peek().span,
                        });
                    }
                    let mut t = self.parse_type_alias()?;
                    t.is_pub = is_pub;
                    type_aliases.push(t);
                }
                TokenKind::Const => {
                    if is_test {
                        return Err(ParseError {
                            message: "`@test` only applies to functions".into(),
                            span: self.peek().span,
                        });
                    }
                    let mut c = self.parse_const()?;
                    c.is_pub = is_pub;
                    consts.push(c);
                }
                TokenKind::Interface => {
                    if is_test {
                        return Err(ParseError {
                            message: "`@test` only applies to functions".into(),
                            span: self.peek().span,
                        });
                    }
                    let mut i = self.parse_interface()?;
                    i.is_pub = is_pub;
                    interfaces.push(i);
                }
                TokenKind::Enum => {
                    if is_test {
                        return Err(ParseError {
                            message: "`@test` only applies to functions".into(),
                            span: self.peek().span,
                        });
                    }
                    let mut e = self.parse_enum()?;
                    e.is_pub = is_pub;
                    enums.push(e);
                }
                TokenKind::Class => {
                    if is_test {
                        return Err(ParseError {
                            message: "`@test` only applies to functions".into(),
                            span: self.peek().span,
                        });
                    }
                    let mut c = self.parse_nominal(NominalKind::Class)?;
                    c.is_pub = is_pub;
                    classes.push(c);
                }
                TokenKind::Struct => {
                    if is_test {
                        return Err(ParseError {
                            message: "`@test` only applies to functions".into(),
                            span: self.peek().span,
                        });
                    }
                    let mut c = self.parse_nominal(NominalKind::Struct)?;
                    c.is_pub = is_pub;
                    classes.push(c);
                }
                TokenKind::Fun => {
                    let mut f = self.parse_fun()?;
                    f.is_pub = is_pub;
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
                TokenKind::Async => {
                    let mut f = self.parse_async_fun()?;
                    f.is_pub = is_pub;
                    f.is_test = is_test;
                    if is_test {
                        return Err(ParseError {
                            message: "`@test` functions must not be async".into(),
                            span: f.name.span,
                        });
                    }
                    async_functions.push(f);
                }
                _ => {
                    return Err(ParseError {
                        message: format!(
                            "expected `type`, `const`, `interface`, `enum`, `class`, `struct`, or `fun`, found {:?}",
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
            imports,
            interfaces,
            enums,
            classes,
            type_aliases,
            consts,
            functions,
            async_functions,
            span: Span::new(start, end),
        })
    }

    /// `import path` or `import path as Ident`.
    pub(crate) fn parse_import(&mut self) -> Result<ImportDecl, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::Import, "`import`")?;
        let path = self.parse_path()?;
        let alias = if matches!(self.peek().kind, TokenKind::As) {
            self.bump();
            Some(self.expect_ident()?)
        } else {
            None
        };
        let end = alias.as_ref().map(|a| a.span.end).unwrap_or(path.span.end);
        Ok(ImportDecl {
            path,
            alias,
            origin_package: String::new(),
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
