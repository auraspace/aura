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

    pub(crate) fn parse_call(&mut self, callee: Expr, type_args: Vec<TypeRef>) -> Result<Expr, ParseError> {
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

