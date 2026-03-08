use crate::compiler::ast::{ClassMethod, Expr, Field, Program, Span, Statement, TypeExpr};
use crate::compiler::frontend::error::{Diagnostic, DiagnosticList};
use crate::compiler::frontend::token::{Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    pub diagnostics: DiagnosticList,
    panic_mode: bool,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            diagnostics: DiagnosticList::new(),
            panic_mode: false,
        }
    }

    fn span(&self) -> Span {
        let token = self.peek();
        Span::new(token.line, token.column)
    }

    pub fn parse_program(&mut self) -> Program {
        let mut statements = Vec::new();
        while !self.is_at_end() {
            match self.parse_statement() {
                Ok(stmt) => {
                    statements.push(stmt);
                    self.panic_mode = false;
                }
                Err(_) => {
                    self.synchronize();
                }
            }
        }
        Program { statements }
    }

    fn parse_statement(&mut self) -> Result<Statement, ()> {
        let s = self.span();
        let doc = self.parse_doc_comments();
        match self.peek().kind {
            TokenKind::Let => self.parse_let_statement(doc),
            TokenKind::Print => self.parse_print_statement(),
            TokenKind::If => self.parse_if_statement(),
            TokenKind::While => self.parse_while_statement(),
            TokenKind::OpenBrace => Ok(self.parse_block()),
            TokenKind::Function => self.parse_function_declaration(doc),
            TokenKind::Return => self.parse_return_statement(),
            TokenKind::Class => self.parse_class_declaration(doc),
            _ => {
                let expr = self.parse_expression();
                self.consume(TokenKind::Semicolon)?;
                Ok(Statement::Expression(expr, s))
            }
        }
    }

    fn parse_doc_comments(&mut self) -> Option<String> {
        let mut docs = Vec::new();
        while let TokenKind::DocComment(content) = self.peek().kind.clone() {
            docs.push(content);
            self.advance();
        }
        if docs.is_empty() {
            None
        } else {
            Some(docs.join("\n"))
        }
    }

    fn parse_block(&mut self) -> Statement {
        let s = self.span();
        let _ = self.consume(TokenKind::OpenBrace);
        let mut statements = Vec::new();
        while self.peek().kind != TokenKind::CloseBrace && !self.is_at_end() {
            match self.parse_statement() {
                Ok(stmt) => {
                    statements.push(stmt);
                    self.panic_mode = false;
                }
                Err(_) => self.synchronize(),
            }
        }
        let _ = self.consume(TokenKind::CloseBrace);
        Statement::Block(statements, s)
    }

    fn parse_return_statement(&mut self) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Return)?;
        let expr = self.parse_expression();
        self.consume(TokenKind::Semicolon)?;
        Ok(Statement::Return(expr, s))
    }

    fn parse_type_expr(&mut self) -> TypeExpr {
        let s = self.span();
        let mut types = Vec::new();
        types.push(self.parse_primary_type());

        while self.peek().kind == TokenKind::Pipe {
            self.advance();
            types.push(self.parse_primary_type());
        }

        if types.len() == 1 {
            types.pop().unwrap()
        } else {
            TypeExpr::Union(types, s)
        }
    }

    fn parse_primary_type(&mut self) -> TypeExpr {
        let s = self.span();
        let kind = self.peek().kind.clone();
        match kind {
            TokenKind::Identifier(name) => {
                self.advance();
                if self.peek().kind == TokenKind::Less {
                    self.advance();
                    let mut args = Vec::new();
                    while self.peek().kind != TokenKind::Greater && !self.is_at_end() {
                        args.push(self.parse_type_expr());
                        if self.peek().kind == TokenKind::Comma {
                            self.advance();
                        }
                    }
                    let _ = self.consume(TokenKind::Greater);
                    TypeExpr::Generic(name, args, s)
                } else {
                    TypeExpr::Name(name, s)
                }
            }
            _ => TypeExpr::Name("unknown".to_string(), s),
        }
    }

    fn parse_function_declaration(&mut self, doc: Option<String>) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Function)?;
        let name = if let TokenKind::Identifier(name) = self.peek().kind.clone() {
            self.advance();
            name
        } else {
            let token = self.peek();
            self.diagnostics.push(Diagnostic::error(
                "Expected function name".to_string(),
                token.line,
                token.column,
            ));
            return Err(());
        };

        self.consume(TokenKind::OpenParen)?;
        let mut params = Vec::new();
        while self.peek().kind != TokenKind::CloseParen && !self.is_at_end() {
            if let TokenKind::Identifier(pname) = self.peek().kind.clone() {
                self.advance();
                self.consume(TokenKind::Colon)?;
                let pty = self.parse_type_expr();
                params.push((pname, pty));
                if self.peek().kind == TokenKind::Comma {
                    self.advance();
                }
            } else {
                break;
            }
        }
        self.consume(TokenKind::CloseParen)?;

        let return_ty = if self.peek().kind == TokenKind::Colon {
            self.advance();
            self.parse_type_expr()
        } else {
            TypeExpr::Name("void".to_string(), s)
        };

        let body = Box::new(self.parse_block());

        Ok(Statement::FunctionDeclaration {
            name,
            params,
            return_ty,
            body,
            span: s,
            doc,
        })
    }

    fn parse_let_statement(&mut self, doc: Option<String>) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Let)?;
        let name = if let TokenKind::Identifier(name) = self.peek().kind.clone() {
            self.advance();
            name
        } else {
            let token = self.peek();
            self.diagnostics.push(Diagnostic::error(
                "Expected variable name after let".to_string(),
                token.line,
                token.column,
            ));
            return Err(());
        };

        let ty = if self.peek().kind == TokenKind::Colon {
            self.advance();
            Some(self.parse_type_expr())
        } else {
            None
        };

        self.consume(TokenKind::Equal)?;
        let value = self.parse_expression();
        self.consume(TokenKind::Semicolon)?;

        Ok(Statement::VarDeclaration {
            name,
            ty,
            value,
            span: s,
            doc,
        })
    }

    fn parse_print_statement(&mut self) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Print)?;
        self.consume(TokenKind::OpenParen)?;
        let expr = self.parse_expression();
        self.consume(TokenKind::CloseParen)?;
        self.consume(TokenKind::Semicolon)?;

        Ok(Statement::Print(expr, s))
    }

    fn parse_if_statement(&mut self) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::If)?;
        let _ = self.consume(TokenKind::OpenParen);
        let condition = self.parse_expression();
        let _ = self.consume(TokenKind::CloseParen);

        let then_branch = Box::new(match self.parse_statement() {
            Ok(stmt) => stmt,
            Err(_) => {
                self.synchronize();
                Statement::Error
            }
        });

        let mut else_branch = None;
        if self.peek().kind == TokenKind::Else {
            self.advance();
            else_branch = Some(Box::new(match self.parse_statement() {
                Ok(stmt) => stmt,
                Err(_) => {
                    self.synchronize();
                    Statement::Error
                }
            }));
        }

        Ok(Statement::If {
            condition,
            then_branch,
            else_branch,
            span: s,
        })
    }

    fn parse_while_statement(&mut self) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::While)?;
        let _ = self.consume(TokenKind::OpenParen);
        let condition = self.parse_expression();
        let _ = self.consume(TokenKind::CloseParen);

        let body = Box::new(match self.parse_statement() {
            Ok(stmt) => stmt,
            Err(_) => {
                self.synchronize();
                Statement::Error
            }
        });

        Ok(Statement::While {
            condition,
            body,
            span: s,
        })
    }

    fn parse_class_declaration(&mut self, doc: Option<String>) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Class)?;
        let name = if let TokenKind::Identifier(name) = self.peek().kind.clone() {
            self.advance();
            name
        } else {
            if !self.panic_mode {
                let token = self.peek();
                self.diagnostics.push(Diagnostic::error(
                    "Expected class name after class keyword".to_string(),
                    token.line,
                    token.column,
                ));
                self.panic_mode = true;
            }
            return Err(());
        };

        self.consume(TokenKind::OpenBrace)?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        let mut constructor = None;

        while self.peek().kind != TokenKind::CloseBrace && !self.is_at_end() {
            let member_doc = self.parse_doc_comments();
            let mut ms = self.span();
            let mut is_static = false;

            if self.peek().kind == TokenKind::Static {
                self.advance();
                is_static = true;
                ms = self.span(); // Update span to include static if needed? or keep original?
            }

            let kind = self.peek().kind.clone();
            match kind {
                TokenKind::Constructor => {
                    self.advance();
                    let _ = self.consume(TokenKind::OpenParen);
                    let mut params = Vec::new();
                    while self.peek().kind != TokenKind::CloseParen && !self.is_at_end() {
                        if let TokenKind::Identifier(pname) = self.peek().kind.clone() {
                            self.advance();
                            let _ = self.consume(TokenKind::Colon);
                            let pty = self.parse_type_expr();
                            params.push((pname, pty));
                            if self.peek().kind == TokenKind::Comma {
                                self.advance();
                            }
                        } else {
                            break;
                        }
                    }
                    let _ = self.consume(TokenKind::CloseParen);
                    let body = Box::new(self.parse_block());
                    constructor = Some(ClassMethod {
                        name: "constructor".to_string(),
                        params,
                        return_ty: TypeExpr::Name(name.clone(), ms),
                        body,
                        is_static: false,
                        span: ms,
                        doc: member_doc,
                    });
                }
                TokenKind::Function => {
                    self.advance();
                    let mname = if let TokenKind::Identifier(mname) = self.peek().kind.clone() {
                        self.advance();
                        Some(mname)
                    } else {
                        if !self.panic_mode {
                            let token = self.peek();
                            self.diagnostics.push(Diagnostic::error(
                                "Expected method name".to_string(),
                                token.line,
                                token.column,
                            ));
                            self.panic_mode = true;
                        }
                        None
                    };

                    if mname.is_none() {
                        self.synchronize();
                        continue;
                    }
                    let mname = mname.unwrap();

                    let _ = self.consume(TokenKind::OpenParen);
                    let mut params = Vec::new();
                    while self.peek().kind != TokenKind::CloseParen && !self.is_at_end() {
                        if let TokenKind::Identifier(pname) = self.peek().kind.clone() {
                            self.advance();
                            let _ = self.consume(TokenKind::Colon);
                            let pty = self.parse_type_expr();
                            params.push((pname, pty));
                            if self.peek().kind == TokenKind::Comma {
                                self.advance();
                            }
                        } else {
                            break;
                        }
                    }
                    let _ = self.consume(TokenKind::CloseParen);

                    let return_ty = if self.peek().kind == TokenKind::Colon {
                        self.advance();
                        self.parse_type_expr()
                    } else {
                        TypeExpr::Name("void".to_string(), ms)
                    };

                    let body = Box::new(self.parse_block());
                    methods.push(ClassMethod {
                        name: mname,
                        params,
                        return_ty,
                        body,
                        is_static,
                        span: ms,
                        doc: member_doc,
                    });
                }
                TokenKind::Identifier(fname) => {
                    let fs = self.span();
                    self.advance();
                    let _ = self.consume(TokenKind::Colon);
                    let fty = self.parse_type_expr();
                    let _ = self.consume(TokenKind::Semicolon);
                    fields.push(Field {
                        name: fname,
                        ty: fty,
                        is_static,
                        span: fs,
                        doc: member_doc,
                    });
                }
                _ => {
                    if !self.panic_mode {
                        let token = self.peek();
                        self.diagnostics.push(Diagnostic::error(
                            format!("Unexpected token in class body: {:?}", token.kind),
                            token.line,
                            token.column,
                        ));
                        self.panic_mode = true;
                    }
                    self.advance();
                    self.synchronize();
                }
            }
        }
        self.consume(TokenKind::CloseBrace)?;

        Ok(Statement::ClassDeclaration {
            name,
            fields,
            methods,
            constructor,
            span: s,
            doc,
        })
    }

    fn parse_expression(&mut self) -> Expr {
        let s = self.span();
        let node = self.parse_comparison();

        if self.peek().kind == TokenKind::Equal {
            self.advance();
            let value = self.parse_expression();
            if let Expr::Variable(name, vs) = node {
                return Expr::Assign(name, Box::new(value), vs);
            } else if let Expr::MemberAccess(obj, member, ms) = node {
                return Expr::MemberAssign(obj, member, Box::new(value), ms);
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

    fn parse_comparison(&mut self) -> Expr {
        let s = self.span();
        let mut node = self.parse_type_test();

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
            let right = self.parse_arithmetic();
            node = Expr::BinaryOp(Box::new(node), op, Box::new(right), s);
        }

        node
    }

    fn parse_type_test(&mut self) -> Expr {
        let s = self.span();
        let mut node = self.parse_arithmetic();
        while self.peek().kind == TokenKind::Is {
            self.advance();
            let ty = self.parse_type_expr();
            node = Expr::TypeTest(Box::new(node), ty, s);
        }
        node
    }

    fn parse_arithmetic(&mut self) -> Expr {
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

    fn parse_multiplicative(&mut self) -> Expr {
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
            let right = self.parse_primary();
            node = Expr::BinaryOp(Box::new(node), op, Box::new(right), s);
        }

        node
    }

    fn parse_unary(&mut self) -> Expr {
        let s = self.span();
        if self.peek().kind == TokenKind::Minus {
            self.advance();
            let expr = self.parse_unary();
            return Expr::UnaryOp("-".to_string(), Box::new(expr), s);
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Expr {
        let s = self.span();
        let node = match self.peek().kind.clone() {
            TokenKind::Number(val) => {
                self.advance();
                Expr::Number(val, s)
            }
            TokenKind::StringLiteral(ls) => {
                self.advance();
                Expr::StringLiteral(ls, s)
            }
            TokenKind::Identifier(name) => {
                self.advance();
                Expr::Variable(name, s)
            }
            TokenKind::This => {
                self.advance();
                Expr::This(s)
            }
            TokenKind::New => {
                self.advance();
                let name = if let TokenKind::Identifier(name) = self.peek().kind.clone() {
                    self.advance();
                    name
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
                    "Error".to_string()
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
                Expr::New(name, args, s)
            }
            TokenKind::OpenParen => {
                self.advance();
                let expr = self.parse_expression();
                let _ = self.consume(TokenKind::CloseParen);
                expr
            }
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

    fn parse_postfix(&mut self, mut node: Expr) -> Expr {
        loop {
            let s = self.span();
            match self.peek().kind {
                TokenKind::Dot => {
                    self.advance();
                    let member = if let TokenKind::Identifier(m) = self.peek().kind.clone() {
                        self.advance();
                        Some(m)
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
                        None
                    };
                    if let Some(m) = member {
                        node = Expr::MemberAccess(Box::new(node), m, s);
                    } else {
                        node = Expr::Error(s);
                    }
                }
                TokenKind::OpenParen => {
                    self.advance();
                    let mut args = Vec::new();
                    while self.peek().kind != TokenKind::CloseParen && !self.is_at_end() {
                        args.push(self.parse_expression());
                        if self.peek().kind == TokenKind::Comma {
                            self.advance();
                        }
                    }
                    let _ = self.consume(TokenKind::CloseParen);

                    if let Expr::Variable(name, _) = node.clone() {
                        node = Expr::Call(name, args, s);
                    } else if let Expr::MemberAccess(obj, member, _) = node.clone() {
                        node = Expr::MethodCall(obj, member, args, s);
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
                _ => break,
            }
        }
        node
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.pos += 1;
        }
        &self.tokens[self.pos - 1]
    }

    fn consume(&mut self, kind: TokenKind) -> Result<(), ()> {
        if self.peek().kind == kind {
            self.advance();
            Ok(())
        } else {
            if !self.panic_mode {
                let token = self.peek();
                self.diagnostics.push(Diagnostic::error(
                    format!("Expected {:?}, found {:?}", kind, token.kind),
                    token.line,
                    token.column,
                ));
                self.panic_mode = true;
            }
            Err(())
        }
    }

    fn synchronize(&mut self) {
        self.panic_mode = false;
        self.advance();

        while !self.is_at_end() {
            if self.tokens[self.pos - 1].kind == TokenKind::Semicolon {
                return;
            }

            match self.peek().kind {
                TokenKind::Class
                | TokenKind::Function
                | TokenKind::Let
                | TokenKind::If
                | TokenKind::While
                | TokenKind::Print
                | TokenKind::Return
                | TokenKind::CloseBrace => return,
                _ => {}
            }

            self.advance();
        }
    }

    fn is_at_end(&self) -> bool {
        self.peek().kind == TokenKind::EOF
    }
}
