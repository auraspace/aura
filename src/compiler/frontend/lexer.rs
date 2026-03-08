use crate::compiler::frontend::error::{Diagnostic, DiagnosticList};
use crate::compiler::frontend::token::{Token, TokenKind, TplPart};

pub struct Lexer<'a> {
    source: &'a str,
    pos: usize,
    line: usize,
    column: usize,
    pub diagnostics: DiagnosticList,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            pos: 0,
            line: 1,
            column: 1,
            diagnostics: DiagnosticList::new(),
        }
    }

    pub fn lex_all(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();
            if token.kind == TokenKind::EOF {
                tokens.push(token);
                break;
            }
            tokens.push(token);
        }
        tokens
    }

    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();
        let current_line = self.line;
        let current_column = self.column;

        if self.is_at_end() {
            return Token::new(TokenKind::EOF, current_line, current_column);
        }

        let ch = self.peek();

        match ch {
            '+' => {
                self.advance();
                Token::new(TokenKind::Plus, current_line, current_column)
            }
            '-' => {
                self.advance();
                Token::new(TokenKind::Minus, current_line, current_column)
            }
            '=' => {
                self.advance();
                if self.peek() == '=' {
                    self.advance();
                    Token::new(TokenKind::EqEqual, current_line, current_column)
                } else {
                    Token::new(TokenKind::Equal, current_line, current_column)
                }
            }
            '!' => {
                self.advance();
                if self.peek() == '=' {
                    self.advance();
                    Token::new(TokenKind::BangEqual, current_line, current_column)
                } else {
                    Token::new(TokenKind::Unknown('!'), current_line, current_column)
                }
            }
            '<' => {
                self.advance();
                if self.peek() == '=' {
                    self.advance();
                    Token::new(TokenKind::LessEqual, current_line, current_column)
                } else {
                    Token::new(TokenKind::Less, current_line, current_column)
                }
            }
            '>' => {
                self.advance();
                if self.peek() == '=' {
                    self.advance();
                    Token::new(TokenKind::GreaterEqual, current_line, current_column)
                } else {
                    Token::new(TokenKind::Greater, current_line, current_column)
                }
            }
            ':' => {
                self.advance();
                Token::new(TokenKind::Colon, current_line, current_column)
            }
            '.' => {
                self.advance();
                Token::new(TokenKind::Dot, current_line, current_column)
            }
            ';' => {
                self.advance();
                Token::new(TokenKind::Semicolon, current_line, current_column)
            }
            '|' => {
                self.advance();
                Token::new(TokenKind::Pipe, current_line, current_column)
            }
            ',' => {
                self.advance();
                Token::new(TokenKind::Comma, current_line, current_column)
            }
            '(' => {
                self.advance();
                Token::new(TokenKind::OpenParen, current_line, current_column)
            }
            ')' => {
                self.advance();
                Token::new(TokenKind::CloseParen, current_line, current_column)
            }
            '{' => {
                self.advance();
                Token::new(TokenKind::OpenBrace, current_line, current_column)
            }
            '}' => {
                self.advance();
                Token::new(TokenKind::CloseBrace, current_line, current_column)
            }
            '/' => {
                self.advance();
                if self.peek() == '/' {
                    self.advance();
                    if self.peek() == '/' {
                        self.advance();
                        // Doc Comment: collect to end of line
                        let start_pos = self.pos;
                        while !self.is_at_end() && self.peek() != '\n' {
                            self.advance();
                        }
                        let content = &self.source[start_pos..self.pos];
                        Token::new(
                            TokenKind::DocComment(content.to_string()),
                            current_line,
                            current_column,
                        )
                    } else {
                        // Regular comment: skip to end of line
                        while !self.is_at_end() && self.peek() != '\n' {
                            self.advance();
                        }
                        self.next_token()
                    }
                } else {
                    Token::new(TokenKind::Slash, current_line, current_column)
                }
            }
            '*' => {
                self.advance();
                Token::new(TokenKind::Star, current_line, current_column)
            }
            '%' => {
                self.advance();
                Token::new(TokenKind::Percent, current_line, current_column)
            }
            '"' => self.lex_string(),
            '`' => self.lex_template_literal(),
            _ if ch.is_ascii_digit() => self.lex_number(),
            _ if ch.is_alphabetic() || ch == '_' => self.lex_identifier(),
            _ => {
                self.advance();
                self.diagnostics.push(Diagnostic::error(
                    format!("Unexpected character: '{}'", ch),
                    current_line,
                    current_column,
                ));
                Token::new(TokenKind::Unknown(ch), current_line, current_column)
            }
        }
    }

    fn lex_template_literal(&mut self) -> Token {
        let line = self.line;
        let column = self.column;
        self.advance(); // skip opening `

        let mut parts: Vec<TplPart> = Vec::new();
        let mut static_buf = String::new();

        while !self.is_at_end() && self.peek() != '`' {
            if self.peek() == '$' {
                // Check for ${
                let next_pos = self.pos + 1;
                if next_pos < self.source.len() && self.source.as_bytes()[next_pos] == b'{' {
                    // Flush static text collected so far
                    if !static_buf.is_empty() {
                        parts.push(TplPart::Str(std::mem::take(&mut static_buf)));
                    }
                    self.advance(); // skip '$'
                    self.advance(); // skip '{'

                    // Collect expression source until matching '}'
                    let mut expr_src = String::new();
                    let mut depth = 1usize;
                    while !self.is_at_end() && depth > 0 {
                        let c = self.peek();
                        if c == '{' {
                            depth += 1;
                            expr_src.push(c);
                            self.advance();
                        } else if c == '}' {
                            depth -= 1;
                            if depth > 0 {
                                expr_src.push(c);
                            }
                            self.advance();
                        } else {
                            if c == '\n' {
                                self.line += 1;
                                self.column = 1;
                            }
                            expr_src.push(c);
                            self.advance();
                        }
                    }
                    parts.push(TplPart::Expr(expr_src));
                } else {
                    static_buf.push(self.advance());
                }
            } else {
                let c = self.peek();
                if c == '\n' {
                    self.line += 1;
                    self.column = 1;
                }
                static_buf.push(c);
                self.advance();
            }
        }

        // Flush remaining static text
        if !static_buf.is_empty() {
            parts.push(TplPart::Str(static_buf));
        }

        if self.is_at_end() {
            self.diagnostics.push(Diagnostic::error(
                "Unterminated template literal".to_string(),
                line,
                column,
            ));
        } else {
            self.advance(); // skip closing `
        }

        Token::new(TokenKind::TemplateLiteral(parts), line, column)
    }

    fn lex_number(&mut self) -> Token {
        let start_pos = self.pos;
        let line = self.line;
        let column = self.column;

        while !self.is_at_end() && self.peek().is_ascii_digit() {
            self.advance();
        }

        let literal = &self.source[start_pos..self.pos];
        let val: i32 = match literal.parse() {
            Ok(v) => v,
            Err(_) => {
                self.diagnostics.push(Diagnostic::error(
                    format!("Number literal too large: '{}'", literal),
                    line,
                    column,
                ));
                0
            }
        };
        Token::new(TokenKind::Number(val), line, column)
    }

    fn lex_string(&mut self) -> Token {
        let line = self.line;
        let column = self.column;
        self.advance(); // skip opening "
        let start_pos = self.pos;

        while !self.is_at_end() && self.peek() != '"' {
            if self.peek() == '\n' {
                self.line += 1;
                self.column = 1;
            }
            self.advance();
        }

        let literal = &self.source[start_pos..self.pos];
        if !self.is_at_end() {
            self.advance(); // skip closing "
        } else {
            self.diagnostics.push(Diagnostic::error(
                "Unterminated string literal".to_string(),
                line,
                column,
            ));
        }
        Token::new(TokenKind::StringLiteral(literal.to_string()), line, column)
    }

    fn lex_identifier(&mut self) -> Token {
        let start_pos = self.pos;
        let line = self.line;
        let column = self.column;

        while !self.is_at_end() && (self.peek().is_alphanumeric() || self.peek() == '_') {
            self.advance();
        }

        let literal = &self.source[start_pos..self.pos];
        let kind = match literal {
            "let" => TokenKind::Let,
            "print" => TokenKind::Print,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "function" => TokenKind::Function,
            "return" => TokenKind::Return,
            "class" => TokenKind::Class,
            "constructor" => TokenKind::Constructor,
            "new" => TokenKind::New,
            "static" => TokenKind::Static,
            "this" => TokenKind::This,
            "is" => TokenKind::Is,
            "import" => TokenKind::Import,
            "export" => TokenKind::Export,
            "from" => TokenKind::From,
            "as" => TokenKind::As,
            _ => TokenKind::Identifier(literal.to_string()),
        };
        Token::new(kind, line, column)
    }

    fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            match self.peek() {
                ' ' | '\t' | '\r' => {
                    self.advance();
                }
                '\n' => {
                    self.line += 1;
                    self.column = 1;
                    self.pos += 1;
                }
                _ => break,
            }
        }
    }

    fn advance(&mut self) -> char {
        let ch = self.peek();
        self.pos += ch.len_utf8();
        self.column += 1;
        ch
    }

    fn peek(&self) -> char {
        self.source[self.pos..].chars().next().unwrap_or('\0')
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.source.len()
    }
}
