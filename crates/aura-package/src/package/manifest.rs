//! Minimal, comment-preserving edits for the dependency table in `aura.toml`.

use std::fs;
use std::path::Path;

pub fn add_dependency(manifest: &Path, name: &str, value: &str) -> Result<(), String> {
    validate_name(name)?;
    let text = fs::read_to_string(manifest)
        .map_err(|e| format!("error: read {}: {e}", manifest.display()))?;
    let mut lines = text.lines().map(str::to_string).collect::<Vec<_>>();
    let (start, end) = dependency_section(&lines);
    if let Some((start, end)) = start.zip(end) {
        if lines[start..end]
            .iter()
            .any(|line| dependency_name(line) == Some(name))
        {
            return Err(format!("error: dependency `{name}` already exists"));
        }
        if let Some(origin) = quoted_field(value, "git") {
            if let Some(existing) = lines[start..end]
                .iter()
                .find(|line| quoted_field(line, "git") == Some(origin))
                .and_then(|line| dependency_name(line))
            {
                return Err(format!(
                    "error: dependency origin `{origin}` conflicts with existing dependency `{existing}`"
                ));
            }
        }
        let mut insert_at = end;
        while insert_at > start && lines[insert_at - 1].trim().is_empty() {
            insert_at -= 1;
        }
        lines.insert(insert_at, format!("{name} = {value}"));
    } else {
        if !lines.is_empty() && !lines.last().is_some_and(String::is_empty) {
            lines.push(String::new());
        }
        lines.push("[dependencies]".into());
        lines.push(format!("{name} = {value}"));
    }
    write_lines(manifest, &lines, text.ends_with('\n'))
}

pub fn remove_dependency(manifest: &Path, name: &str) -> Result<(), String> {
    validate_name(name)?;
    let text = fs::read_to_string(manifest)
        .map_err(|e| format!("error: read {}: {e}", manifest.display()))?;
    let lines = text.lines().map(str::to_string).collect::<Vec<_>>();
    let (start, end) = dependency_section(&lines);
    let Some((start, end)) = start.zip(end) else {
        return Err(format!("error: dependency `{name}` is not declared"));
    };
    let Some(index) = (start..end).find(|&index| dependency_name(&lines[index]) == Some(name))
    else {
        return Err(format!("error: dependency `{name}` is not declared"));
    };
    let mut updated = lines;
    updated.remove(index);
    write_lines(manifest, &updated, text.ends_with('\n'))
}

fn dependency_section(lines: &[String]) -> (Option<usize>, Option<usize>) {
    let Some(start) = lines
        .iter()
        .position(|line| line.trim() == "[dependencies]")
    else {
        return (None, None);
    };
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, line)| line.trim_start().starts_with('['))
        .map(|(index, _)| index)
        .unwrap_or(lines.len());
    (Some(start + 1), Some(end))
}

fn dependency_name(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let (name, _) = trimmed.split_once('=')?;
    let name = name.trim();
    validate_name(name).ok().map(|_| name)
}

fn quoted_field<'a>(line: &'a str, field: &str) -> Option<&'a str> {
    let marker = format!("{field} = \"");
    let start = line.find(&marker)? + marker.len();
    let end = line[start..].find('"')? + start;
    Some(&line[start..end])
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(format!("error: invalid dependency name `{name}`"));
    }
    Ok(())
}

fn write_lines(manifest: &Path, lines: &[String], trailing_newline: bool) -> Result<(), String> {
    let mut output = lines.join("\n");
    if trailing_newline {
        output.push('\n');
    }
    fs::write(manifest, output).map_err(|e| format!("error: write {}: {e}", manifest.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(contents: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "aura-manifest-edit-{}-{}.toml",
            std::process::id(),
            contents.len()
        ));
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn adds_dependency_without_rewriting_other_sections() {
        let path = fixture("[package]\nname = \"app\"\n\n[dependencies]\nold = \"1.0\"\n\n[profile.dev]\ndebug = true\n");
        add_dependency(
            &path,
            "new.pkg",
            "{ git = \"https://example.com/new.git\", tag = \"v1.0.0\" }",
        )
        .unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("old = \"1.0\"\nnew.pkg = { git"));
        assert!(text.contains("[profile.dev]\ndebug = true"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn adds_dependency_table_when_missing() {
        let path = fixture("[package]\nname = \"app\"\n");
        add_dependency(&path, "new", "\"1.0\"").unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[package]\nname = \"app\"\n\n[dependencies]\nnew = \"1.0\"\n"
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn removes_only_the_named_dependency() {
        let path = fixture("[dependencies]\nold = \"1.0\"\nkeep = \"2.0\"\n");
        remove_dependency(&path, "old").unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[dependencies]\nkeep = \"2.0\"\n"
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn rejects_duplicate_origin_under_another_name() {
        let path = fixture("[dependencies]\nfirst = { git = \"https://example.com/pkg\" }\n");
        let error =
            add_dependency(&path, "second", "{ git = \"https://example.com/pkg\" }").unwrap_err();
        assert!(error.contains("conflicts with existing dependency `first`"));
        let _ = fs::remove_file(path);
    }
}
