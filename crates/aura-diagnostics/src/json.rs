//! Stable JSON representation for diagnostics.

use aura_ast::Span;

use crate::offset_to_line_col;

/// Severity of a structured diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Help,
}

impl Severity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Help => "help",
        }
    }
}

/// A source location expressed in byte offsets and 1-based line/column pairs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonSpan {
    pub start: u32,
    pub end: u32,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

impl JsonSpan {
    pub fn from_source(src: &str, span: Span) -> Self {
        let start = offset_to_line_col(src, span.start);
        let end = offset_to_line_col(src, span.end.max(span.start));
        Self {
            start: span.start,
            end: span.end,
            start_line: start.line,
            start_column: start.column,
            end_line: end.line,
            end_column: end.column,
        }
    }
}

/// A diagnostic with a stable, CLI-independent JSON representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonDiagnostic {
    pub path: String,
    pub span: JsonSpan,
    pub severity: Severity,
    pub message: String,
    pub notes: Vec<String>,
    pub code: Option<String>,
}

impl JsonDiagnostic {
    pub fn new(
        path: impl Into<String>,
        src: &str,
        severity: Severity,
        message: impl Into<String>,
        span: Span,
    ) -> Self {
        Self {
            path: path.into(),
            span: JsonSpan::from_source(src, span),
            severity,
            message: message.into(),
            notes: Vec::new(),
            code: None,
        }
    }

    pub fn with_notes<I, S>(mut self, notes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.notes = notes.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Serialize with deterministic field ordering and no external JSON dependency.
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\"path\":");
        push_json_string(&mut out, &self.path);
        out.push_str(",\"span\":{");
        out.push_str(&format!(
            "\"start\":{},\"end\":{},\"start_line\":{},\"start_column\":{},\"end_line\":{},\"end_column\":{}",
            self.span.start,
            self.span.end,
            self.span.start_line,
            self.span.start_column,
            self.span.end_line,
            self.span.end_column
        ));
        out.push_str("},\"severity\":");
        push_json_string(&mut out, self.severity.as_str());
        out.push_str(",\"message\":");
        push_json_string(&mut out, &self.message);
        out.push_str(",\"notes\":[");
        for (i, note) in self.notes.iter().enumerate() {
            if i != 0 {
                out.push(',');
            }
            push_json_string(&mut out, note);
        }
        out.push(']');
        if let Some(code) = &self.code {
            out.push_str(",\"code\":");
            push_json_string(&mut out, code);
        }
        out.push('}');
        out
    }
}

fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_stable_fields_and_locations() {
        let src = "one\nsecond";
        let diagnostic = JsonDiagnostic::new(
            "src/a\"b.aura",
            src,
            Severity::Warning,
            "bad \"value\"",
            Span::new(4, 10),
        )
        .with_notes(["first", "line\nsecond"])
        .with_code("E001");
        assert_eq!(
            diagnostic.to_json(),
            r#"{"path":"src/a\"b.aura","span":{"start":4,"end":10,"start_line":2,"start_column":1,"end_line":2,"end_column":7},"severity":"warning","message":"bad \"value\"","notes":["first","line\nsecond"],"code":"E001"}"#
        );
    }

    #[test]
    fn omits_unavailable_code() {
        let diagnostic = JsonDiagnostic::new("x.aura", "x", Severity::Error, "oops", Span::new(0, 1));
        assert_eq!(diagnostic.to_json(), r#"{"path":"x.aura","span":{"start":0,"end":1,"start_line":1,"start_column":1,"end_line":1,"end_column":2},"severity":"error","message":"oops","notes":[]}"#);
    }
}
