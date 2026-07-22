//! Line/column mapping and pretty diagnostics.

use aura_ast::{BytePos, Span};

pub mod json;
pub use json::{JsonDiagnostic, JsonSpan, Severity};

/// 1-based line and column (column counts UTF-8 bytes on the line, like rustc for ASCII).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineCol {
    pub line: u32,
    pub column: u32,
}

/// Convert a byte offset into 1-based line/column within `src`.
pub fn offset_to_line_col(src: &str, offset: BytePos) -> LineCol {
    let offset = offset as usize;
    let offset = offset.min(src.len());
    let mut line = 1u32;
    let mut col = 1u32;
    for (i, b) in src.as_bytes().iter().enumerate() {
        if i == offset {
            return LineCol { line, column: col };
        }
        if *b == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    LineCol { line, column: col }
}

/// Format `path:line:col` for the start of a span.
pub fn location_label(path: &str, src: &str, span: Span) -> String {
    let lc = offset_to_line_col(src, span.start);
    format!("{path}:{}:{}", lc.line, lc.column)
}

/// Options for pretty multi-line diagnostics (C10b).
#[derive(Debug, Clone, Default)]
pub struct FormatOptions<'a> {
    /// Extra `= note:` / `= help:` lines after the snippet.
    pub notes: &'a [String],
    /// Include the source line immediately before the error (when available).
    pub context_before: bool,
}

/// Pretty multi-line diagnostic:
/// ```text
/// error: message
///   --> path:line:col
///    |
/// N-1 |  previous line   (optional context)
///  N  |  source line
///     |  ^^^
///    = note: …
/// ```
pub fn format_error(path: &str, src: &str, message: &str, span: Span) -> String {
    format_error_with(path, src, message, span, &FormatOptions::default())
}

/// Pretty diagnostic with notes and optional context line (C10b).
pub fn format_error_with(
    path: &str,
    src: &str,
    message: &str,
    span: Span,
    opts: &FormatOptions<'_>,
) -> String {
    let start = offset_to_line_col(src, span.start);
    let end = offset_to_line_col(src, span.end.max(span.start));
    let lines: Vec<&str> = src.lines().collect();
    let line_idx = (start.line as usize).saturating_sub(1);
    let line_text = lines.get(line_idx).copied().unwrap_or("");
    let col = start.column as usize;
    let mark_len = if start.line == end.line {
        let n = (span.end.saturating_sub(span.start)) as usize;
        n.max(1)
    } else {
        1
    };
    // columns are 1-based; underline starts at col-1
    let pad = " ".repeat(col.saturating_sub(1));
    let carets = "^".repeat(mark_len.clamp(1, 80));
    let line_no = start.line;
    let gutter = format!("{line_no}");
    let gw = gutter.len().max(2);

    let mut out = format!(
        "error: {message}\n  --> {path}:{line_no}:{col}\n{blank:>gw$} |",
        blank = "",
        col = start.column,
    );

    if opts.context_before && line_idx > 0 {
        let prev_no = line_no - 1;
        let prev = lines.get(line_idx - 1).copied().unwrap_or("");
        out.push_str(&format!("\n{prev_no:>gw$} | {prev}"));
    }

    out.push_str(&format!(
        "\n{line_no:>gw$} | {line_text}\n{blank:>gw$} | {pad}{carets}",
        blank = "",
    ));

    // Auto notes from common type-mismatch wording when caller left notes empty.
    let mut notes: Vec<String> = opts.notes.to_vec();
    if notes.is_empty() {
        if let Some((exp, found)) = parse_expected_found(message) {
            notes.push(format!("expected type `{exp}`"));
            notes.push(format!("found type `{found}`"));
        }
    }
    for n in &notes {
        // Allow pre-tagged notes (`note: …` / `help: …`) or plain strings.
        if n.starts_with("note:") || n.starts_with("help:") {
            out.push_str(&format!("\n{blank:>gw$} = {n}", blank = ""));
        } else {
            out.push_str(&format!("\n{blank:>gw$} = note: {n}", blank = ""));
        }
    }

    out
}

