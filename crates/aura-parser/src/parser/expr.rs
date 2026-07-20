//! Expressions (Pratt parser).

use aura_ast::*;
use aura_lexer::TokenKind;

use super::Parser;
use crate::error::ParseError;

impl Parser {
    pub(crate) fn parse_expr(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
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
                    safe: false,
                    span,
                });
                continue;
            }
            // C4s: `?.` safe field / method chain.
            if matches!(self.peek().kind, TokenKind::QuestionDot) {
                self.bump();
                let field = self.expect_ident()?;
                let span = Span::new(lhs.span().start, field.span.end);
                lhs = Expr::Field(FieldExpr {
                    object: Box::new(lhs),
                    field,
                    safe: true,
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
            // C9i: `expr is Type`
            if matches!(self.peek().kind, TokenKind::Is) {
                self.bump();
                let ty = self.parse_type()?;
                let span = Span::new(lhs.span().start, ty.span.end);
                lhs = Expr::Is(IsExpr {
                    expr: Box::new(lhs),
                    ty,
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
                TokenKind::QuestionColon => BinOp::Coalesce,
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

    pub(crate) fn parse_prefix(&mut self) -> Result<Expr, ParseError> {
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

    pub(crate) fn parse_primary(&mut self) -> Result<Expr, ParseError> {
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
                // C9h: desugar `"hi ${name}"` → `"hi " + name` (+ further parts).
                if value.contains("${") {
                    self.desugar_string_interp(&value, span)
                } else {
                    Ok(Expr::String(StringLit { value, span }))
                }
            }
            TokenKind::True => {
                let span = self.bump().span;
                Ok(Expr::Bool(BoolLit { value: true, span }))
            }
            TokenKind::False => {
                let span = self.bump().span;
                Ok(Expr::Bool(BoolLit { value: false, span }))
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
            // C4t: if-expression requires `else` branch.
            TokenKind::If => {
                let start = self.peek().span.start;
                self.expect(TokenKind::If, "`if`")?;
                self.expect(TokenKind::LParen, "`(`")?;
                let cond = self.parse_expr(0)?;
                self.expect(TokenKind::RParen, "`)`")?;
                let then_block = self.parse_block()?;
                if !matches!(self.peek().kind, TokenKind::Else) {
                    return Err(ParseError {
                        message: "if-expression requires an `else` branch".into(),
                        span: then_block.span,
                    });
                }
                self.bump();
                // Allow `else if` by nesting as expression? For MVP require block or if.
                let else_block = if matches!(self.peek().kind, TokenKind::If) {
                    // `else if` → wrap if-expr as single-stmt block of Expr::If
                    let nested = self.parse_primary()?; // will re-enter If
                    let span = nested.span();
                    Block {
                        stmts: vec![Stmt::Expr(nested)],
                        span,
                    }
                } else {
                    self.parse_block()?
                };
                let end = else_block.span.end;
                Ok(Expr::If(Box::new(IfExpr {
                    cond,
                    then_block,
                    else_block,
                    span: Span::new(start, end),
                })))
            }
            _ => Err(ParseError {
                message: format!("expected expression, found {:?}", self.peek().kind),
                span: self.peek().span,
            }),
        }
    }

    pub(crate) fn parse_call(
        &mut self,
        callee: Expr,
        type_args: Vec<TypeRef>,
    ) -> Result<Expr, ParseError> {
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
    pub(crate) fn try_parse_generic_call(&mut self, lhs: Expr) -> Result<Option<Expr>, ParseError> {
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

    /// C9h: `"a ${x} b"` → (`"a " + x) + " b"`. Only bare identifiers inside `${}`.
    pub(crate) fn desugar_string_interp(
        &self,
        value: &str,
        span: Span,
    ) -> Result<Expr, ParseError> {
        let mut parts: Vec<Expr> = Vec::new();
        let bytes = value.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'{' {
                let start = i + 2;
                let mut j = start;
                while j < bytes.len() && bytes[j] != b'}' {
                    j += 1;
                }
                if j >= bytes.len() {
                    return Err(ParseError {
                        message: "unterminated `${` in string interpolation".into(),
                        span,
                    });
                }
                let name = &value[start..j];
                if name.is_empty()
                    || !name.chars().next().unwrap().is_ascii_alphabetic()
                        && name.chars().next() != Some('_')
                    || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    return Err(ParseError {
                        message: format!(
                            "C9h: interpolation `${{{name}}}` must be a simple identifier"
                        ),
                        span,
                    });
                }
                parts.push(Expr::Ident(Ident {
                    name: name.to_string(),
                    span,
                }));
                i = j + 1;
            } else {
                let start = i;
                while i < bytes.len() {
                    if i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'{' {
                        break;
                    }
                    i += 1;
                }
                let lit = value[start..i].to_string();
                if !lit.is_empty() {
                    parts.push(Expr::String(StringLit { value: lit, span }));
                }
            }
        }
        if parts.is_empty() {
            return Ok(Expr::String(StringLit {
                value: String::new(),
                span,
            }));
        }
        let mut acc = parts.remove(0);
        for p in parts {
            let left = acc;
            let right = p;
            acc = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op: BinOp::Add,
                right: Box::new(right),
                span,
            });
        }
        Ok(acc)
    }
}

fn prefix_binding_power() -> u8 {
    9
}

fn infix_binding_power(op: BinOp) -> (u8, u8) {
    match op {
        // C4m: `?:` binds looser than `||` (right-associative-ish via r_bp).
        BinOp::Coalesce => (1, 1),
        BinOp::Or => (2, 3),
        BinOp::And => (4, 5),
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => (6, 7),
        BinOp::Add | BinOp::Sub => (8, 9),
        BinOp::Mul | BinOp::Div | BinOp::Rem => (10, 11),
    }
}
