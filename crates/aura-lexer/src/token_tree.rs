//! Delimiter-aware token trees used by RFC-010 macro expansion.

use crate::{Token, TokenKind};
use aura_ast::Span;
use std::collections::BTreeMap;

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

pub type MacroCaptures = BTreeMap<String, TokenTree>;
pub type RepeatedMacroCaptures = BTreeMap<String, Vec<TokenTree>>;
/// Captures for a repetition nested one level inside another repetition.
/// Each outer item owns its own sequence of inner captures.
pub type NestedMacroCaptures = BTreeMap<String, Vec<Vec<TokenTree>>>;

pub fn match_nested_repeated_pattern(
    pattern: &[TokenTree],
    input: &[TokenTree],
) -> Option<NestedMacroCaptures> {
    let (names, separator, requires_one) = repetition_parts(pattern)?;
    let TokenTree::Group { children, .. } = pattern.get(1)? else {
        return None;
    };
    let items = split_repetition_input(input, separator);
    if requires_one && items.is_empty() {
        return None;
    }
    let mut captures = names
        .iter()
        .map(|name| (name.clone(), Vec::with_capacity(items.len())))
        .collect::<NestedMacroCaptures>();
    for item in items {
        let matched = match_repeated_pattern(children, item)?;
        for name in &names {
            captures.get_mut(name)?.push(matched.get(name)?.clone());
        }
    }
    Some(captures)
}

pub fn substitute_nested_repeated(
    template: &[TokenTree],
    captures: &NestedMacroCaptures,
) -> Vec<TokenTree> {
    substitute_nested_sequence(template, captures)
}

fn substitute_nested_sequence(
    template: &[TokenTree],
    captures: &NestedMacroCaptures,
) -> Vec<TokenTree> {
    let mut output = Vec::new();
    let mut index = 0;
    while index < template.len() {
        let Some((consumed, names, separator)) = repetition_at(template, index) else {
            output.push(match &template[index] {
                TokenTree::Leaf(_) => template[index].clone(),
                TokenTree::Group {
                    delimiter,
                    span,
                    children,
                } => TokenTree::Group {
                    delimiter: delimiter.clone(),
                    span: *span,
                    children: substitute_nested_sequence(children, captures),
                },
            });
            index += 1;
            continue;
        };
        let Some(first) = captures.get(&names[0]) else {
            output.extend_from_slice(&template[index..index + consumed]);
            index += consumed;
            continue;
        };
        let TokenTree::Group { children, .. } = &template[index + 1] else {
            index += consumed;
            continue;
        };
        for (outer_index, _) in first.iter().enumerate() {
            if outer_index > 0 {
                if let Some(separator) = separator {
                    output.push(separator.clone());
                }
            }
            let inner = names
                .iter()
                .filter_map(|name| {
                    captures
                        .get(name)
                        .and_then(|items| items.get(outer_index))
                        .map(|values| (name.clone(), values.clone()))
                })
                .collect::<RepeatedMacroCaptures>();
            output.extend(substitute_repeated(children, &inner));
        }
        index += consumed;
    }
    output
}

/// Match the common RFC-010 repetition form `$( $name:fragment ),*` when it
/// occupies the complete rule pattern.
pub fn match_repeated_pattern(
    pattern: &[TokenTree],
    input: &[TokenTree],
) -> Option<RepeatedMacroCaptures> {
    let (names, separator, requires_one) = repetition_parts(pattern)?;
    let TokenTree::Group { children, .. } = pattern.get(1)? else {
        return None;
    };
    let items = split_repetition_input(input, separator);
    if requires_one && items.is_empty() {
        return None;
    }
    let mut captures = names
        .iter()
        .map(|name| (name.clone(), Vec::with_capacity(items.len())))
        .collect::<RepeatedMacroCaptures>();
    for item in items {
        let matched = match_pattern(children, item)?;
        for name in &names {
            captures.get_mut(name)?.push(matched.get(name)?.clone());
        }
    }
    Some(captures)
}

/// Expand a template containing `$( $name ),*`, preserving its separator.
pub fn substitute_repeated(
    template: &[TokenTree],
    captures: &RepeatedMacroCaptures,
) -> Vec<TokenTree> {
    substitute_repeated_sequence(template, captures)
}

