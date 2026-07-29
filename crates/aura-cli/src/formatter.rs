//! Deterministic MVP formatter for one Aura source file.
//!
//! The parser is used as the syntax gate.  The small trivia-preserving scanner
//! below is intentional: Aura's AST does not retain comments yet.

use aura_analysis::parse_file;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Word,
    String,
    Comment,
    BlankLine,
    Punct,
}

struct Token {
    text: String,
    kind: Kind,
}

pub fn format_source(source: &str) -> Result<String, String> {
    parse_file(source).map_err(|e| e.to_string())?;
    let tokens = scan(source)?;
    Ok(render(&tokens))
}

/// Format one source file, a project manifest, or every source below a directory.
pub fn format_path(path: &Path) -> Result<Vec<PathBuf>, String> {
    let root =
        if path.is_file() && path.file_name().and_then(|name| name.to_str()) == Some("aura.toml") {
            path.parent().unwrap_or_else(|| Path::new("."))
        } else {
            path
        };
    let files = collect_source_files(root)?;
    if files.is_empty() {
        return Err(format!(
            "error: no `.aura` files found under {}",
            root.display()
        ));
    }

    // Format everything before writing so one invalid file cannot leave a partial result.
    let mut formatted = Vec::with_capacity(files.len());
    for file in &files {
        let source = fs::read_to_string(file)
            .map_err(|error| format!("error: cannot read {}: {error}", file.display()))?;
        let output = format_source(&source)
            .map_err(|error| format!("error: cannot format {}: {error}", file.display()))?;
        formatted.push((file, output));
    }
    for (file, output) in formatted {
        fs::write(file, output)
            .map_err(|error| format!("error: cannot write {}: {error}", file.display()))?;
    }
    Ok(files)
}

fn collect_source_files(path: &Path) -> Result<Vec<PathBuf>, String> {
    if path.is_file() {
        if path.extension().and_then(|ext| ext.to_str()) != Some("aura") {
            return Err(format!(
                "error: {}: expected `.aura` file, directory, or `aura.toml`",
                path.display()
            ));
        }
        return Ok(vec![path.to_path_buf()]);
    }
    if !path.is_dir() {
        return Err(format!("error: path not found: {}", path.display()));
    }

    let mut files = Vec::new();
    collect_source_files_rec(path, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_source_files_rec(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|error| format!("error: cannot read dir {}: {error}", dir.display()))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("error: cannot read dir {}: {error}", dir.display()))?;
        let path = entry.path();
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') || name == "target" {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|error| format!("error: cannot inspect {}: {error}", path.display()))?;
        if file_type.is_dir() {
            collect_source_files_rec(&path, files)?;
        } else if file_type.is_file()
            && path.extension().and_then(|ext| ext.to_str()) == Some("aura")
        {
            files.push(path);
        }
    }
    Ok(())
}

fn scan(source: &str) -> Result<Vec<Token>, String> {
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\r' | b'\n' => {
                let mut newlines = 0;
                while i < bytes.len() {
                    match bytes[i] {
                        b' ' | b'\t' | b'\r' => i += 1,
                        b'\n' => {
                            newlines += 1;
                            i += 1;
                        }
                        _ => break,
                    }
                }
                if newlines >= 2 {
                    out.push(Token {
                        text: String::new(),
                        kind: Kind::BlankLine,
                    });
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                let start = i;
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                out.push(Token {
                    text: source[start..i].trim_end().into(),
                    kind: Kind::Comment,
                });
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                let start = i;
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                if i + 1 >= bytes.len() {
                    return Err("unterminated block comment".into());
                }
                i += 2;
                out.push(Token {
                    text: source[start..i].into(),
                    kind: Kind::Comment,
                });
            }
            b'"' => {
                let start = i;
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                if i > bytes.len() || !source[start..i].ends_with('"') {
                    return Err("unterminated string".into());
                }
                out.push(Token {
                    text: source[start..i].into(),
                    kind: Kind::String,
                });
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'0'..=b'9' => {
                let start = i;
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                out.push(Token {
                    text: source[start..i].into(),
                    kind: Kind::Word,
                });
            }
            _ => {
                let start = i;
                if i + 2 <= bytes.len()
                    && matches!(
                        &source[i..i + 2],
                        "==" | "!="
                            | "<="
                            | ">="
                            | "&&"
                            | "||"
                            | "=>"
                            | "->"
                            | "!!"
                            | ".."
                            | "?:"
                            | "?."
                    )
                {
                    i += 2;
                    if &source[start..i] == ".." && bytes.get(i) == Some(&b'=') {
                        i += 1;
                    }
                } else {
                    i += 1;
                }
                out.push(Token {
                    text: source[start..i].into(),
                    kind: Kind::Punct,
                });
            }
        }
    }
    Ok(out)
}

