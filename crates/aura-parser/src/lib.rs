use aura_ast::*;
use aura_lexer::{Keyword, Operator, Punct, Token, TokenKind};
use aura_span::{BytePos, Span};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub span: Span,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseOutput<T> {
    pub value: T,
    pub errors: Vec<ParseError>,
}

pub fn parse_program(source: &str) -> ParseOutput<Program> {
    let tokens = aura_lexer::lex(source);
    Parser::new(source, tokens).parse_program()
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    idx: usize,
    errors: Vec<ParseError>,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, tokens: Vec<Token>) -> Self {
        Self {
            source,
            tokens,
            idx: 0,
            errors: Vec::new(),
        }
    }

    fn parse_program(mut self) -> ParseOutput<Program> {
        let mut items = Vec::new();
        while !self.at(TokenKind::Eof) {
            if self.at_keyword(Keyword::Function) {
                items.push(TopLevel::Function(self.parse_function_decl()));
            } else {
                items.push(TopLevel::Stmt(self.parse_stmt()));
            }
        }
        ParseOutput {
            value: Program { items },
            errors: self.errors,
        }
    }

    fn parse_function_decl(&mut self) -> FunctionDecl {
        let start = self.current_span_start();
        self.expect_keyword(Keyword::Function);
        let name = self.parse_ident();
        self.expect_punct(Punct::LParen);
        let mut params = Vec::new();
        if !self.at_punct(Punct::RParen) && !self.at(TokenKind::Eof) {
            loop {
                let param_start = self.current_span_start();
                let param_name = self.parse_ident();
                self.expect_punct(Punct::Colon);
                let ty = self.parse_type_ref();
                let param_end = self.prev_span_end();
                params.push(Param {
                    name: param_name,
                    ty,
                    span: Span::new(param_start, param_end),
                });
                if self.eat_punct(Punct::Comma) {
                    continue;
                }
                break;
            }
        }
        self.expect_punct(Punct::RParen);

        let return_type = if self.eat_punct(Punct::Colon) {
            Some(self.parse_type_ref())
        } else {
            None
        };

        let body = self.parse_block();
        let end = body.span.end;
        FunctionDecl {
            name,
            params,
            return_type,
            body,
            span: Span::new(start, end),
        }
    }

    fn parse_type_ref(&mut self) -> TypeRef {
        let start = self.current_span_start();
        let name = self.parse_ident();
        let end = self.prev_span_end();
        TypeRef {
            name,
            span: Span::new(start, end),
        }
    }

    fn parse_block(&mut self) -> Block {
        let start = self.current_span_start();
        self.expect_punct(Punct::LBrace);
        let mut stmts = Vec::new();
        while !self.at_punct(Punct::RBrace) && !self.at(TokenKind::Eof) {
            stmts.push(self.parse_stmt());
        }
        self.expect_punct(Punct::RBrace);
        let end = self.prev_span_end();
        Block {
            stmts,
            span: Span::new(start, end),
        }
    }

    fn parse_stmt(&mut self) -> Stmt {
        if self.eat_punct(Punct::Semi) {
            return Stmt::Empty(self.prev_span());
        }
        if self.at_punct(Punct::LBrace) {
            return Stmt::Block(self.parse_block());
        }
        if self.at_keyword(Keyword::Let) {
            return Stmt::Let(self.parse_let_like(Keyword::Let));
        }
        if self.at_keyword(Keyword::Const) {
            return Stmt::Const(self.parse_let_like(Keyword::Const));
        }
        if self.at_keyword(Keyword::Return) {
            return self.parse_return();
        }
        if self.at_keyword(Keyword::If) {
            return Stmt::If(self.parse_if());
        }
        if self.at_keyword(Keyword::While) {
            return Stmt::While(self.parse_while());
        }

        let start = self.current_span_start();
        let expr = self.parse_expr();
        self.expect_stmt_semi();
        let end = self.prev_span_end();
        Stmt::Expr(ExprStmt {
            expr,
            span: Span::new(start, end),
        })
    }

    fn parse_let_like(&mut self, kw: Keyword) -> LetStmt {
        let start = self.current_span_start();
        self.expect_keyword(kw);
        let name = self.parse_ident();

        let ty = if self.eat_punct(Punct::Colon) {
            Some(self.parse_type_ref())
        } else {
            None
        };

        let init = if self.eat_operator(Operator::Eq) {
            Some(self.parse_expr())
        } else {
            None
        };

        self.expect_stmt_semi();
        let end = self.prev_span_end();
        LetStmt {
            name,
            ty,
            init,
            span: Span::new(start, end),
        }
    }

    fn parse_return(&mut self) -> Stmt {
        let start = self.current_span_start();
        self.expect_keyword(Keyword::Return);
        let value = if self.at_punct(Punct::Semi) {
            None
        } else {
            Some(self.parse_expr())
        };
        self.expect_stmt_semi();
        let end = self.prev_span_end();
        Stmt::Return(ReturnStmt {
            value,
            span: Span::new(start, end),
        })
    }

    fn parse_if(&mut self) -> IfStmt {
        let start = self.current_span_start();
        self.expect_keyword(Keyword::If);
        self.expect_punct(Punct::LParen);
        let cond = self.parse_expr();
        self.expect_punct_with_sync(
            Punct::RParen,
            &[
                TokenKind::Punct(Punct::RParen),
                TokenKind::Punct(Punct::LBrace),
            ],
        );
        let then_block = self.parse_block();
        let else_block = if self.eat_keyword(Keyword::Else) {
            Some(self.parse_block())
        } else {
            None
        };
        let end = else_block
            .as_ref()
            .map(|b| b.span.end)
            .unwrap_or(then_block.span.end);
        IfStmt {
            cond,
            then_block,
            else_block,
            span: Span::new(start, end),
        }
    }

    fn parse_while(&mut self) -> WhileStmt {
        let start = self.current_span_start();
        self.expect_keyword(Keyword::While);
        self.expect_punct(Punct::LParen);
        let cond = self.parse_expr();
        self.expect_punct_with_sync(
            Punct::RParen,
            &[
                TokenKind::Punct(Punct::RParen),
                TokenKind::Punct(Punct::LBrace),
            ],
        );
        let body = self.parse_block();
        WhileStmt {
            cond,
            body: body.clone(),
            span: Span::new(start, body.span.end),
        }
    }

    fn parse_expr(&mut self) -> Expr {
        self.parse_assign()
    }

    fn parse_assign(&mut self) -> Expr {
        let start = self.current_span_start();
        let mut expr = self.parse_or();
        if self.eat_operator(Operator::Eq) {
            let value = self.parse_assign();
            let end = self.prev_span_end();
            expr = Expr::Assign {
                target: Box::new(expr),
                value: Box::new(value),
                span: Span::new(start, end),
            };
        }
        expr
    }

    fn parse_or(&mut self) -> Expr {
        self.parse_binary_left_assoc(Self::parse_and, |op| matches!(op, Operator::OrOr), |span| {
            Expr::Binary {
                op: BinaryOp::OrOr,
                left: Box::new(Expr::IntLit(span)),
                right: Box::new(Expr::IntLit(span)),
                span,
            }
        })
    }

    fn parse_and(&mut self) -> Expr {
        self.parse_binary_left_assoc(Self::parse_equality, |op| matches!(op, Operator::AndAnd), |span| {
            Expr::Binary {
                op: BinaryOp::AndAnd,
                left: Box::new(Expr::IntLit(span)),
                right: Box::new(Expr::IntLit(span)),
                span,
            }
        })
    }

    fn parse_equality(&mut self) -> Expr {
        let mut left = self.parse_comparison();
        loop {
            let op = if self.at_operator(Operator::EqEq) {
                Some(BinaryOp::EqEq)
            } else if self.at_operator(Operator::NotEq) {
                Some(BinaryOp::NotEq)
            } else {
                None
            };
            let Some(op) = op else { break };
            let start = self.span_of_expr(&left).start;
            self.bump();
            let right = self.parse_comparison();
            let end = self.span_of_expr(&right).end;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span: Span::new(start, end),
            };
        }
        left
    }

    fn parse_comparison(&mut self) -> Expr {
        let mut left = self.parse_term();
        loop {
            let op = match self.current_kind() {
                TokenKind::Operator(Operator::Lt) => Some(BinaryOp::Lt),
                TokenKind::Operator(Operator::LtEq) => Some(BinaryOp::LtEq),
                TokenKind::Operator(Operator::Gt) => Some(BinaryOp::Gt),
                TokenKind::Operator(Operator::GtEq) => Some(BinaryOp::GtEq),
                _ => None,
            };
            let Some(op) = op else { break };
            let start = self.span_of_expr(&left).start;
            self.bump();
            let right = self.parse_term();
            let end = self.span_of_expr(&right).end;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span: Span::new(start, end),
            };
        }
        left
    }

    fn parse_term(&mut self) -> Expr {
        let mut left = self.parse_factor();
        loop {
            let op = match self.current_kind() {
                TokenKind::Operator(Operator::Plus) => Some(BinaryOp::Add),
                TokenKind::Operator(Operator::Minus) => Some(BinaryOp::Sub),
                _ => None,
            };
            let Some(op) = op else { break };
            let start = self.span_of_expr(&left).start;
            self.bump();
            let right = self.parse_factor();
            let end = self.span_of_expr(&right).end;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span: Span::new(start, end),
            };
        }
        left
    }

    fn parse_factor(&mut self) -> Expr {
        let mut left = self.parse_unary();
        loop {
            let op = match self.current_kind() {
                TokenKind::Operator(Operator::Star) => Some(BinaryOp::Mul),
                TokenKind::Operator(Operator::Slash) => Some(BinaryOp::Div),
                _ => None,
            };
            let Some(op) = op else { break };
            let start = self.span_of_expr(&left).start;
            self.bump();
            let right = self.parse_unary();
            let end = self.span_of_expr(&right).end;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span: Span::new(start, end),
            };
        }
        left
    }

    fn parse_unary(&mut self) -> Expr {
        let start = self.current_span_start();
        match self.current_kind() {
            TokenKind::Operator(Operator::Minus) => {
                self.bump();
                let expr = self.parse_unary();
                let end = self.span_of_expr(&expr).end;
                Expr::Unary {
                    op: UnaryOp::Neg,
                    expr: Box::new(expr),
                    span: Span::new(start, end),
                }
            }
            TokenKind::Operator(Operator::Bang) => {
                self.bump();
                let expr = self.parse_unary();
                let end = self.span_of_expr(&expr).end;
                Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(expr),
                    span: Span::new(start, end),
                }
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Expr {
        let mut expr = self.parse_primary();
        loop {
            if self.eat_punct(Punct::LParen) {
                let start = self.span_of_expr(&expr).start;
                let mut args = Vec::new();
                if !self.at_punct(Punct::RParen) && !self.at(TokenKind::Eof) {
                    loop {
                        args.push(self.parse_expr());
                        if self.eat_punct(Punct::Comma) {
                            continue;
                        }
                        break;
                    }
                }
                self.expect_punct_with_sync(
                    Punct::RParen,
                    &[
                        TokenKind::Punct(Punct::RParen),
                        TokenKind::Punct(Punct::Semi),
                        TokenKind::Punct(Punct::RBrace),
                    ],
                );
                let end = self.prev_span_end();
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                    span: Span::new(start, end),
                };
                continue;
            }
            if self.eat_punct(Punct::Dot) {
                let start = self.span_of_expr(&expr).start;
                let field = self.parse_ident();
                let end = self.prev_span_end();
                expr = Expr::Member {
                    object: Box::new(expr),
                    field,
                    span: Span::new(start, end),
                };
                continue;
            }
            break;
        }
        expr
    }

    fn parse_primary(&mut self) -> Expr {
        let span = self.current_span();
        match self.current_kind() {
            TokenKind::Ident => {
                let ident = Ident { span };
                self.bump();
                Expr::Ident(ident)
            }
            TokenKind::Int => {
                self.bump();
                Expr::IntLit(span)
            }
            TokenKind::Float => {
                self.bump();
                Expr::FloatLit(span)
            }
            TokenKind::String => {
                self.bump();
                Expr::StringLit(span)
            }
            TokenKind::Keyword(Keyword::True) => {
                self.bump();
                Expr::BoolLit(true, span)
            }
            TokenKind::Keyword(Keyword::False) => {
                self.bump();
                Expr::BoolLit(false, span)
            }
            TokenKind::Punct(Punct::LParen) => {
                let start = span.start;
                self.bump();
                let inner = self.parse_expr();
                self.expect_punct_with_sync(
                    Punct::RParen,
                    &[
                        TokenKind::Punct(Punct::RParen),
                        TokenKind::Punct(Punct::Semi),
                        TokenKind::Punct(Punct::RBrace),
                    ],
                );
                let end = self.prev_span_end();
                Expr::Paren {
                    expr: Box::new(inner),
                    span: Span::new(start, end),
                }
            }
            _ => {
                self.error(span, "expected expression");
                self.bump();
                Expr::IntLit(span)
            }
        }
    }

    fn parse_ident(&mut self) -> Ident {
        let span = self.current_span();
        if self.at(TokenKind::Ident) {
            self.bump();
            Ident { span }
        } else {
            self.error(span, "expected identifier");
            self.bump();
            Ident { span }
        }
    }

    fn parse_binary_left_assoc(
        &mut self,
        sub: fn(&mut Self) -> Expr,
        matches_op: impl Fn(Operator) -> bool,
        _dummy: impl Fn(Span) -> Expr,
    ) -> Expr {
        let mut left = sub(self);
        loop {
            let op = match self.current_kind() {
                TokenKind::Operator(op) if matches_op(op) => Some(op),
                _ => None,
            };
            let Some(op) = op else { break };
            let (ast_op, _) = match op {
                Operator::OrOr => (BinaryOp::OrOr, op),
                Operator::AndAnd => (BinaryOp::AndAnd, op),
                _ => break,
            };
            let start = self.span_of_expr(&left).start;
            self.bump();
            let right = sub(self);
            let end = self.span_of_expr(&right).end;
            left = Expr::Binary {
                op: ast_op,
                left: Box::new(left),
                right: Box::new(right),
                span: Span::new(start, end),
            };
        }
        left
    }

    fn span_of_expr(&self, expr: &Expr) -> Span {
        match expr {
            Expr::Ident(i) => i.span,
            Expr::IntLit(s) => *s,
            Expr::FloatLit(s) => *s,
            Expr::StringLit(s) => *s,
            Expr::BoolLit(_, s) => *s,
            Expr::Unary { span, .. } => *span,
            Expr::Binary { span, .. } => *span,
            Expr::Assign { span, .. } => *span,
            Expr::Call { span, .. } => *span,
            Expr::Member { span, .. } => *span,
            Expr::Paren { span, .. } => *span,
        }
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.current_kind() == kind
    }

    fn at_keyword(&self, kw: Keyword) -> bool {
        self.current_kind() == TokenKind::Keyword(kw)
    }

    fn at_operator(&self, op: Operator) -> bool {
        self.current_kind() == TokenKind::Operator(op)
    }

    fn at_punct(&self, p: Punct) -> bool {
        self.current_kind() == TokenKind::Punct(p)
    }

    fn eat_keyword(&mut self, kw: Keyword) -> bool {
        if self.at_keyword(kw) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_operator(&mut self, op: Operator) -> bool {
        if self.at_operator(op) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_punct(&mut self, p: Punct) -> bool {
        if self.at_punct(p) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect_keyword(&mut self, kw: Keyword) {
        if !self.eat_keyword(kw) {
            self.error(self.current_span(), &format!("expected keyword `{}`", keyword_text(kw)));
            self.bump();
        }
    }

    fn expect_punct(&mut self, p: Punct) {
        if !self.eat_punct(p) {
            self.error(self.current_span(), &format!("expected `{}`", punct_text(p)));
            self.bump();
        }
    }

    fn expect_stmt_semi(&mut self) {
        self.expect_punct_with_sync(
            Punct::Semi,
            &[
                TokenKind::Punct(Punct::Semi),
                TokenKind::Punct(Punct::RBrace),
                TokenKind::Eof,
            ],
        );
    }

    fn expect_punct_with_sync(&mut self, punct: Punct, sync: &[TokenKind]) {
        if self.eat_punct(punct) {
            return;
        }
        self.error(self.current_span(), &format!("expected `{}`", punct_text(punct)));
        self.synchronize(sync);
        self.eat_punct(punct);
    }

    fn synchronize(&mut self, sync: &[TokenKind]) {
        if self.at(TokenKind::Eof) {
            return;
        }
        if sync.contains(&self.current_kind()) {
            return;
        }
        self.bump();
        while !self.at(TokenKind::Eof) && !sync.contains(&self.current_kind()) {
            self.bump();
        }
    }

    fn bump(&mut self) {
        if !self.at(TokenKind::Eof) {
            self.idx += 1;
        }
    }

    fn current_kind(&self) -> TokenKind {
        self.tokens
            .get(self.idx)
            .map(|t| t.kind)
            .unwrap_or(TokenKind::Eof)
    }

    fn current_span(&self) -> Span {
        self.tokens
            .get(self.idx)
            .map(|t| t.span)
            .unwrap_or_else(|| Span::empty(BytePos::new(self.source.len() as u32)))
    }

    fn current_span_start(&self) -> BytePos {
        self.current_span().start
    }

    fn prev_span(&self) -> Span {
        self.tokens
            .get(self.idx.saturating_sub(1))
            .map(|t| t.span)
            .unwrap_or_else(|| Span::empty(BytePos::new(0)))
    }

    fn prev_span_end(&self) -> BytePos {
        self.prev_span().end
    }

    fn error(&mut self, span: Span, message: &str) {
        self.errors.push(ParseError {
            span,
            message: message.to_string(),
        });
    }
}

fn keyword_text(kw: Keyword) -> &'static str {
    match kw {
        Keyword::Class => "class",
        Keyword::Interface => "interface",
        Keyword::Extends => "extends",
        Keyword::Implements => "implements",
        Keyword::Function => "function",
        Keyword::Return => "return",
        Keyword::Let => "let",
        Keyword::Const => "const",
        Keyword::If => "if",
        Keyword::Else => "else",
        Keyword::While => "while",
        Keyword::For => "for",
        Keyword::Break => "break",
        Keyword::Continue => "continue",
        Keyword::Try => "try",
        Keyword::Catch => "catch",
        Keyword::Finally => "finally",
        Keyword::Throw => "throw",
        Keyword::Import => "import",
        Keyword::Export => "export",
        Keyword::New => "new",
        Keyword::This => "this",
        Keyword::True => "true",
        Keyword::False => "false",
    }
}

fn punct_text(p: Punct) -> &'static str {
    match p {
        Punct::LParen => "(",
        Punct::RParen => ")",
        Punct::LBrace => "{",
        Punct::RBrace => "}",
        Punct::LBracket => "[",
        Punct::RBracket => "]",
        Punct::Comma => ",",
        Punct::Dot => ".",
        Punct::Colon => ":",
        Punct::Semi => ";",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_function_with_return() {
        let src = r#"
function add(a: i32, b: i32): i32 {
  return a + b;
}
"#;

        let out = parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);
        assert_eq!(out.value.items.len(), 1);
        match &out.value.items[0] {
            TopLevel::Function(f) => {
                assert_eq!(f.params.len(), 2);
                assert!(f.return_type.is_some());
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn recovers_from_missing_semicolon() {
        let src = r#"
function f(): void {
  let x: i32 = 1
  return x
}
"#;
        let out = parse_program(src);
        assert!(!out.errors.is_empty());
        assert_eq!(out.value.items.len(), 1);
    }
}
