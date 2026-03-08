use crate::compiler::ast::{ClassMethod, Expr, Field, Program, Statement, TypeExpr};
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
        match self.peek().kind {
            TokenKind::Let => self.parse_let_statement(),
            TokenKind::Print => self.parse_print_statement(),
            TokenKind::If => self.parse_if_statement(),
            TokenKind::While => self.parse_while_statement(),
            TokenKind::OpenBrace => Ok(self.parse_block()),
            TokenKind::Function => self.parse_function_declaration(),
            TokenKind::Return => self.parse_return_statement(),
            TokenKind::Class => self.parse_class_declaration(),
            _ => {
                let expr = self.parse_expression();
                self.consume(TokenKind::Semicolon)?;
                Ok(Statement::Expression(expr))
            }
        }
    }

    fn parse_block(&mut self) -> Statement {
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
        Statement::Block(statements)
    }

    fn parse_return_statement(&mut self) -> Result<Statement, ()> {
        self.consume(TokenKind::Return)?;
        let expr = self.parse_expression();
        self.consume(TokenKind::Semicolon)?;
        Ok(Statement::Return(expr))
    }

    fn parse_type_expr(&mut self) -> TypeExpr {
        let mut types = Vec::new();
        types.push(self.parse_primary_type());

        while self.peek().kind == TokenKind::Pipe {
            self.advance();
            types.push(self.parse_primary_type());
        }

        if types.len() == 1 {
            types.pop().unwrap()
        } else {
            TypeExpr::Union(types)
        }
    }

    fn parse_primary_type(&mut self) -> TypeExpr {
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
                    TypeExpr::Generic(name, args)
                } else {
                    TypeExpr::Name(name)
                }
            }
            _ => TypeExpr::Name("unknown".to_string()),
        }
    }

    fn parse_function_declaration(&mut self) -> Result<Statement, ()> {
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
            TypeExpr::Name("void".to_string())
        };

        let body = Box::new(self.parse_block());

        Ok(Statement::FunctionDeclaration {
            name,
            params,
            return_ty,
            body,
        })
    }

    fn parse_let_statement(&mut self) -> Result<Statement, ()> {
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

        Ok(Statement::VarDeclaration { name, ty, value })
    }

    fn parse_print_statement(&mut self) -> Result<Statement, ()> {
        self.consume(TokenKind::Print)?;
        self.consume(TokenKind::OpenParen)?;
        let expr = self.parse_expression();
        self.consume(TokenKind::CloseParen)?;
        self.consume(TokenKind::Semicolon)?;

        Ok(Statement::Print(expr))
    }

    fn parse_if_statement(&mut self) -> Result<Statement, ()> {
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
        })
    }

    fn parse_while_statement(&mut self) -> Result<Statement, ()> {
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

        Ok(Statement::While { condition, body })
    }

    fn parse_class_declaration(&mut self) -> Result<Statement, ()> {
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
                        return_ty: TypeExpr::Name(name.clone()),
                        body,
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
                        TypeExpr::Name("void".to_string())
                    };

                    let body = Box::new(self.parse_block());
                    methods.push(ClassMethod {
                        name: mname,
                        params,
                        return_ty,
                        body,
                    });
                }
                TokenKind::Identifier(fname) => {
                    self.advance();
                    let _ = self.consume(TokenKind::Colon);
                    let fty = self.parse_type_expr();
                    let _ = self.consume(TokenKind::Semicolon);
                    fields.push(Field {
                        name: fname,
                        ty: fty,
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
        })
    }

    fn parse_expression(&mut self) -> Expr {
        let node = self.parse_comparison();

        if self.peek().kind == TokenKind::Equal {
            self.advance();
            let value = self.parse_expression();
            if let Expr::Variable(name) = node {
                return Expr::Assign(name, Box::new(value));
            } else if let Expr::MemberAccess(obj, member) = node {
                return Expr::MemberAssign(obj, member, Box::new(value));
            } else {
                let token = self.peek();
                self.diagnostics.push(Diagnostic::error(
                    "Invalid assignment target".to_string(),
                    token.line,
                    token.column,
                ));
                return Expr::Error;
            }
        }

        node
    }

    fn parse_comparison(&mut self) -> Expr {
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
            node = Expr::BinaryOp(Box::new(node), op, Box::new(right));
        }

        node
    }

    fn parse_type_test(&mut self) -> Expr {
        let mut node = self.parse_arithmetic();
        while self.peek().kind == TokenKind::Is {
            self.advance();
            let ty = self.parse_type_expr();
            node = Expr::TypeTest(Box::new(node), ty);
        }
        node
    }

    fn parse_arithmetic(&mut self) -> Expr {
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
            node = Expr::BinaryOp(Box::new(node), op, Box::new(right));
        }

        node
    }

    fn parse_multiplicative(&mut self) -> Expr {
        let mut node = self.parse_primary();

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
            node = Expr::BinaryOp(Box::new(node), op, Box::new(right));
        }

        node
    }

    fn parse_primary(&mut self) -> Expr {
        let node = match self.peek().kind.clone() {
            TokenKind::Number(val) => {
                self.advance();
                Expr::Number(val)
            }
            TokenKind::StringLiteral(s) => {
                self.advance();
                Expr::StringLiteral(s)
            }
            TokenKind::Identifier(name) => {
                self.advance();
                Expr::Variable(name)
            }
            TokenKind::This => {
                self.advance();
                Expr::This
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
                Expr::New(name, args)
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
                Expr::Error
            }
        };
        self.parse_postfix(node)
    }

    fn parse_postfix(&mut self, mut node: Expr) -> Expr {
        loop {
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
                        node = Expr::MemberAccess(Box::new(node), m);
                    } else {
                        node = Expr::Error;
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

                    if let Expr::Variable(name) = node {
                        node = Expr::Call(name, args);
                    } else if let Expr::MemberAccess(obj, member) = node {
                        node = Expr::MethodCall(obj, member, args);
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
                        node = Expr::Error;
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
