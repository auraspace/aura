//! Recursive-descent parser core.

use aura_ast::*;
use aura_lexer::{match_pattern, substitute, Delimiter, Token, TokenKind, TokenTree};

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
        let mut foreign_functions = Vec::new();
        let mut async_functions = Vec::new();
        let mut classes = Vec::new();
        let mut interfaces = Vec::new();
        let mut enums = Vec::new();
        let mut type_aliases = Vec::new();
        let mut consts = Vec::new();
        while !matches!(self.peek().kind, TokenKind::Eof) {
            let attributes = self.parse_attributes()?;
            let modifiers = self.parse_modifiers()?;
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
                    let mut c = self.parse_nominal(NominalKind::Class, modifiers.clone())?;
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
                    let mut c = self.parse_nominal(NominalKind::Struct, modifiers.clone())?;
                    c.is_pub = is_pub;
                    c.attributes = attributes;
                    classes.push(c);
                }
                TokenKind::Fun => {
                    let mut f = self.parse_fun()?;
                    f.modifiers = modifiers;
                    f.visibility = if is_pub {
                        MemberVisibility::Public
                    } else {
                        MemberVisibility::Package
                    };
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
                TokenKind::Extern => {
                    if is_test {
                        return Err(ParseError {
                            message: "`@test` only applies to Aura functions".into(),
                            span: self.peek().span,
                        });
                    }
                    let mut f = self.parse_foreign_fun(&attributes)?;
                    f.is_pub = is_pub;
                    f.attributes = attributes;
                    foreign_functions.push(f);
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
                            "expected `type`, `const`, `interface`, `enum`, `class`, `struct`, `fun`, or `extern`, found {:?}",
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
            foreign_functions,
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

    fn parse_modifiers(&mut self) -> Result<Vec<Modifier>, ParseError> {
        let mut modifiers = Vec::new();
        loop {
            let modifier = match self.peek().kind {
                TokenKind::Open => Modifier::Open,
                TokenKind::Final => Modifier::Final,
                TokenKind::Abstract => Modifier::Abstract,
                TokenKind::Override => Modifier::Override,
                TokenKind::Static => Modifier::Static,
                _ => break,
            };
            self.bump();
            if modifiers.contains(&modifier) {
                return Err(ParseError {
                    message: "duplicate declaration modifier".into(),
                    span: self.peek().span,
                });
            }
            modifiers.push(modifier);
        }
        Ok(modifiers)
    }

    pub(crate) fn parse_member_visibility(&mut self) -> MemberVisibility {
        match self.peek().kind {
            TokenKind::Pub => {
                self.bump();
                MemberVisibility::Public
            }
            TokenKind::Protected => {
                self.bump();
                MemberVisibility::Protected
            }
            TokenKind::Private => {
                self.bump();
                MemberVisibility::Private
            }
            _ => MemberVisibility::Package,
        }
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

#[derive(Debug, Clone)]
struct DeclarativeMacroRule {
    pattern: Vec<TokenTree>,
    template: Vec<TokenTree>,
}

#[derive(Debug, Clone)]
struct DeclarativeMacro {
    name: String,
    rules: Vec<DeclarativeMacroRule>,
}

/// Expand the RFC-010 declarative macro subset before the ordinary AST parser.
/// Macro definitions are top-level; invocations can occur in any token group.
pub(crate) fn expand_declarative_macros(tokens: Vec<Token>) -> Result<Vec<Token>, String> {
    let eof = tokens
        .last()
        .filter(|token| token.kind == TokenKind::Eof)
        .cloned()
        .ok_or_else(|| "lexer did not produce EOF".to_string())?;
    let tree = TokenTree::from_tokens(&tokens)?;
    let (mut body, macros) = collect_macro_definitions(tree)?;
    if macros.is_empty() {
        return Ok(tokens);
    }
    for _ in 0..64 {
        let (expanded, changed) = expand_tree_list(body, &macros);
        body = expanded;
        if !changed {
            let mut output = Vec::new();
            for item in &body {
                item.flatten(&mut output);
            }
            output.push(eof);
            return Ok(output);
        }
    }
    Err("declarative macro expansion exceeded the recursion limit (64)".into())
}

fn collect_macro_definitions(
    tree: Vec<TokenTree>,
) -> Result<(Vec<TokenTree>, Vec<DeclarativeMacro>), String> {
    let mut body = Vec::new();
    let mut macros = Vec::new();
    let mut index = 0;
    while index < tree.len() {
        let is_macro = matches!(
            tree.get(index),
            Some(TokenTree::Leaf(Token {
                kind: TokenKind::Macro,
                ..
            }))
        );
        if !is_macro {
            body.push(tree[index].clone());
            index += 1;
            continue;
        }
        let Some(TokenTree::Leaf(Token {
            kind: TokenKind::Bang,
            ..
        })) = tree.get(index + 1)
        else {
            return Err("expected `!` after `macro`".into());
        };
        let Some(TokenTree::Leaf(Token {
            kind: TokenKind::Ident(name),
            ..
        })) = tree.get(index + 2)
        else {
            return Err("expected macro name after `macro!`".into());
        };
        let Some(TokenTree::Group {
            delimiter: Delimiter::Brace,
            children,
            ..
        }) = tree.get(index + 3)
        else {
            return Err("expected `{ ... }` after macro name".into());
        };
        macros.push(DeclarativeMacro {
            name: name.clone(),
            rules: parse_macro_rules(children)?,
        });
        index += 4;
    }
    Ok((body, macros))
}

fn parse_macro_rules(children: &[TokenTree]) -> Result<Vec<DeclarativeMacroRule>, String> {
    let mut rules = Vec::new();
    let mut index = 0;
    while index < children.len() {
        if matches!(
            children[index],
            TokenTree::Leaf(Token {
                kind: TokenKind::Semi,
                ..
            })
        ) {
            index += 1;
            continue;
        }
        let Some(TokenTree::Group {
            children: pattern, ..
        }) = children.get(index)
        else {
            return Err("expected macro rule pattern group".into());
        };
        if !matches!(
            children.get(index + 1),
            Some(TokenTree::Leaf(Token {
                kind: TokenKind::FatArrow,
                ..
            }))
        ) {
            return Err("expected `=>` after macro rule pattern".into());
        }
        let Some(TokenTree::Group {
            children: template, ..
        }) = children.get(index + 2)
        else {
            return Err("expected macro rule template group".into());
        };
        rules.push(DeclarativeMacroRule {
            pattern: pattern.clone(),
            template: template.clone(),
        });
        index += 3;
    }
    if rules.is_empty() {
        return Err("macro declaration must contain at least one rule".into());
    }
    Ok(rules)
}

fn expand_tree_list(tree: Vec<TokenTree>, macros: &[DeclarativeMacro]) -> (Vec<TokenTree>, bool) {
    let mut output = Vec::new();
    let mut changed = false;
    let mut index = 0;
    while index < tree.len() {
        if index + 2 < tree.len() {
            if let (
                TokenTree::Leaf(Token {
                    kind: TokenKind::Ident(name),
                    ..
                }),
                TokenTree::Leaf(Token {
                    kind: TokenKind::Bang,
                    ..
                }),
                TokenTree::Group {
                    children: input, ..
                },
            ) = (&tree[index], &tree[index + 1], &tree[index + 2])
            {
                if let Some(definition) = macros.iter().find(|item| item.name == *name) {
                    let mut matched = None;
                    for rule in &definition.rules {
                        if let Some(captures) = match_pattern(&rule.pattern, input) {
                            matched = Some(substitute(&rule.template, &captures));
                            break;
                        }
                    }
                    if let Some(expansion) = matched {
                        output.extend(expansion);
                        changed = true;
                        index += 3;
                        continue;
                    }
                }
            }
        }
        match &tree[index] {
            TokenTree::Group {
                delimiter,
                span,
                children,
            } => {
                let (children, child_changed) = expand_tree_list(children.clone(), macros);
                changed |= child_changed;
                output.push(TokenTree::Group {
                    delimiter: delimiter.clone(),
                    span: *span,
                    children,
                });
            }
            item => output.push(item.clone()),
        }
        index += 1;
    }
    (output, changed)
}
