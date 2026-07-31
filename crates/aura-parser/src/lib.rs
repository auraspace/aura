//! Recursive-descent + Pratt expression parser for Aura C0–C1b (RFC-001 §6.0).

mod error;
mod parser;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub use error::ParseError;

use aura_lexer::lex;
use parser::Parser;

/// Parse a full Aura source file into an AST.
pub fn parse_file(src: &str) -> Result<aura_ast::File, ParseError> {
    let tokens = lex(src)?;
    let tokens = parser::expand_declarative_macros(tokens).map_err(|message| ParseError {
        message,
        span: aura_ast::Span::new(0, 0),
    })?;
    let mut p = Parser::new(tokens);
    p.parse_file()
}
