use crate::compiler::ast::{Program, Span};
use crate::compiler::frontend::error::DiagnosticList;
use crate::compiler::frontend::token::{Token, TokenKind};

pub mod expr;
pub mod stmt;

pub struct Parser {
    pub(crate) tokens: Vec<Token>,
    pub(crate) pos: usize,
    pub diagnostics: DiagnosticList,
    pub(crate) panic_mode: bool,
    pub(crate) file_path: String,
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

    pub(crate) fn span(&self) -> Span {
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

    pub(crate) fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    pub(crate) fn peek_n(&self, n: usize) -> &Token {
        if self.pos + n >= self.tokens.len() {
            return self.tokens.last().unwrap();
        }
        &self.tokens[self.pos + n]
    }

    pub(crate) fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.pos += 1;
        }
        &self.tokens[self.pos - 1]
    }

    pub(crate) fn consume(&mut self, kind: TokenKind) -> Result<(), ()> {
        if self.peek().kind == kind {
            self.advance();
            Ok(())
        } else {
            if !self.panic_mode {
                let token = self.peek();
                self.diagnostics
                    .push(crate::compiler::frontend::error::Diagnostic::error(
                        format!("Expected {:?}, found {:?}", kind, token.kind),
                        token.line,
                        token.column,
                    ));
                self.panic_mode = true;
            }
            Err(())
        }
    }

    pub(crate) fn synchronize(&mut self) {
        self.panic_mode = false;
        self.advance();

        while !self.is_at_end() {
            if self.tokens[self.pos - 1].kind == TokenKind::Semicolon {
                return;
            }

            match self.peek().kind {
                TokenKind::Class
                | TokenKind::Interface
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

    pub(crate) fn is_at_end(&self) -> bool {
        self.peek().kind == TokenKind::EOF
    }
}
