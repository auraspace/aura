use crate::compiler::ast::*;

mod doc;
mod expr;
mod stmt;
mod ty;

pub struct Formatter {
    pub(crate) indent_level: usize,
    pub(crate) result: String,
    pub(crate) source: Option<String>,
}

impl Formatter {
    pub fn new() -> Self {
        Self {
            indent_level: 0,
            result: String::new(),
            source: None,
        }
    }

    pub fn with_source(mut self, source: String) -> Self {
        self.source = Some(source);
        self
    }

    pub fn format_program(mut self, program: &Program) -> String {
        for (i, stmt) in program.statements.iter().enumerate() {
            if i > 0 && self.needs_blank_line(&program.statements[i - 1], stmt) {
                self.result.push('\n');
            }
            self.format_statement(stmt);
            self.result.push('\n');
        }
        self.result.trim_end().to_string() + "\n"
    }

    fn needs_blank_line(&self, prev: &Statement, next: &Statement) -> bool {
        let next_is_comment = matches!(
            next,
            Statement::Comment(_, _) | Statement::RegularBlockComment(_, _)
        );
        let prev_is_comment = matches!(
            prev,
            Statement::Comment(_, _) | Statement::RegularBlockComment(_, _)
        );

        let next_has_doc = match next {
            Statement::VarDeclaration { doc, .. } => doc.is_some(),
            Statement::FunctionDeclaration { doc, .. } => doc.is_some(),
            Statement::ClassDeclaration { doc, .. } => doc.is_some(),
            Statement::Enum(d) => d.doc.is_some(),
            Statement::Export { decl, .. } => match decl.as_ref() {
                Statement::VarDeclaration { doc, .. } => doc.is_some(),
                Statement::FunctionDeclaration { doc, .. } => doc.is_some(),
                Statement::ClassDeclaration { doc, .. } => doc.is_some(),
                Statement::Enum(d) => d.doc.is_some(),
                _ => false,
            },
            _ => false,
        };

        if (next_is_comment || next_has_doc) && !prev_is_comment {
            return true;
        }

        if let Some(source) = &self.source {
            let next_line = next.span().line;
            if next_line > 1 {
                let lines: Vec<&str> = source.lines().collect();
                if let Some(prev_line_text) = lines.get(next_line - 2) {
                    return prev_line_text.trim().is_empty();
                }
            }
        }
        false
    }

    pub(crate) fn format_statement(&mut self, stmt: &Statement) {
        stmt::format_statement(self, stmt);
    }

    pub(crate) fn format_statement_internal(&mut self, stmt: &Statement, include_doc: bool) {
        stmt::format_statement_internal(self, stmt, include_doc);
    }

    pub(crate) fn format_expr(&mut self, expr: &Expr) {
        expr::format_expr(self, expr);
    }

    pub(crate) fn format_type_expr(&mut self, ty: &TypeExpr) {
        ty::format_type_expr(self, ty);
    }

    pub(crate) fn format_doc(&mut self, doc: &Option<DocComment>) {
        doc::format_doc(self, doc);
    }

    pub(crate) fn indent(&mut self) {
        for _ in 0..self.indent_level {
            self.result.push_str("  ");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::frontend::lexer::Lexer;
    use crate::compiler::frontend::parser::Parser;

    #[test]
    fn test_formatter() {
        let source = r#"
class Test {
  static  x : number = 10;
    constructor( a:number) {
    this.x = a;
  }
}
function main ( ) : void {
  let t = new Test(5);
  print t.x;
}
"#;
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, "test.aura".to_string());
        let program = parser.parse_program();

        let formatter = Formatter::new();
        let formatted = formatter.format_program(&program);

        let expected = r#"class Test {
  public static x: number = 10;

  public constructor(a: number) {
    this.x = a;
  }
}
function main(): void {
  let t = new Test(5);
  print t.x;
}
"#;
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_doc_formatter() {
        let source = r#"
/**
 * Main function
 * with multiple lines
 */
function main(): void {}
"#;
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, "test.aura".to_string());
        let program = parser.parse_program();

        let formatter = Formatter::new();
        let formatted = formatter.format_program(&program);

        let expected =
            "/**\n * Main function\n * with multiple lines\n */\nfunction main(): void {\n}\n";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_mixed_doc_styles() {
        let source = r#"
/// Line comment
function a() {}

/**
 * Block comment
 */
function b() {}
"#;
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, "test.aura".to_string());
        let program = parser.parse_program();

        let formatter = Formatter::new();
        let formatted = formatter.format_program(&program);

        assert!(formatted.contains("/// Line comment"));
        assert!(formatted.contains("/**\n * Block comment\n */"));
    }

