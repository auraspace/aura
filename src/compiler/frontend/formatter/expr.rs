use crate::compiler::ast::*;
use crate::compiler::frontend::formatter::Formatter;

pub(crate) fn format_expr(f: &mut Formatter, expr: &Expr) {
    match expr {
        Expr::Number(n, _) => f.result.push_str(&n.to_string()),
        Expr::Float(n, _) => f.result.push_str(&n.to_string()),
        Expr::StringLiteral(s, _) => {
            f.result.push('"');
            f.result.push_str(s);
            f.result.push('"');
        }
        Expr::Variable(name, _) => f.result.push_str(name),
        Expr::BinaryOp(left, op, right, _) => {
            let parent_prec = get_precedence(op);

            // Left child
            let left_prec = get_expr_precedence(left);
            if left_prec < parent_prec {
                f.result.push('(');
                format_expr(f, left);
                f.result.push(')');
            } else {
                format_expr(f, left);
            }

            f.result.push(' ');
            f.result.push_str(op);
            f.result.push(' ');

            // Right child
            let right_prec = get_expr_precedence(right);
            // For left-associative operators, we need parentheses if the right child
            // has the same precedence (e.g., a - (b - c) vs a - b - c).
            // Aura operators are mostly left-associative.
            if right_prec <= parent_prec {
                f.result.push('(');
                format_expr(f, right);
                f.result.push(')');
            } else {
                format_expr(f, right);
            }
        }
        Expr::Assign(name, val, _) => {
            f.result.push_str(name);
            f.result.push_str(" = ");
            format_expr(f, val);
        }
        Expr::Call(name, type_args, _, args, _) => {
            f.result.push_str(name);
            if !type_args.is_empty() {
                f.result.push('<');
                for (i, ta) in type_args.iter().enumerate() {
                    if i > 0 {
                        f.result.push_str(", ");
                    }
                    f.format_type_expr(ta);
                }
                f.result.push('>');
            }
            f.result.push('(');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    f.result.push_str(", ");
                }
                format_expr(f, arg);
            }
            f.result.push(')');
        }
        Expr::MethodCall(obj, name, type_args, _, args, _) => {
            format_expr(f, obj);
            f.result.push('.');
            f.result.push_str(name);
            if !type_args.is_empty() {
                f.result.push('<');
                for (i, ta) in type_args.iter().enumerate() {
                    if i > 0 {
                        f.result.push_str(", ");
                    }
                    f.format_type_expr(ta);
                }
                f.result.push('>');
            }
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
        Expr::New(name, type_args, _, args, _) => {
            f.result.push_str("new ");
            f.result.push_str(name);
            if !type_args.is_empty() {
                f.result.push('<');
                for (i, ta) in type_args.iter().enumerate() {
                    if i > 0 {
                        f.result.push_str(", ");
                    }
                    f.format_type_expr(ta);
                }
                f.result.push('>');
            }
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
            let child_prec = get_expr_precedence(expr);
            if child_prec < 13 {
                f.result.push('(');
                format_expr(f, expr);
                f.result.push(')');
            } else {
                format_expr(f, expr);
            }
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

fn get_expr_precedence(expr: &Expr) -> i32 {
    match expr {
        Expr::BinaryOp(_, op, _, _) => get_precedence(op),
        Expr::Assign(_, _, _) => 2,
        Expr::TypeTest(_, _, _) => 10,
        Expr::UnaryOp(_, _, _) => 13, // Unary is stronger than binary
        Expr::Call(_, _, _, _, _)
        | Expr::MethodCall(_, _, _, _, _, _)
        | Expr::MemberAccess(_, _, _, _)
        | Expr::Index(_, _, _)
        | Expr::New(_, _, _, _, _) => 14, // Postfix/Primary-like are strongest
        _ => 15,                      // Primary (numbers, variables, etc.)
    }
}

fn get_precedence(op: &str) -> i32 {
    match op {
        "*" | "/" | "%" => 12,
        "+" | "-" => 11,
        "is" => 10,
        "<<" | ">>" => 9,
        "<" | "<=" | ">" | ">=" | "==" | "!=" => 8,
        "&" => 7,
        "^" => 6,
        "|" => 5,
        "&&" => 4,
        "||" => 3,
        "=" => 2,
        _ => 0,
    }
}