fn substitute_repeated_sequence(
    template: &[TokenTree],
    captures: &RepeatedMacroCaptures,
) -> Vec<TokenTree> {
    let mut output = Vec::new();
    let mut index = 0;
    while index < template.len() {
        if let Some((consumed, names, separator)) = repetition_at(template, index) {
            let Some(first_name) = names.first() else {
                index += consumed;
                continue;
            };
            let Some(values) = captures.get(first_name) else {
                output.extend_from_slice(&template[index..index + consumed]);
                index += consumed;
                continue;
            };
            let TokenTree::Group {
                children: inner, ..
            } = &template[index + 1]
            else {
                unreachable!("repetition_at only accepts grouped repetitions");
            };
            for (value_index, _) in values.iter().enumerate() {
                if value_index > 0 {
                    if let Some(separator) = separator {
                        output.push(separator.clone());
                    }
                }
                let mut one = BTreeMap::new();
                for name in &names {
                    if let Some(value) = captures.get(name).and_then(|items| items.get(value_index))
                    {
                        one.insert(name.clone(), value.clone());
                    }
                }
                output.extend(substitute(inner, &one));
            }
            index += consumed;
            continue;
        }
        output.push(substitute_repeated_tree(&template[index], captures));
        index += 1;
    }
    output
}

fn substitute_repeated_tree(tree: &TokenTree, captures: &RepeatedMacroCaptures) -> TokenTree {
    match tree {
        TokenTree::Leaf(_) => tree.clone(),
        TokenTree::Group {
            delimiter,
            span,
            children,
        } => TokenTree::Group {
            delimiter: delimiter.clone(),
            span: *span,
            children: substitute_repeated_sequence(children, captures),
        },
    }
}

/// Match one declarative macro rule against a token-tree sequence. Metavariables
/// use `$name` or `$name:fragment`; the fragment label is retained for callers
/// but the lexer-level matcher deliberately leaves fragment parsing to the
/// language parser.
pub fn match_pattern(pattern: &[TokenTree], input: &[TokenTree]) -> Option<MacroCaptures> {
    fn visit(pattern: &[TokenTree], input: &[TokenTree], captures: &mut MacroCaptures) -> bool {
        if pattern.is_empty() {
            return input.is_empty();
        }
        if let Some((name, consumed)) = metavariable(pattern) {
            let Some(value) = input.first() else {
                return false;
            };
            if let Some(existing) = captures.get(&name) {
                if !same_tree(existing, value) {
                    return false;
                }
            } else {
                captures.insert(name, value.clone());
            }
            return visit(&pattern[consumed..], &input[1..], captures);
        }
        let Some((expected, rest)) = pattern.split_first() else {
            return input.is_empty();
        };
        let Some(actual) = input.first() else {
            return false;
        };
        if let (
            TokenTree::Group {
                delimiter: expected_delimiter,
                children: expected_children,
                ..
            },
            TokenTree::Group {
                delimiter: actual_delimiter,
                children: actual_children,
                ..
            },
        ) = (expected, actual)
        {
            if expected_delimiter != actual_delimiter
                || !visit(expected_children, actual_children, captures)
            {
                return false;
            }
        } else if !same_tree(expected, actual) {
            return false;
        }
        visit(rest, &input[1..], captures)
    }

    let mut captures = MacroCaptures::new();
    visit(pattern, input, &mut captures).then_some(captures)
}

/// Substitute `$name` occurrences in a macro template. Unknown metavariables
/// remain untouched so the caller can report a hygienic expansion error.
pub fn substitute(template: &[TokenTree], captures: &MacroCaptures) -> Vec<TokenTree> {
    let mut output = Vec::new();
    let mut index = 0;
    while index < template.len() {
        if let Some((name, consumed)) = metavariable(&template[index..]) {
            if let Some(value) = captures.get(&name) {
                output.push(value.clone());
            } else {
                output.extend_from_slice(&template[index..index + consumed]);
            }
            index += consumed;
        } else {
            output.push(substitute_tree(&template[index], captures));
            index += 1;
        }
    }
    output
}

