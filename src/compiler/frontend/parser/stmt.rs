use crate::compiler::ast::*;
use crate::compiler::frontend::error::Diagnostic;
use crate::compiler::frontend::parser::Parser;
use crate::compiler::frontend::token::{Token, TokenKind};

impl Parser {
    pub(crate) fn parse_statement(&mut self) -> Result<Statement, ()> {
        let s = self.span();
        let doc = self.parse_doc_comments();
        let mut is_async = false;
        if self.peek().kind == TokenKind::Async {
            self.advance();
            is_async = true;
        }

        let mut is_abstract = false;
        if self.peek().kind == TokenKind::Abstract {
            self.advance();
            is_abstract = true;
        }

        match &self.peek().kind {
            TokenKind::Let | TokenKind::Const => {
                if is_async || is_abstract {
                    self.diagnostics.push(Diagnostic::error(
                        "Async or Abstract is not allowed on variable declarations".to_string(),
                        s.line,
                        s.column,
                    ));
                }
                self.parse_var_declaration(doc)
            }
            TokenKind::Print => {
                if is_async || is_abstract {
                    self.diagnostics.push(Diagnostic::error(
                        "Async or Abstract is not allowed on print statements".to_string(),
                        s.line,
                        s.column,
                    ));
                }
                self.parse_print_statement()
            }
            TokenKind::If => self.parse_if_statement(),
            TokenKind::While => self.parse_while_statement(),
            TokenKind::For => self.parse_for_statement(),
            TokenKind::OpenBrace => Ok(self.parse_block()),
            TokenKind::Function => self.parse_function_declaration(doc, is_async),
            TokenKind::Return => self.parse_return_statement(),
            TokenKind::Class => self.parse_class_declaration(doc, is_abstract),
            TokenKind::Enum => self.parse_enum_declaration(doc),
            TokenKind::Type => self.parse_type_alias_declaration(doc),
            TokenKind::Interface => self.parse_interface_declaration(doc),
            TokenKind::Import => self.parse_import_statement(),
            TokenKind::Export => self.parse_export_statement(doc),
            TokenKind::Try => self.parse_try_statement(),
            TokenKind::Throw => self.parse_throw_statement(),
            TokenKind::Comment(content) => {
                let content = content.clone();
                self.advance();
                Ok(Statement::Comment(content, s))
            }
            TokenKind::RegularBlockComment(content) => {
                let content = content.clone();
                self.advance();
                Ok(Statement::RegularBlockComment(content, s))
            }
            TokenKind::Semicolon => {
                self.advance();
                Ok(Statement::Empty(s))
            }
            _ => {
                let expr = self.parse_expression();
                self.consume(TokenKind::Semicolon)?;
                Ok(Statement::Expression(expr, s))
            }
        }
    }

    pub(crate) fn parse_doc_comments(&mut self) -> Option<DocComment> {
        let mut last_doc = None;
        loop {
            match self.peek().kind.clone() {
                TokenKind::LineDoc(content) => {
                    let mut docs = vec![content];
                    self.advance();
                    while let TokenKind::LineDoc(content) = self.peek().kind.clone() {
                        docs.push(content);
                        self.advance();
                    }
                    last_doc = Some(DocComment::Line(docs.join("\n")));
                }
                TokenKind::BlockDoc(content) => {
                    self.advance();
                    last_doc = Some(DocComment::Block(content));
                }
                _ => break,
            }
        }
        last_doc
    }

    pub(crate) fn parse_block(&mut self) -> Statement {
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

    pub(crate) fn parse_return_statement(&mut self) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Return)?;
        let expr = if self.peek().kind == TokenKind::Semicolon {
            Expr::Number(0, s)
        } else {
            self.parse_expression()
        };

