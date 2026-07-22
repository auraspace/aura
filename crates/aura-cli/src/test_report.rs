use aura_ast::Span;
use aura_diagnostics::{JsonDiagnostic, Severity};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestStatus {
    Passed,
    Failed,
    Skipped,
}

impl TestStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCase {
    pub name: String,
    pub status: TestStatus,
    pub duration_ms: u128,
    pub diagnostic: Option<JsonDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestReport {
    pub package: String,
    pub duration_ms: u128,
    pub tests: Vec<TestCase>,
}

impl TestReport {
    pub fn to_json(&self) -> String {
        let passed = self
            .tests
            .iter()
            .filter(|t| t.status == TestStatus::Passed)
            .count();
        let failed = self
            .tests
            .iter()
            .filter(|t| t.status == TestStatus::Failed)
            .count();
        let skipped = self
            .tests
            .iter()
            .filter(|t| t.status == TestStatus::Skipped)
            .count();
        let mut out = format!(
            "{{\"package\":{},\"duration_ms\":{},\"passed\":{},\"failed\":{},\"skipped\":{},\"tests\":[",
            quote(&self.package), self.duration_ms, passed, failed, skipped
        );
        for (i, test) in self.tests.iter().enumerate() {
            if i != 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"name\":{},\"status\":{},\"duration_ms\":{}",
                quote(&test.name),
                quote(test.status.as_str()),
                test.duration_ms
            ));
            if let Some(diagnostic) = &test.diagnostic {
                out.push_str(",\"diagnostic\":");
                out.push_str(&diagnostic.to_json());
            }
            out.push('}');
        }
        out.push_str("]}");
        out
    }
}

pub fn failure_diagnostic(package: &str, name: &str, message: &str) -> JsonDiagnostic {
    JsonDiagnostic::new(
        package,
        "",
        Severity::Error,
        format!("test `{name}` failed: {message}"),
        Span::new(0, 0),
    )
    .with_code("ETEST")
}

pub fn cases_from_output(
    package: &str,
    all_tests: &[String],
    selected: &[String],
    stdout: &[u8],
    stderr: &[u8],
    process_succeeded: bool,
) -> Vec<TestCase> {
    let output = format!(
        "{}\n{}",
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr)
    );
    let mut cases = Vec::with_capacity(all_tests.len());
    for name in all_tests {
        if !selected.iter().any(|selected_name| selected_name == name) {
            cases.push(TestCase {
                name: name.clone(),
                status: TestStatus::Skipped,
                duration_ms: 0,
                diagnostic: None,
            });
            continue;
        }
        let line = output.lines().find(|line| {
            let mut words = line.split_whitespace();
            words.next() == Some("test") && words.next() == Some(name.as_str())
        });
        let failed_message = line
            .and_then(|line| line.strip_prefix(&format!("test {name} ... FAILED (")))
            .and_then(|message| message.strip_suffix(')'));
        let failed = failed_message.is_some() || line.is_some_and(|line| line.contains("FAILED"));
        let status = if failed {
            TestStatus::Failed
        } else if line.is_some() || process_succeeded {
            TestStatus::Passed
        } else {
            TestStatus::Failed
        };
        let diagnostic = if status == TestStatus::Failed {
            Some(failure_diagnostic(
                package,
                name,
                failed_message.unwrap_or("test process failed"),
            ))
        } else {
            None
        };
        cases.push(TestCase {
            name: name.clone(),
            status,
            duration_ms: 0,
            diagnostic,
        });
    }
    cases
}

fn quote(value: &str) -> String {
    let mut out = String::from("\"");
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
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_has_stable_summary_and_diagnostic() {
        let report = TestReport {
            package: "demo".into(),
            duration_ms: 4,
            tests: vec![
                TestCase {
                    name: "ok".into(),
                    status: TestStatus::Passed,
                    duration_ms: 1,
                    diagnostic: None,
                },
                TestCase {
                    name: "nope".into(),
                    status: TestStatus::Failed,
                    duration_ms: 3,
                    diagnostic: Some(failure_diagnostic("demo", "nope", "assertion")),
                },
                TestCase {
                    name: "other".into(),
                    status: TestStatus::Skipped,
                    duration_ms: 0,
                    diagnostic: None,
                },
            ],
        };
        let json = report.to_json();
        assert!(json.contains("\"passed\":1"));
        assert!(json.contains("\"failed\":1"));
        assert!(json.contains("\"skipped\":1"));
        assert!(json.contains("\"code\":\"ETEST\""));
    }
}
