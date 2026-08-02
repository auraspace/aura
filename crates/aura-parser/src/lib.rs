//! Recursive-descent + Pratt expression parser for Aura C0–C1b (RFC-001 §6.0).

mod error;
mod parser;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub use error::ParseError;

use aura_lexer::lex;
use aura_lexer::TokenTree;
use parser::Parser;

/// Parse a full Aura source file into an AST.
pub fn parse_file(src: &str) -> Result<aura_ast::File, ParseError> {
    parse_file_with_macro_sources(src, &[])
}

/// Parse a source file after making package-exported declarative macro
/// definitions visible to it. Definitions are appended because macro
/// collection is a separate phase and therefore does not require source order.
pub fn parse_file_with_macro_sources(
    src: &str,
    macro_sources: &[String],
) -> Result<aura_ast::File, ParseError> {
    let mut input = src.to_owned();
    for macro_source in macro_sources {
        input.push('\n');
        input.push_str(macro_source);
    }
    let tokens = lex(&input)?;
    let tokens = parser::expand_declarative_macros(tokens).map_err(|message| ParseError {
        message,
        span: aura_ast::Span::new(0, 0),
    })?;
    let mut p = Parser::new(tokens);
    p.parse_file()
}

/// Return top-level declarative macro definitions from a source file in their
/// original source spelling. Package loading uses these definitions to make
/// macros available across source files without storing syntax-only nodes in
/// the semantic AST.
pub fn declarative_macro_sources(src: &str) -> Result<Vec<String>, ParseError> {
    let tokens = lex(src)?;
    let tree = TokenTree::from_tokens(&tokens).map_err(|message| ParseError {
        message,
        span: aura_ast::Span::new(0, 0),
    })?;
    let mut sources = Vec::new();
    let mut index = 0;
    while index + 3 < tree.len() {
        let is_macro = matches!(
            tree.get(index),
            Some(TokenTree::Leaf(aura_lexer::Token {
                kind: aura_lexer::TokenKind::Macro,
                ..
            }))
        );
        let is_bang = matches!(
            tree.get(index + 1),
            Some(TokenTree::Leaf(aura_lexer::Token {
                kind: aura_lexer::TokenKind::Bang,
                ..
            }))
        );
        let Some(TokenTree::Group { span, .. }) = tree.get(index + 3) else {
            index += 1;
            continue;
        };
        if is_macro && is_bang {
            let start = tree[index].span().start as usize;
            let end = span.end as usize;
            let Some(source) = src.get(start..end) else {
                return Err(ParseError {
                    message: "macro definition span is outside source".into(),
                    span: aura_ast::Span::new(start as u32, end as u32),
                });
            };
            sources.push(source.to_owned());
            index += 4;
        } else {
            index += 1;
        }
    }
    Ok(sources)
}

/// Return the exported declarative macro names in a source unit. Package
/// resolution uses this before concatenating dependency macro definitions so
/// an ambiguous name fails deterministically instead of depending on load
/// order.
pub fn declarative_macro_names(src: &str) -> Result<Vec<String>, ParseError> {
    let tokens = lex(src)?;
    let tree = TokenTree::from_tokens(&tokens).map_err(|message| ParseError {
        message,
        span: aura_ast::Span::new(0, 0),
    })?;
    let mut names = Vec::new();
    let mut index = 0;
    while index + 2 < tree.len() {
        let is_macro = matches!(
            tree.get(index),
            Some(TokenTree::Leaf(aura_lexer::Token {
                kind: aura_lexer::TokenKind::Macro,
                ..
            }))
        );
        let is_bang = matches!(
            tree.get(index + 1),
            Some(TokenTree::Leaf(aura_lexer::Token {
                kind: aura_lexer::TokenKind::Bang,
                ..
            }))
        );
        if is_macro && is_bang {
            if let Some(TokenTree::Leaf(aura_lexer::Token {
                kind: aura_lexer::TokenKind::Ident(name),
                ..
            })) = tree.get(index + 2)
            {
                names.push(name.clone());
            }
            index += 4;
        } else {
            index += 1;
        }
    }
    Ok(names)
}
