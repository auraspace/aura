use crate::compiler::ast::{Expr, TemplatePart, TplPart};
use crate::compiler::frontend::error::Diagnostic;
use crate::compiler::frontend::parser::Parser;
use crate::compiler::frontend::token::TokenKind;

impl Parser {
    pub(crate) fn parse_expression(&mut self) -> Expr {
        let s = self.span();
        let node = self.parse_logical_or();

        if self.peek().kind == TokenKind::Equal {
            self.advance();
            let value = self.parse_expression();
            if let Expr::Variable(name, vs) = node {
                return Expr::Assign(name, Box::new(value), vs);
            } else if let Expr::MemberAccess(obj, member, name_span, _ms) = node {
                return Expr::MemberAssign(obj, member, Box::new(value), name_span, s);
            } else {
                let token = self.peek();
                self.diagnostics.push(Diagnostic::error(
                    "Invalid assignment target".to_string(),
                    token.line,
                    token.column,
                ));
                return Expr::Error(s);
            }
        }

        node
    }

    pub(crate) fn parse_logical_or(&mut self) -> Expr {
        let s = self.span();
        let mut node = self.parse_logical_and();
        while self.peek().kind == TokenKind::Or {
            self.advance();
            let right = self.parse_logical_and();
            node = Expr::BinaryOp(Box::new(node), "||".to_string(), Box::new(right), s);
        }
        node
    }

    pub(crate) fn parse_logical_and(&mut self) -> Expr {
        let s = self.span();
        let mut node = self.parse_bitwise_or();
        while self.peek().kind == TokenKind::And {
            self.advance();
            let right = self.parse_bitwise_or();
            node = Expr::BinaryOp(Box::new(node), "&&".to_string(), Box::new(right), s);
        }
        node
    }

    pub(crate) fn parse_comparison(&mut self) -> Expr {
        let s = self.span();
        let mut node = self.parse_shift();

        while let TokenKind::Less
        | TokenKind::LessEqual
        | TokenKind::Greater
        | TokenKind::GreaterEqual
        | TokenKind::EqEqual
        | TokenKind::BangEqual = self.peek().kind
        {
            let op = match self.peek().kind {
                TokenKind::Less => "<",
                TokenKind::LessEqual => "<=",
                TokenKind::Greater => ">",
                TokenKind::GreaterEqual => ">=",
                TokenKind::EqEqual => "==",
                TokenKind::BangEqual => "!=",
                _ => unreachable!(),
            }
            .to_string();
            self.advance();
            let right = self.parse_shift();
            node = Expr::BinaryOp(Box::new(node), op, Box::new(right), s);
        }

        node
    }

    pub(crate) fn parse_bitwise_or(&mut self) -> Expr {
        let s = self.span();
        let mut node = self.parse_bitwise_xor();
        while self.peek().kind == TokenKind::Pipe {
            self.advance();
            let right = self.parse_bitwise_xor();
            node = Expr::BinaryOp(Box::new(node), "|".to_string(), Box::new(right), s);
        }
        node
    }

    pub(crate) fn parse_bitwise_xor(&mut self) -> Expr {
        let s = self.span();
        let mut node = self.parse_bitwise_and();
        while self.peek().kind == TokenKind::Caret {
            self.advance();
            let right = self.parse_bitwise_and();
            node = Expr::BinaryOp(Box::new(node), "^".to_string(), Box::new(right), s);
        }
        node
    }

    pub(crate) fn parse_bitwise_and(&mut self) -> Expr {
        let s = self.span();
        let mut node = self.parse_comparison();
        while self.peek().kind == TokenKind::Ampersand {
            self.advance();
            let right = self.parse_comparison();
            node = Expr::BinaryOp(Box::new(node), "&".to_string(), Box::new(right), s);
        }
        node
    }

    pub(crate) fn parse_shift(&mut self) -> Expr {
        let s = self.span();
        let mut node = self.parse_type_test();

        while let TokenKind::LessLess | TokenKind::GreaterGreater = self.peek().kind {
            let op = match self.peek().kind {
                TokenKind::LessLess => "<<",
                TokenKind::GreaterGreater => ">>",
                _ => unreachable!(),
            }
            .to_string();
            self.advance();
            let right = self.parse_type_test();
            node = Expr::BinaryOp(Box::new(node), op, Box::new(right), s);
        }

        node
    }

    pub(crate) fn parse_type_test(&mut self) -> Expr {
        let s = self.span();
        let mut node = self.parse_arithmetic();
        while self.peek().kind == TokenKind::Is {
            self.advance();
            let ty = self.parse_type_expr();
            node = Expr::TypeTest(Box::new(node), ty, s);
        }
        node
    }