fn substitute_tree(tree: &TokenTree, captures: &MacroCaptures) -> TokenTree {
    match tree {
        TokenTree::Leaf(_) => tree.clone(),
        TokenTree::Group {
            delimiter,
            span,
            children,
        } => TokenTree::Group {
            delimiter: delimiter.clone(),
            span: *span,
            children: substitute(children, captures),
        },
    }
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

fn metavariable(tokens: &[TokenTree]) -> Option<(String, usize)> {
    let [TokenTree::Leaf(Token {
        kind: TokenKind::Dollar,
        ..
    }), TokenTree::Leaf(Token {
        kind: TokenKind::Ident(name),
        ..
    }), ..] = tokens
    else {
        return None;
    };
    let consumed = if tokens.get(2).is_some_and(|token| {
        matches!(
            token,
            TokenTree::Leaf(Token {
                kind: TokenKind::Colon,
                ..
            })
        )
    }) {
        if !matches!(
            tokens.get(3),
            Some(TokenTree::Leaf(Token {
                kind: TokenKind::Ident(_),
                ..
            }))
        ) {
            return None;
        }
        4
    } else {
        2
    };
    Some((name.clone(), consumed))
}

fn repetition_parts(tokens: &[TokenTree]) -> Option<(Vec<String>, Option<&TokenTree>, bool)> {
    if tokens.len() < 3
        || !matches!(
            tokens.first(),
            Some(TokenTree::Leaf(Token {
                kind: TokenKind::Dollar,
                ..
            }))
        )
        || !matches!(tokens.get(1), Some(TokenTree::Group { .. }))
        || !matches!(
            tokens.last(),
            Some(TokenTree::Leaf(Token {
                kind: TokenKind::Star | TokenKind::Plus,
                ..
            }))
        )
    {
        return None;
    }
    let TokenTree::Group { children, .. } = &tokens[1] else {
        return None;
    };
    let names = metavariable_names(children);
    if names.is_empty() {
        return None;
    }
    let separator = (tokens.len() == 4).then(|| &tokens[2]);
    let requires_one = matches!(
        tokens.last(),
        Some(TokenTree::Leaf(Token {
            kind: TokenKind::Plus,
            ..
        }))
    );
    Some((names, separator, requires_one))
}

fn repetition_at(
    tokens: &[TokenTree],
    index: usize,
) -> Option<(usize, Vec<String>, Option<&TokenTree>)> {
    let remaining = &tokens[index..];
    for length in [3, 4] {
        if let Some(parts) = remaining.get(..length).and_then(repetition_parts) {
            return Some((length, parts.0, parts.1));
        }
    }
    None
}

fn metavariable_names(tokens: &[TokenTree]) -> Vec<String> {
    let mut names = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if let Some((name, consumed)) = metavariable(&tokens[index..]) {
            if !names.contains(&name) {
                names.push(name);
            }
            index += consumed;
        } else {
            if let TokenTree::Group { children, .. } = &tokens[index] {
                for name in metavariable_names(children) {
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
            index += 1;
        }
    }
    names
}

fn split_repetition_input<'a>(
    input: &'a [TokenTree],
    separator: Option<&TokenTree>,
) -> Vec<&'a [TokenTree]> {
    let Some(separator) = separator else {
        return if input.is_empty() {
            Vec::new()
        } else {
            vec![input]
        };
    };
    let mut items = Vec::new();
    let mut start = 0;
    for (index, token) in input.iter().enumerate() {
        if same_tree(separator, token) {
            if start == index {
                return Vec::new();
            }
            items.push(&input[start..index]);
            start = index + 1;
        }
    }
    if start == input.len() {
        return Vec::new();
    }
    items.push(&input[start..]);
    items
}