    #[test]
    fn test_multiple_formats_doc() {
        let source = "/// a\nlet x = 1;";

        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, "test.aura".to_string());
        let program = parser.parse_program();
        let formatter = Formatter::new();
        let formatted1 = formatter.format_program(&program);

        let mut lexer = Lexer::new(&formatted1);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, "test.aura".to_string());
        let program = parser.parse_program();
        let formatter = Formatter::new();
        let formatted2 = formatter.format_program(&program);

        assert_eq!(formatted1, formatted2);
    }

    #[test]
    fn test_export_doc_formatter() {
        let source = r#"
/**
 * X is a variable
 */
export let x = 1;
"#;
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, "test.aura".to_string());
        let program = parser.parse_program();

        let formatter = Formatter::new();
        let formatted = formatter.format_program(&program);

        let expected = "/**\n * X is a variable\n */\nexport let x = 1;\n";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_enum_formatter() {
        let source = r#"
enum Direction {
  Up = 1,
  Down
}

enum Status { Ok = "ok", Err = "err" }
"#;
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, "test.aura".to_string());
        let program = parser.parse_program();

        let formatter = Formatter::new();
        let formatted = formatter.format_program(&program);

        let expected = "enum Direction {\n  Up = 1,\n  Down,\n}\nenum Status {\n  Ok = \"ok\",\n  Err = \"err\",\n}\n";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_regular_comment_preservation() {
        let source = r#"
function main(): void {
    // test cmt
}
"#;
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, "test.aura".to_string());
        let program = parser.parse_program();

        let formatter = Formatter::new();
        let formatted = formatter.format_program(&program);

        let expected = "function main(): void {\n  // test cmt\n}\n";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_consecutive_comments_no_blank_line() {
        let source = r#"
// comment 1
// comment 2
/* block 1 */
/* block 2 */
"#;
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, "test.aura".to_string());
        let program = parser.parse_program();

        let formatter = Formatter::new().with_source(source.to_string());
        let formatted = formatter.format_program(&program);

        let expected = "// comment 1\n// comment 2\n/* block 1 */\n/* block 2 */\n";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_always_breakline_before_comment() {
        let source = r#"
let x = 10;
// comment
function f() {}
"#;
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, "test.aura".to_string());
        let program = parser.parse_program();
        let formatter = Formatter::new();
        let formatted = formatter.format_program(&program);
        let expected = "let x = 10;\n\n// comment\nfunction f(): void {\n}\n";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_export_enum_doc_formatter() {
        let source = r#"
/**
 * Color enum
 */
export enum Color { Red, Blue }
"#;
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, "test.aura".to_string());
        let program = parser.parse_program();
        let formatter = Formatter::new();
        let formatted = formatter.format_program(&program);
        let expected = "/**\n * Color enum\n */\nexport enum Color {\n  Red,\n  Blue,\n}\n";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_consecutive_imports_vars_no_blank_line() {
        let source = r#"
import { A } from "a";
import { B } from "b";
let x = 1;
let y = 2;
"#;
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, "test.aura".to_string());
        let program = parser.parse_program();
        let formatter = Formatter::new();
        let formatted = formatter.format_program(&program);
        let expected =
            "import { A } from \"a\";\nimport { B } from \"b\";\nlet x = 1;\nlet y = 2;\n";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_preserve_manual_blank_lines() {
        let source = r#"
let x = 1;

let y = 2;
"#;
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, "test.aura".to_string());
        let program = parser.parse_program();
        let formatter = Formatter::new().with_source(source.to_string());
        let formatted = formatter.format_program(&program);
        let expected = "let x = 1;\n\nlet y = 2;\n";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_class_method_doc_indentation() {
        let source = r#"
class T {
  /**
   * multi
   * line
   */
  m() {}
}
"#;
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, "test.aura".to_string());
        let program = parser.parse_program();
        let formatter = Formatter::new();
        let formatted = formatter.format_program(&program);
        let expected =
            "class T {\n  /**\n   * multi\n   * line\n   */\n  public m(): void {\n  }\n}\n";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_oop_keywords_preservation() {
        let source = r#"
interface Printable {
  print(): void;
}

class Base {
  public m(): void {
  }
}

class Test extends Base implements Printable {
  public override m(): void {
  }

  public print(): void {
  }
}
"#;
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, "test.aura".to_string());
        let program = parser.parse_program();

        let formatter = Formatter::new();
        let formatted = formatter.format_program(&program);

        assert!(formatted.contains("class Test extends Base implements Printable"));
        assert!(formatted.contains("public override m(): void"));
    }
}