    pub(crate) fn parse_arithmetic(&mut self) -> Expr {
        let s = self.span();
        let mut node = self.parse_multiplicative();

        while let TokenKind::Plus | TokenKind::Minus = self.peek().kind {
            let op = match self.peek().kind {
                TokenKind::Plus => "+",
                TokenKind::Minus => "-",
                _ => unreachable!(),
            }
            .to_string();
            self.advance();
            let right = self.parse_multiplicative();
            node = Expr::BinaryOp(Box::new(node), op, Box::new(right), s);
        }

        node
    }

    pub(crate) fn parse_multiplicative(&mut self) -> Expr {
        let s = self.span();
        let mut node = self.parse_unary();

        while let TokenKind::Star | TokenKind::Slash | TokenKind::Percent = self.peek().kind {
            let op = match self.peek().kind {
                TokenKind::Star => "*",
                TokenKind::Slash => "/",
                TokenKind::Percent => "%",
                _ => unreachable!(),
            }
            .to_string();
            self.advance();
            let right = self.parse_unary();
            node = Expr::BinaryOp(Box::new(node), op, Box::new(right), s);
        }

        node
    }

    pub(crate) fn parse_unary(&mut self) -> Expr {
        let s = self.span();
        if self.peek().kind == TokenKind::Minus {
            self.advance();
            let expr = self.parse_unary();
            return Expr::UnaryOp("-".to_string(), Box::new(expr), s);
        }
        if self.peek().kind == TokenKind::Tilde {
            self.advance();
            let expr = self.parse_unary();
            return Expr::UnaryOp("~".to_string(), Box::new(expr), s);
        }
        self.parse_primary()
    }

    pub(crate) fn parse_primary(&mut self) -> Expr {
        let s = self.span();
        let node = match self.peek().kind.clone() {
            TokenKind::Number(val) => {
                self.advance();
                Expr::Number(val, s)
            }
            TokenKind::Float(val) => {
                self.advance();
                Expr::Float(val, s)
            }
            TokenKind::StringLiteral(ls) => {
                self.advance();
                Expr::StringLiteral(ls, s)
            }
            TokenKind::TemplateLiteral(parts) => {
                self.advance();
                let mut ast_parts = Vec::new();
                for part in parts {
                    match part {
                        TplPart::Str(st) => ast_parts.push(TemplatePart::Str(st)),
                        TplPart::Expr(src, line, col) => {
                            let mut sub_lexer =
                                crate::compiler::frontend::lexer::Lexer::new_with_offset(
                                    &src, line, col,
                                );
                            let sub_tokens = sub_lexer.lex_all();
                            let mut sub_parser = Parser::new(sub_tokens, self.file_path.clone());
                            let expr = sub_parser.parse_expression();
                            for mut d in sub_lexer.diagnostics.diagnostics {
                                d.line += s.line - 1;
                                self.diagnostics.push(d);
                            }
                            for mut d in sub_parser.diagnostics.diagnostics {
                                d.line += s.line - 1;
                                self.diagnostics.push(d);
                            }
                            ast_parts.push(TemplatePart::Expr(Box::new(expr)));
                        }
                    }
                }
                Expr::Template(ast_parts, s)
            }
            TokenKind::Null => {
                self.advance();
                Expr::Null(s)
            }
            TokenKind::Identifier(name) => {
                self.advance();
                Expr::Variable(name, s)
            }
            TokenKind::This => {
                self.advance();
                Expr::This(s)
            }
            TokenKind::Super => {
                self.advance();
                if self.peek().kind == TokenKind::OpenParen {
                    self.advance();
                    let mut args = Vec::new();
                    while self.peek().kind != TokenKind::CloseParen && !self.is_at_end() {
                        args.push(self.parse_expression());
                        if self.peek().kind == TokenKind::Comma {
                            self.advance();
                        }
                    }
                    let _ = self.consume(TokenKind::CloseParen);
                    Expr::SuperCall(args, s)
                } else {
                    Expr::Super(s)
                }
            }
            TokenKind::New => {
                let ns = self.span();
                self.advance();
                let (name, name_span) = if let crate::compiler::frontend::token::Token {
                    kind: TokenKind::Identifier(name),
                    line,
                    column,
                } = self.peek().clone()
                {
                    self.advance();
                    (name, crate::compiler::ast::Span::new(line, column))
                } else {
                    if !self.panic_mode {
                        let token = self.peek();
                        self.diagnostics.push(Diagnostic::error(
                            "Expected class name after new".to_string(),
                            token.line,
                            token.column,
                        ));
                        self.panic_mode = true;
                    }
                    ("Error".to_string(), crate::compiler::ast::Span::new(0, 0))
                };

                let _ = self.consume(TokenKind::OpenParen);
                let mut args = Vec::new();
                while self.peek().kind != TokenKind::CloseParen && !self.is_at_end() {
                    args.push(self.parse_expression());
                    if self.peek().kind == TokenKind::Comma {
                        self.advance();
                    }
                }
                let _ = self.consume(TokenKind::CloseParen);
                Expr::New(name, name_span, args, ns)
            }
            TokenKind::OpenParen => {
                self.advance();
                let expr = self.parse_expression();
                let _ = self.consume(TokenKind::CloseParen);
                expr
            }
            TokenKind::Await => {
                self.advance();
                let expr = self.parse_unary();
                Expr::Await(Box::new(expr), s)
            }
            TokenKind::OpenBracket => self.parse_array_literal(),
            _ => {
                if !self.panic_mode {
                    let token = self.peek();
                    self.diagnostics.push(Diagnostic::error(
                        format!("Unexpected token {:?}", token.kind),
                        token.line,
                        token.column,
                    ));
                    self.panic_mode = true;
                }
                Expr::Error(s)
            }
        };
        self.parse_postfix(node)
    }

