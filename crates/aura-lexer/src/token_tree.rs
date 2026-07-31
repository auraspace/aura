//! Delimiter-aware token trees used by RFC-010 macro expansion.

use crate::{Token, TokenKind};
use aura_ast::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delimiter {
    Parenthesis,
    Bracket,
    Brace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenTree {
    Leaf(Token),
    Group {
        delimiter: Delimiter,
        span: Span,
        children: Vec<TokenTree>,
    },
}

impl TokenTree {
    /// Group a token stream while preserving every source span. The terminal
    /// `Eof` token is intentionally omitted from the tree.
    pub fn from_tokens(tokens: &[Token]) -> Result<Vec<Self>, String> {
        let mut cursor = Cursor { tokens, index: 0 };
        let tree = cursor.parse_until(None)?;
        if cursor.index != tokens.len() {
            return Err("token tree parser did not consume the token stream".into());
        }
        Ok(tree)
    }

    pub fn span(&self) -> Span {
        match self {
            Self::Leaf(token) => token.span,
            Self::Group { span, .. } => *span,
        }
    }

    pub fn children(&self) -> Option<&[Self]> {
        match self {
            Self::Group { children, .. } => Some(children),
            Self::Leaf(_) => None,
        }
    }

    /// Flatten the tree back to source-order tokens, excluding EOF.
    pub fn flatten(&self, out: &mut Vec<Token>) {
        match self {
            Self::Leaf(token) => out.push(token.clone()),
            Self::Group {
                delimiter,
                children,
                span,
            } => {
                let (open, close) = delimiter_tokens(delimiter, *span);
                out.push(open);
                for child in children {
                    child.flatten(out);
                }
                out.push(close);
            }
        }
    }
}

struct Cursor<'a> {
    tokens: &'a [Token],
    index: usize,
}

impl<'a> Cursor<'a> {
    fn parse_until(&mut self, closing: Option<TokenKind>) -> Result<Vec<TokenTree>, String> {
        let mut children = Vec::new();
        while self.index < self.tokens.len() {
            let token = &self.tokens[self.index];
            if token.kind == TokenKind::Eof {
                if closing.is_some() {
                    return Err("unterminated token-tree group".into());
                }
                self.index += 1;
                break;
            }
            if closing.as_ref().is_some_and(|kind| kind == &token.kind) {
                return Ok(children);
            }
            if let Some((delimiter, close)) = opening(&token.kind) {
                let open = token.clone();
                self.index += 1;
                let nested = self.parse_until(Some(close.clone()))?;
                let Some(end) = self.tokens.get(self.index) else {
                    return Err("unterminated token-tree group".into());
                };
                if end.kind != close {
                    return Err("mismatched token-tree delimiter".into());
                }
                self.index += 1;
                children.push(TokenTree::Group {
                    delimiter,
                    span: Span {
                        start: open.span.start,
                        end: end.span.end,
                    },
                    children: nested,
                });
                continue;
            }
            if is_closing(&token.kind) {
                return Err("unexpected token-tree closing delimiter".into());
            }
            children.push(TokenTree::Leaf(token.clone()));
            self.index += 1;
        }
        if closing.is_some() {
            return Err("unterminated token-tree group".into());
        }
        Ok(children)
    }
}

fn opening(kind: &TokenKind) -> Option<(Delimiter, TokenKind)> {
    match kind {
        TokenKind::LParen => Some((Delimiter::Parenthesis, TokenKind::RParen)),
        TokenKind::LBracket => Some((Delimiter::Bracket, TokenKind::RBracket)),
        TokenKind::LBrace => Some((Delimiter::Brace, TokenKind::RBrace)),
        _ => None,
    }
}

fn is_closing(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace
    )
}

fn delimiter_tokens(delimiter: &Delimiter, span: Span) -> (Token, Token) {
    let (open, close) = match delimiter {
        Delimiter::Parenthesis => (TokenKind::LParen, TokenKind::RParen),
        Delimiter::Bracket => (TokenKind::LBracket, TokenKind::RBracket),
        Delimiter::Brace => (TokenKind::LBrace, TokenKind::RBrace),
    };
    (Token { kind: open, span }, Token { kind: close, span })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex;

    #[test]
    fn groups_nested_delimiters_and_round_trips() {
        let tokens = lex("package demo\nfun main() { println(1) }\n").expect("lex");
        let tree = TokenTree::from_tokens(&tokens).expect("group tokens");
        let mut flattened = Vec::new();
        for item in &tree {
            item.flatten(&mut flattened);
        }
        assert_eq!(
            flattened
                .iter()
                .map(|token| &token.kind)
                .collect::<Vec<_>>(),
            tokens[..tokens.len() - 1]
                .iter()
                .map(|token| &token.kind)
                .collect::<Vec<_>>()
        );
        assert!(tree.iter().any(|item| {
            matches!(
                item,
                TokenTree::Leaf(Token {
                    kind: TokenKind::Package,
                    ..
                })
            )
        }));
    }

    #[test]
    fn rejects_unbalanced_groups() {
        let tokens = lex("package demo\nfun main() {\n").expect("lex");
        let error = TokenTree::from_tokens(&tokens).expect_err("missing close must fail");
        assert!(error.contains("unterminated"));
    }
}
