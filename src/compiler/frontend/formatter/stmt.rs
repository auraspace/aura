use crate::compiler::ast::*;
use crate::compiler::frontend::formatter::Formatter;

pub(crate) fn format_statement(f: &mut Formatter, stmt: &Statement) {
    format_statement_internal(f, stmt, true);
}

pub(crate) fn format_statement_internal(f: &mut Formatter, stmt: &Statement, include_doc: bool) {
    match stmt {
        Statement::Enum(decl) => {
            if include_doc {
                f.format_doc(&decl.doc);
            }
            f.indent();
            f.result.push_str("enum ");
            f.result.push_str(&decl.name);
            f.result.push_str(" {\n");
            f.indent_level += 1;
            for member in &decl.members {
                f.indent();
                f.result.push_str(&member.name);
                if let Some(val) = &member.value {
                    f.result.push_str(" = ");
                    f.format_expr(val);
                }
                f.result.push_str(",\n");
            }
            f.indent_level -= 1;
            f.indent();
            f.result.push('}');
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
                f.format_doc(doc);
            }
            f.indent();
            if *is_const {
                f.result.push_str("const ");
            } else {
                f.result.push_str("let ");
            }
            f.result.push_str(name);
            if let Some(ty_expr) = ty {
                f.result.push_str(": ");
                f.format_type_expr(ty_expr);
            }
            f.result.push_str(" = ");
            f.format_expr(value);
            f.result.push(';');
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
                f.format_doc(doc);
            }
            f.indent();
            if *is_async {
                f.result.push_str("async ");
            }
            f.result.push_str("function ");
            f.result.push_str(name);
            f.result.push('(');
            for (i, (pname, pty)) in params.iter().enumerate() {
                if i > 0 {
                    f.result.push_str(", ");
                }
                f.result.push_str(pname);
                f.result.push_str(": ");
                f.format_type_expr(pty);
            }
            f.result.push_str("): ");
            f.format_type_expr(return_ty);
            f.result.push_str(" ");
            format_statement_internal(f, body, true);
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
                f.format_doc(doc);
            }
            f.indent();
            f.result.push_str("class ");
            f.result.push_str(name);
            f.result.push_str(" {\n");
            f.indent_level += 1;

            for field in fields {
                f.format_doc(&field.doc);
                f.indent();
                f.result.push_str(field.access.as_str());
                f.result.push(' ');
                if field.is_static {
                    f.result.push_str("static ");
                }
                if field.is_readonly {
                    f.result.push_str("readonly ");
                }
                f.result.push_str(&field.name);
                f.result.push_str(": ");
                f.format_type_expr(&field.ty);
                if let Some(val) = &field.value {
                    f.result.push_str(" = ");
                    f.format_expr(val);
                }
                f.result.push_str(";\n");
            }

            if let Some(ctor) = constructor {
                if !fields.is_empty() {
                    f.result.push('\n');
                }
                f.format_doc(&ctor.doc);
                f.indent();
                f.result.push_str(ctor.access.as_str());
                f.result.push_str(" constructor(");
                for (i, (pname, pty)) in ctor.params.iter().enumerate() {
                    if i > 0 {
                        f.result.push_str(", ");
                    }
                    f.result.push_str(pname);
                    f.result.push_str(": ");
                    f.format_type_expr(pty);
                }
                f.result.push_str(") ");
                format_statement_internal(f, &ctor.body, true);
                f.result.push('\n');
            }

            for (i, method) in methods.iter().enumerate() {
                if i > 0 || constructor.is_some() || !fields.is_empty() {
                    f.result.push('\n');
                }
                f.format_doc(&method.doc);
                f.indent();
                f.result.push_str(method.access.as_str());
                f.result.push(' ');
                if method.is_static {
                    f.result.push_str("static ");
                }
                if method.is_async {
                    f.result.push_str("async ");
                }
                f.result.push_str(&method.name);
                f.result.push('(');
                for (i, (pname, pty)) in method.params.iter().enumerate() {
                    if i > 0 {
                        f.result.push_str(", ");
                    }
                    f.result.push_str(pname);
                    f.result.push_str(": ");
                    f.format_type_expr(pty);
                }
                f.result.push_str("): ");
                f.format_type_expr(&method.return_ty);
                f.result.push_str(" ");
                format_statement_internal(f, &method.body, true);
                f.result.push('\n');
            }

            f.indent_level -= 1;
            f.indent();
            f.result.push('}');
        }
        Statement::Return(expr, _) => {
            f.indent();
            f.result.push_str("return ");
            f.format_expr(expr);
            f.result.push(';');
        }
        Statement::Print(expr, _) => {
            f.indent();
            f.result.push_str("print ");
            f.format_expr(expr);
            f.result.push(';');
        }
        Statement::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            f.indent();
            f.result.push_str("if (");
            f.format_expr(condition);
            f.result.push_str(") ");
            format_statement_internal(f, then_branch, true);
            if let Some(else_b) = else_branch {
                f.result.push_str(" else ");
                format_statement_internal(f, else_b, true);
            }
        }
        Statement::While {
            condition, body, ..
        } => {
            f.indent();
            f.result.push_str("while (");
            f.format_expr(condition);
            f.result.push_str(") ");
            format_statement_internal(f, body, true);
        }
        Statement::Block(stmts, _) => {
            f.result.push_str("{\n");
            f.indent_level += 1;
            for stmt in stmts {
                format_statement_internal(f, stmt, true);
                f.result.push('\n');
            }
            f.indent_level -= 1;
            f.indent();
            f.result.push('}');
        }
        Statement::Expression(expr, _) => {
            f.indent();
            f.format_expr(expr);
            f.result.push(';');
        }
        Statement::Import { item, path, .. } => {
            f.indent();
            f.result.push_str("import ");
            match item {
                ImportItem::Named(names) => {
                    f.result.push_str("{ ");
                    for (i, (name, _)) in names.iter().enumerate() {
                        if i > 0 {
                            f.result.push_str(", ");
                        }
                        f.result.push_str(name);
                    }
                    f.result.push_str(" }");
                }
                ImportItem::Namespace((name, _)) => {
                    f.result.push_str("* as ");
                    f.result.push_str(name);
                }
            }
            f.result.push_str(" from \"");
            f.result.push_str(path);
            f.result.push_str("\";");
        }
        Statement::Export { decl, .. } => {
            let doc = match &**decl {
                Statement::VarDeclaration { doc, .. } => doc,
                Statement::FunctionDeclaration { doc, .. } => doc,
                Statement::ClassDeclaration { doc, .. } => doc,
                Statement::Enum(d) => &d.doc,
                _ => &None,
            };

            f.format_doc(doc);

            f.indent();
            f.result.push_str("export ");
            let mut sub_formatter = Formatter::new();
            sub_formatter.format_statement_internal(decl, false);
            f.result.push_str(sub_formatter.result.trim_start());
        }
        Statement::TryCatch {
            try_block,
            catch_param,
            catch_block,
            finally_block,
            ..
        } => {
            f.indent();
            f.result.push_str("try ");
            format_statement_internal(f, try_block, true);
            if let Some((pname, pty)) = catch_param {
                f.result.push_str(" catch (");
                f.result.push_str(pname);
                f.result.push_str(": ");
                f.format_type_expr(pty);
                f.result.push_str(") ");
                if let Some(cb) = catch_block {
                    format_statement_internal(f, cb, true);
                }
            }
            if let Some(fb) = finally_block {
                f.result.push_str(" finally ");
                format_statement_internal(f, fb, true);
            }
        }
        Statement::Comment(content, _) => {
            f.indent();
            f.result.push_str("//");
            f.result.push_str(content);
        }
        Statement::RegularBlockComment(content, _) => {
            f.indent();
            f.result.push_str("/*");
            f.result.push_str(content);
            f.result.push_str("*/");
        }
        Statement::Error => {
            f.indent();
            f.result.push_str("// ERROR");
        }
    }
}
