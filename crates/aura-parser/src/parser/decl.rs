//! Declarations, types, and paths.

use aura_ast::*;
use aura_lexer::TokenKind;

use super::Parser;
use crate::error::ParseError;

impl Parser {
    /// F1: `extern "C" fun name(...): T` has no Aura body. Link and target
    /// metadata is carried by one explicit `@foreign(...)` attribute.
    pub(crate) fn parse_foreign_fun(
        &mut self,
        attributes: &[Attribute],
    ) -> Result<ForeignDecl, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::Extern, "`extern`")?;
        let convention_token = self.bump();
        let convention = match convention_token.kind {
            TokenKind::String(name) if name == "C" => ForeignCallingConvention::C,
            TokenKind::String(name) => ForeignCallingConvention::Other {
                name,
                span: convention_token.span,
            },
            other => {
                return Err(ParseError {
                    message: format!(
                        "expected calling-convention string after `extern`, found {other:?}"
                    ),
                    span: convention_token.span,
                });
            }
        };
        self.expect(TokenKind::Fun, "`fun` after foreign calling convention")?;
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
        let end = return_type.as_ref().map_or(name.span.end, |t| t.span.end);
        let (library, target, link, abi, failure) = foreign_metadata(attributes);
        Ok(ForeignDecl {
            is_pub: false,
            origin_package: String::new(),
            attributes: Vec::new(),
            name,
            params,
            return_type,
            convention,
            library,
            target,
            link,
            abi,
            failure,
            span: Span::new(start, end),
        })
    }

    pub(crate) fn parse_enum(&mut self) -> Result<EnumDecl, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::Enum, "`enum`")?;
        let name = self.expect_ident()?;
        let mut type_params = self.parse_type_params_opt()?;
        self.apply_where_clause(&mut type_params)?;
        self.expect(TokenKind::LBrace, "`{`")?;
        let mut variants = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace | TokenKind::Eof) {
            let attributes = self.parse_attributes()?;
            let mut variant = self.parse_enum_variant()?;
            variant.attributes = attributes;
            variants.push(variant);
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
            attributes: Vec::new(),
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
            attributes: Vec::new(),
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
            let attributes = self.parse_attributes()?;
            if matches!(self.peek().kind, TokenKind::Pub) {
                self.bump();
            }
            let mut method = self.parse_method_sig()?;
            method.attributes = attributes;
            methods.push(method);
        }
        let end = self.expect(TokenKind::RBrace, "`}`")?.span.end;
        Ok(InterfaceDecl {
            is_pub: false,
            origin_package: String::new(),
            attributes: Vec::new(),
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
            attributes: Vec::new(),
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
                let attributes = self.parse_attributes()?;
                let mut field = self.parse_field()?;
                field.attributes = attributes;
                fields.push(field);
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
            let attributes = self.parse_attributes()?;
            if matches!(self.peek().kind, TokenKind::Pub) {
                self.bump();
            }
            let mut method = self.parse_fun()?;
            method.attributes = attributes;
            methods.push(method);
        }
        let end = self.expect(TokenKind::RBrace, "`}`")?.span.end;
        Ok(ClassDecl {
            is_pub: false,
            origin_package: String::new(),
            attributes: Vec::new(),
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
            attributes: Vec::new(),
            mutable,
            name,
            ty,
            span: Span::new(start, end),
        })
    }

    /// C9f: `type Name = Type`
    pub(crate) fn parse_type_alias(&mut self) -> Result<TypeAliasDecl, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::Type, "`type`")?;
        let name = self.expect_ident()?;
        self.expect(TokenKind::Eq, "`=`")?;
        let ty = self.parse_type()?;
        let end = ty.span.end;
        Ok(TypeAliasDecl {
            is_pub: false,
            origin_package: String::new(),
            attributes: Vec::new(),
            name,
            ty,
            span: Span::new(start, end),
        })
    }

    /// C9g: `const Name: Type = expr` (literal-ish expr checked in sema).
    pub(crate) fn parse_const(&mut self) -> Result<ConstDecl, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::Const, "`const`")?;
        let name = self.expect_ident()?;
        self.expect(TokenKind::Colon, "`:`")?;
        let ty = self.parse_type()?;
        self.expect(TokenKind::Eq, "`=`")?;
        let value = self.parse_expr(0)?;
        let end = value.span().end;
        Ok(ConstDecl {
            is_pub: false,
            origin_package: String::new(),
            attributes: Vec::new(),
            name,
            ty,
            value,
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
            attributes: Vec::new(),
            is_test: false,
            name,
            type_params,
            params,
            return_type,
            body,
            span: Span::new(start, end),
        })
    }

    /// C22f: `async fun name(...): T { ... }`.
    pub(crate) fn parse_async_fun(&mut self) -> Result<AsyncFunDecl, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::Async, "`async`")?;
        self.expect(TokenKind::Fun, "`fun` after `async`")?;
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
        let body = if matches!(self.peek().kind, TokenKind::Eq) {
            self.bump();
            let expr = self.parse_expr(0)?;
            let end = expr.span().end;
            Block {
                stmts: vec![Stmt::Return(ReturnStmt {
                    value: Some(expr),
                    span: Span::new(start, end),
                })],
                span: Span::new(start, end),
            }
        } else {
            self.parse_block()?
        };
        let end = body.span.end;
        Ok(AsyncFunDecl {
            is_pub: false,
            origin_package: String::new(),
            attributes: Vec::new(),
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
        let attributes = self.parse_attributes()?;
        let name = self.expect_ident()?;
        let start = name.span.start;
        self.expect(TokenKind::Colon, "`:`")?;
        let ty = self.parse_type()?;
        let end = ty.span.end;
        Ok(Param {
            attributes,
            name,
            ty,
            span: Span::new(start, end),
        })
    }

    pub(crate) fn parse_type(&mut self) -> Result<TypeRef, ParseError> {
        // C21b: `ref T` is intentionally parsed as a marker on the underlying
        // type so the existing compiler representation remains compatible.
        if matches!(&self.peek().kind, TokenKind::Ident(name) if name == "ref") {
            let start = self.bump().span.start;
            let mut ty = self.parse_type()?;
            if ty.reference {
                return Err(ParseError {
                    message: "nested `ref` types are not allowed in the MVP".into(),
                    span: Span::new(start, ty.span.end),
                });
            }
            ty.reference = true;
            ty.span = Span::new(start, ty.span.end);
            return Ok(ty);
        }
        // C10f: function type `(T, U) -> R` / `() -> R`.
        if matches!(self.peek().kind, TokenKind::LParen) {
            return self.parse_fun_type();
        }
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
            reference: false,
            span: Span::new(start, end),
            fun: None,
        })
    }

    /// C10f: `(Int, Bool) -> String` or `() -> Int`.
    pub(crate) fn parse_fun_type(&mut self) -> Result<TypeRef, ParseError> {
        let start = self.expect(TokenKind::LParen, "`(`")?.span.start;
        let mut params = Vec::new();
        if !matches!(self.peek().kind, TokenKind::RParen) {
            loop {
                params.push(self.parse_type()?);
                if matches!(self.peek().kind, TokenKind::Comma) {
                    self.bump();
                    continue;
                }
                break;
            }
        }
        self.expect(TokenKind::RParen, "`)`")?;
        self.expect(TokenKind::ThinArrow, "`->`")?;
        let ret = self.parse_type()?;
        let nullable = if matches!(self.peek().kind, TokenKind::Question) {
            self.bump();
            true
        } else {
            false
        };
        let end = if nullable {
            self.tokens[self.idx.saturating_sub(1)].span.end
        } else {
            ret.span.end
        };
        Ok(TypeRef {
            qualifier: None,
            name: Ident {
                name: "fn".into(),
                span: Span::new(start, end),
            },
            type_args: Vec::new(),
            nullable,
            reference: false,
            span: Span::new(start, end),
            fun: Some(Box::new(FunTypeRef { params, ret })),
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

fn foreign_metadata(
    attributes: &[Attribute],
) -> (
    Option<ForeignLibrary>,
    Option<ForeignTarget>,
    Option<ForeignLink>,
    Option<ForeignAbi>,
    Option<String>,
) {
    let Some(attribute) = attributes.iter().find(|a| a.name.name == "foreign") else {
        return (None, None, None, None, None);
    };
    let args = &attribute.args;
    let mut library = None;
    let mut target = None;
    let mut link = None;
    let mut abi_version = None;
    let mut abi_identity = None;
    let mut failure = None;
    let mut abi_span = attribute.span;
    for arg in args {
        let AttributeArg::Named { name, value, .. } = arg else {
            continue;
        };
        match (name.name.as_str(), value) {
            ("library", AttributeValue::String { value, span }) => {
                library = Some(ForeignLibrary {
                    name: value.clone(),
                    span: *span,
                });
            }
            ("target", AttributeValue::String { value, span }) => {
                target = Some(ForeignTarget {
                    triple: value.clone(),
                    span: *span,
                });
            }
            ("link", AttributeValue::String { value, span }) => {
                let kind = match value.as_str() {
                    "dynamic" => Some(ForeignLinkKind::Dynamic),
                    "static" => Some(ForeignLinkKind::Static),
                    _ => None,
                };
                if let Some(kind) = kind {
                    link = Some(ForeignLink { kind, span: *span });
                }
            }
            ("abi", AttributeValue::Int { value, span }) if *value >= 0 => {
                abi_version = u32::try_from(*value).ok();
                abi_span = *span;
            }
            ("abi_id", AttributeValue::String { value, span }) => {
                abi_identity = Some(value.clone());
                abi_span = *span;
            }
            ("failure", AttributeValue::String { value, .. }) => {
                failure = Some(value.clone());
            }
            _ => {}
        }
    }
    let abi = match (abi_version, abi_identity) {
        (Some(version), Some(identity)) => Some(ForeignAbi {
            version,
            identity,
            span: abi_span,
        }),
        _ => None,
    };
    (library, target, link, abi, failure)
}
