//! Stable JSON representation for diagnostics.

use aura_ast::Span;

use crate::offset_to_line_col;

/// Stable diagnostic metadata for the C22 async/task surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncDiagnostic {
    pub code: &'static str,
    pub operation: &'static str,
    pub notes: &'static [&'static str],
}

/// Classify existing semantic-error wording without coupling diagnostics to sema internals.
pub fn classify_async(message: &str) -> Option<AsyncDiagnostic> {
    let lower = message.to_ascii_lowercase();
    let operation = if lower.contains("await") {
        "await"
    } else if lower.contains("spawn") {
        "spawn"
    } else if lower.contains("join") {
        "join"
    } else if lower.contains("cancel") {
        "cancel"
    } else if lower.contains("send") {
        "channel.send"
    } else if lower.contains("receive") || lower.contains("recv") {
        "channel.receive"
    } else if lower.contains("close") {
        "channel.close"
    } else if lower.contains("channel") {
        "channel"
    } else {
        "task"
    };

    if lower.contains("borrow") || lower.contains("reference") || lower.contains("borrowed") {
        return Some(AsyncDiagnostic {
            code: "E-BORROW-ASYNC-ESCAPE",
            operation,
            notes: &["owned values may cross async suspension and task/channel boundaries"],
        });
    }
    if lower.contains("cancel") || lower.contains("cancellation") {
        return Some(AsyncDiagnostic {
            code: "E-ASYNC-CANCEL",
            operation: "cancel",
            notes: &["cancellation is observed at the next task scheduling boundary"],
        });
    }
    if lower.contains("channel") || lower.contains("send") || lower.contains("receive") {
        return Some(AsyncDiagnostic {
            code: "E-ASYNC-CHANNEL-STATE",
            operation,
            notes: &["a channel must be open and used with its declared element type"],
        });
    }
    if lower.contains("task")
        || lower.contains("handle")
        || lower.contains("join")
        || lower.contains("spawn")
    {
        return Some(AsyncDiagnostic {
            code: "E-ASYNC-TASK-OP",
            operation,
            notes: &["task operations require a compatible Task<T> or task handle"],
        });
    }
    None
}

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
    pub operation: Option<String>,
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
            operation: None,
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

    pub fn with_operation(mut self, operation: impl Into<String>) -> Self {
        self.operation = Some(operation.into());
        self
    }

    pub fn with_async_metadata(mut self, metadata: &AsyncDiagnostic) -> Self {
        self.code = Some(metadata.code.into());
        self.operation = Some(metadata.operation.into());
        self.notes
            .extend(metadata.notes.iter().map(|note| (*note).into()));
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
        if let Some(operation) = &self.operation {
            out.push_str(",\"operation\":");
            push_json_string(&mut out, operation);
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
        let diagnostic =
            JsonDiagnostic::new("x.aura", "x", Severity::Error, "oops", Span::new(0, 1));
        assert_eq!(
            diagnostic.to_json(),
            r#"{"path":"x.aura","span":{"start":0,"end":1,"start_line":1,"start_column":1,"end_line":1,"end_column":2},"severity":"error","message":"oops","notes":[]}"#
        );
    }

    #[test]
    fn async_metadata_is_stable_and_keeps_span() {
        let src = "async fun f() { await x }";
        let metadata = classify_async("borrowed value may not cross await").unwrap();
        let diagnostic = JsonDiagnostic::new(
            "x.aura",
            src,
            Severity::Error,
            "bad await",
            Span::new(16, 21),
        )
        .with_async_metadata(&metadata);
        let json = diagnostic.to_json();
        assert!(json.contains("\"code\":\"E-BORROW-ASYNC-ESCAPE\""));
        assert!(json.contains("\"operation\":\"await\""));
        assert!(json.contains("\"start\":16,\"end\":21"));
        assert!(json.contains("owned values may cross"));
    }
}
