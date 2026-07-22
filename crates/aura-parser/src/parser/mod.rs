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
            let attributes = self.parse_attributes()?;
            let is_test = attributes
                .iter()
                .any(|attribute| attribute.name.name == "test");
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
                    t.attributes = attributes;
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
                    c.attributes = attributes;
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
                    i.attributes = attributes;
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
                    e.attributes = attributes;
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
                    c.attributes = attributes;
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
                    c.attributes = attributes;
                    classes.push(c);
                }
                TokenKind::Fun => {
                    let mut f = self.parse_fun()?;
                    f.is_pub = is_pub;
                    f.attributes = attributes;
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
                    f.attributes = attributes;
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

    pub(crate) fn parse_attributes(&mut self) -> Result<Vec<Attribute>, ParseError> {
        let mut attributes = Vec::new();
        while matches!(self.peek().kind, TokenKind::At) {
            attributes.push(self.parse_attribute()?);
        }
        Ok(attributes)
    }

    fn parse_attribute(&mut self) -> Result<Attribute, ParseError> {
        let start = self.expect(TokenKind::At, "`@`")?.span.start;
        let name = self.expect_ident()?;
        let (args, end) = if matches!(self.peek().kind, TokenKind::LParen) {
            self.bump();
            let args = self.parse_attribute_args()?;
            let end = self
                .expect(TokenKind::RParen, "`)` after attribute arguments")?
                .span
                .end;
            (args, end)
        } else {
            (Vec::new(), name.span.end)
        };
        Ok(Attribute {
            name,
            args,
            span: Span::new(start, end),
        })
    }

    fn parse_attribute_args(&mut self) -> Result<Vec<AttributeArg>, ParseError> {
        let mut args = Vec::new();
        if matches!(self.peek().kind, TokenKind::RParen) {
            return Ok(args);
        }
        loop {
            let arg = if let TokenKind::Ident(_) = &self.peek().kind {
                let lookahead = self.tokens.get(self.idx + 1).map(|token| &token.kind);
                if matches!(lookahead, Some(TokenKind::Eq)) {
                    let name = self.expect_ident()?;
                    let start = name.span.start;
                    self.bump();
                    let value = self.parse_attribute_value()?;
                    let span = Span::new(start, value.span().end);
                    AttributeArg::Named { name, value, span }
                } else {
                    AttributeArg::Positional(self.parse_attribute_value()?)
                }
            } else {
                AttributeArg::Positional(self.parse_attribute_value()?)
            };
            args.push(arg);
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
                if matches!(self.peek().kind, TokenKind::RParen) {
                    break;
                }
                continue;
            }
            break;
        }
        Ok(args)
    }

    fn parse_attribute_value(&mut self) -> Result<AttributeValue, ParseError> {
        let token = self.bump();
        match token.kind {
            TokenKind::Ident(name) => {
                let ident = Ident {
                    name,
                    span: token.span,
                };
                if matches!(self.peek().kind, TokenKind::LParen) {
                    self.bump();
                    let args = self.parse_attribute_args()?;
                    let end = self
                        .expect(TokenKind::RParen, "`)` after nested attribute value")?
                        .span
                        .end;
                    Ok(AttributeValue::Call {
                        name: ident,
                        args,
                        span: Span::new(token.span.start, end),
                    })
                } else {
                    Ok(AttributeValue::Ident(ident))
                }
            }
            TokenKind::Int(value) => Ok(AttributeValue::Int {
                value,
                span: token.span,
            }),
            TokenKind::String(value) => Ok(AttributeValue::String {
                value,
                span: token.span,
            }),
            TokenKind::True => Ok(AttributeValue::Bool {
                value: true,
                span: token.span,
            }),
            TokenKind::False => Ok(AttributeValue::Bool {
                value: false,
                span: token.span,
            }),
            TokenKind::LBracket => {
                let mut values = Vec::new();
                if !matches!(self.peek().kind, TokenKind::RBracket) {
                    loop {
                        values.push(self.parse_attribute_value()?);
                        if matches!(self.peek().kind, TokenKind::Comma) {
                            self.bump();
                            if matches!(self.peek().kind, TokenKind::RBracket) {
                                break;
                            }
                            continue;
                        }
                        break;
                    }
                }
                let end = self
                    .expect(TokenKind::RBracket, "`]` after attribute array")?
                    .span
                    .end;
                Ok(AttributeValue::Array {
                    values,
                    span: Span::new(token.span.start, end),
                })
            }
            other => Err(ParseError {
                message: format!("expected attribute value, found {other:?}"),
                span: token.span,
            }),
        }
    }
}