fn same_tree(left: &TokenTree, right: &TokenTree) -> bool {
    match (left, right) {
        (TokenTree::Leaf(left), TokenTree::Leaf(right)) => left.kind == right.kind,
        (
            TokenTree::Group {
                delimiter: left_delimiter,
                children: left_children,
                ..
            },
            TokenTree::Group {
                delimiter: right_delimiter,
                children: right_children,
                ..
            },
        ) => {
            left_delimiter == right_delimiter
                && left_children.len() == right_children.len()
                && left_children
                    .iter()
                    .zip(right_children)
                    .all(|(left, right)| same_tree(left, right))
        }
        _ => false,
    }
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

    #[test]
    fn matches_and_substitutes_metavariables_without_spans() {
        let pattern = TokenTree::from_tokens(&lex("$value:expr").expect("lex pattern"))
            .expect("group pattern");
        let input = TokenTree::from_tokens(&lex("42").expect("lex input")).expect("group input");
        let captures = match_pattern(&pattern, &input).expect("match");
        let template = TokenTree::from_tokens(&lex("wrap($value)").expect("lex template"))
            .expect("group template");
        let expanded = substitute(&template, &captures);
        let mut flattened = Vec::new();
        for item in &expanded {
            item.flatten(&mut flattened);
        }
        assert!(flattened
            .iter()
            .any(|token| token.kind == TokenKind::Int(42)));
    }

    #[test]
    fn matches_and_substitutes_comma_repetition() {
        let pattern = TokenTree::from_tokens(&lex("$($value:expr),*").expect("lex pattern"))
            .expect("group pattern");
        let input = TokenTree::from_tokens(&lex("1,2,3").expect("lex input")).expect("group input");
        let captures = match_repeated_pattern(&pattern, &input).expect("match repetition");
        assert_eq!(captures["value"].len(), 3);
        let template = TokenTree::from_tokens(&lex("wrap($($value),*)").expect("lex template"))
            .expect("group template");
        let group = template
            .get(1)
            .and_then(TokenTree::children)
            .expect("template group");
        let expanded = substitute_repeated(group, &captures);
        let mut flattened = Vec::new();
        for item in &expanded {
            item.flatten(&mut flattened);
        }
        assert_eq!(
            flattened
                .iter()
                .filter(|token| matches!(token.kind, TokenKind::Int(_)))
                .count(),
            3
        );
    }

    #[test]
    fn matches_multiple_captures_and_plus_repetition() {
        let pattern =
            TokenTree::from_tokens(&lex("$( $name:ident = $value:expr ),+").expect("lex pattern"))
                .expect("group pattern");
        let input = TokenTree::from_tokens(&lex("first = 1, second = 2").expect("lex input"))
            .expect("group input");
        let captures = match_repeated_pattern(&pattern, &input).expect("match repetition");
        assert_eq!(captures["name"].len(), 2);
        assert_eq!(captures["value"].len(), 2);

        let template =
            TokenTree::from_tokens(&lex("make($($name = $value),*)").expect("lex template"))
                .expect("group template");
        let expanded = substitute_repeated(&template, &captures);
        let mut flattened = Vec::new();
        for item in &expanded {
            item.flatten(&mut flattened);
        }
        assert_eq!(
            flattened
                .iter()
                .filter(|token| matches!(token.kind, TokenKind::Ident(_)))
                .count(),
            3
        );
        assert_eq!(
            flattened
                .iter()
                .filter(|token| matches!(token.kind, TokenKind::Int(_)))
                .count(),
            2
        );
    }

    #[test]
    fn matches_and_substitutes_nested_repetition() {
        let pattern = TokenTree::from_tokens(&lex("$($($value:expr),*);*").expect("lex pattern"))
            .expect("group pattern");
        let input =
            TokenTree::from_tokens(&lex("1,2;3,4").expect("lex input")).expect("group input");
        let captures = match_nested_repeated_pattern(&pattern, &input).expect("nested match");
        assert_eq!(captures["value"].len(), 2);
        assert_eq!(captures["value"][0].len(), 2);
        assert_eq!(captures["value"][1].len(), 2);
        let template =
            TokenTree::from_tokens(&lex("emit($($($value),*);*)").expect("lex template"))
                .expect("group template");
        let expanded = substitute_nested_repeated(&template, &captures);
        let mut flattened = Vec::new();
        for item in &expanded {
            item.flatten(&mut flattened);
        }
        assert_eq!(
            flattened
                .iter()
                .filter(|token| token.kind == TokenKind::Int(1))
                .count(),
            1
        );
        assert_eq!(
            flattened
                .iter()
                .filter(|token| token.kind == TokenKind::Int(4))
                .count(),
            1
        );
    }
}
