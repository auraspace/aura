//! Statements and blocks.

use aura_ast::*;
use aura_lexer::TokenKind;

use super::Parser;
use crate::error::ParseError;

impl Parser {
    pub(crate) fn parse_block(&mut self) -> Result<Block, ParseError> {
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

    pub(crate) fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        match self.peek().kind {
            TokenKind::Val | TokenKind::Var => Ok(Stmt::Var(self.parse_var()?)),
            TokenKind::If => Ok(Stmt::If(self.parse_if()?)),
            TokenKind::While => Ok(Stmt::While(self.parse_while()?)),
            TokenKind::For => self.parse_for(),
            TokenKind::Break => {
                let tok = self.bump();
                Ok(Stmt::Break(tok.span))
            }
            TokenKind::Continue => {
                let tok = self.bump();
                Ok(Stmt::Continue(tok.span))
            }
            TokenKind::Match => Ok(Stmt::Match(self.parse_match()?)),
            TokenKind::Try => Ok(Stmt::Try(self.parse_try()?)),
            TokenKind::Throw => Ok(Stmt::Throw(self.parse_throw()?)),
            TokenKind::Return => Ok(Stmt::Return(self.parse_return()?)),
            _ => Ok(Stmt::Expr(self.parse_expr(0)?)),
        }
    }

    pub(crate) fn parse_throw(&mut self) -> Result<ThrowStmt, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::Throw, "`throw`")?;
        let value = self.parse_expr(0)?;
        let end = value.span().end;
        Ok(ThrowStmt {
            value,
            span: Span::new(start, end),
        })
    }

    pub(crate) fn parse_try(&mut self) -> Result<TryStmt, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::Try, "`try`")?;
        let try_block = self.parse_block()?;
        let catch = if matches!(self.peek().kind, TokenKind::Catch) {
            Some(self.parse_catch()?)
        } else {
            None
        };
        let finally = if matches!(self.peek().kind, TokenKind::Finally) {
            self.bump();
            Some(self.parse_block()?)
        } else {
            None
        };
        if catch.is_none() && finally.is_none() {
            return Err(ParseError {
                message: "`try` needs `catch` and/or `finally`".into(),
                span: try_block.span,
            });
        }
        let end = finally
            .as_ref()
            .map(|b| b.span.end)
            .or_else(|| catch.as_ref().map(|c| c.span.end))
            .unwrap_or(try_block.span.end);
        Ok(TryStmt {
            try_block,
            catch,
            finally,
            span: Span::new(start, end),
        })
    }

    pub(crate) fn parse_catch(&mut self) -> Result<CatchClause, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::Catch, "`catch`")?;
        self.expect(TokenKind::LParen, "`(`")?;
        let name = self.expect_ident()?;
        self.expect(TokenKind::Colon, "`:`")?;
        let ty = self.parse_type()?;
        self.expect(TokenKind::RParen, "`)`")?;
        let body = self.parse_block()?;
        let end = body.span.end;
        Ok(CatchClause {
            name,
            ty,
            body,
            span: Span::new(start, end),
        })
    }

    pub(crate) fn parse_match(&mut self) -> Result<MatchStmt, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::Match, "`match`")?;
        self.expect(TokenKind::LParen, "`(`")?;
        let scrutinee = self.parse_expr(0)?;
        self.expect(TokenKind::RParen, "`)`")?;
        self.expect(TokenKind::LBrace, "`{`")?;
        let mut arms = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace | TokenKind::Eof) {
            arms.push(self.parse_match_arm()?);
        }
        let end = self.expect(TokenKind::RBrace, "`}`")?.span.end;
        if arms.is_empty() {
            return Err(ParseError {
                message: "`match` needs at least one `case` arm".into(),
                span: Span::new(start, end),
            });
        }
        Ok(MatchStmt {
            scrutinee,
            arms,
            span: Span::new(start, end),
        })
    }

    pub(crate) fn parse_match_arm(&mut self) -> Result<MatchArm, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::Case, "`case`")?;
        let pattern = self.parse_pattern()?;
        self.expect(TokenKind::FatArrow, "`=>`")?;
        let body = self.parse_block()?;
        let end = body.span.end;
        Ok(MatchArm {
            pattern,
            body,
            span: Span::new(start, end),
        })
    }

    pub(crate) fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        let name = self.expect_ident()?;
        let start = name.span.start;
        let mut bindings = Vec::new();
        if matches!(self.peek().kind, TokenKind::LParen) {
            self.bump();
            if !matches!(self.peek().kind, TokenKind::RParen) {
                loop {
                    bindings.push(self.expect_ident()?);
                    if matches!(self.peek().kind, TokenKind::Comma) {
                        self.bump();
                        continue;
                    }
                    break;
                }
            }
            let end = self.expect(TokenKind::RParen, "`)`")?.span.end;
            Ok(Pattern::Variant {
                name,
                bindings,
                span: Span::new(start, end),
            })
        } else {
            Ok(Pattern::Variant {
                span: name.span,
                name,
                bindings,
            })
        }
    }

    pub(crate) fn parse_var(&mut self) -> Result<VarStmt, ParseError> {
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

    pub(crate) fn parse_if(&mut self) -> Result<IfStmt, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::If, "`if`")?;
        self.expect(TokenKind::LParen, "`(`")?;
        let cond = self.parse_expr(0)?;
        self.expect(TokenKind::RParen, "`)`")?;
        let then_block = self.parse_block()?;
        // C4l: `else if` desugars to `else { if … }` (nested IfStmt in a block).
        let else_block = if matches!(self.peek().kind, TokenKind::Else) {
            self.bump();
            if matches!(self.peek().kind, TokenKind::If) {
                let nested = self.parse_if()?;
                let span = nested.span;
                Some(Block {
                    stmts: vec![Stmt::If(nested)],
                    span,
                })
            } else {
                Some(self.parse_block()?)
            }
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

    pub(crate) fn parse_while(&mut self) -> Result<WhileStmt, ParseError> {
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

    /// `for (name in start..end)` (C3h) or `for (name in iterable)` (C3k Array).
    pub(crate) fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        let start = self.peek().span.start;
        self.expect(TokenKind::For, "`for`")?;
        self.expect(TokenKind::LParen, "`(`")?;
        let name = self.expect_ident()?;
        self.expect(TokenKind::In, "`in`")?;
        let first = self.parse_expr(0)?;
        let inclusive = match &self.peek().kind {
            TokenKind::DotDot => {
                self.bump();
                false
            }
            TokenKind::DotDotEq => {
                self.bump();
                true
            }
            _ => {
                // for-in over iterable (C3k)
                self.expect(TokenKind::RParen, "`)`")?;
                let body = self.parse_block()?;
                let end = body.span.end;
                return Ok(Stmt::ForIn(ForInStmt {
                    name,
                    iterable: first,
                    body,
                    span: Span::new(start, end),
                }));
            }
        };
        let range_end = self.parse_expr(0)?;
        self.expect(TokenKind::RParen, "`)`")?;
        let body = self.parse_block()?;
        let end = body.span.end;
        Ok(Stmt::ForRange(ForRangeStmt {
            name,
            start: first,
            end: range_end,
            inclusive,
            body,
            span: Span::new(start, end),
        }))
    }

    pub(crate) fn parse_return(&mut self) -> Result<ReturnStmt, ParseError> {
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
            | TokenKind::Bang
            | TokenKind::Await => Some(self.parse_expr(0)?),
            _ => None,
        };
        let end = value.as_ref().map(|e| e.span().end).unwrap_or(tok.span.end);
        Ok(ReturnStmt {
            value,
            span: Span::new(start, end),
        })
    }
}
