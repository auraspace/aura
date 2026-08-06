use super::*;

/// Parse the editor buffer while preserving source offsets for LSP ranges.
/// Diagnostics continue to use the strict parser separately.
pub(super) fn parse_editor_file(source: &str, cursor: usize) -> Option<(String, File)> {
    if let Ok(file) = parse_file(source) {
        return Some((source.to_owned(), file));
    }
    if let Some(repaired) = source_with_repaired_member_dot(source, cursor) {
        if let Ok(file) = parse_file(&repaired) {
            return Some((repaired, file));
        }
    }
    for (dot, character) in source.char_indices() {
        if character != '.' || dot == 0 {
            continue;
        }
        let line_end = source[dot..]
            .find('\n')
            .map(|offset| dot + offset)
            .unwrap_or(source.len());
        let tail = source[dot + 1..line_end].trim();
        if !tail.is_empty()
            && !tail
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            continue;
        }
        let member_end = if tail.is_empty() {
            dot + 1
        } else {
            line_end - source[dot + 1..line_end].len()
                + source[dot + 1..line_end].trim_start().len()
                + tail.len()
        };
        let repaired = repair_member_dot(source, dot, member_end);
        if let Ok(file) = parse_file(&repaired) {
            return Some((repaired, file));
        }
    }
    None
}

fn source_with_repaired_member_dot(source: &str, member_start: usize) -> Option<String> {
    let start = word_start(source, member_start);
    let (dot, blank_member) = if let Some((dot, _character)) = source[..start]
        .char_indices()
        .next_back()
        .filter(|(_, character)| *character == '.')
    {
        (dot, true)
    } else {
        let (dot, _) = source[..member_start]
            .char_indices()
            .rev()
            .find(|(dot, character)| {
                if *character != '.' || *dot == 0 {
                    return false;
                }
                let tail = &source[dot + 1..member_start];
                let tail = tail.trim();
                tail.is_empty()
                    || tail
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric() || character == '_')
                    || tail.chars().all(|character| {
                        character.is_ascii_alphanumeric()
                            || character == '_'
                            || character.is_ascii_whitespace()
                            || matches!(character, '}' | ')' | ']' | ';')
                    })
            })?;
        (dot, false)
    };
    Some(repair_member_dot(
        source,
        dot,
        if blank_member { member_start } else { dot + 1 },
    ))
}

fn repair_member_dot(source: &str, dot: usize, member_end: usize) -> String {
    let mut repaired = source.to_owned();
    let operator_start = if dot > 0 && source.as_bytes().get(dot - 1) == Some(&b'?') {
        dot - 1
    } else {
        dot
    };
    repaired.replace_range(operator_start..dot + 1, "  ");
    let blank_start = dot + 1;
    if blank_start < member_end {
        repaired.replace_range(
            blank_start..member_end,
            &" ".repeat(member_end - blank_start),
        );
    }
    repaired
}