        self.consume(TokenKind::Semicolon)?;
        Ok(Statement::Return(expr, s))
    }

    pub(crate) fn parse_import_statement(&mut self) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Import)?;

        let item = if self.peek().kind == TokenKind::Star {
            self.advance();
            self.consume(TokenKind::As)?;
            let ns_span = self.span();
            let ns = if let TokenKind::Identifier(name) = self.peek().kind.clone() {
                self.advance();
                name
            } else {
                return Err(());
            };
            ImportItem::Namespace((ns, ns_span))
        } else if self.peek().kind == TokenKind::OpenBrace {
            self.advance();
            let mut names = Vec::new();
            while self.peek().kind != TokenKind::CloseBrace && !self.is_at_end() {
                let name_span = self.span();
                if let TokenKind::Identifier(name) = self.peek().kind.clone() {
                    self.advance();
                    names.push((name, name_span));
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

        let path_span = self.span();
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
            path_span,
            span: s,
        })
    }

    pub(crate) fn parse_export_statement(
        &mut self,
        doc: Option<DocComment>,
    ) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Export)?;

        let decl = match self.peek().kind {
            TokenKind::Let | TokenKind::Const => self.parse_var_declaration(doc)?,
            TokenKind::Function => {
                let mut is_async = false;
                if self.peek().kind == TokenKind::Async {
                    self.advance();
                    is_async = true;
                }
                self.parse_function_declaration(doc, is_async)?
            }
            TokenKind::Type => self.parse_type_alias_declaration(doc)?,
            TokenKind::Class => {
                let mut is_abstract = false;
                if self.peek().kind == TokenKind::Abstract {
                    self.advance();
                    is_abstract = true;
                    // doc might have been passed in, but parse_export_statement is called with it.
                }
                self.parse_class_declaration(doc, is_abstract)?
            }
            TokenKind::Enum => self.parse_enum_declaration(doc)?,
            TokenKind::Interface => self.parse_interface_declaration(doc)?,
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

    pub(crate) fn parse_type_expr(&mut self) -> TypeExpr {
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

        while self.peek().kind == TokenKind::OpenBracket {
            self.advance();
            let _ = self.consume(TokenKind::CloseBracket);
            ty = TypeExpr::Array(Box::new(ty), s);
        }

        ty
    }

    pub(crate) fn parse_primary_type(&mut self) -> TypeExpr {
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
                let type_params = self.parse_type_params();
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
                TypeExpr::Function(type_params, params, ret, s)
            }
            _ => TypeExpr::Name("unknown".to_string(), s),
        }
    }

    pub(crate) fn parse_function_declaration(
        &mut self,
        doc: Option<DocComment>,
        is_async: bool,
    ) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Function)?;
        let (name, name_span) = if let Token {
            kind: TokenKind::Identifier(name),
            line,
            column,
        } = self.peek().clone()
        {
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

        let type_params = self.parse_type_params();

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
            type_params,
            params,
            return_ty,
            body,
            is_async,
            span: s,
            doc,
        })
    }

    pub(crate) fn parse_var_declaration(
        &mut self,
        doc: Option<DocComment>,
    ) -> Result<Statement, ()> {
        let s = self.span();
        let is_const = if self.peek().kind == TokenKind::Const {
            self.advance();
            true
        } else {
            self.consume(TokenKind::Let)?;
            false
        };

        let (name, name_span) = if let Token {
            kind: TokenKind::Identifier(name),
            line,
            column,
        } = self.peek().clone()
        {
            self.advance();
            (name, Span::new(line, column))
        } else {
            let token = self.peek();
            let msg = if is_const {
                "Expected variable name after const"
            } else {
                "Expected variable name after let"
            };
            self.diagnostics
                .push(Diagnostic::error(msg.to_string(), token.line, token.column));
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
            is_const,
            span: s,
            doc,
        })
    }

    pub(crate) fn parse_print_statement(&mut self) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Print)?;

        let expr = self.parse_expression();
        self.consume(TokenKind::Semicolon)?;

        Ok(Statement::Print(expr, s))
    }

    pub(crate) fn parse_if_statement(&mut self) -> Result<Statement, ()> {
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

    pub(crate) fn parse_while_statement(&mut self) -> Result<Statement, ()> {
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

    pub(crate) fn parse_for_statement(&mut self) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::For)?;
        self.consume(TokenKind::OpenParen)?;

        // Check if it's a for...of loop
        // for ( [let|const] id of expr )
        let mut is_for_of = false;

        if (self.peek().kind == TokenKind::Let || self.peek().kind == TokenKind::Const)
            && matches!(self.peek_n(1).kind, TokenKind::Identifier(_))
            && self.peek_n(2).kind == TokenKind::Of
        {
            is_for_of = true;
        }

        if is_for_of {
            let is_const = if self.peek().kind == TokenKind::Const {
                self.advance();
                true
            } else {
                self.consume(TokenKind::Let)?;
                false
            };

            let (variable, variable_span) =
                if let TokenKind::Identifier(name) = self.peek().kind.clone() {
                    let token = self.advance();
                    (name, Span::new(token.line, token.column))
                } else {
                    return Err(());
                };

            self.consume(TokenKind::Of)?;
            let iterable = self.parse_expression();
            self.consume(TokenKind::CloseParen)?;
            let body = Box::new(match self.parse_statement() {
                Ok(stmt) => stmt,
                Err(_) => {
                    self.synchronize();
                    Statement::Error
                }
            });

            return Ok(Statement::ForOf {
                variable,
                variable_span,
                is_const,
                iterable,
                body,
                span: s,
            });
        }

        // C-style for loop: for (initializer; condition; increment)
        let initializer = if self.peek().kind == TokenKind::Semicolon {
            self.advance();
            None
        } else if self.peek().kind == TokenKind::Let || self.peek().kind == TokenKind::Const {
            Some(Box::new(self.parse_var_declaration(None)?))
        } else {
            let expr = self.parse_expression();
            self.consume(TokenKind::Semicolon)?;
            Some(Box::new(Statement::Expression(expr, s)))
        };

        let condition = if self.peek().kind == TokenKind::Semicolon {
            None
        } else {
            Some(self.parse_expression())
        };
        self.consume(TokenKind::Semicolon)?;

        let increment = if self.peek().kind == TokenKind::CloseParen {
            None
        } else {
            Some(self.parse_expression())
        };
        self.consume(TokenKind::CloseParen)?;

        let body = Box::new(match self.parse_statement() {
            Ok(stmt) => stmt,
            Err(_) => {
                self.synchronize();
                Statement::Error
            }
        });

        Ok(Statement::For {
            initializer,
            condition,
            increment,
            body,
            span: s,
        })
    }

    pub(crate) fn parse_type_params(&mut self) -> Vec<TypeParam> {
        let mut params = Vec::new();
        if self.peek().kind == TokenKind::Less {
            self.advance();
            while self.peek().kind != TokenKind::Greater && !self.is_at_end() {
                let s = self.span();
                if let TokenKind::Identifier(name) = self.peek().kind.clone() {
                    self.advance();
                    let mut constraint = None;
                    if self.peek().kind == TokenKind::Extends {
                        self.advance();
                        constraint = Some(self.parse_type_expr());
                    }
                    params.push(TypeParam {
                        name,
                        constraint,
                        span: s,
                    });
                    if self.peek().kind == TokenKind::Comma {
                        self.advance();
                    }
                } else {
                    break;
                }
            }
            let _ = self.consume(TokenKind::Greater);
        }
        params
    }

    pub(crate) fn parse_generic_args(&mut self) -> Vec<TypeExpr> {
        let mut args = Vec::new();
        if self.peek().kind == TokenKind::Less {
            self.advance();
            while self.peek().kind != TokenKind::Greater && !self.is_at_end() {
                args.push(self.parse_type_expr());
                if self.peek().kind == TokenKind::Comma {
                    self.advance();
                }
            }
            let _ = self.consume(TokenKind::Greater);
        }
        args
    }

    pub(crate) fn parse_class_declaration(
        &mut self,
        doc: Option<DocComment>,
        is_abstract: bool,
    ) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Class)?;
        let (name, name_span) = if let Token {
            kind: TokenKind::Identifier(name),
            line,
            column,
        } = self.peek().clone()
        {
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

        let type_params = self.parse_type_params();

        let mut extends = None;
        if self.peek().kind == TokenKind::Extends {
            self.advance();
            extends = Some(self.parse_type_expr());
        }

        let mut implements = Vec::new();
        if self.peek().kind == TokenKind::Implements {
            self.advance();
            loop {
                implements.push(self.parse_type_expr());
                if self.peek().kind == TokenKind::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        self.consume(TokenKind::OpenBrace)?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        let mut constructor = None;

        while self.peek().kind != TokenKind::CloseBrace && !self.is_at_end() {
            if matches!(
                self.peek().kind,
                TokenKind::Comment(_) | TokenKind::RegularBlockComment(_) | TokenKind::Semicolon
            ) {
                self.advance();
                continue;
            }
            let member_doc = self.parse_doc_comments();
            let mut ms = self.span();

            let mut access = AccessModifier::Public;
            match self.peek().kind {
                TokenKind::Public => {
                    self.advance();
                    access = AccessModifier::Public;
                    ms = self.span();
                }
                TokenKind::Private => {
                    self.advance();
                    access = AccessModifier::Private;
                    ms = self.span();
                }
                TokenKind::Protected => {
                    self.advance();
                    access = AccessModifier::Protected;
                    ms = self.span();
                }
                _ => {}
            }

            let mut is_static = false;
            if self.peek().kind == TokenKind::Static {
                self.advance();
                is_static = true;
                ms = self.span();
            }

            let mut is_async = false;
            if self.peek().kind == TokenKind::Async {
                self.advance();
                is_async = true;
                ms = self.span();
            }

            let mut is_readonly = false;
            if self.peek().kind == TokenKind::Readonly {
                self.advance();
                is_readonly = true;
                ms = self.span();
            }

            let mut is_override = false;
            if self.peek().kind == TokenKind::Override {
                self.advance();
                is_override = true;
                ms = self.span();
            }

            let mut is_abstract_member = false;
            if self.peek().kind == TokenKind::Abstract {
                self.advance();
                is_abstract_member = true;
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
                        type_params: vec![],
                        params,
                        return_ty: TypeExpr::Name(name.clone(), ms),
                        body,
                        is_static: false,
                        is_async: false,
                        is_override: false,
                        is_abstract: false,
                        access,
                        span: ms,
                        doc: member_doc,
                    });
                }
                TokenKind::Function => {
                    self.advance();
                    let (mname, mname_span) = if let Token {
                        kind: TokenKind::Identifier(mname),
                        line,
                        column,
                    } = self.peek().clone()
                    {
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

                    let type_params = self.parse_type_params();

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

                    let body = if is_abstract_member {
                        let _ = self.consume(TokenKind::Semicolon);
                        Box::new(Statement::Error) // Placeholder for abstract body
                    } else {
                        Box::new(self.parse_block())
                    };

                    methods.push(ClassMethod {
                        name: mname,
                        name_span: mname_span,
                        type_params,
                        params,
                        return_ty,
                        body,
                        is_static,
                        is_async,
                        is_override,
                        is_abstract: is_abstract_member,
                        access,
                        span: ms,
                        doc: member_doc,
                    });
                }
                TokenKind::Identifier(fname) => {
                    let fs = self.span();
                    self.advance();
                    let method_type_params = self.parse_type_params();
                    if self.peek().kind == TokenKind::OpenParen {
                        self.advance();
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

                        let body = if is_abstract_member {
                            let _ = self.consume(TokenKind::Semicolon);
                            Box::new(Statement::Error) // Placeholder for abstract body
                        } else {
                            Box::new(self.parse_block())
                        };

                        methods.push(ClassMethod {
                            name: fname,
                            name_span: fs,
                            type_params: method_type_params,
                            params,
                            return_ty,
                            body,
                            is_static,
                            is_async,
                            is_override,
                            is_abstract: is_abstract_member,
                            access,
                            span: ms,
                            doc: member_doc,
                        });
                    } else {
                        if is_abstract_member {
                            self.diagnostics.push(Diagnostic::error(
                                "Fields cannot be abstract".to_string(),
                                fs.line,
                                fs.column,
                            ));
                        }
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
                            is_readonly,
                            access,
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
            type_params,
            extends,
            implements,
            fields,
            methods,
            constructor,
            is_abstract,
            span: s,
            doc,
        })
    }

    pub(crate) fn parse_interface_declaration(
        &mut self,
        doc: Option<DocComment>,
    ) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Interface)?;
        let (name, name_span) = if let Token {
            kind: TokenKind::Identifier(name),
            line,
            column,
        } = self.peek().clone()
        {
            self.advance();
            (name, Span::new(line, column))
        } else {
            if !self.panic_mode {
                let token = self.peek();
                self.diagnostics.push(Diagnostic::error(
                    "Expected interface name after interface keyword".to_string(),
                    token.line,
                    token.column,
                ));
                self.panic_mode = true;
            }
            return Err(());
        };

        let type_params = self.parse_type_params();

        self.consume(TokenKind::OpenBrace)?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();

        while self.peek().kind != TokenKind::CloseBrace && !self.is_at_end() {
            if matches!(
                self.peek().kind,
                TokenKind::Comment(_) | TokenKind::RegularBlockComment(_) | TokenKind::Semicolon
            ) {
                self.advance();
                continue;
            }
            let member_doc = self.parse_doc_comments();
            let mut ms = self.span();

            let mut is_readonly = false;
            if self.peek().kind == TokenKind::Readonly {
                self.advance();
                is_readonly = true;
                ms = self.span();
            }

            let kind = self.peek().kind.clone();
            match kind {
                TokenKind::Identifier(fname) => {
                    let fs = self.span();
                    self.advance();
                    let method_type_params = self.parse_type_params();
                    if self.peek().kind == TokenKind::OpenParen {
                        self.advance();
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

                        let _ = self.consume(TokenKind::Semicolon);
                        methods.push(ClassMethod {
                            name: fname,
                            name_span: fs,
                            type_params: method_type_params,
                            params,
                            return_ty,
                            body: Box::new(Statement::Error), // Interfaces have no bodies
                            is_static: false,
                            is_async: false,
                            is_override: false,
                            is_abstract: false,
                            access: AccessModifier::Public,
                            span: ms,
                            doc: member_doc,
                        });
                    } else {
                        let _ = self.consume(TokenKind::Colon);
                        let fty = self.parse_type_expr();
                        let _ = self.consume(TokenKind::Semicolon);
                        fields.push(Field {
                            name: fname,
                            name_span: fs,
                            ty: fty,
                            value: None,
                            is_static: false,
                            is_readonly,
                            access: AccessModifier::Public,
                            span: fs,
                            doc: member_doc,
                        });
                    }
                }
                _ => {
                    if !self.panic_mode {
                        let token = self.peek();
                        self.diagnostics.push(Diagnostic::error(
                            format!("Unexpected token in interface body: {:?}", token.kind),
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

        Ok(Statement::Interface(crate::compiler::ast::InterfaceDecl {
            name,
            name_span,
            type_params,
            fields,
            methods,
            span: s,
            doc,
        }))
    }

    pub(crate) fn parse_enum_declaration(
        &mut self,
        doc: Option<DocComment>,
    ) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Enum)?;
        let (name, name_span) = if let Token {
            kind: TokenKind::Identifier(name),
            line,
            column,
        } = self.peek().clone()
        {
            self.advance();
            (name, Span::new(line, column))
        } else {
            if !self.panic_mode {
                let token = self.peek();
                self.diagnostics.push(Diagnostic::error(
                    "Expected enum name after enum keyword".to_string(),
                    token.line,
                    token.column,
                ));
                self.panic_mode = true;
            }
            return Err(());
        };

        self.consume(TokenKind::OpenBrace)?;
        let mut members = Vec::new();

        while self.peek().kind != TokenKind::CloseBrace && !self.is_at_end() {
            if self.peek().kind == TokenKind::Semicolon {
                self.advance();
                continue;
            }
            if let TokenKind::Identifier(mname) = self.peek().kind.clone() {
                let name_span = self.span();
                self.advance();

                let value = if self.peek().kind == TokenKind::Equal {
                    self.advance();
                    Some(self.parse_expression())
                } else {
                    None
                };

                members.push(crate::compiler::ast::EnumMember {
                    name: mname,
                    name_span,
                    value,
                });

                if self.peek().kind == TokenKind::Comma {
                    self.advance();
                }
            } else {
                if !self.panic_mode {
                    let token = self.peek();
                    self.diagnostics.push(Diagnostic::error(
                        format!("Unexpected token in enum body: {:?}", token.kind),
                        token.line,
                        token.column,
                    ));
                    self.panic_mode = true;
                }
                self.advance();
                self.synchronize();
            }
        }
        self.consume(TokenKind::CloseBrace)?;

        Ok(Statement::Enum(crate::compiler::ast::EnumDecl {
            name,
            name_span,
            members,
            span: s,
            doc,
        }))
    }

    pub(crate) fn parse_try_statement(&mut self) -> Result<Statement, ()> {
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

    pub(crate) fn parse_throw_statement(&mut self) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Throw)?;
        let expr = self.parse_expression();
        self.consume(TokenKind::Semicolon)?;
        Ok(Statement::Expression(Expr::Throw(Box::new(expr), s), s))
    }

    pub(crate) fn parse_type_alias_declaration(
        &mut self,
        doc: Option<DocComment>,
    ) -> Result<Statement, ()> {
        let s = self.span();
        self.consume(TokenKind::Type)?;
        let name_span = self.span();
        let name = if let TokenKind::Identifier(name) = self.peek().kind.clone() {
            self.advance();
            name
        } else {
            return Err(());
        };

        self.consume(TokenKind::Equal)?;
        let ty = self.parse_type_expr();
        self.consume(TokenKind::Semicolon)?;

        Ok(Statement::TypeAlias(TypeAliasDecl {
            name,
            name_span,
            ty,
            span: s,
            doc,
        }))
    }
}