fn render(tokens: &[Token]) -> String {
    let mut out = String::new();
    let mut indent = 0usize;
    let mut line_start = true;
    let mut previous: Option<&str> = None;
    for token in tokens {
        let t = token.text.as_str();
        if token.kind == Kind::BlankLine {
            if !out.is_empty() && !out.ends_with("\n") {
                newline(&mut out);
            }
            if !out.ends_with("\n\n") {
                out.push('\n');
            }
            line_start = true;
            previous = None;
            continue;
        }
        if token.kind == Kind::Comment {
            if !line_start {
                newline(&mut out);
            }
            out.push_str(t);
            newline(&mut out);
            line_start = true;
            previous = None;
            continue;
        }
        if should_start_line(indent, line_start, previous, t, token.kind) {
            newline(&mut out);
            line_start = true;
            previous = None;
        }
        if t == "}" {
            if !line_start {
                newline(&mut out);
            }
            indent = indent.saturating_sub(1);
            write_indent(&mut out, indent);
            out.push('}');
            newline(&mut out);
            line_start = true;
            previous = None;
            continue;
        }
        if line_start {
            write_indent(&mut out, indent);
            line_start = false;
        }
        if t == "{" {
            trim_space(&mut out);
            out.push_str(" {");
            newline(&mut out);
            indent += 1;
            line_start = true;
        } else if t == "," {
            trim_space(&mut out);
            out.push_str(", ");
        } else if t == ":" {
            trim_space(&mut out);
            out.push_str(": ");
        } else if t == "." || t == "?." || t == "!!" || t == ")" || t == "]" {
            trim_space(&mut out);
            out.push_str(t);
        } else if t == "(" || t == "[" {
            trim_space(&mut out);
            if t == "("
                && matches!(
                    previous,
                    Some("if" | "else" | "while" | "for" | "match" | "catch")
                )
            {
                out.push(' ');
            }
            out.push_str(t);
        } else if t == "="
            || matches!(
                t,
                "+" | "-"
                    | "*"
                    | "/"
                    | "%"
                    | "=="
                    | "!="
                    | "<"
                    | "<="
                    | ">"
                    | ">="
                    | "&&"
                    | "||"
                    | "=>"
                    | "->"
                    | "?:"
            )
        {
            trim_space(&mut out);
            out.push(' ');
            out.push_str(t);
            out.push(' ');
        } else if needs_space(previous, token.kind) {
            trim_space(&mut out);
            out.push(' ');
            out.push_str(t);
        } else {
            out.push_str(t);
        }
        previous = Some(t);
    }
    trim_space(&mut out);
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn needs_space(previous: Option<&str>, kind: Kind) -> bool {
    matches!(kind, Kind::Word | Kind::String)
        && previous.is_some_and(|p| !matches!(p, "@" | "." | "{" | "(" | "["))
}

fn should_start_line(
    indent: usize,
    line_start: bool,
    previous: Option<&str>,
    text: &str,
    kind: Kind,
) -> bool {
    if line_start || kind != Kind::Word {
        return false;
    }

    if indent == 0
        && (matches!(text, "package" | "import" | "extern")
            || (is_decl_start(text)
                && !(text == "fun" && previous.is_some_and(|p| p.starts_with('"')))))
    {
        return true;
    }

    indent > 0
        && matches!(text, "val" | "var" | "if" | "return" | "throw")
        && previous != Some("else")
}

fn is_decl_start(text: &str) -> bool {
    matches!(
        text,
        "fun" | "class" | "struct" | "enum" | "interface" | "type" | "const"
    )
}

fn write_indent(out: &mut String, indent: usize) {
    out.push_str(&"    ".repeat(indent));
}
fn newline(out: &mut String) {
    trim_space(out);
    out.push('\n');
}
fn trim_space(out: &mut String) {
    while out.ends_with(' ') {
        out.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::{format_path, format_source};
    use std::fs;

    #[test]
    fn formats_and_is_idempotent() {
        let source = "package demo\n// keep me\nfun main(){val x:Int=1\nif(x>0){print(\"x\")} }";
        let once = format_source(source).unwrap();
        assert!(once.contains("// keep me"));
        assert_eq!(once, format_source(&once).unwrap());
    }

    #[test]
    fn formats_http_health_cli_boundaries_and_spacing() {
        let source = r#"package http_health_cli

import std.io as Io

// Bounded CLI bridge: the opaque HTTP runtime stays native while Aura owns
// the documented command entrypoint and exit status.
@foreign(library = "m", target = "native", link = "dynamic", abi = 1, abi_id = "c")
extern "C" fun aura_http_health_smoke(): Int

fun main() {
  val status: Int = aura_http_health_smoke()
  if (status != 0) {
    Io.exit(status)
  }
  println("http-health-cli: passed")
}
"#;
        let formatted = format_source(source).unwrap();

        assert!(formatted.contains("package http_health_cli\n"));
        assert!(formatted.contains("package http_health_cli\n\nimport std.io as Io\n"));
        assert!(formatted.contains("import std.io as Io\n"));
        assert!(formatted.contains("as Io\n\n// Bounded CLI bridge"));
        assert!(formatted.contains("@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 1, abi_id = \"c\")\n"));
        assert!(formatted.contains("val status: Int = aura_http_health_smoke()\n"));
        assert!(formatted.contains("if (status != 0) {\n"));
        assert!(formatted.contains("println(\"http-health-cli: passed\")\n"));
        assert_eq!(formatted, format_source(&formatted).unwrap());
    }

    #[test]
    fn formats_project_manifest_and_nested_aura_files() {
        let root =
            std::env::temp_dir().join(format!("aura-formatter-project-{}", std::process::id()));
        let src = root.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("aura.toml"), "name = \"demo\"\n").unwrap();
        fs::write(
            src.join("main.aura"),
            "package demo\nfun main(){println(\"ok\")}\n",
        )
        .unwrap();
        fs::write(src.join("util.aura"), "package demo\nfun util(){return}\n").unwrap();

        let files = format_path(&root.join("aura.toml")).unwrap();

        assert_eq!(files.len(), 2);
        assert!(fs::read_to_string(src.join("main.aura"))
            .unwrap()
            .contains("fun main() {\n"));
        assert!(fs::read_to_string(src.join("util.aura"))
            .unwrap()
            .contains("fun util() {\n"));
        fs::remove_dir_all(root).unwrap();
    }
}
