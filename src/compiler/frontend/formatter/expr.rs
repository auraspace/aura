use crate::compiler::ast::*;
use crate::compiler::frontend::formatter::Formatter;

pub(crate) fn format_expr(f: &mut Formatter, expr: &Expr) {
    match expr {
        Expr::Number(n, _) => f.result.push_str(&n.to_string()),
        Expr::StringLiteral(s, _) => {
            f.result.push('"');
            f.result.push_str(s);
            f.result.push('"');
        }
        Expr::Variable(name, _) => f.result.push_str(name),
        Expr::BinaryOp(left, op, right, _) => {
            format_expr(f, left);
            f.result.push(' ');
            f.result.push_str(op);
            f.result.push(' ');
            format_expr(f, right);
        }
        Expr::Assign(name, val, _) => {
            f.result.push_str(name);
            f.result.push_str(" = ");
            format_expr(f, val);
        }
        Expr::Call(name, _, args, _) => {
            f.result.push_str(name);
            f.result.push('(');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    f.result.push_str(", ");
                }
                format_expr(f, arg);
            }
            f.result.push(')');
        }
        Expr::MethodCall(obj, name, _, args, _) => {
            format_expr(f, obj);
            f.result.push('.');
            f.result.push_str(name);
            f.result.push('(');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    f.result.push_str(", ");
                }
                format_expr(f, arg);
            }
            f.result.push(')');
        }
        Expr::This(_) => f.result.push_str("this"),
        Expr::New(name, _, args, _) => {
            f.result.push_str("new ");
            f.result.push_str(name);
            f.result.push('(');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    f.result.push_str(", ");
                }
                format_expr(f, arg);
            }
            f.result.push(')');
        }
        Expr::MemberAccess(obj, name, _, _) => {
            format_expr(f, obj);
            f.result.push('.');
            f.result.push_str(name);
        }
        Expr::MemberAssign(obj, name, val, _, _) => {
            format_expr(f, obj);
            f.result.push('.');
            f.result.push_str(name);
            f.result.push_str(" = ");
            format_expr(f, val);
        }
        Expr::UnaryOp(op, expr, _) => {
            f.result.push_str(op);
            format_expr(f, expr);
        }
        Expr::Throw(expr, _) => {
            f.result.push_str("throw ");
            format_expr(f, expr);
        }
        Expr::TypeTest(expr, ty, _) => {
            format_expr(f, expr);
            f.result.push_str(" is ");
            f.format_type_expr(ty);
        }
        Expr::Template(parts, _) => {
            f.result.push('`');
            for part in parts {
                match part {
                    TemplatePart::Str(s) => f.result.push_str(s),
                    TemplatePart::Expr(e) => {
                        f.result.push_str("${");
                        format_expr(f, e);
                        f.result.push('}');
                    }
                }
            }
            f.result.push('`');
        }
        Expr::Await(expr, _) => {
            f.result.push_str("await ");
            format_expr(f, expr);
        }
        Expr::ArrayLiteral(elements, _) => {
            f.result.push('[');
            for (i, el) in elements.iter().enumerate() {
                if i > 0 {
                    f.result.push_str(", ");
                }
                format_expr(f, el);
            }
            f.result.push(']');
        }
        Expr::Index(obj, idx, _) => {
            format_expr(f, obj);
            f.result.push('[');
            format_expr(f, idx);
            f.result.push(']');
        }
        Expr::Null(_) => f.result.push_str("null"),
        Expr::Super(_) => f.result.push_str("super"),
        Expr::SuperCall(args, _) => {
            f.result.push_str("super(");
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    f.result.push_str(", ");
                }
                format_expr(f, arg);
            }
            f.result.push(')');
        }
        Expr::Error(_) => f.result.push_str("/* ERROR */"),
    }
}
