use aura_ast::*;
use aura_diagnostics::Diagnostic;
use aura_lexer::{Keyword, Operator, Punct, Token, TokenKind};
use aura_span::{BytePos, Span};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseOutput<T> {
    pub value: T,
    pub errors: Vec<Diagnostic>,
}

pub fn parse_program(source: &str) -> ParseOutput<Program> {
    let tokens = aura_lexer::lex(source);
    Parser::new(source, tokens).parse_program()
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    idx: usize,
    errors: Vec<Diagnostic>,
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
            if self.at_keyword(Keyword::Import) {
                items.push(TopLevel::Import(self.parse_import_decl()));
            } else if self.at_keyword(Keyword::Export) {
                items.push(TopLevel::Export(self.parse_export_decl()));
            } else if self.at_keyword(Keyword::Function) {
                items.push(TopLevel::Function(self.parse_function_decl()));
            } else if self.at_keyword(Keyword::Class) {
                items.push(TopLevel::Class(self.parse_class_decl()));
            } else if self.at_keyword(Keyword::Interface) {
                items.push(TopLevel::Interface(self.parse_interface_decl()));
            } else {
                items.push(TopLevel::Stmt(self.parse_stmt()));
            }
        }
        ParseOutput {
            value: Program { items },
            errors: self.errors,
        }
    }

    fn parse_export_decl(&mut self) -> ExportDecl {
        let start = self.current_span_start();
        self.expect_keyword(Keyword::Export);

        let item = if self.at_keyword(Keyword::Function) {
            Some(ExportedDecl::Function(self.parse_function_decl()))
        } else if self.at_keyword(Keyword::Class) {
            Some(ExportedDecl::Class(self.parse_class_decl()))
        } else if self.at_keyword(Keyword::Interface) {
            Some(ExportedDecl::Interface(self.parse_interface_decl()))
        } else {
            let span = self.current_span();
            self.error(
                span,
                &format!(
                    "expected `function`, `class`, or `interface` after `export`, found {}",
                    token_desc(self.current_kind())
                ),
            );
            self.bump();
            self.synchronize(&[
                TokenKind::Punct(Punct::Semi),
                TokenKind::Punct(Punct::RBrace),
            ]);
            None
        };

        let end = match &item {
            Some(ExportedDecl::Function(func)) => func.span.end,
            Some(ExportedDecl::Class(class_decl)) => class_decl.span.end,
            Some(ExportedDecl::Interface(iface)) => iface.span.end,
            None => self.prev_span_end(),
        };

        ExportDecl {
            item,
            span: Span::new(start, end),
        }
    }

    fn parse_import_decl(&mut self) -> ImportDecl {
        let start = self.current_span_start();
        self.expect_keyword(Keyword::Import);

        let clause = if self.eat_punct(Punct::LBrace) {
            let mut names = Vec::new();
            if !self.at_punct(Punct::RBrace) && !self.at(TokenKind::Eof) {
                loop {
                    names.push(self.parse_ident());
                    if self.eat_punct(Punct::Comma) {
                        continue;
                    }
                    break;
                }
            }
            self.expect_punct(Punct::RBrace);
            ImportClause::Named(names)
        } else {
            ImportClause::Default(self.parse_ident())
        };

        self.expect_ident_text("from");

        let from_path = self.current_span();
        if self.at(TokenKind::String) {
            self.bump();
        } else {
            self.error(
                from_path,
                &format!(
                    "expected string literal, found {}",
                    token_desc(self.current_kind())
                ),
            );
            self.bump();
        }

        // Semicolons are optional for import declarations.
        self.eat_punct(Punct::Semi);

        let end = self.prev_span_end();
        ImportDecl {
            clause,
            from_path,
            span: Span::new(start, end),
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

    fn parse_class_decl(&mut self) -> ClassDecl {
        let start = self.current_span_start();
        self.expect_keyword(Keyword::Class);
        let name = self.parse_ident();

        let extends = if self.eat_keyword(Keyword::Extends) {
            Some(self.parse_type_ref())
        } else {
            None
        };

        let mut implements = Vec::new();
        if self.eat_keyword(Keyword::Implements) {
            loop {
                implements.push(self.parse_type_ref());
                if self.eat_punct(Punct::Comma) {
                    continue;
                }
                break;
            }
        }

        self.expect_punct(Punct::LBrace);
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        while !self.at_punct(Punct::RBrace) && !self.at(TokenKind::Eof) {
            if self.eat_punct(Punct::Semi) {
                continue;
            }
            if self.at_keyword(Keyword::Function) {
                methods.push(self.parse_method_decl());
                continue;
            }
            if self.at(TokenKind::Ident) {
                fields.push(self.parse_field_decl());
                continue;
            }
            let span = self.current_span();
            self.error(
                span,
                &format!(
                    "expected class member, found {}",
                    token_desc(self.current_kind())
                ),
            );
            self.bump();
            self.synchronize(&[
                TokenKind::Punct(Punct::Semi),
                TokenKind::Punct(Punct::RBrace),
            ]);
        }
        self.expect_punct(Punct::RBrace);
        let end = self.prev_span_end();
        ClassDecl {
            name,
            extends,
            implements,
            fields,
            methods,
            span: Span::new(start, end),
        }
    }

    fn parse_interface_decl(&mut self) -> InterfaceDecl {
        let start = self.current_span_start();
        self.expect_keyword(Keyword::Interface);
        let name = self.parse_ident();
        self.expect_punct(Punct::LBrace);
        let mut methods = Vec::new();
        while !self.at_punct(Punct::RBrace) && !self.at(TokenKind::Eof) {
            if self.eat_punct(Punct::Semi) {
                continue;
            }
            methods.push(self.parse_interface_method_decl());
        }
        self.expect_punct(Punct::RBrace);
        let end = self.prev_span_end();
        InterfaceDecl {
            name,
            methods,
            span: Span::new(start, end),
        }
    }

    fn parse_interface_method_decl(&mut self) -> InterfaceMethodDecl {
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

        self.eat_punct(Punct::Semi);

        let end = self.prev_span_end();
        InterfaceMethodDecl {
            name,
            params,
            return_type,
            span: Span::new(start, end),
        }
    }

    fn parse_field_decl(&mut self) -> FieldDecl {
        let start = self.current_span_start();
        let name = self.parse_ident();
        self.expect_punct(Punct::Colon);
        let ty = self.parse_type_ref();
        self.eat_punct(Punct::Semi);
        let end = self.prev_span_end();
        FieldDecl {
            name,
            ty,
            span: Span::new(start, end),
        }
    }

    fn parse_method_decl(&mut self) -> MethodDecl {
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
        MethodDecl {
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
        if self.at_keyword(Keyword::Try) {
            return Stmt::Try(self.parse_try());
        }
        if self.at_keyword(Keyword::Throw) {
            return Stmt::Throw(self.parse_throw());
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

    fn parse_try(&mut self) -> TryStmt {
        let start = self.current_span_start();
        self.expect_keyword(Keyword::Try);
        let try_block = self.parse_block();

        let catch = if self.eat_keyword(Keyword::Catch) {
            Some(self.parse_catch_clause())
        } else {
            None
        };

        let finally_block = if self.eat_keyword(Keyword::Finally) {
            Some(self.parse_block())
        } else {
            None
        };

        if catch.is_none() && finally_block.is_none() {
            self.error(
                try_block.span,
                "expected `catch` or `finally` after `try` block",
            );
        }

        let end = finally_block
            .as_ref()
            .map(|b| b.span.end)
            .or_else(|| catch.as_ref().map(|c| c.span.end))
            .unwrap_or(try_block.span.end);

        TryStmt {
            try_block,
            catch,
            finally_block,
            span: Span::new(start, end),
        }
    }

    fn parse_catch_clause(&mut self) -> CatchClause {
        let start = self.current_span_start();
        self.expect_punct(Punct::LParen);
        let binding = self.parse_ident();
        let ty = if self.eat_punct(Punct::Colon) {
            Some(self.parse_type_ref())
        } else {
            None
        };
        self.expect_punct(Punct::RParen);
        let block = self.parse_block();
        let end = block.span.end;
        CatchClause {
            binding,
            ty,
            block,
            span: Span::new(start, end),
        }
    }

    fn parse_throw(&mut self) -> ThrowStmt {
        let start = self.current_span_start();
        self.expect_keyword(Keyword::Throw);
        let value = self.parse_expr();
        self.expect_stmt_semi();
        let end = self.prev_span_end();
        ThrowStmt {
            value,
            span: Span::new(start, end),
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
        self.parse_binary_left_assoc(
            Self::parse_and,
            |op| matches!(op, Operator::OrOr),
            |span| Expr::Binary {
                op: BinaryOp::OrOr,
                left: Box::new(Expr::IntLit(span)),
                right: Box::new(Expr::IntLit(span)),
                span,
            },
        )
    }

    fn parse_and(&mut self) -> Expr {
        self.parse_binary_left_assoc(
            Self::parse_equality,
            |op| matches!(op, Operator::AndAnd),
            |span| Expr::Binary {
                op: BinaryOp::AndAnd,
                left: Box::new(Expr::IntLit(span)),
                right: Box::new(Expr::IntLit(span)),
                span,
            },
        )
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
            TokenKind::Keyword(Keyword::This) => {
                self.bump();
                Expr::This(span)
            }
            TokenKind::Keyword(Keyword::New) => {
                let start = span.start;
                self.bump();
                let class = self.parse_ident();
                self.expect_punct_with_sync(
                    Punct::LParen,
                    &[
                        TokenKind::Punct(Punct::LParen),
                        TokenKind::Punct(Punct::Semi),
                        TokenKind::Punct(Punct::RBrace),
                    ],
                );
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
                Expr::New {
                    class,
                    args,
                    span: Span::new(start, end),
                }
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
                self.error(
                    span,
                    &format!(
                        "expected expression, found {}",
                        token_desc(self.current_kind())
                    ),
                );
                self.bump();
                self.synchronize(&[
                    TokenKind::Punct(Punct::Semi),
                    TokenKind::Punct(Punct::RParen),
                    TokenKind::Punct(Punct::RBrace),
                    TokenKind::Punct(Punct::Comma),
                    TokenKind::Eof,
                ]);
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
            self.error(
                span,
                &format!(
                    "expected identifier, found {}",
                    token_desc(self.current_kind())
                ),
            );
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
            Expr::This(s) => *s,
            Expr::IntLit(s) => *s,
            Expr::FloatLit(s) => *s,
            Expr::StringLit(s) => *s,
            Expr::BoolLit(_, s) => *s,
            Expr::Unary { span, .. } => *span,
            Expr::Binary { span, .. } => *span,
            Expr::Assign { span, .. } => *span,
            Expr::Call { span, .. } => *span,
            Expr::New { span, .. } => *span,
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

    fn at_ident_text(&self, expected: &str) -> bool {
        if !self.at(TokenKind::Ident) {
            return false;
        }
        let span = self.current_span();
        let start = span.start.raw() as usize;
        let end = span.end.raw() as usize;
        matches!(self.source.get(start..end), Some(text) if text == expected)
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
            self.error(
                self.current_span(),
                &format!("expected keyword `{}`", keyword_text(kw)),
            );
            self.bump();
        }
    }

    fn expect_punct(&mut self, p: Punct) {
        if !self.eat_punct(p) {
            self.error(
                self.current_span(),
                &format!("expected `{}`", punct_text(p)),
            );
            self.bump();
        }
    }

    fn expect_ident_text(&mut self, expected: &str) {
        if self.at_ident_text(expected) {
            self.bump();
            return;
        }
        self.error(self.current_span(), &format!("expected `{expected}`"));
        self.bump();
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
        self.error(
            self.current_span(),
            &format!("expected `{}`", punct_text(punct)),
        );
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
        self.errors.push(Diagnostic::error(span, message));
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

fn operator_text(op: Operator) -> &'static str {
    match op {
        Operator::Plus => "+",
        Operator::Minus => "-",
        Operator::Star => "*",
        Operator::Slash => "/",
        Operator::EqEq => "==",
        Operator::NotEq => "!=",
        Operator::Lt => "<",
        Operator::LtEq => "<=",
        Operator::Gt => ">",
        Operator::GtEq => ">=",
        Operator::AndAnd => "&&",
        Operator::OrOr => "||",
        Operator::Bang => "!",
        Operator::Eq => "=",
    }
}

fn token_desc(kind: TokenKind) -> String {
    match kind {
        TokenKind::Ident => "identifier".to_string(),
        TokenKind::Int => "integer literal".to_string(),
        TokenKind::Float => "float literal".to_string(),
        TokenKind::String => "string literal".to_string(),
        TokenKind::Keyword(kw) => format!("keyword `{}`", keyword_text(kw)),
        TokenKind::Operator(op) => format!("operator `{}`", operator_text(op)),
        TokenKind::Punct(p) => format!("`{}`", punct_text(p)),
        TokenKind::Eof => "end of file".to_string(),
        TokenKind::Unknown => "unknown token".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_diagnostics::format_all;

    #[test]
    fn parses_imports() {
        let src = r#"
import { Foo, bar } from "./foo"
import Baz from "./baz";

function main(): i32 { return 0; }
"#;

        let out = parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);
        assert_eq!(out.value.items.len(), 3);
        assert!(matches!(out.value.items[0], TopLevel::Import(_)));
        assert!(matches!(out.value.items[1], TopLevel::Import(_)));
        assert!(matches!(out.value.items[2], TopLevel::Function(_)));
    }

    #[test]
    fn parses_exported_function() {
        let src = r#"
export function helper(x: i32): i32 {
  return x;
}
"#;

        let out = parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);
        assert_eq!(out.value.items.len(), 1);
        match &out.value.items[0] {
            TopLevel::Export(export) => match export.item.as_ref() {
                Some(ExportedDecl::Function(func)) => {
                    assert_eq!(func.params.len(), 1);
                }
                _ => panic!("expected exported function"),
            },
            _ => panic!("expected export"),
        }
    }

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

    #[test]
    fn snapshot_missing_semicolon() {
        let src = r#"function f(): void { return 1 }"#;
        let out = parse_program(src);
        let rendered = format_all(src, &out.errors);
        assert_eq!(
            rendered,
            r#"error[E]: expected `;`
 --> line 1, col 31
  |
 1 | function f(): void { return 1 }
  |                               ^
"#
        );
    }

    #[test]
    fn snapshot_unmatched_rbrace() {
        let src = r#"}"#;
        let out = parse_program(src);
        let rendered = format_all(src, &out.errors);
        assert_eq!(
            rendered,
            r#"error[E]: expected expression, found `}`
 --> line 1, col 1
  |
 1 | }
  | ^

error[E]: expected `;`
 --> line 1, col 2
  |
 1 | }
  |  ^
"#
        );
    }

    #[test]
    fn snapshot_bad_token_in_expression() {
        let src = r#"function f(): void { return + 1; }"#;
        let out = parse_program(src);
        let rendered = format_all(src, &out.errors);
        assert_eq!(
            rendered,
            r#"error[E]: expected expression, found operator `+`
 --> line 1, col 29
  |
 1 | function f(): void { return + 1; }
  |                             ^
"#
        );
    }

    #[test]
    fn parses_this_and_new_expressions() {
        let src = r#"
function f(): void {
  let p = new Point(1, 2);
  this.x = 1;
}
"#;
        let out = parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);
        assert_eq!(out.value.items.len(), 1);
    }

    #[test]
    fn parses_try_catch_finally_and_throw() {
        let src = r#"
function f(): void {
  try {
    throw 1;
  } catch (e) {
    return;
  } finally {
    let done = 1;
  }
}
"#;
        let out = parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);
        match &out.value.items[0] {
            TopLevel::Function(func) => {
                assert!(matches!(func.body.stmts[0], Stmt::Try(_)));
            }
            _ => panic!("expected function"),
        }
    }
}
