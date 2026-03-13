use crate::compiler::ast::*;

pub struct Formatter {
    indent_level: usize,
    result: String,
    source: Option<String>,
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
                // Check if the line immediately before the next statement is empty
                if let Some(prev_line_text) = lines.get(next_line - 2) {
                    return prev_line_text.trim().is_empty();
                }
            }
        }
        false
    }

    fn format_statement(&mut self, stmt: &Statement) {
        self.format_statement_internal(stmt, true);
    }

    fn format_statement_internal(&mut self, stmt: &Statement, include_doc: bool) {
        match stmt {
            Statement::Enum(decl) => {
                if include_doc {
                    self.format_doc(&decl.doc);
                }
                self.indent();
                self.result.push_str("enum ");
                self.result.push_str(&decl.name);
                self.result.push_str(" {\n");
                self.indent_level += 1;
                for member in &decl.members {
                    self.indent();
                    self.result.push_str(&member.name);
                    if let Some(val) = &member.value {
                        self.result.push_str(" = ");
                        self.format_expr(val);
                    }
                    self.result.push_str(",\n");
                }
                self.indent_level -= 1;
                self.indent();
                self.result.push('}');
            }
            Statement::VarDeclaration {
                name,
                ty,
                value,
                is_const,
                doc,
                ..
            } => {
                if include_doc {
                    self.format_doc(doc);
                }
                self.indent();
                if *is_const {
                    self.result.push_str("const ");
                } else {
                    self.result.push_str("let ");
                }
                self.result.push_str(name);
                if let Some(ty_expr) = ty {
                    self.result.push_str(": ");
                    self.format_type_expr(ty_expr);
                }
                self.result.push_str(" = ");
                self.format_expr(value);
                self.result.push(';');
            }
            Statement::FunctionDeclaration {
                name,
                params,
                return_ty,
                body,
                is_async,
                doc,
                ..
            } => {
                if include_doc {
                    self.format_doc(doc);
                }
                self.indent();
                if *is_async {
                    self.result.push_str("async ");
                }
                self.result.push_str("function ");
                self.result.push_str(name);
                self.result.push('(');
                for (i, (pname, pty)) in params.iter().enumerate() {
                    if i > 0 {
                        self.result.push_str(", ");
                    }
                    self.result.push_str(pname);
                    self.result.push_str(": ");
                    self.format_type_expr(pty);
                }
                self.result.push_str("): ");
                self.format_type_expr(return_ty);
                self.result.push_str(" ");
                self.format_statement_internal(body, true);
            }
            Statement::ClassDeclaration {
                name,
                fields,
                methods,
                constructor,
                doc,
                ..
            } => {
                if include_doc {
                    self.format_doc(doc);
                }
                self.indent();
                self.result.push_str("class ");
                self.result.push_str(name);
                self.result.push_str(" {\n");
                self.indent_level += 1;

                for field in fields {
                    self.format_doc(&field.doc);
                    self.indent();
                    if field.is_static {
                        self.result.push_str("static ");
                    }
                    self.result.push_str(&field.name);
                    self.result.push_str(": ");
                    self.format_type_expr(&field.ty);
                    if let Some(val) = &field.value {
                        self.result.push_str(" = ");
                        self.format_expr(val);
                    }
                    self.result.push_str(";\n");
                }

                if let Some(ctor) = constructor {
                    if !fields.is_empty() {
                        self.result.push('\n');
                    }
                    self.format_doc(&ctor.doc);
                    self.indent();
                    self.result.push_str("constructor(");
                    for (i, (pname, pty)) in ctor.params.iter().enumerate() {
                        if i > 0 {
                            self.result.push_str(", ");
                        }
                        self.result.push_str(pname);
                        self.result.push_str(": ");
                        self.format_type_expr(pty);
                    }
                    self.result.push_str(") ");
                    self.format_statement_internal(&ctor.body, true);
                    self.result.push('\n');
                }

                for (i, method) in methods.iter().enumerate() {
                    if i > 0 || constructor.is_some() || !fields.is_empty() {
                        self.result.push('\n');
                    }
                    self.format_doc(&method.doc);
                    self.indent();
                    if method.is_static {
                        self.result.push_str("static ");
                    }
                    if method.is_async {
                        self.result.push_str("async ");
                    }
                    self.result.push_str(&method.name);
                    self.result.push('(');
                    for (i, (pname, pty)) in method.params.iter().enumerate() {
                        if i > 0 {
                            self.result.push_str(", ");
                        }
                        self.result.push_str(pname);
                        self.result.push_str(": ");
                        self.format_type_expr(pty);
                    }
                    self.result.push_str("): ");
                    self.format_type_expr(&method.return_ty);
                    self.result.push_str(" ");
                    self.format_statement_internal(&method.body, true);
                    self.result.push('\n');
                }

                self.indent_level -= 1;
                self.indent();
                self.result.push('}');
            }
            Statement::Return(expr, _) => {
                self.indent();
                self.result.push_str("return ");
                self.format_expr(expr);
                self.result.push(';');
            }
            Statement::Print(expr, _) => {
                self.indent();
                self.result.push_str("print ");
                self.format_expr(expr);
                self.result.push(';');
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.indent();
                self.result.push_str("if (");
                self.format_expr(condition);
                self.result.push_str(") ");
                self.format_statement_internal(then_branch, true);
                if let Some(else_b) = else_branch {
                    self.result.push_str(" else ");
                    self.format_statement_internal(else_b, true);
                }
            }
            Statement::While {
                condition, body, ..
            } => {
                self.indent();
                self.result.push_str("while (");
                self.format_expr(condition);
                self.result.push_str(") ");
                self.format_statement_internal(body, true);
            }
            Statement::Block(stmts, _) => {
                self.result.push_str("{\n");
                self.indent_level += 1;
                for stmt in stmts {
                    self.format_statement_internal(stmt, true);
                    self.result.push('\n');
                }
                self.indent_level -= 1;
                self.indent();
                self.result.push('}');
            }
            Statement::Expression(expr, _) => {
                self.indent();
                self.format_expr(expr);
                self.result.push(';');
            }
            Statement::Import { item, path, .. } => {
                self.indent();
                self.result.push_str("import ");
                match item {
                    ImportItem::Named(names) => {
                        self.result.push_str("{ ");
                        for (i, (name, _)) in names.iter().enumerate() {
                            if i > 0 {
                                self.result.push_str(", ");
                            }
                            self.result.push_str(name);
                        }
                        self.result.push_str(" }");
                    }
                    ImportItem::Namespace((name, _)) => {
                        self.result.push_str("* as ");
                        self.result.push_str(name);
                    }
                }
                self.result.push_str(" from \"");
                self.result.push_str(path);
                self.result.push_str("\";");
            }
            Statement::Export { decl, .. } => {
                // Get doc from inner declaration
                let doc = match &**decl {
                    Statement::VarDeclaration { doc, .. } => doc,
                    Statement::FunctionDeclaration { doc, .. } => doc,
                    Statement::ClassDeclaration { doc, .. } => doc,
                    Statement::Enum(d) => &d.doc,
                    _ => &None,
                };

                // Format doc first
                self.format_doc(doc);

                self.indent();
                self.result.push_str("export ");
                // Format declaration without its doc
                let mut sub_formatter = Formatter::new();
                sub_formatter.format_statement_internal(decl, false);
                self.result.push_str(sub_formatter.result.trim_start());
            }
            Statement::TryCatch {
                try_block,
                catch_param,
                catch_block,
                finally_block,
                ..
            } => {
                self.indent();
                self.result.push_str("try ");
                self.format_statement_internal(try_block, true);
                if let Some((pname, pty)) = catch_param {
                    self.result.push_str(" catch (");
                    self.result.push_str(pname);
                    self.result.push_str(": ");
                    self.format_type_expr(pty);
                    self.result.push_str(") ");
                    if let Some(cb) = catch_block {
                        self.format_statement_internal(cb, true);
                    }
                }
                if let Some(fb) = finally_block {
                    self.result.push_str(" finally ");
                    self.format_statement_internal(fb, true);
                }
            }
            Statement::Comment(content, _) => {
                self.indent();
                self.result.push_str("//");
                self.result.push_str(content);
            }
            Statement::RegularBlockComment(content, _) => {
                self.indent();
                self.result.push_str("/*");
                self.result.push_str(content);
                self.result.push_str("*/");
            }
            Statement::Error => {
                self.indent();
                self.result.push_str("// ERROR");
            }
        }
    }

    fn format_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Number(n, _) => self.result.push_str(&n.to_string()),
            Expr::StringLiteral(s, _) => {
                self.result.push('"');
                self.result.push_str(s);
                self.result.push('"');
            }
            Expr::Variable(name, _) => self.result.push_str(name),
            Expr::BinaryOp(left, op, right, _) => {
                self.format_expr(left);
                self.result.push(' ');
                self.result.push_str(op);
                self.result.push(' ');
                self.format_expr(right);
            }
            Expr::Assign(name, val, _) => {
                self.result.push_str(name);
                self.result.push_str(" = ");
                self.format_expr(val);
            }
            Expr::Call(name, _, args, _) => {
                self.result.push_str(name);
                self.result.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.result.push_str(", ");
                    }
                    self.format_expr(arg);
                }
                self.result.push(')');
            }
            Expr::MethodCall(obj, name, _, args, _) => {
                self.format_expr(obj);
                self.result.push('.');
                self.result.push_str(name);
                self.result.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.result.push_str(", ");
                    }
                    self.format_expr(arg);
                }
                self.result.push(')');
            }
            Expr::This(_) => self.result.push_str("this"),
            Expr::New(name, _, args, _) => {
                self.result.push_str("new ");
                self.result.push_str(name);
                self.result.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.result.push_str(", ");
                    }
                    self.format_expr(arg);
                }
                self.result.push(')');
            }
            Expr::MemberAccess(obj, name, _, _) => {
                self.format_expr(obj);
                self.result.push('.');
                self.result.push_str(name);
            }
            Expr::MemberAssign(obj, name, val, _, _) => {
                self.format_expr(obj);
                self.result.push('.');
                self.result.push_str(name);
                self.result.push_str(" = ");
                self.format_expr(val);
            }
            Expr::UnaryOp(op, expr, _) => {
                self.result.push_str(op);
                self.format_expr(expr);
            }
            Expr::Throw(expr, _) => {
                self.result.push_str("throw ");
                self.format_expr(expr);
            }
            Expr::TypeTest(expr, ty, _) => {
                self.format_expr(expr);
                self.result.push_str(" is ");
                self.format_type_expr(ty);
            }
            Expr::Template(parts, _) => {
                self.result.push('`');
                for part in parts {
                    match part {
                        TemplatePart::Str(s) => self.result.push_str(s),
                        TemplatePart::Expr(e) => {
                            self.result.push_str("${");
                            self.format_expr(e);
                            self.result.push('}');
                        }
                    }
                }
                self.result.push('`');
            }
            Expr::Await(expr, _) => {
                self.result.push_str("await ");
                self.format_expr(expr);
            }
            Expr::ArrayLiteral(elements, _) => {
                self.result.push('[');
                for (i, el) in elements.iter().enumerate() {
                    if i > 0 {
                        self.result.push_str(", ");
                    }
                    self.format_expr(el);
                }
                self.result.push(']');
            }
            Expr::Index(obj, idx, _) => {
                self.format_expr(obj);
                self.result.push('[');
                self.format_expr(idx);
                self.result.push(']');
            }
            Expr::Null(_) => self.result.push_str("null"),
            Expr::Error(_) => self.result.push_str("/* ERROR */"),
        }
    }

    fn format_type_expr(&mut self, ty: &TypeExpr) {
        match ty {
            TypeExpr::Name(name, _) => self.result.push_str(name),
            TypeExpr::Union(tys, _) => {
                for (i, t) in tys.iter().enumerate() {
                    if i > 0 {
                        self.result.push_str(" | ");
                    }
                    self.format_type_expr(t);
                }
            }
            TypeExpr::Generic(name, args, _) => {
                self.result.push_str(name);
                self.result.push('<');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.result.push_str(", ");
                    }
                    self.format_type_expr(arg);
                }
                self.result.push('>');
            }
            TypeExpr::Array(item, _) => {
                self.format_type_expr(item);
                self.result.push_str("[]");
            }
            TypeExpr::Function(params, ret, _) => {
                self.result.push_str("fn(");
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        self.result.push_str(", ");
                    }
                    self.format_type_expr(p);
                }
                self.result.push_str(") -> ");
                self.format_type_expr(ret);
            }
        }
    }

    fn format_doc(&mut self, doc: &Option<DocComment>) {
        if let Some(d) = doc {
            match d {
                DocComment::Line(content) => {
                    for line in content.lines() {
                        self.indent();
                        self.result.push_str("///");
                        if !line.is_empty() && !line.starts_with(' ') {
                            self.result.push(' ');
                        }
                        self.result.push_str(line.trim_end());
                        self.result.push('\n');
                    }
                }
                DocComment::Block(content) => {
                    self.indent();
                    self.result.push_str("/**");
                    self.result.push_str(content);
                    self.result.push_str("*/\n");
                }
            }
        }
    }

    fn indent(&mut self) {
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
  static x: number = 10;

  constructor(a: number) {
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

        let expected = "/**\n * Main function\n * with multiple lines\n */\nfunction main(): void {\n}\n";
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
    fn test_consecutive_imports_vars_no_blank_line() {
        let source = r#"
import { a } from "b";
import { c } from "d";
let x = 1;
let y = 2;
"#;
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, "test.aura".to_string());
        let program = parser.parse_program();

        let formatter = Formatter::new();
        let formatted = formatter.format_program(&program);

        let expected = "import { a } from \"b\";\nimport { c } from \"d\";\nlet x = 1;\nlet y = 2;\n";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_export_enum_doc_formatter() {
        let source = r#"
/**
 * Test Enum
 */
export enum Test {
  A,
  B
}
"#;
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, "test.aura".to_string());
        let program = parser.parse_program();

        let formatter = Formatter::new();
        let formatted = formatter.format_program(&program);

        let expected = "/**\n * Test Enum\n */\nexport enum Test {\n  A,\n  B,\n}\n";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_preserve_manual_blank_lines() {
        let source = r#"
function a() {}

function b() {}
"#;
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, "test.aura".to_string());
        let program = parser.parse_program();

        let formatter = Formatter::new().with_source(source.to_string());
        let formatted = formatter.format_program(&program);

        let expected = "function a(): void {\n}\n\nfunction b(): void {\n}\n";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_always_breakline_before_comment() {
        let source = r#"
let x = 1;
// comment
function f() {}
"#;
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, "test.aura".to_string());
        let program = parser.parse_program();

        let formatter = Formatter::new().with_source(source.to_string());
        let formatted = formatter.format_program(&program);

        let expected = "let x = 1;\n\n// comment\nfunction f(): void {\n}\n";
        assert_eq!(formatted, expected);
    }
}
