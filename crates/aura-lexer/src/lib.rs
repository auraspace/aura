//! Hand-written lexer for Aura C0 (RFC-001 §6.0.1).

use aura_ast::{BytePos, Span};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    // Keywords
    Package,
    Import,
    As,
    Class,
    Struct,
    Enum,
    Interface,
    Fun,
    Val,
    Var,
    If,
    Else,
    While,
    Match,
    Case,
    Try,
    Catch,
    Finally,
    Throw,
    Return,
    True,
    False,
    Null,
    Pub,
    This,
    /// Generic constraint clause: `where T : Named`.
    Where,

    Ident(String),
    Int(i64),
    String(String),

    // Punctuation / operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    AndAnd,
    OrOr,
    Bang,
    /// `!!` force-unwrap (lexed as one token).
    BangBang,
    Eq,
    /// `=>` match / lambda arrow.
    FatArrow,
    Dot,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Question,

    Eof,
}

impl TokenKind {
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            TokenKind::Package
                | TokenKind::Import
                | TokenKind::As
                | TokenKind::Class
                | TokenKind::Struct
                | TokenKind::Enum
                | TokenKind::Interface
                | TokenKind::Fun
                | TokenKind::Val
                | TokenKind::Var
                | TokenKind::If
                | TokenKind::Else
                | TokenKind::While
                | TokenKind::Match
                | TokenKind::Case
                | TokenKind::Try
                | TokenKind::Catch
                | TokenKind::Finally
                | TokenKind::Throw
                | TokenKind::Return
                | TokenKind::True
                | TokenKind::False
                | TokenKind::Null
                | TokenKind::Pub
                | TokenKind::This
                | TokenKind::Where
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub message: String,
    pub span: Span,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at bytes {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}

impl std::error::Error for LexError {}

pub struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
        }
    }

    pub fn tokenize(mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token()?;
            let is_eof = tok.kind == TokenKind::Eof;
            tokens.push(tok);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }

    fn next_token(&mut self) -> Result<Token, LexError> {
        self.skip_trivia()?;
        let start = self.pos as BytePos;
        if self.pos >= self.bytes.len() {
            return Ok(Token {
                kind: TokenKind::Eof,
                span: Span::new(start, start),
            });
        }

        let b = self.bytes[self.pos];
        match b {
            b'(' => self.simple(TokenKind::LParen, 1),
            b')' => self.simple(TokenKind::RParen, 1),
            b'{' => self.simple(TokenKind::LBrace, 1),
            b'}' => self.simple(TokenKind::RBrace, 1),
            b',' => self.simple(TokenKind::Comma, 1),
            b':' => self.simple(TokenKind::Colon, 1),
            b'?' => self.simple(TokenKind::Question, 1),
            b'.' => self.simple(TokenKind::Dot, 1),
            b'+' => self.simple(TokenKind::Plus, 1),
            b'-' => self.simple(TokenKind::Minus, 1),
            b'*' => self.simple(TokenKind::Star, 1),
            b'%' => self.simple(TokenKind::Percent, 1),
            b'/' => self.simple(TokenKind::Slash, 1),
            b'=' => {
                if self.peek_at(1) == Some(b'=') {
                    self.simple(TokenKind::EqEq, 2)
                } else if self.peek_at(1) == Some(b'>') {
                    self.simple(TokenKind::FatArrow, 2)
                } else {
                    self.simple(TokenKind::Eq, 1)
                }
            }
            b'!' => {
                if self.peek_at(1) == Some(b'=') {
                    self.simple(TokenKind::Ne, 2)
                } else if self.peek_at(1) == Some(b'!') {
                    self.simple(TokenKind::BangBang, 2)
                } else {
                    self.simple(TokenKind::Bang, 1)
                }
            }
            b'<' => {
                if self.peek_at(1) == Some(b'=') {
                    self.simple(TokenKind::Le, 2)
                } else {
                    self.simple(TokenKind::Lt, 1)
                }
            }
            b'>' => {
                if self.peek_at(1) == Some(b'=') {
                    self.simple(TokenKind::Ge, 2)
                } else {
                    self.simple(TokenKind::Gt, 1)
                }
            }
            b'&' => {
                if self.peek_at(1) == Some(b'&') {
                    self.simple(TokenKind::AndAnd, 2)
                } else {
                    Err(LexError {
                        message: "unexpected '&'; use '&&'".into(),
                        span: Span::new(start, start + 1),
                    })
                }
            }
            b'|' => {
                if self.peek_at(1) == Some(b'|') {
                    self.simple(TokenKind::OrOr, 2)
                } else {
                    Err(LexError {
                        message: "unexpected '|'; use '||'".into(),
                        span: Span::new(start, start + 1),
                    })
                }
            }
            b'"' => self.string(start),
            b'0'..=b'9' => self.number(start),
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.ident_or_kw(start),
            _ => Err(LexError {
                message: format!("unexpected character {:?}", self.src[self.pos..].chars().next()),
                span: Span::new(start, start + 1),
            }),
        }
    }

    fn simple(&mut self, kind: TokenKind, len: usize) -> Result<Token, LexError> {
        let start = self.pos as BytePos;
        self.pos += len;
        Ok(Token {
            kind,
            span: Span::new(start, start + len as BytePos),
        })
    }

    fn skip_trivia(&mut self) -> Result<(), LexError> {
        loop {
            if self.pos >= self.bytes.len() {
                return Ok(());
            }
            match self.bytes[self.pos] {
                b' ' | b'\t' | b'\r' | b'\n' => self.pos += 1,
                b'/' if self.peek_at(1) == Some(b'/') => {
                    self.pos += 2;
                    while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
                        self.pos += 1;
                    }
                }
                b'/' if self.peek_at(1) == Some(b'*') => {
                    let start = self.pos as BytePos;
                    self.pos += 2;
                    loop {
                        if self.pos + 1 >= self.bytes.len() {
                            return Err(LexError {
                                message: "unterminated block comment".into(),
                                span: Span::new(start, self.pos as BytePos),
                            });
                        }
                        if self.bytes[self.pos] == b'*' && self.bytes[self.pos + 1] == b'/' {
                            self.pos += 2;
                            break;
                        }
                        self.pos += 1;
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn number(&mut self, start: BytePos) -> Result<Token, LexError> {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        // skip underscores in middle? C0: allow 1_000
        // keep simple: only pure digits for C0 first slice
        let end = self.pos as BytePos;
        let text = &self.src[start as usize..end as usize];
        let value: i64 = text.parse().map_err(|_| LexError {
            message: format!("invalid integer literal `{text}`"),
            span: Span::new(start, end),
        })?;
        Ok(Token {
            kind: TokenKind::Int(value),
            span: Span::new(start, end),
        })
    }

    fn string(&mut self, start: BytePos) -> Result<Token, LexError> {
        self.pos += 1; // opening "
        let mut out = String::new();
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b'"' => {
                    self.pos += 1;
                    return Ok(Token {
                        kind: TokenKind::String(out),
                        span: Span::new(start, self.pos as BytePos),
                    });
                }
                b'\\' => {
                    self.pos += 1;
                    if self.pos >= self.bytes.len() {
                        break;
                    }
                    let esc = self.bytes[self.pos];
                    self.pos += 1;
                    match esc {
                        b'n' => out.push('\n'),
                        b't' => out.push('\t'),
                        b'r' => out.push('\r'),
                        b'\\' => out.push('\\'),
                        b'"' => out.push('"'),
                        other => {
                            return Err(LexError {
                                message: format!("unknown string escape `\\{}`", other as char),
                                span: Span::new(self.pos as BytePos - 2, self.pos as BytePos),
                            });
                        }
                    }
                }
                b'\n' => {
                    return Err(LexError {
                        message: "unterminated string literal".into(),
                        span: Span::new(start, self.pos as BytePos),
                    });
                }
                c => {
                    // multi-byte UTF-8: advance by char
                    let ch = self.src[self.pos..].chars().next().unwrap_or(c as char);
                    out.push(ch);
                    self.pos += ch.len_utf8();
                }
            }
        }
        Err(LexError {
            message: "unterminated string literal".into(),
            span: Span::new(start, self.pos as BytePos),
        })
    }

    fn ident_or_kw(&mut self, start: BytePos) -> Result<Token, LexError> {
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let end = self.pos as BytePos;
        let text = &self.src[start as usize..end as usize];
        let kind = match text {
            "package" => TokenKind::Package,
            "import" => TokenKind::Import,
            "as" => TokenKind::As,
            "class" => TokenKind::Class,
            "struct" => TokenKind::Struct,
            "enum" => TokenKind::Enum,
            "interface" => TokenKind::Interface,
            "fun" => TokenKind::Fun,
            "val" => TokenKind::Val,
            "var" => TokenKind::Var,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "match" => TokenKind::Match,
            "case" => TokenKind::Case,
            "try" => TokenKind::Try,
            "catch" => TokenKind::Catch,
            "finally" => TokenKind::Finally,
            "throw" => TokenKind::Throw,
            "return" => TokenKind::Return,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "null" => TokenKind::Null,
            "pub" => TokenKind::Pub,
            "this" => TokenKind::This,
            "where" => TokenKind::Where,
            _ => TokenKind::Ident(text.to_string()),
        };
        Ok(Token {
            kind,
            span: Span::new(start, end),
        })
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }
}

/// Convenience: lex source into tokens (including trailing Eof).
pub fn lex(src: &str) -> Result<Vec<Token>, LexError> {
    Lexer::new(src).tokenize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_hello_tokens() {
        let src = r#"
package main

fun main() {
  println("Hello, Aura")
}
"#;
        let tokens = lex(src).expect("lex");
        let kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();
        assert!(matches!(kinds[0], TokenKind::Package));
        assert!(matches!(kinds[1], TokenKind::Ident(s) if s == "main"));
        assert!(matches!(kinds[2], TokenKind::Fun));
        assert!(matches!(kinds.iter().last().unwrap(), TokenKind::Eof));
        assert!(kinds.iter().any(|k| matches!(k, TokenKind::String(s) if s == "Hello, Aura")));
    }

    #[test]
    fn skips_comments() {
        let tokens = lex("// line\nfun /* block */ x").unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Fun));
        assert!(matches!(&tokens[1].kind, TokenKind::Ident(s) if s == "x"));
    }

    #[test]
    fn operators() {
        let tokens = lex("a == b != c && d || e").unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::EqEq));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Ne));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::AndAnd));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::OrOr));
    }
}
