//! Recursive-descent + Pratt expression parser for Aura C0–C1b (RFC-001 §6.0).

use aura_ast::*;
use aura_lexer::{lex, LexError, Token, TokenKind};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at bytes {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}

impl std::error::Error for ParseError {}

impl From<LexError> for ParseError {
    fn from(e: LexError) -> Self {
        Self {
            message: e.message,
            span: e.span,
        }
    }
}

pub fn parse_file(src: &str) -> Result<File, ParseError> {
    let tokens = lex(src)?;
    let mut p = Parser::new(tokens);
    p.parse_file()
}

struct Parser {
    tokens: Vec<Token>,
    idx: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, idx: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.idx]
    }

    fn bump(&mut self) -> Token {
        let t = self.tokens[self.idx].clone();
        if self.idx + 1 < self.tokens.len() {
            self.idx += 1;
        }
        t
    }

    fn expect(&mut self, kind: TokenKind, what: &str) -> Result<Token, ParseError> {
        if self.peek().kind == kind {
            return Ok(self.bump());
        }
        Err(ParseError {
            message: format!("expected {what}, found {:?}", self.peek().kind),
            span: self.peek().span,
        })
    }

    fn expect_ident(&mut self) -> Result<Ident, ParseError> {
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

    fn parse_file(&mut self) -> Result<File, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::Package, "`package`")?;
        let package = self.parse_path()?;
        let mut functions = Vec::new();
        let mut classes = Vec::new();
        let mut interfaces = Vec::new();
        while !matches!(self.peek().kind, TokenKind::Eof) {
            if matches!(self.peek().kind, TokenKind::Pub) {
                self.bump();
            }
            match self.peek().kind {
                TokenKind::Interface => interfaces.push(self.parse_interface()?),
                TokenKind::Class => classes.push(self.parse_nominal(NominalKind::Class)?),
                TokenKind::Struct => classes.push(self.parse_nominal(NominalKind::Struct)?),
                TokenKind::Fun => functions.push(self.parse_fun()?),
                _ => {
                    return Err(ParseError {
                        message: format!(
                            "expected `interface`, `class`, `struct`, or `fun`, found {:?}",
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
            classes,
            functions,
            span: Span::new(start, end),
        })
    }

    fn parse_interface(&mut self) -> Result<InterfaceDecl, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::Interface, "`interface`")?;
        let name = self.expect_ident()?;
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
            name,
            methods,
            span: Span::new(start, end),
        })
    }

    /// `fun name(...): T` without body (interface member).
    fn parse_method_sig(&mut self) -> Result<MethodSig, ParseError> {
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

    fn parse_path(&mut self) -> Result<Path, ParseError> {
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

    fn parse_nominal(&mut self, kind: NominalKind) -> Result<ClassDecl, ParseError> {
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
                implements.push(self.expect_ident()?);
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
    fn parse_type_params_opt(&mut self) -> Result<Vec<TypeParam>, ParseError> {
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
    fn apply_where_clause(&mut self, type_params: &mut [TypeParam]) -> Result<(), ParseError> {
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

    fn parse_field(&mut self) -> Result<FieldDecl, ParseError> {
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

    fn parse_fun(&mut self) -> Result<FunDecl, ParseError> {
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
        let body = self.parse_block()?;
        let end = body.span.end;
        Ok(FunDecl {
            name,
            type_params,
            params,
            return_type,
            body,
            span: Span::new(start, end),
        })
    }

    fn parse_param(&mut self) -> Result<Param, ParseError> {
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

    fn parse_type(&mut self) -> Result<TypeRef, ParseError> {
        let name = self.expect_ident()?;
        let start = name.span.start;
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
            name,
            type_args,
            nullable,
            span: Span::new(start, end),
        })
    }

    fn parse_type_args_opt(&mut self) -> Result<Vec<TypeRef>, ParseError> {
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

    fn parse_block(&mut self) -> Result<Block, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::LBrace, "`{`")?;
        let mut stmts = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace | TokenKind::Eof) {
            stmts.push(self.parse_stmt()?);
        }
        let end_tok = self.expect(TokenKind::RBrace, "`}`")?;
        Ok(Block {
            stmts,
            span: Span::new(start, end_tok.span.end),
        })
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        match self.peek().kind {
            TokenKind::Val | TokenKind::Var => Ok(Stmt::Var(self.parse_var()?)),
            TokenKind::If => Ok(Stmt::If(self.parse_if()?)),
            TokenKind::While => Ok(Stmt::While(self.parse_while()?)),
            TokenKind::Return => Ok(Stmt::Return(self.parse_return()?)),
            _ => Ok(Stmt::Expr(self.parse_expr(0)?)),
        }
    }

    fn parse_var(&mut self) -> Result<VarStmt, ParseError> {
        let start = self.peek().span.start;
        let mutable = matches!(self.peek().kind, TokenKind::Var);
        self.bump();
        let name = self.expect_ident()?;
        let ty = if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(TokenKind::Eq, "`=`")?;
        let init = self.parse_expr(0)?;
        let end = init.span().end;
        Ok(VarStmt {
            mutable,
            name,
            ty,
            init,
            span: Span::new(start, end),
        })
    }

    fn parse_if(&mut self) -> Result<IfStmt, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::If, "`if`")?;
        self.expect(TokenKind::LParen, "`(`")?;
        let cond = self.parse_expr(0)?;
        self.expect(TokenKind::RParen, "`)`")?;
        let then_block = self.parse_block()?;
        let else_block = if matches!(self.peek().kind, TokenKind::Else) {
            self.bump();
            Some(self.parse_block()?)
        } else {
            None
        };
        let end = else_block
            .as_ref()
            .map(|b| b.span.end)
            .unwrap_or(then_block.span.end);
        Ok(IfStmt {
            cond,
            then_block,
            else_block,
            span: Span::new(start, end),
        })
    }

    fn parse_while(&mut self) -> Result<WhileStmt, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::While, "`while`")?;
        self.expect(TokenKind::LParen, "`(`")?;
        let cond = self.parse_expr(0)?;
        self.expect(TokenKind::RParen, "`)`")?;
        let body = self.parse_block()?;
        let end = body.span.end;
        Ok(WhileStmt {
            cond,
            body,
            span: Span::new(start, end),
        })
    }

    fn parse_return(&mut self) -> Result<ReturnStmt, ParseError> {
        let start = self.peek().span.start;
        let tok = self.expect(TokenKind::Return, "`return`")?;
        let value = match self.peek().kind {
            TokenKind::Ident(_)
            | TokenKind::Int(_)
            | TokenKind::String(_)
            | TokenKind::True
            | TokenKind::False
            | TokenKind::Null
            | TokenKind::This
            | TokenKind::LParen
            | TokenKind::Minus
            | TokenKind::Bang => Some(self.parse_expr(0)?),
            _ => None,
        };
        let end = value.as_ref().map(|e| e.span().end).unwrap_or(tok.span.end);
        Ok(ReturnStmt {
            value,
            span: Span::new(start, end),
        })
    }

    fn parse_expr(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_prefix()?;

        // Postfix: generic call `F<T>(...)`, call, field access
        loop {
            // `Ident<TypeArgs>(...)` — only if `<...>` is followed by `(`
            if matches!(self.peek().kind, TokenKind::Lt) {
                if let Some(call) = self.try_parse_generic_call(lhs.clone())? {
                    lhs = call;
                    continue;
                }
            }
            if matches!(self.peek().kind, TokenKind::LParen) {
                lhs = self.parse_call(lhs, Vec::new())?;
                continue;
            }
            if matches!(self.peek().kind, TokenKind::Dot) {
                self.bump();
                let field = self.expect_ident()?;
                let span = Span::new(lhs.span().start, field.span.end);
                lhs = Expr::Field(FieldExpr {
                    object: Box::new(lhs),
                    field,
                    span,
                });
                continue;
            }
            if matches!(self.peek().kind, TokenKind::BangBang) {
                let end = self.bump().span.end;
                let span = Span::new(lhs.span().start, end);
                lhs = Expr::ForceUnwrap(ForceUnwrapExpr {
                    expr: Box::new(lhs),
                    span,
                });
                continue;
            }
            break;
        }

        if min_bp <= 0 && matches!(self.peek().kind, TokenKind::Eq) {
            if let Expr::Ident(name) = &lhs {
                let name = name.clone();
                self.bump();
                let value = self.parse_expr(0)?;
                let span = Span::new(name.span.start, value.span().end);
                return Ok(Expr::Assign(AssignExpr {
                    name,
                    value: Box::new(value),
                    span,
                }));
            }
            return Err(ParseError {
                message: "invalid assignment target".into(),
                span: self.peek().span,
            });
        }

        loop {
            let op = match self.peek().kind {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Rem,
                TokenKind::EqEq => BinOp::Eq,
                TokenKind::Ne => BinOp::Ne,
                TokenKind::Lt => BinOp::Lt,
                TokenKind::Le => BinOp::Le,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::Ge => BinOp::Ge,
                TokenKind::AndAnd => BinOp::And,
                TokenKind::OrOr => BinOp::Or,
                _ => break,
            };
            let (l_bp, r_bp) = infix_binding_power(op);
            if l_bp < min_bp {
                break;
            }
            self.bump();
            let rhs = self.parse_expr(r_bp)?;
            let span = Span::new(lhs.span().start, rhs.span().end);
            lhs = Expr::Binary(BinaryExpr {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
                span,
            });
        }

        Ok(lhs)
    }

    fn parse_prefix(&mut self) -> Result<Expr, ParseError> {
        match &self.peek().kind {
            TokenKind::Minus => {
                let start = self.peek().span.start;
                self.bump();
                let expr = self.parse_expr(prefix_binding_power())?;
                let span = Span::new(start, expr.span().end);
                Ok(Expr::Unary(UnaryExpr {
                    op: UnOp::Neg,
                    expr: Box::new(expr),
                    span,
                }))
            }
            TokenKind::Bang => {
                let start = self.peek().span.start;
                self.bump();
                let expr = self.parse_expr(prefix_binding_power())?;
                let span = Span::new(start, expr.span().end);
                Ok(Expr::Unary(UnaryExpr {
                    op: UnOp::Not,
                    expr: Box::new(expr),
                    span,
                }))
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match &self.peek().kind {
            TokenKind::Ident(name) => {
                let span = self.peek().span;
                let name = name.clone();
                self.bump();
                Ok(Expr::Ident(Ident { name, span }))
            }
            TokenKind::This => {
                let span = self.bump().span;
                Ok(Expr::This(span))
            }
            TokenKind::Int(v) => {
                let span = self.peek().span;
                let value = *v;
                self.bump();
                Ok(Expr::Int(IntLit { value, span }))
            }
            TokenKind::String(s) => {
                let span = self.peek().span;
                let value = s.clone();
                self.bump();
                Ok(Expr::String(StringLit { value, span }))
            }
            TokenKind::True => {
                let span = self.bump().span;
                Ok(Expr::Bool(BoolLit { value: true, span }))
            }
            TokenKind::False => {
                let span = self.bump().span;
                Ok(Expr::Bool(BoolLit {
                    value: false,
                    span,
                }))
            }
            TokenKind::Null => {
                let span = self.bump().span;
                Ok(Expr::Null(span))
            }
            TokenKind::LParen => {
                let start = self.bump().span.start;
                let inner = self.parse_expr(0)?;
                let end = self.expect(TokenKind::RParen, "`)`")?.span.end;
                Ok(Expr::Group(Box::new(inner), Span::new(start, end)))
            }
            _ => Err(ParseError {
                message: format!("expected expression, found {:?}", self.peek().kind),
                span: self.peek().span,
            }),
        }
    }

    fn parse_call(&mut self, callee: Expr, type_args: Vec<TypeRef>) -> Result<Expr, ParseError> {
        let start = callee.span().start;
        self.expect(TokenKind::LParen, "`(`")?;
        let mut args = Vec::new();
        if !matches!(self.peek().kind, TokenKind::RParen) {
            loop {
                args.push(self.parse_expr(0)?);
                if matches!(self.peek().kind, TokenKind::Comma) {
                    self.bump();
                    continue;
                }
                break;
            }
        }
        let end = self.expect(TokenKind::RParen, "`)`")?.span.end;
        Ok(Expr::Call(CallExpr {
            callee: Box::new(callee),
            type_args,
            args,
            span: Span::new(start, end),
        }))
    }

    /// Try `lhs<T, U>(...)`. Restores position if it is a comparison (`a < b`).
    fn try_parse_generic_call(&mut self, lhs: Expr) -> Result<Option<Expr>, ParseError> {
        if !matches!(lhs, Expr::Ident(_)) {
            return Ok(None);
        }
        let saved = self.idx;
        if !matches!(self.peek().kind, TokenKind::Lt) {
            return Ok(None);
        }
        self.bump(); // <
        let mut type_args = Vec::new();
        // Must start with a type (ident)
        if !matches!(self.peek().kind, TokenKind::Ident(_)) {
            self.idx = saved;
            return Ok(None);
        }
        loop {
            match self.parse_type() {
                Ok(t) => type_args.push(t),
                Err(_) => {
                    self.idx = saved;
                    return Ok(None);
                }
            }
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
                continue;
            }
            break;
        }
        if !matches!(self.peek().kind, TokenKind::Gt) {
            self.idx = saved;
            return Ok(None);
        }
        self.bump(); // >
        if !matches!(self.peek().kind, TokenKind::LParen) {
            self.idx = saved;
            return Ok(None);
        }
        Ok(Some(self.parse_call(lhs, type_args)?))
    }
}

