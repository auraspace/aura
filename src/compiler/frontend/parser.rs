use crate::compiler::ast::{
    ClassMethod, Expr, Field, ImportItem, Program, Span, Statement, TemplatePart, TplPart, TypeExpr,
};
use crate::compiler::frontend::error::{Diagnostic, DiagnosticList};
use crate::compiler::frontend::token::{Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    pub diagnostics: DiagnosticList,
    panic_mode: bool,
    file_path: String,
}

impl Parser {
    pub fn new(tokens: Vec<Token>, file_path: String) -> Self {
        Self {
            tokens,
            pos: 0,
            diagnostics: DiagnosticList::new(),
            panic_mode: false,
            file_path,
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
        Program {
            statements,
            file_path: self.file_path.clone(),
        }
    }

    fn parse_statement(&mut self) -> Result<Statement, ()> {
        let s = self.span();
        let doc = self.parse_doc_comments();
        let mut is_async = false;
        if self.peek().kind == TokenKind::Async {
            self.advance();
            is_async = true;
        }

        match self.peek().kind {
            TokenKind::Let => {
                if is_async {
                    self.diagnostics.push(Diagnostic::error(
                        "Async is not allowed on variable declarations".to_string(),
                        s.line,
                        s.column,
                    ));
                }
                self.parse_let_statement(doc)
            }
            TokenKind::Print => {
                if is_async {
                    self.diagnostics.push(Diagnostic::error(
                        "Async is not allowed on print statements".to_string(),
                        s.line,
                        s.column,
                    ));
                }
                self.parse_print_statement()
            }
            TokenKind::If => self.parse_if_statement(),
            TokenKind::While => self.parse_while_statement(),
            TokenKind::OpenBrace => Ok(self.parse_block()),
            TokenKind::Function => self.parse_function_declaration(doc, is_async),
            TokenKind::Return => self.parse_return_statement(),
            TokenKind::Class => self.parse_class_declaration(doc),
            TokenKind::Import => self.parse_import_statement(),
            TokenKind::Export => self.parse_export_statement(doc),
            TokenKind::Try => self.parse_try_statement(),
            TokenKind::Throw => self.parse_throw_statement(),
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
        let expr = if self.peek().kind == TokenKind::Semicolon {
            Expr::Number(0, s) // Default to 0/void-ish for now if empty?
                               // Actually, better to have a Expr::Void or handle it in AST
                               // Let's check Expr variants.
        } else {
            self.parse_expression()
        };

        // Wait, if I use Expr::Number(0, s), it might not be what's wanted for void.
        // Let's check Expr in ast.rs.
        self.consume(TokenKind::Semicolon)?;
        Ok(Statement::Return(expr, s))
    }

    fn parse_import_statement(&mut self) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Import)?;

        let item = if self.peek().kind == TokenKind::Star {
            self.advance();
            self.consume(TokenKind::As)?;
            let ns = if let TokenKind::Identifier(name) = self.peek().kind.clone() {
                self.advance();
                name
            } else {
                return Err(());
            };
            ImportItem::Namespace(ns)
        } else if self.peek().kind == TokenKind::OpenBrace {
            self.advance();
            let mut names = Vec::new();
            while self.peek().kind != TokenKind::CloseBrace && !self.is_at_end() {
                if let TokenKind::Identifier(name) = self.peek().kind.clone() {
                    self.advance();
                    names.push(name);
                    if self.peek().kind == TokenKind::Comma {
                        self.advance();
                    }
                } else {
                    break;
                }
            }
            self.consume(TokenKind::CloseBrace)?;
            ImportItem::Named(names)
        } else {
            return Err(());
        };

        self.consume(TokenKind::From)?;

        let path = if let TokenKind::StringLiteral(p) = self.peek().kind.clone() {
            self.advance();
            p
        } else {
            return Err(());
        };

        self.consume(TokenKind::Semicolon)?;

        Ok(Statement::Import {
            item,
            path,
            span: s,
        })
    }

