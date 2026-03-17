use crate::compiler::ast::*;
use crate::compiler::frontend::formatter::Formatter;

pub(crate) fn format_type_expr(f: &mut Formatter, ty: &TypeExpr) {
    match ty {
        TypeExpr::Name(name, _) => f.result.push_str(name),
        TypeExpr::Union(tys, _) => {
            for (i, t) in tys.iter().enumerate() {
                if i > 0 {
                    f.result.push_str(" | ");
                }
                format_type_expr(f, t);
            }
        }
        TypeExpr::Generic(name, args, _) => {
            f.result.push_str(name);
            f.result.push('<');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    f.result.push_str(", ");
                }
                format_type_expr(f, arg);
            }
            f.result.push('>');
        }
        TypeExpr::Array(item, _) => {
            format_type_expr(f, item);
            f.result.push_str("[]");
        }
        TypeExpr::Function(tparams, params, ret, _) => {
            if !tparams.is_empty() {
                f.result.push('<');
                for (i, tp) in tparams.iter().enumerate() {
                    if i > 0 {
                        f.result.push_str(", ");
                    }
                    f.result.push_str(&tp.name);
                }
                f.result.push('>');
            }
            f.result.push_str("function(");
            for (i, p) in params.iter().enumerate() {
                if i > 0 {
                    f.result.push_str(", ");
                }
                format_type_expr(f, p);
            }
            f.result.push_str("): ");
            format_type_expr(f, ret);
        }
    }
}
