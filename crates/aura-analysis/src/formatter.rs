//! Deterministic MVP formatter for one Aura source file.
//!
//! The parser is used as the syntax gate.  The small trivia-preserving scanner
//! below is intentional: Aura's AST does not retain comments yet.

use crate::parse_file;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Word,
    String,
    Comment,
    LineBreak,
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
pub fn format_path(path: &Path, check: bool) -> Result<Vec<PathBuf>, String> {
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
    let mut changed = Vec::new();
    for file in &files {
        let source = fs::read_to_string(file)
            .map_err(|error| format!("error: cannot read {}: {error}", file.display()))?;
        let output = format_source(&source)
            .map_err(|error| format!("error: cannot format {}: {error}", file.display()))?;
        if output != source {
            changed.push(file.display().to_string());
        }
        formatted.push((file, output));
    }
    if check {
        if changed.is_empty() {
            return Ok(files);
        }
        return Err(format!(
            "error: files are not formatted:\n{}",
            changed
                .iter()
                .map(|file| format!("  {file}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
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
                } else if newlines == 1 {
                    out.push(Token {
                        text: String::new(),
                        kind: Kind::LineBreak,
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
    let mut generic_depth = 0usize;
    let mut class_header = false;
    let mut previous_was_unary_minus = false;
    let mut previous_was_generic_open = false;
    let mut inline_blocks = Vec::new();
    let mut paren_continuation_columns = Vec::new();
    let mut initializer_parens = Vec::new();
    let mut pending_continuation_column = None;
    let mut chain_continuation_column = None;
    for (index, token) in tokens.iter().enumerate() {
        let t = token.text.as_str();
        let after_unary_minus = previous_was_unary_minus;
        previous_was_unary_minus = false;
        let after_generic_open = previous_was_generic_open;
        previous_was_generic_open = false;
        if token.kind == Kind::LineBreak {
            let next_is_chain = tokens
                .get(index + 1)
                .is_some_and(|next| matches!(next.text.as_str(), "." | "?."));
            if !line_start {
                if next_is_chain && chain_continuation_column.is_none() {
                    // Align a named receiver with its token; calls keep the chain column.
                    let named_receiver = index
                        .checked_sub(1)
                        .and_then(|receiver| tokens.get(receiver))
                        .is_some_and(|token| {
                            token.kind == Kind::Word
                                && !matches!(token.text.as_str(), "return" | "throw")
                        });
                    chain_continuation_column = if named_receiver {
                        Some(current_line_len(&out))
                    } else {
                        Some(indent * 4 + 2)
                    };
                }
                newline(&mut out);
            }
            line_start = true;
            if next_is_chain {
                pending_continuation_column = chain_continuation_column;
            } else if previous == Some(",") {
                chain_continuation_column = None;
                pending_continuation_column = if initializer_parens.last().copied() == Some(true) {
                    Some(indent * 4 + 4)
                } else {
                    paren_continuation_columns.last().copied()
                };
            } else if previous == Some("(") {
                chain_continuation_column = None;
                // Constructor initializer arguments use the surrounding block indent.
                let opener_is_initializer = index >= 2
                    && tokens[index - 2].kind == Kind::Word
                    && tokens[index - 2].text == "this";
                if opener_is_initializer {
                    pending_continuation_column = Some(indent * 4 + 4);
                } else {
                    pending_continuation_column = paren_continuation_columns.last().copied();
                }
            } else {
                chain_continuation_column = None;
            }
            if !matches!(
                previous,
                Some(
                    "=" | ":"
                        | ","
                        | "+"
                        | "-"
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
            ) {
                previous = None;
            }
            continue;
        }
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
            if !line_start && t.starts_with("//") {
                out.push(' ');
            } else if !line_start {
                newline(&mut out);
                write_indent(&mut out, indent);
            } else {
                write_indent(&mut out, indent);
            }
            out.push_str(t);
            newline(&mut out);
            line_start = true;
            previous = None;
            continue;
        }
        let inside_inline_block = inline_blocks.last().copied().unwrap_or(false);
        if !inside_inline_block && should_start_line(indent, line_start, previous, t, token.kind) {
            newline(&mut out);
            line_start = true;
            previous = None;
        }
        if t == "}" {
            let inline = inline_blocks.pop().unwrap_or(false);
            if inline {
                trim_space(&mut out);
                if previous != Some("{") {
                    out.push(' ');
                }
                out.push('}');
                line_start = false;
                previous = Some("}");
                continue;
            }
            if !line_start {
                newline(&mut out);
            }
            indent = indent.saturating_sub(1);
            write_indent(&mut out, indent);
            out.push('}');
            let joins_clause = tokens.get(index + 1).is_some_and(|next| {
                next.kind == Kind::Word
                    && matches!(next.text.as_str(), "else" | "catch" | "finally")
            });
            if joins_clause {
                line_start = false;
                previous = Some("}");
                continue;
            }
            newline(&mut out);
            line_start = true;
            previous = None;
            continue;
        }
        if line_start {
            if t == ")" {
                pending_continuation_column = None;
                write_indent(&mut out, indent);
            } else if let Some(column) = pending_continuation_column.take() {
                out.push_str(&" ".repeat(column));
                previous = None;
            } else {
                write_indent(&mut out, indent);
            }
            line_start = false;
        }
        if t == "class" {
            class_header = true;
        }
        if t == "{" {
            let inline = is_inline_block(tokens, index);
            inline_blocks.push(inline);
            trim_space(&mut out);
            out.push_str(" {");
            if inline {
                if tokens.get(index + 1).is_some_and(|next| next.text != "}") {
                    out.push(' ');
                }
                line_start = false;
            } else {
                newline(&mut out);
                indent += 1;
                line_start = true;
            }
            class_header = false;
        } else if t == "," {
            trim_space(&mut out);
            out.push_str(", ");
        } else if t == ":" {
            if class_header && previous == Some(")") {
                trim_space(&mut out);
                out.push_str(" : ");
            } else {
                trim_space(&mut out);
                out.push_str(": ");
            }
        } else if t == "." || t == "?." || t == "!!" || t == ")" || t == "]" {
            if t == "."
                && previous
                    .is_some_and(|value| value != "return" && value != "throw" && value != ")")
            {
                chain_continuation_column = Some(current_line_len(&out));
            }
            if previous.is_some() {
                trim_space(&mut out);
            }
            out.push_str(t);
        } else if t == "(" || t == "[" {
            let keep_space = matches!(
                previous,
                Some(
                    "=" | ":"
                        | ","
                        | "+"
                        | "-"
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
                        | "return"
                )
            );
            if !keep_space && previous.is_some() {
                trim_space(&mut out);
            } else if previous == Some("return") {
                out.push(' ');
            }
            if t == "("
                && matches!(
                    previous,
                    Some("if" | "else" | "while" | "for" | "match" | "catch")
                )
            {
                out.push(' ');
            }
            out.push_str(t);
            if t == "(" {
                // Align manually broken parameters with the first parameter.
                paren_continuation_columns.push(current_line_len(&out));
                initializer_parens.push(previous == Some("this"));
            }
        } else if t == "-" && is_unary_minus(previous) {
            trim_space(&mut out);
            if previous.is_some_and(|p| !matches!(p, "(" | "[")) {
                out.push(' ');
            }
            out.push('-');
            previous_was_unary_minus = true;
        } else if t == "!" {
            trim_space(&mut out);
            if previous.is_some_and(|p| !matches!(p, "(" | "[")) {
                out.push(' ');
            }
            out.push('!');
        } else if t == "<" && angle_is_generic(tokens, index) {
            trim_space(&mut out);
            out.push('<');
            generic_depth += 1;
            previous_was_generic_open = true;
        } else if t == ">" && generic_depth > 0 {
            trim_space(&mut out);
            out.push('>');
            generic_depth -= 1;
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
        } else if t == ";" {
            newline(&mut out);
            line_start = true;
            previous = None;
            continue;
        } else if !after_generic_open && !after_unary_minus && needs_space(previous, token.kind) {
            trim_space(&mut out);
            out.push(' ');
            out.push_str(t);
        } else {
            out.push_str(t);
        }
        if t == ")" {
            paren_continuation_columns.pop();
            initializer_parens.pop();
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
        && previous
            .is_some_and(|p| !matches!(p, "@" | "." | "?." | "!" | ".." | "..=" | "{" | "(" | "["))
}

fn is_unary_minus(previous: Option<&str>) -> bool {
    previous.is_none()
        || previous.is_some_and(|p| {
            matches!(
                p,
                "=" | ":"
                    | ","
                    | "("
                    | "["
                    | "+"
                    | "-"
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
                    | "return"
                    | "throw"
            )
        })
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
                && !matches!(previous, Some("pub" | "async" | "test"))
                && !(text == "fun" && previous.is_some_and(|p| p.starts_with('"')))))
    {
        return true;
    }

    indent > 0
        && matches!(text, "val" | "var" | "if" | "return" | "throw")
        && !matches!(previous, Some("else" | "=" | ":" | "," | "=>" | "->"))
}

fn angle_is_generic(tokens: &[Token], index: usize) -> bool {
    if index == 0 || !matches!(tokens[index - 1].kind, Kind::Word | Kind::Punct) {
        return false;
    }
    let mut depth = 0usize;
    for token in &tokens[index + 1..] {
        match token.text.as_str() {
            "<" => depth += 1,
            ">" if depth == 0 => return true,
            ">" => depth -= 1,
            "=" | "==" | "!=" | "<=" | ">=" | "&&" | "||" | "+" | "-" | "*" | "/" | "?:" => {
                return false
            }
            _ => {}
        }
    }
    false
}

fn is_inline_block(tokens: &[Token], index: usize) -> bool {
    let mut depth = 0usize;
    for token in &tokens[index..] {
        if token.kind == Kind::LineBreak || token.kind == Kind::BlankLine {
            return false;
        }
        match token.text.as_str() {
            "{" => depth += 1,
            "}" => {
                depth -= 1;
                if depth == 0 {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
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

fn current_line_len(out: &str) -> usize {
    out.len() - out.rfind('\n').map_or(0, |index| index + 1)
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
        let formatted = format_source(&source).unwrap();

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
    fn preserves_modifiers_statement_lines_comments_and_generic_spacing() {
        let source = r#"package demo

pub fun identity<T>(value: T): T {
  // Keep this comment indented.
  return value
}

fun mapped(value: Int): Int {
  return map<Int, Int>(value, (x: Int) => x)
}

async fun run() {
  first()
  second()
}
"#;
        let formatted = format_source(source).unwrap();
        let expected = r#"package demo

pub fun identity<T>(value: T): T {
    // Keep this comment indented.
    return value
}

fun mapped(value: Int): Int {
    return map<Int, Int>(value, (x: Int) => x)
}

async fun run() {
    first()
    second()
}
"#;
        assert_eq!(formatted, expected);
        assert_eq!(formatted, format_source(&formatted).unwrap());
    }

    #[test]
    fn formats_safe_calls_unary_ops_if_expressions_and_inline_comments() {
        let source = r#"package demo

class Range3(val n: Int) : Iterable {
  fun len(): Int { return n }
}

fun sample(x: Int, g: Greeter, flag: Bool, b: Box): String {
  val a: String? = g?.greet()
  val negative: Int = -1
  val difference: Int = x - 1
  val grouped: Bool = flag || (x == 1)
  val s: String = if (x == 2) {
    "two"
  } else {
    "other"
  }
  val r1 = twice(3) // 5
  return !flag
  return () => b.get()
  return a + s
}

fun borrowed(task: Task<ref String>) {}
"#;
        let formatted = format_source(source).unwrap();
        let expected = r#"package demo

class Range3(val n: Int) : Iterable {
    fun len(): Int { return n }
}

fun sample(x: Int, g: Greeter, flag: Bool, b: Box): String {
    val a: String? = g?.greet()
    val negative: Int = -1
    val difference: Int = x - 1
    val grouped: Bool = flag || (x == 1)
    val s: String = if (x == 2) {
        "two"
    } else {
        "other"
    }
    val r1 = twice(3) // 5
    return !flag
    return () => b.get()
    return a + s
}

fun borrowed(task: Task<ref String>) {}
"#;
        assert_eq!(formatted, expected);
        assert_eq!(formatted, format_source(&formatted).unwrap());
    }

    #[test]
    fn aligns_broken_parameters_and_class_fields_without_wrapping() {
        let source = r#"package demo

class Context(pub val request: Request, pub val response: Response,
 pub val params: Array<Param>, private val bodyLimit: Int) {
    companion object {
        pub fun empty(req: Request, res: Response,
                      p: Array<Param> = Array<Param>(0),
                      limit: Int = 1048576): Context {
            return Context(req, res, p, limit)
        }
    }
}

fun short(a: Int, b: Int, c: Int): Int { return a }
"#;
        let formatted = format_source(source).unwrap();
        let expected = r#"package demo

class Context(pub val request: Request, pub val response: Response,
              pub val params: Array<Param>, private val bodyLimit: Int) {
    companion object {
        pub fun empty(req: Request, res: Response,
                      p: Array<Param> = Array<Param>(0),
                      limit: Int = 1048576): Context {
            return Context(req, res, p, limit)
        }
    }
}

fun short(a: Int, b: Int, c: Int): Int { return a }
"#;
        assert_eq!(formatted, expected);
        assert_eq!(formatted, format_source(&formatted).unwrap());
    }

    #[test]
    fn preserves_router_style_class_and_constructor_layout() {
        let source = r#"package demo

pub class Router(
                 private val routes: Array<Route>, private val middlewares: Array<Middleware>,
                 private val mounted: Array<MountedMiddleware>, private val bodyLimit: Int,
                 private val matcher: RouteMatcher) {
    constructor(bodyLimit: Int = 1048576): this(
        Array<Route>(0), Array<Middleware>(0), Array<MountedMiddleware>(0),
        bodyLimit,
        DefaultRouteMatcher(),
        (error: String, context: Context) => emptyErrorHandler(error, context),
        false,
        Array<Middleware>(0),
        Array<Middleware>(0),
        Array<Middleware>(0), Array<Middleware>(0), Array<Middleware>(0)
    ) {}
}
"#;
        let formatted = format_source(source).unwrap();
        assert_eq!(formatted, source);
        assert_eq!(formatted, format_source(&formatted).unwrap());
    }

    #[test]
    fn preserves_method_chain_continuation_indent() {
        let source = format!(
            "package demo\nfun main() {{\n    app()\n      .a()\n      .b()\n    return this.a()\n{}.b()\n    return this\n{}.a()\n{}.b()\n}}\n",
            " ".repeat(15),
            " ".repeat(15),
            " ".repeat(15)
        );
        let formatted = format_source(&source).unwrap();
        assert_eq!(formatted, source);
        assert_eq!(formatted, format_source(&formatted).unwrap());
    }

    #[test]
    fn formats_unary_not_ranges_and_else_clauses() {
        let source = r#"package demo
fun main() {
  assert(!nb.contains("buy milk"))
  for (i in 0..again.len()) {
    if (i == 0) {
      return
    } else if (start == n) {
      return
    }
  }
}
"#;
        let formatted = format_source(source).unwrap();
        assert!(formatted.contains("assert(!nb.contains(\"buy milk\"))\n"));
        assert!(formatted.contains("for (i in 0..again.len()) {\n"));
        assert!(formatted.contains("    } else if (start == n) {\n"));
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

        let files = format_path(&root.join("aura.toml"), false).unwrap();

        assert_eq!(files.len(), 2);
        assert!(fs::read_to_string(src.join("main.aura"))
            .unwrap()
            .contains("fun main() { println(\"ok\") }\n"));
        assert!(fs::read_to_string(src.join("util.aura"))
            .unwrap()
            .contains("fun util() { return }\n"));
        assert!(format_path(&root.join("aura.toml"), true).is_ok());
        fs::write(src.join("util.aura"), "package demo\nfun util(){return}\n").unwrap();
        let error = format_path(&root.join("aura.toml"), true).unwrap_err();
        assert!(error.contains("util.aura"));
        fs::remove_dir_all(root).unwrap();
    }
}
