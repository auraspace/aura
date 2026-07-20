//! Declarations, types, and paths.

use aura_ast::*;
use aura_lexer::TokenKind;

use super::Parser;
use crate::error::ParseError;

impl Parser {
    pub(crate) fn parse_enum(&mut self) -> Result<EnumDecl, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::Enum, "`enum`")?;
        let name = self.expect_ident()?;
        let mut type_params = self.parse_type_params_opt()?;
        self.apply_where_clause(&mut type_params)?;
        self.expect(TokenKind::LBrace, "`{`")?;
        let mut variants = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace | TokenKind::Eof) {
            variants.push(self.parse_enum_variant()?);
        }
        let end = self.expect(TokenKind::RBrace, "`}`")?.span.end;
        if variants.is_empty() {
            return Err(ParseError {
                message: "enum must have at least one variant".into(),
                span: Span::new(start, end),
            });
        }
        Ok(EnumDecl {
            is_pub: false,
            origin_package: String::new(),
            name,
            type_params,
            variants,
            span: Span::new(start, end),
        })
    }

    pub(crate) fn parse_enum_variant(&mut self) -> Result<EnumVariant, ParseError> {
        let start = self.peek().span.start;
        // `case Name` or bare `Name` (unit-style Color { Red, Green })
        if matches!(self.peek().kind, TokenKind::Case) {
            self.bump();
        }
        let name = self.expect_ident()?;
        let mut fields = Vec::new();
        if matches!(self.peek().kind, TokenKind::LParen) {
            self.bump();
            if !matches!(self.peek().kind, TokenKind::RParen) {
                loop {
                    // Named payload fields only: `value: T`
                    fields.push(self.parse_param()?);
                    if matches!(self.peek().kind, TokenKind::Comma) {
                        self.bump();
                        continue;
                    }
                    break;
                }
            }
            self.expect(TokenKind::RParen, "`)`")?;
        }
        // optional comma between variants
        if matches!(self.peek().kind, TokenKind::Comma) {
            self.bump();
        }
        let end = name.span.end;
        Ok(EnumVariant {
            name,
            fields,
            span: Span::new(start, end),
        })
    }

    pub(crate) fn parse_interface(&mut self) -> Result<InterfaceDecl, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::Interface, "`interface`")?;
        let name = self.expect_ident()?;
        // C7i/C8c: `interface Iterable<E> { … }` — mono implements supported.
        let type_params = self.parse_type_params_opt()?;
        self.expect(TokenKind::LBrace, "`{`")?;
        let mut methods = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace | TokenKind::Eof) {
            if matches!(self.peek().kind, TokenKind::Pub) {
                self.bump();
            }
            methods.push(self.parse_method_sig()?);
        }
        let end = self.expect(TokenKind::RBrace, "`}`")?.span.end;
        Ok(InterfaceDecl {
            is_pub: false,
            origin_package: String::new(),
            name,
            type_params,
            methods,
            span: Span::new(start, end),
        })
    }

    /// `fun name(...): T` without body (interface member).
    pub(crate) fn parse_method_sig(&mut self) -> Result<MethodSig, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::Fun, "`fun`")?;
        let name = self.expect_ident()?;
        self.expect(TokenKind::LParen, "`(`")?;
        let mut params = Vec::new();
        if !matches!(self.peek().kind, TokenKind::RParen) {
            loop {
                params.push(self.parse_param()?);
                if matches!(self.peek().kind, TokenKind::Comma) {
                    self.bump();
                    continue;
                }
                break;
            }
        }
        self.expect(TokenKind::RParen, "`)`")?;
        let return_type = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        let end = return_type
            .as_ref()
            .map(|t| t.span.end)
            .unwrap_or(name.span.end);
        Ok(MethodSig {
            name,
            params,
            return_type,
            span: Span::new(start, end),
        })
    }

    pub(crate) fn parse_path(&mut self) -> Result<Path, ParseError> {
        let first = self.expect_ident()?;
        let start = first.span.start;
        let mut segments = vec![first];
        while matches!(self.peek().kind, TokenKind::Dot) {
            self.bump();
            segments.push(self.expect_ident()?);
        }
        let end = segments.last().map(|s| s.span.end).unwrap_or(start);
        Ok(Path {
            segments,
            span: Span::new(start, end),
        })
    }

    pub(crate) fn parse_nominal(&mut self, kind: NominalKind) -> Result<ClassDecl, ParseError> {
        let start = self.peek().span.start;
        match kind {
            NominalKind::Class => {
                self.expect(TokenKind::Class, "`class`")?;
            }
            NominalKind::Struct => {
                self.expect(TokenKind::Struct, "`struct`")?;
            }
        }
        let name = self.expect_ident()?;
        let mut type_params = self.parse_type_params_opt()?;
        self.expect(TokenKind::LParen, "`(`")?;
        let mut fields = Vec::new();
        if !matches!(self.peek().kind, TokenKind::RParen) {
            loop {
                fields.push(self.parse_field()?);
                if matches!(self.peek().kind, TokenKind::Comma) {
                    self.bump();
                    continue;
                }
                break;
            }
        }
        self.expect(TokenKind::RParen, "`)`")?;
        // Optional `: Iface, Iface2` — classes only
        let mut implements = Vec::new();
        if matches!(self.peek().kind, TokenKind::Colon) {
            if kind == NominalKind::Struct {
                return Err(ParseError {
                    message: "structs cannot implement interfaces (use a class)".into(),
                    span: self.peek().span,
                });
            }
            self.bump();
            loop {
                // C8c: `Iface` or `Iface<T, …>` (TypeRef, not bare Ident).
                implements.push(self.parse_type()?);
                if matches!(self.peek().kind, TokenKind::Comma) {
                    self.bump();
                    continue;
                }
                break;
            }
        }
        self.apply_where_clause(&mut type_params)?;
        self.expect(TokenKind::LBrace, "`{`")?;
        let mut methods = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace | TokenKind::Eof) {
            if matches!(self.peek().kind, TokenKind::Pub) {
                self.bump();
            }
            methods.push(self.parse_fun()?);
        }
        let end = self.expect(TokenKind::RBrace, "`}`")?.span.end;
        Ok(ClassDecl {
            is_pub: false,
            origin_package: String::new(),
            kind,
            name,
            type_params,
            implements,
            fields,
            methods,
            span: Span::new(start, end),
        })
    }

    /// `TypeParams?` = `<` TypeParam (`,` TypeParam)* `>`
    /// TypeParam = Ident (`:` Ident)?  — single inline bound only (multi via `where`).
    pub(crate) fn parse_type_params_opt(&mut self) -> Result<Vec<TypeParam>, ParseError> {
        if !matches!(self.peek().kind, TokenKind::Lt) {
            return Ok(Vec::new());
        }
        self.bump();
        let mut params = Vec::new();
        loop {
            let name = self.expect_ident()?;
            let mut bounds = Vec::new();
            if matches!(self.peek().kind, TokenKind::Colon) {
                self.bump();
                bounds.push(self.expect_ident()?);
            }
            params.push(TypeParam { name, bounds });
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
                continue;
            }
            break;
        }
        self.expect(TokenKind::Gt, "`>`")?;
        Ok(params)
    }

    /// Optional `where T : Bound (, U : Bound)*` — merges into type param bounds.
    pub(crate) fn apply_where_clause(
        &mut self,
        type_params: &mut [TypeParam],
    ) -> Result<(), ParseError> {
        if !matches!(self.peek().kind, TokenKind::Where) {
            return Ok(());
        }
        self.bump();
        if type_params.is_empty() {
            return Err(ParseError {
                message: "`where` requires type parameters".into(),
                span: self.peek().span,
            });
        }
        loop {
            let param_name = self.expect_ident()?;
            self.expect(TokenKind::Colon, "`:` after type parameter in where")?;
            let bound = self.expect_ident()?;
            let Some(tp) = type_params
                .iter_mut()
                .find(|p| p.name.name == param_name.name)
            else {
                return Err(ParseError {
                    message: format!(
                        "`where` refers to unknown type parameter `{}`",
                        param_name.name
                    ),
                    span: param_name.span,
                });
            };
            if !tp.bounds.iter().any(|b| b.name == bound.name) {
                tp.bounds.push(bound);
            }
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
                continue;
            }
            break;
        }
        Ok(())
    }

    pub(crate) fn parse_field(&mut self) -> Result<FieldDecl, ParseError> {
        let start = self.peek().span.start;
        let mutable = match self.peek().kind {
            TokenKind::Val => {
                self.bump();
                false
            }
            TokenKind::Var => {
                self.bump();
                true
            }
            _ => {
                return Err(ParseError {
                    message: "expected `val` or `var` in primary constructor".into(),
                    span: self.peek().span,
                });
            }
        };
        let name = self.expect_ident()?;
        self.expect(TokenKind::Colon, "`:`")?;
        let ty = self.parse_type()?;
        let end = ty.span.end;
        Ok(FieldDecl {
            mutable,
            name,
            ty,
            span: Span::new(start, end),
        })
    }

    pub(crate) fn parse_fun(&mut self) -> Result<FunDecl, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::Fun, "`fun`")?;
        let name = self.expect_ident()?;
        let mut type_params = self.parse_type_params_opt()?;
        self.expect(TokenKind::LParen, "`(`")?;
        let mut params = Vec::new();
        if !matches!(self.peek().kind, TokenKind::RParen) {
            loop {
                params.push(self.parse_param()?);
                if matches!(self.peek().kind, TokenKind::Comma) {
                    self.bump();
                    continue;
                }
                break;
            }
        }
        self.expect(TokenKind::RParen, "`)`")?;
        let return_type = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        self.apply_where_clause(&mut type_params)?;
        // C9e: expression body `fun f(): T = expr` desugars to `{ return expr }`.
        let body = if matches!(self.peek().kind, TokenKind::Eq) {
            self.bump();
            let expr = self.parse_expr(0)?;
            let end = expr.span().end;
            let ret_span = Span::new(expr.span().start, end);
            Block {
                stmts: vec![Stmt::Return(ReturnStmt {
                    value: Some(expr),
                    span: ret_span,
                })],
                span: Span::new(start, end),
            }
        } else {
            self.parse_block()?
        };
        let end = body.span.end;
        Ok(FunDecl {
            is_pub: false,
            origin_package: String::new(),
            is_test: false,
            name,
            type_params,
            params,
            return_type,
            body,
            span: Span::new(start, end),
        })
    }

    pub(crate) fn parse_param(&mut self) -> Result<Param, ParseError> {
        let name = self.expect_ident()?;
        let start = name.span.start;
        self.expect(TokenKind::Colon, "`:`")?;
        let ty = self.parse_type()?;
        let end = ty.span.end;
        Ok(Param {
            name,
            ty,
            span: Span::new(start, end),
        })
    }

    pub(crate) fn parse_type(&mut self) -> Result<TypeRef, ParseError> {
        let first = self.expect_ident()?;
        let start = first.span.start;
        // C3u: `Alias.Type` package-qualified type name.
        let (qualifier, name) = if matches!(self.peek().kind, TokenKind::Dot) {
            self.bump();
            let name = self.expect_ident()?;
            (Some(first), name)
        } else {
            (None, first)
        };
        let type_args = self.parse_type_args_opt()?;
        let nullable = if matches!(self.peek().kind, TokenKind::Question) {
            self.bump();
            true
        } else {
            false
        };
        let end = if nullable {
            self.tokens[self.idx.saturating_sub(1)].span.end
        } else if let Some(last) = type_args.last() {
            last.span.end
        } else {
            name.span.end
        };
        Ok(TypeRef {
            qualifier,
            name,
            type_args,
            nullable,
            span: Span::new(start, end),
        })
    }

    pub(crate) fn parse_type_args_opt(&mut self) -> Result<Vec<TypeRef>, ParseError> {
        if !matches!(self.peek().kind, TokenKind::Lt) {
            return Ok(Vec::new());
        }
        self.bump();
        let mut args = Vec::new();
        loop {
            args.push(self.parse_type()?);
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
                continue;
            }
            break;
        }
        self.expect(TokenKind::Gt, "`>`")?;
        Ok(args)
    }
}
