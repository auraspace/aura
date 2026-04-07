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

#[cfg(test)]
mod tests {
    use super::*;
    use aura_span::{BytePos, Span};

    #[test]
    fn token_carries_span_and_trivia() {
        let span = Span::new(BytePos::new(1), BytePos::new(4));
        let trivia_span = Span::new(BytePos::new(0), BytePos::new(1));

        let mut token = Token::new(TokenKind::Ident, span);
        token.leading_trivia.push(Trivia {
            kind: TriviaKind::Whitespace,
            span: trivia_span,
        });

        assert_eq!(token.span, span);
        assert_eq!(token.leading_trivia.len(), 1);
        assert_eq!(token.leading_trivia[0].span, trivia_span);
    }
}
