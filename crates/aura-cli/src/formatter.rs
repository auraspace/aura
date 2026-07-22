//! Deterministic MVP formatter for one Aura source file.
//!
//! The parser is used as the syntax gate.  The small trivia-preserving scanner
//! below is intentional: Aura's AST does not retain comments yet.

use aura_parser::parse_file;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Word,
    String,
    Comment,
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

fn scan(source: &str) -> Result<Vec<Token>, String> {
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\r' | b'\n' => i += 1,
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
        if token.kind == Kind::Comment {
            if !line_start {
                out.push(' ');
            }
            out.push_str(t);
            newline(&mut out);
            line_start = true;
            previous = None;
            continue;
        }
        if indent == 0 && !line_start && token.kind == Kind::Word && is_decl_start(t) {
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
            if t == "("
                && matches!(
                    previous,
                    Some("if" | "else" | "while" | "for" | "match" | "catch")
                )
            {
                out.push(' ');
            }
            trim_space(&mut out);
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
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

fn needs_space(previous: Option<&str>, kind: Kind) -> bool {
    matches!(kind, Kind::Word | Kind::String)
        && previous.is_some_and(|p| p != "@" && p != "." && p != "{")
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
    use super::format_source;

    #[test]
    fn formats_and_is_idempotent() {
        let source = "package demo\n// keep me\nfun main(){val x:Int=1\nif(x>0){print(\"x\")} }";
        let once = format_source(source).unwrap();
        assert!(once.contains("// keep me"));
        assert_eq!(once, format_source(&once).unwrap());
    }
}