fn prefix_binding_power() -> u8 {
    9
}

fn infix_binding_power(op: BinOp) -> (u8, u8) {
    match op {
        BinOp::Or => (1, 2),
        BinOp::And => (3, 4),
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => (5, 6),
        BinOp::Add | BinOp::Sub => (7, 8),
        BinOp::Mul | BinOp::Div | BinOp::Rem => (9, 10),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hello() {
        let src = r#"
package main

fun main() {
  println("Hello, Aura")
}
"#;
        let file = parse_file(src).expect("parse");
        assert_eq!(file.package.segments[0].name, "main");
        assert_eq!(file.functions.len(), 1);
        assert_eq!(file.functions[0].name.name, "main");
        assert_eq!(file.functions[0].body.stmts.len(), 1);
    }

    #[test]
    fn parses_control_flow() {
        let src = r#"
package demo

fun add(a: Int, b: Int): Int {
  val sum: Int = a + b
  if (sum > 0) {
    return sum
  } else {
    return 0
  }
}
"#;
        let file = parse_file(src).expect("parse");
        assert_eq!(file.functions[0].params.len(), 2);
        assert!(file.functions[0].return_type.is_some());
        assert_eq!(file.functions[0].body.stmts.len(), 2);
    }

    #[test]
    fn parses_while_and_nullable() {
        let src = r#"
package demo

fun loop(n: Int): Int {
  var i: Int = 0
  var s: String? = null
  while (i < n) {
    i = i + 1
  }
  return i
}
"#;
        let file = parse_file(src).expect("parse");
        assert_eq!(file.functions[0].body.stmts.len(), 4);
    }

    #[test]
    fn parses_assignment() {
        let src = r#"
package demo
fun main() {
  var x: Int = 1
  x = x + 1
}
"#;
        let file = parse_file(src).expect("parse");
        assert!(matches!(
            file.functions[0].body.stmts[1],
            Stmt::Expr(Expr::Assign(_))
        ));
    }

    #[test]
    fn parses_class_and_method_call() {
        let src = r#"
package main

class Greeter(val name: String) {
  fun greet(): String {
    return this.name
  }
}

fun main() {
  val g: Greeter = Greeter("Aura")
  println(g.greet())
}
"#;
        let file = parse_file(src).expect("parse");
        assert_eq!(file.classes.len(), 1);
        assert_eq!(file.classes[0].name.name, "Greeter");
        assert_eq!(file.classes[0].fields.len(), 1);
        assert_eq!(file.classes[0].methods.len(), 1);
        assert_eq!(file.functions.len(), 1);
        // second stmt is println(g.greet())
        match &file.functions[0].body.stmts[1] {
            Stmt::Expr(Expr::Call(c)) => match c.args[0].clone() {
                Expr::Call(inner) => {
                    assert!(matches!(inner.callee.as_ref(), Expr::Field(_)));
                }
                other => panic!("expected method call, got {other:?}"),
            },
            other => panic!("expected call stmt, got {other:?}"),
        }
    }

    #[test]
    fn parses_interface_and_implements() {
        let src = r#"
package main

interface Named {
  fun name(): String
}

class User(val n: String) : Named {
  fun name(): String {
    return this.n
  }
}

fun show(x: Named) {
  println(x.name())
}
"#;
        let file = parse_file(src).expect("parse");
        assert_eq!(file.interfaces.len(), 1);
        assert_eq!(file.interfaces[0].methods.len(), 1);
        assert_eq!(file.classes[0].implements.len(), 1);
        assert_eq!(file.classes[0].implements[0].name, "Named");
    }

    #[test]
    fn parses_force_unwrap() {
        let src = r#"
package t
fun f(x: String?): String {
  return x!!
}
"#;
        let file = parse_file(src).expect("parse");
        match &file.functions[0].body.stmts[0] {
            Stmt::Return(r) => {
                assert!(matches!(r.value, Some(Expr::ForceUnwrap(_))));
            }
            other => panic!("expected return, got {other:?}"),
        }
    }

    #[test]
    fn parses_generic_class_and_ctor() {
        let src = r#"
package main

class Box<T>(val value: T) {
  fun get(): T {
    return this.value
  }
}

fun id<T>(x: T): T {
  return x
}

fun main() {
  val b: Box<String> = Box<String>("hi")
  println(b.get())
  println(id<String>("ok"))
}
"#;
        let file = parse_file(src).expect("parse");
        assert_eq!(file.classes[0].type_params.len(), 1);
        assert_eq!(file.functions[0].type_params.len(), 1);
        assert_eq!(file.functions[0].name.name, "id");
        // first stmt init is Call with type_args
        match &file.functions[1].body.stmts[0] {
            Stmt::Var(v) => match &v.init {
                Expr::Call(c) => {
                    assert_eq!(c.type_args.len(), 1);
                    assert_eq!(c.type_args[0].name.name, "String");
                }
                other => panic!("expected call, got {other:?}"),
            },
            other => panic!("expected var, got {other:?}"),
        }
    }

    #[test]
    fn parses_struct() {
        let src = r#"
package main
struct Point(val x: Int, val y: Int) {
  fun sum(): Int {
    return this.x + this.y
  }
}
"#;
        let file = parse_file(src).expect("parse");
        assert_eq!(file.classes.len(), 1);
        assert_eq!(file.classes[0].kind, NominalKind::Struct);
        assert_eq!(file.classes[0].name.name, "Point");
        assert_eq!(file.classes[0].fields.len(), 2);
        assert_eq!(file.classes[0].methods.len(), 1);
    }

    #[test]
    fn rejects_struct_implements() {
        let src = r#"
package main
interface Named { fun name(): String }
struct S(val n: String) : Named {
  fun name(): String { return this.n }
}
"#;
        let err = parse_file(src).expect_err("struct implements");
        assert!(err.message.contains("struct"), "{}", err.message);
    }

    #[test]
    fn parses_type_param_bounds_and_where() {
        let src = r#"
package main

interface Named {
  fun name(): String
}

interface Id {
  fun id(): Int
}

fun greet<T : Named>(x: T): String {
  return x.name()
}

fun both<T>(x: T) where T : Named, T : Id {
  println(x.name())
}

class Holder<T>(val item: T) where T : Named {
  fun label(): String {
    return this.item.name()
  }
}
"#;
        let file = parse_file(src).expect("parse");
        assert_eq!(file.functions[0].type_params[0].name.name, "T");
        assert_eq!(file.functions[0].type_params[0].bounds.len(), 1);
        assert_eq!(file.functions[0].type_params[0].bounds[0].name, "Named");
        assert_eq!(file.functions[1].type_params[0].bounds.len(), 2);
        assert_eq!(file.functions[1].type_params[0].bounds[0].name, "Named");
        assert_eq!(file.functions[1].type_params[0].bounds[1].name, "Id");
        assert_eq!(file.classes[0].type_params[0].bounds.len(), 1);
        assert_eq!(file.classes[0].type_params[0].bounds[0].name, "Named");
    }

    #[test]
    fn rejects_missing_package() {
        let err = parse_file("fun main() {}").unwrap_err();
        assert!(err.message.contains("package"));
    }
}
