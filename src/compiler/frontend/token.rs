#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Keywords
    Let,
    Const,
    Print,
    If,
    Else,
    While,
    Function,
    Return,
    Static, // Added Static variant
    Class,
    Constructor,
    New,
    Enum,
    Public,
    Private,
    Protected,
    Readonly,
    This,
    Is,
    Import,
    Export,
    From,
    As,
    Async,
    Await,
    Try,
    Catch,
    Throw,
    Finally,
    Null,

    // Identifiers & Literals
    Identifier(String),
    StringLiteral(String),
    Number(i64),
    LineDoc(String),
    BlockDoc(String),
    Comment(String),
    RegularBlockComment(String),
    /// Template literal: `Hello, ${name}! You are ${age} years old.`
    /// Pre-parsed at lex time into alternating parts.
    TemplateLiteral(Vec<TplPart>),

    // Operators
    Plus,         // +
    Minus,        // -
    Equal,        // =
    EqEqual,      // ==
    BangEqual,    // !=
    Less,         // <
    LessEqual,    // <=
    Greater,      // >
    GreaterEqual, // >=
    And,          // &&
    Or,           // ||
    Dot,          // .
    Pipe,         // |
    Slash,        // /
    Star,         // *
    Percent,      // %

    // Punctuation
    Colon,        // :
    Semicolon,    // ;
    Comma,        // ,
    OpenParen,    // (
    CloseParen,   // )
    OpenBrace,    // {
    CloseBrace,   // }
    OpenBracket,  // [
    CloseBracket, // ]

    // End of file
    EOF,

    // Error
    Unknown(char),
}

/// A single segment inside a template literal.
#[derive(Debug, Clone, PartialEq)]
pub enum TplPart {
    /// A static string segment (already unescaped).
    Str(String),
    /// An interpolated expression, stored as raw source text with its start position.
    Expr(String, usize, usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub column: usize,
}

impl Token {
    pub fn new(kind: TokenKind, line: usize, column: usize) -> Self {
        Self { kind, line, column }
    }
}