/// Pull type-mismatch `expected X, found Y` from a message when present.
fn parse_expected_found(message: &str) -> Option<(String, String)> {
    // Only for type-mismatch wording (sema C5k / init / lambda), not parser "expected `(`".
    if !(message.contains("type mismatch")
        || message.contains("initializing")
        || message.contains("assigning to")
        || message.contains("lambda body"))
    {
        return None;
    }
    let exp_key = "expected ";
    let found_key = ", found ";
    let exp_pos = message.find(exp_key)?;
    let after_exp = &message[exp_pos + exp_key.len()..];
    let found_pos = after_exp.find(found_key)?;
    let expected = after_exp[..found_pos].trim();
    let mut found = after_exp[found_pos + found_key.len()..].trim();
    // Truncate trailing punctuation / extra clauses.
    if let Some(i) = found.find(';') {
        found = found[..i].trim();
    }
    if expected.is_empty() || found.is_empty() {
        return None;
    }
    // Avoid false positives on unrelated wording with huge spans.
    if expected.len() > 80 || found.len() > 80 {
        return None;
    }
    Some((expected.to_string(), found.to_string()))
}

/// Convenience when only message + span are available (no snippet).
pub fn format_short(path: &str, src: &str, message: &str, span: Span) -> String {
    format!("{}: {}", location_label(path, src, span), message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_ast::Span;

    #[test]
    fn first_line() {
        let src = "fun main() {\n}\n";
        assert_eq!(offset_to_line_col(src, 0), LineCol { line: 1, column: 1 });
        assert_eq!(offset_to_line_col(src, 4), LineCol { line: 1, column: 5 });
    }

    #[test]
    fn second_line() {
        let src = "package main\nfun x() {}\n";
        // 'f' of fun is after "package main\n"
        let off = "package main\n".len() as u32;
        assert_eq!(offset_to_line_col(src, off), LineCol { line: 2, column: 1 });
    }

    #[test]
    fn pretty_contains_arrow() {
        let src = "package main\nfun main() {\n  bad\n}\n";
        // point at `bad`
        let start = src.find("bad").unwrap() as u32;
        let msg = format_error(
            "t.aura",
            src,
            "undefined name `bad`",
            Span::new(start, start + 3),
        );
        assert!(msg.contains("--> t.aura:"));
        assert!(msg.contains("bad"));
        assert!(msg.contains("^^^") || msg.contains("^"));
    }

    #[test]
    fn context_before_line() {
        let src = "package main\nfun main() {\n  bad\n}\n";
        let start = src.find("bad").unwrap() as u32;
        let msg = format_error_with(
            "t.aura",
            src,
            "undefined name `bad`",
            Span::new(start, start + 3),
            &FormatOptions {
                notes: &[],
                context_before: true,
            },
        );
        assert!(msg.contains("fun main() {"), "context line missing:\n{msg}");
        assert!(msg.contains("bad"));
    }

    #[test]
    fn auto_notes_expected_found() {
        let src = "package main\nfun main() {\n  val x: Int = \"hi\"\n}\n";
        let start = src.find("\"hi\"").unwrap() as u32;
        let msg = format_error(
            "t.aura",
            src,
            "type mismatch initializing `x`: expected Int, found String",
            Span::new(start, start + 4),
        );
        assert!(msg.contains("= note: expected type `Int`"), "{msg}");
        assert!(msg.contains("= note: found type `String`"), "{msg}");
    }

    #[test]
    fn explicit_notes() {
        let src = "x\n";
        let msg = format_error_with(
            "t.aura",
            src,
            "boom",
            Span::new(0, 1),
            &FormatOptions {
                notes: &[String::from("help: try something else")],
                context_before: false,
            },
        );
        assert!(msg.contains("= help: try something else"), "{msg}");
    }
}
