use aura_span::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub span: Span,
    pub message: String,
    pub help: Option<String>,
    pub note: Option<String>,
}

impl Diagnostic {
    pub fn error(span: Span, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            span,
            message: message.into(),
            help: None,
            note: None,
        }
    }

    pub fn warning(span: Span, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            span,
            message: message.into(),
            help: None,
            note: None,
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

pub fn format(source: &str, diag: &Diagnostic) -> String {
    let (label, kind) = match diag.severity {
        Severity::Error => ("error", "E"),
        Severity::Warning => ("warning", "W"),
    };

    let start = diag.span.start.raw() as usize;
    let end = diag.span.end.raw() as usize;
    let (line_no, col, line_text) = line_at(source, start);
    let caret_len = (end.saturating_sub(start)).max(1);

    let mut out = String::new();
    out.push_str(&format!("{label}[{kind}]: {}\n", diag.message));
    out.push_str(&format!(" --> line {line_no}, col {col}\n"));
    out.push_str("  |\n");
    out.push_str(&format!("{:>2} | {}\n", line_no, line_text));
    out.push_str("  | ");
    out.push_str(&" ".repeat(col.saturating_sub(1)));
    out.push_str(&"^".repeat(caret_len));
    out.push('\n');
    if let Some(help) = &diag.help {
        out.push_str(&format!("help: {help}\n"));
    }
    if let Some(note) = &diag.note {
        out.push_str(&format!("note: {note}\n"));
    }
    out
}

pub fn format_all(source: &str, diags: &[Diagnostic]) -> String {
    let mut out = String::new();
    for (i, d) in diags.iter().enumerate() {
        if i != 0 {
            out.push('\n');
        }
        out.push_str(&format(source, d));
    }
    out
}

fn line_at(source: &str, byte_idx: usize) -> (usize, usize, String) {
    let byte_idx = byte_idx.min(source.len());
    let before = &source[..byte_idx];
    let line_no = before.bytes().filter(|&b| b == b'\n').count() + 1;
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = source[byte_idx..]
        .find('\n')
        .map(|i| byte_idx + i)
        .unwrap_or(source.len());
    let line = source[line_start..line_end].to_string();
    let col = (byte_idx.saturating_sub(line_start)) + 1;
    (line_no, col, line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_span::{BytePos, Span};

    #[test]
    fn formats_single_line_span() {
        let src = "let x: i32 = 1 + 2;\n";
        let diag = Diagnostic::error(
            Span::new(BytePos::new(4), BytePos::new(5)),
            "expected identifier",
        );
        let rendered = format(src, &diag);
        assert!(rendered.contains("error[E]: expected identifier"));
        assert!(rendered.contains("line 1, col 5"));
        assert!(rendered.contains("^"));
    }
}
