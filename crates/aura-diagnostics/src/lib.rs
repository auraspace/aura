//! Line/column mapping and pretty diagnostics.

use aura_ast::{BytePos, Span};

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
            return LineCol {
                line,
                column: col,
            };
        }
        if *b == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    LineCol {
        line,
        column: col,
    }
}

/// Format `path:line:col` for the start of a span.
pub fn location_label(path: &str, src: &str, span: Span) -> String {
    let lc = offset_to_line_col(src, span.start);
    format!("{path}:{}:{}", lc.line, lc.column)
}

/// Pretty multi-line diagnostic:
/// ```text
/// error: message
///   --> path:line:col
///    |
///  N |  source line
///    |  ^^^
/// ```
pub fn format_error(path: &str, src: &str, message: &str, span: Span) -> String {
    let start = offset_to_line_col(src, span.start);
    let end = offset_to_line_col(src, span.end.max(span.start));
    let line_idx = (start.line as usize).saturating_sub(1);
    let line_text = src.lines().nth(line_idx).unwrap_or("");
    let col = start.column as usize;
    let mark_len = if start.line == end.line {
        let n = (span.end.saturating_sub(span.start)) as usize;
        n.max(1)
    } else {
        1
    };
    // columns are 1-based; underline starts at col-1
    let pad = " ".repeat(col.saturating_sub(1));
    let carets = "^".repeat(mark_len.min(80).max(1));
    let line_no = start.line;
    let gutter = format!("{line_no}");
    let gw = gutter.len().max(2);

    format!(
        "error: {message}\n  --> {path}:{line_no}:{col}\n{blank:>gw$} |\n{line_no:>gw$} | {line_text}\n{blank:>gw$} | {pad}{carets}",
        blank = "",
        col = start.column,
    )
}

/// Convenience when only message + span are available (no snippet).
pub fn format_short(path: &str, src: &str, message: &str, span: Span) -> String {
    format!(
        "{}: {}",
        location_label(path, src, span),
        message
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_ast::Span;

    #[test]
    fn first_line() {
        let src = "fun main() {\n}\n";
        assert_eq!(
            offset_to_line_col(src, 0),
            LineCol {
                line: 1,
                column: 1
            }
        );
        assert_eq!(
            offset_to_line_col(src, 4),
            LineCol {
                line: 1,
                column: 5
            }
        );
    }

    #[test]
    fn second_line() {
        let src = "package main\nfun x() {}\n";
        // 'f' of fun is after "package main\n"
        let off = "package main\n".len() as u32;
        assert_eq!(
            offset_to_line_col(src, off),
            LineCol {
                line: 2,
                column: 1
            }
        );
    }

    #[test]
    fn pretty_contains_arrow() {
        let src = "package main\nfun main() {\n  bad\n}\n";
        // point at `bad`
        let start = src.find("bad").unwrap() as u32;
        let msg = format_error("t.aura", src, "undefined name `bad`", Span::new(start, start + 3));
        assert!(msg.contains("--> t.aura:"));
        assert!(msg.contains("bad"));
        assert!(msg.contains("^^^") || msg.contains("^"));
    }
}
