pub mod completion;
pub mod definition;
pub mod diagnostic;
pub mod formatting;
pub mod hover;
pub mod symbol;

pub fn format_doc_comment(doc: &str) -> String {
    let mut lines: Vec<String> = doc
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with('*') {
                let content = &trimmed[1..];
                if content.starts_with(' ') {
                    &content[1..]
                } else {
                    content
                }
                .trim_end()
            } else {
                line.trim_end()
            }
            .to_string()
        })
        .collect();

    // Trim leading/trailing empty lines
    while lines.first().map_or(false, |s| s.is_empty()) {
        lines.remove(0);
    }
    while lines.last().map_or(false, |s| s.is_empty()) {
        lines.pop();
    }

    lines.join("  \n")
}