    fn parse_export_statement(&mut self, doc: Option<String>) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Export)?;

        let decl = match self.peek().kind {
            TokenKind::Let => self.parse_let_statement(doc)?,
            TokenKind::Function => {
                let mut is_async = false;
                if self.peek().kind == TokenKind::Async {
                    self.advance();
                    is_async = true;
                }
                self.parse_function_declaration(doc, is_async)?
            }
            TokenKind::Class => self.parse_class_declaration(doc)?,
            _ => {
                if !self.panic_mode {
                    let token = self.peek();
                    self.diagnostics.push(Diagnostic::error(
                        "Export is only allowed on declarations".to_string(),
                        token.line,
                        token.column,
                    ));
                    self.panic_mode = true;
                }
                return Err(());
            }
        };

        Ok(Statement::Export {
            decl: Box::new(decl),
            span: s,
        })
    }

    fn parse_type_expr(&mut self) -> TypeExpr {
        let s = self.span();
        let mut types = Vec::new();
        types.push(self.parse_primary_type());

        while self.peek().kind == TokenKind::Pipe {
            self.advance();
            types.push(self.parse_primary_type());
        }

        let mut ty = if types.len() == 1 {
            types.pop().unwrap()
        } else {
            TypeExpr::Union(types, s)
        };

        // Support array types: string[], i32[][], etc.
        while self.peek().kind == TokenKind::OpenBracket {
            self.advance();
            let _ = self.consume(TokenKind::CloseBracket);
            ty = TypeExpr::Array(Box::new(ty), s);
        }

        ty
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
            TokenKind::Function => {
                self.advance();
                let params = if self.peek().kind == TokenKind::OpenParen {
                    self.advance();
                    let mut p = Vec::new();
                    while self.peek().kind != TokenKind::CloseParen && !self.is_at_end() {
                        p.push(self.parse_type_expr());
                        if self.peek().kind == TokenKind::Comma {
                            self.advance();
                        }
                    }
                    let _ = self.consume(TokenKind::CloseParen);
                    p
                } else {
                    Vec::new()
                };
                let ret = if self.peek().kind == TokenKind::Colon {
                    self.advance();
                    Box::new(self.parse_type_expr())
                } else {
                    Box::new(TypeExpr::Name("void".to_string(), s))
                };
                TypeExpr::Function(params, ret, s)
            }
            _ => TypeExpr::Name("unknown".to_string(), s),
        }
    }

    fn parse_function_declaration(
        &mut self,
        doc: Option<String>,
        is_async: bool,
    ) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Function)?;
        let (name, name_span) = if let Token { kind: TokenKind::Identifier(name), line, column } = self.peek().clone() {
            self.advance();
            (name, Span::new(line, column))
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
            name_span,
            params,
            return_ty,
            body,
            is_async,
            span: s,
            doc,
        })
    }

    fn parse_let_statement(&mut self, doc: Option<String>) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Let)?;
        let (name, name_span) = if let Token { kind: TokenKind::Identifier(name), line, column } = self.peek().clone() {
            self.advance();
            (name, Span::new(line, column))
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
            name_span,
            ty,
            value,
            span: s,
            doc,
        })
    }

    fn parse_print_statement(&mut self) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Print)?;

        let has_paren = if self.peek().kind == TokenKind::OpenParen {
            self.advance();
            true
        } else {
            false
        };

        let expr = self.parse_expression();

        if has_paren {
            self.consume(TokenKind::CloseParen)?;
        }
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
        let (name, name_span) = if let Token { kind: TokenKind::Identifier(name), line, column } = self.peek().clone() {
            self.advance();
            (name, Span::new(line, column))
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

            let mut is_async = false;
            if self.peek().kind == TokenKind::Async {
                self.advance();
                is_async = true;
                ms = self.span();
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
                        name_span: ms,
                        params,
                        return_ty: TypeExpr::Name(name.clone(), ms),
                        body,
                        is_static: false,
                        is_async: false,
                        span: ms,
                        doc: member_doc,
                    });
                }
                TokenKind::Function => {
                    self.advance();
                    let (mname, mname_span) = if let Token { kind: TokenKind::Identifier(mname), line, column } = self.peek().clone() {
                        self.advance();
                        (mname, Span::new(line, column))
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
                        (String::new(), Span::new(0, 0))
                    };

                    if mname.is_empty() {
                        self.synchronize();
                        continue;
                    }

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
                        name_span: mname_span,
                        params,
                        return_ty,
                        body,
                        is_static,
                        is_async,
                        span: ms,
                        doc: member_doc,
                    });
                }
                TokenKind::Identifier(fname) => {
                    let fs = self.span();
                    self.advance();
                    if self.peek().kind == TokenKind::OpenParen {
                        // Method without 'function' keyword
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
                            name: fname,
                            name_span: fs,
                            params,
                            return_ty,
                            body,
                            is_static,
                            is_async,
                            span: ms,
                            doc: member_doc,
                        });
                    } else {
                        // It's a field
                        let _ = self.consume(TokenKind::Colon);
                        let fty = self.parse_type_expr();
                        let value = if self.peek().kind == TokenKind::Equal {
                            self.advance();
                            Some(self.parse_expression())
                        } else {
                            None
                        };
                        let _ = self.consume(TokenKind::Semicolon);
                        fields.push(Field {
                            name: fname,
                            name_span: fs,
                            ty: fty,
                            value,
                            is_static,
                            span: fs,
                            doc: member_doc,
                        });
                    }
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
            name_span,
            fields,
            methods,
            constructor,
            span: s,
            doc,
        })
    }

    fn parse_expression(&mut self) -> Expr {
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

    fn parse_logical_or(&mut self) -> Expr {
        let s = self.span();
        let mut node = self.parse_logical_and();
        while self.peek().kind == TokenKind::Or {
            self.advance();
            let right = self.parse_logical_and();
            node = Expr::BinaryOp(Box::new(node), "||".to_string(), Box::new(right), s);
        }
        node
    }

    fn parse_logical_and(&mut self) -> Expr {
        let s = self.span();
        let mut node = self.parse_comparison();
        while self.peek().kind == TokenKind::And {
            self.advance();
            let right = self.parse_comparison();
            node = Expr::BinaryOp(Box::new(node), "&&".to_string(), Box::new(right), s);
        }
        node
    }

    fn parse_comparison(&mut self) -> Expr {
        let s = self.span();
        let mut node = self.parse_bitwise_or();

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
            let right = self.parse_bitwise_or(); // Use bitwise_or for right side too
            node = Expr::BinaryOp(Box::new(node), op, Box::new(right), s);
        }

        node
    }

    fn parse_bitwise_or(&mut self) -> Expr {
        let s = self.span();
        let mut node = self.parse_type_test();
        while self.peek().kind == TokenKind::Pipe {
            self.advance();
            let right = self.parse_type_test();
            node = Expr::BinaryOp(Box::new(node), "|".to_string(), Box::new(right), s);
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
            TokenKind::New => {
                let ns = self.span();
                self.advance();
                let (name, name_span) = if let Token { kind: TokenKind::Identifier(name), line, column } = self.peek().clone() {
                    self.advance();
                    (name, Span::new(line, column))
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
                    ("Error".to_string(), Span::new(0, 0))
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

    fn parse_array_literal(&mut self) -> Expr {
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

    fn parse_postfix(&mut self, mut node: Expr) -> Expr {
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
                    let (member, name_span) = if let Token {
                        kind: TokenKind::Identifier(m),
                        line,
                        column,
                    } = self.peek().clone()
                    {
                        self.advance();
                        (Some(m), Span::new(line, column))
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
                        (None, Span::new(0, 0))
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
                            // Failsafe: if we didn't advance, skip one token
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
                | TokenKind::Import
                | TokenKind::Export
                | TokenKind::Try
                | TokenKind::Throw
                | TokenKind::CloseBrace => return,
                _ => {}
            }

            self.advance();
        }
    }

    fn parse_try_statement(&mut self) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Try)?;
        let try_block = Box::new(self.parse_block());

        let mut catch_param = None;
        let mut catch_block = None;
        if self.peek().kind == TokenKind::Catch {
            self.advance();
            if self.peek().kind == TokenKind::OpenParen {
                self.advance();
                let name = if let TokenKind::Identifier(n) = self.peek().kind.clone() {
                    self.advance();
                    n
                } else {
                    return Err(());
                };

                let ty = if self.peek().kind == TokenKind::Colon {
                    self.advance();
                    self.parse_type_expr()
                } else {
                    TypeExpr::Name("Error".to_string(), s)
                };
                self.consume(TokenKind::CloseParen)?;
                catch_param = Some((name, ty));
            }
            catch_block = Some(Box::new(self.parse_block()));
        }

        let mut finally_block = None;
        if self.peek().kind == TokenKind::Finally {
            self.advance();
            finally_block = Some(Box::new(self.parse_block()));
        }

        Ok(Statement::TryCatch {
            try_block,
            catch_param,
            catch_block,
            finally_block,
            span: s,
        })
    }

    fn parse_throw_statement(&mut self) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Throw)?;
        let expr = self.parse_expression();
        self.consume(TokenKind::Semicolon)?;
        Ok(Statement::Expression(Expr::Throw(Box::new(expr), s), s))
    }

    fn is_at_end(&self) -> bool {
        self.peek().kind == TokenKind::EOF
    }
}