    pub(crate) fn parse_array_literal(&mut self) -> Expr {
        let s = self.span();
        let _ = self.consume(TokenKind::OpenBracket);
        let mut elements = Vec::new();
        while self.peek().kind != TokenKind::CloseBracket && !self.is_at_end() {
            elements.push(self.parse_expression());
            if self.peek().kind == TokenKind::Comma {
                self.advance();
            }
        }
        let _ = self.consume(TokenKind::CloseBracket);
        Expr::ArrayLiteral(elements, s)
    }

    pub(crate) fn parse_postfix(&mut self, mut node: Expr) -> Expr {
        let mut loop_count = 0;
        loop {
            loop_count += 1;
            let token = self.peek();
            if loop_count > 100 {
                panic!(
                    "Infinite loop detected in parse_postfix at line {}, col {}. Near token: {:?}",
                    token.line, token.column, token.kind
                );
            }
            let s = self.span();
            match self.peek().kind {
                TokenKind::Dot => {
                    self.advance();
                    let (member, name_span) = if let crate::compiler::frontend::token::Token {
                        kind: TokenKind::Identifier(m),
                        line,
                        column,
                    } = self.peek().clone()
                    {
                        self.advance();
                        (Some(m), crate::compiler::ast::Span::new(line, column))
                    } else {
                        if !self.panic_mode {
                            let token = self.peek();
                            self.diagnostics.push(Diagnostic::error(
                                "Expected member name after .".to_string(),
                                token.line,
                                token.column,
                            ));
                            self.panic_mode = true;
                        }
                        (None, crate::compiler::ast::Span::new(0, 0))
                    };
                    if let Some(m) = member {
                        node = Expr::MemberAccess(Box::new(node), m, name_span, s);
                    } else {
                        node = Expr::Error(s);
                    }
                }
                TokenKind::OpenParen => {
                    self.advance();
                    let mut args = Vec::new();
                    let mut sub_loop_count = 0;
                    while self.peek().kind != TokenKind::CloseParen && !self.is_at_end() {
                        sub_loop_count += 1;
                        if sub_loop_count > 1000 {
                            panic!("Infinite loop detected in parse_postfix arguments at line {}, col {}", self.peek().line, self.peek().column);
                        }
                        let start_pos = self.pos;
                        args.push(self.parse_expression());
                        if self.peek().kind == TokenKind::Comma {
                            self.advance();
                        }
                        if self.pos == start_pos {
                            self.advance();
                        }
                    }
                    let _ = self.consume(TokenKind::CloseParen);

                    if let Expr::Variable(name, name_span) = node.clone() {
                        node = Expr::Call(name, name_span, args, s);
                    } else if let Expr::MemberAccess(obj, member, name_span, _) = node.clone() {
                        node = Expr::MethodCall(obj, member, name_span, args, s);
                    } else {
                        if !self.panic_mode {
                            let token = self.peek();
                            self.diagnostics.push(Diagnostic::error(
                                "Invalid call target".to_string(),
                                token.line,
                                token.column,
                            ));
                            self.panic_mode = true;
                        }
                        node = Expr::Error(s);
                    }
                }
                TokenKind::OpenBracket => {
                    self.advance();
                    let index = self.parse_expression();
                    let _ = self.consume(TokenKind::CloseBracket);
                    node = Expr::Index(Box::new(node), Box::new(index), s);
                }
                _ => break,
            }
        }
        node
    }
}
