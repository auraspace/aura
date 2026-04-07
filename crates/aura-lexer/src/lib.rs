use aura_span::BytePos;
use aura_span::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TriviaKind {
    Whitespace,
    LineComment,
    BlockComment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Trivia {
    pub kind: TriviaKind,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Keyword {
    Class,
    Interface,
    Extends,
    Implements,
    Function,
    Return,
    Let,
    Const,
    If,
    Else,
    While,
    For,
    Break,
    Continue,
    Try,
    Catch,
    Finally,
    Throw,
    Import,
    Export,
    New,
    This,
    True,
    False,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Operator {
    Plus,
    Minus,
    Star,
    Slash,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    AndAnd,
    OrOr,
    Bang,
    Eq,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Punct {
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Dot,
    Colon,
    Semi,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TokenKind {
    Ident,
    Int,
    Float,
    String,
    Keyword(Keyword),
    Operator(Operator),
    Punct(Punct),
    Eof,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub leading_trivia: Vec<Trivia>,
    pub trailing_trivia: Vec<Trivia>,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self {
            kind,
            span,
            leading_trivia: Vec::new(),
            trailing_trivia: Vec::new(),
        }
    }
}

pub fn lex(source: &str) -> Vec<Token> {
    Lexer::new(source).lex()
}

#[derive(Clone, Debug)]
pub struct Lexer<'a> {
    source: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            pos: 0,
        }
    }

    pub fn lex(mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let leading_trivia = self.lex_trivia();
            let token = self.lex_token(leading_trivia);
            let is_eof = token.kind == TokenKind::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        tokens
    }

    fn lex_token(&mut self, leading_trivia: Vec<Trivia>) -> Token {
        let start = self.pos;

        if self.is_eof() {
            let at = self.byte_pos();
            return Token {
                kind: TokenKind::Eof,
                span: Span::empty(at),
                leading_trivia,
                trailing_trivia: Vec::new(),
            };
        }

        let kind = match self.peek_byte() {
            Some(b'"') => self.lex_string(),
            Some(b'0'..=b'9') => self.lex_number(),
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'_') => self.lex_ident_or_keyword(),
            _ => self.lex_operator_or_punct_or_unknown(),
        };

        let end = self.pos;
        Token {
            kind,
            span: self.span_from(start, end),
            leading_trivia,
            trailing_trivia: Vec::new(),
        }
    }

    fn lex_trivia(&mut self) -> Vec<Trivia> {
        let mut trivia = Vec::new();
        loop {
            let start = self.pos;
            let Some(byte) = self.peek_byte() else { break };

            if byte.is_ascii_whitespace() {
                self.bump_while(|b| b.is_ascii_whitespace());
                trivia.push(Trivia {
                    kind: TriviaKind::Whitespace,
                    span: self.span_from(start, self.pos),
                });
                continue;
            }

            if self.starts_with(b"//") {
                self.pos += 2;
                self.bump_while(|b| b != b'\n');
                trivia.push(Trivia {
                    kind: TriviaKind::LineComment,
                    span: self.span_from(start, self.pos),
                });
                continue;
            }

            if self.starts_with(b"/*") {
                self.pos += 2;
                while !self.is_eof() && !self.starts_with(b"*/") {
                    self.pos += 1;
                }
                if self.starts_with(b"*/") {
                    self.pos += 2;
                }
                trivia.push(Trivia {
                    kind: TriviaKind::BlockComment,
                    span: self.span_from(start, self.pos),
                });
                continue;
            }

            break;
        }
        trivia
    }

    fn lex_string(&mut self) -> TokenKind {
        debug_assert_eq!(self.peek_byte(), Some(b'"'));
        self.pos += 1;
        while let Some(b) = self.peek_byte() {
            match b {
                b'\\' => {
                    self.pos += 1;
                    if !self.is_eof() {
                        self.pos += 1;
                    }
                }
                b'"' => {
                    self.pos += 1;
                    break;
                }
                _ => self.pos += 1,
            }
        }
        TokenKind::String
    }

    fn lex_number(&mut self) -> TokenKind {
        debug_assert!(matches!(self.peek_byte(), Some(b'0'..=b'9')));
        if self.starts_with(b"0x") || self.starts_with(b"0X") {
            self.pos += 2;
            self.bump_while(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F'));
            return TokenKind::Int;
        }
        if self.starts_with(b"0b") || self.starts_with(b"0B") {
            self.pos += 2;
            self.bump_while(|b| matches!(b, b'0' | b'1'));
            return TokenKind::Int;
        }

        self.bump_while(|b| matches!(b, b'0'..=b'9'));
        if self.peek_byte() == Some(b'.')
            && matches!(self.peek_byte_at(self.pos + 1), Some(b'0'..=b'9'))
        {
            self.pos += 1;
            self.bump_while(|b| matches!(b, b'0'..=b'9'));
            return TokenKind::Float;
        }
        TokenKind::Int
    }

    fn lex_ident_or_keyword(&mut self) -> TokenKind {
        debug_assert!(matches!(
            self.peek_byte(),
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'_')
        ));
        let start = self.pos;
        self.pos += 1;
        self.bump_while(|b| matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_'));
        let text = &self.source[start..self.pos];
        match keyword_from(text) {
            Some(kw) => TokenKind::Keyword(kw),
            None => TokenKind::Ident,
        }
    }

    fn lex_operator_or_punct_or_unknown(&mut self) -> TokenKind {
        if self.starts_with(b"==") {
            self.pos += 2;
            return TokenKind::Operator(Operator::EqEq);
        }
        if self.starts_with(b"!=") {
            self.pos += 2;
            return TokenKind::Operator(Operator::NotEq);
        }
        if self.starts_with(b"<=") {
            self.pos += 2;
            return TokenKind::Operator(Operator::LtEq);
        }
        if self.starts_with(b">=") {
            self.pos += 2;
            return TokenKind::Operator(Operator::GtEq);
        }
        if self.starts_with(b"&&") {
            self.pos += 2;
            return TokenKind::Operator(Operator::AndAnd);
        }
        if self.starts_with(b"||") {
            self.pos += 2;
            return TokenKind::Operator(Operator::OrOr);
        }

        let Some(b) = self.peek_byte() else {
            return TokenKind::Eof;
        };
        self.pos += 1;

        let punct = match b {
            b'(' => Some(Punct::LParen),
            b')' => Some(Punct::RParen),
            b'{' => Some(Punct::LBrace),
            b'}' => Some(Punct::RBrace),
            b'[' => Some(Punct::LBracket),
            b']' => Some(Punct::RBracket),
            b',' => Some(Punct::Comma),
            b'.' => Some(Punct::Dot),
            b':' => Some(Punct::Colon),
            b';' => Some(Punct::Semi),
            _ => None,
        };
        if let Some(punct) = punct {
            return TokenKind::Punct(punct);
        }

        let op = match b {
            b'+' => Some(Operator::Plus),
            b'-' => Some(Operator::Minus),
            b'*' => Some(Operator::Star),
            b'/' => Some(Operator::Slash),
            b'<' => Some(Operator::Lt),
            b'>' => Some(Operator::Gt),
            b'!' => Some(Operator::Bang),
            b'=' => Some(Operator::Eq),
            _ => None,
        };
        if let Some(op) = op {
            return TokenKind::Operator(op);
        }

        TokenKind::Unknown
    }

    fn bump_while(&mut self, mut predicate: impl FnMut(u8) -> bool) {
        while let Some(b) = self.peek_byte() {
            if !predicate(b) {
                break;
            }
            self.pos += 1;
        }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn peek_byte(&self) -> Option<u8> {
        self.peek_byte_at(self.pos)
    }

    fn peek_byte_at(&self, idx: usize) -> Option<u8> {
        self.bytes.get(idx).copied()
    }

    fn starts_with(&self, prefix: &[u8]) -> bool {
        matches!(
            self.bytes.get(self.pos..),
            Some(slice) if slice.starts_with(prefix)
        )
    }

    fn byte_pos(&self) -> BytePos {
        BytePos::new(self.pos as u32)
    }

    fn span_from(&self, start: usize, end: usize) -> Span {
        Span::new(BytePos::new(start as u32), BytePos::new(end as u32))
    }
}

fn keyword_from(text: &str) -> Option<Keyword> {
    Some(match text {
        "class" => Keyword::Class,
        "interface" => Keyword::Interface,
        "extends" => Keyword::Extends,
        "implements" => Keyword::Implements,
        "function" => Keyword::Function,
        "return" => Keyword::Return,
        "let" => Keyword::Let,
        "const" => Keyword::Const,
        "if" => Keyword::If,
        "else" => Keyword::Else,
        "while" => Keyword::While,
        "for" => Keyword::For,
        "break" => Keyword::Break,
        "continue" => Keyword::Continue,
        "try" => Keyword::Try,
        "catch" => Keyword::Catch,
        "finally" => Keyword::Finally,
        "throw" => Keyword::Throw,
        "import" => Keyword::Import,
        "export" => Keyword::Export,
        "new" => Keyword::New,
        "this" => Keyword::This,
        "true" => Keyword::True,
        "false" => Keyword::False,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_keywords_ops_punct_and_trivia() {
        let src = "  // hi\nclass Foo { let x: i32 = 1 + 2; }";
        let tokens = lex(src);

        assert!(matches!(tokens[0].kind, TokenKind::Keyword(Keyword::Class)));
        assert!(matches!(tokens[1].kind, TokenKind::Ident));
        assert!(matches!(tokens[2].kind, TokenKind::Punct(Punct::LBrace)));
        assert!(matches!(
            tokens
                .iter()
                .find(|t| t.kind == TokenKind::Operator(Operator::Plus)),
            Some(_)
        ));

        assert!(tokens[0]
            .leading_trivia
            .iter()
            .any(|t| t.kind == TriviaKind::LineComment));
        assert!(matches!(
            tokens.last().map(|t| t.kind),
            Some(TokenKind::Eof)
        ));
    }
}
