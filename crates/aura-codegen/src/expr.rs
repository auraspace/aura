//! Expression emission.

use aura_ast::*;
use aura_sema::{CheckedFile, Ty};

use crate::ctx::EmitCtx;
use crate::names::*;
use crate::call_emit::emit_call;

pub(crate) fn infer_type_name(e: &Expr, ctx: &EmitCtx<'_>) -> String {
    match e {
        Expr::Int(_) => "Int".into(),
        Expr::Bool(_) => "Bool".into(),
        Expr::String(_) => "String".into(),
        Expr::Call(c) => match c.callee.as_ref() {
            Expr::Ident(id)
                if ctx
                    .checked
                    .call_instantiations
                    .get(&c.span.start)
                    .and_then(|i| i.variant.as_ref())
                    .is_some() =>
            {
                let inst = ctx.checked.call_instantiations.get(&c.span.start).unwrap();
                mono_key(&inst.name, &inst.type_args)
            }
            Expr::Ident(id)
                if id.name == "Array"
                    || ctx
                        .checked
                        .ast
                        .classes
                        .iter()
                        .any(|x| x.name.name == id.name)
                    || ctx
                        .checked
                        .call_instantiations
                        .get(&c.span.start)
                        .map(|i| i.is_constructor && i.name == id.name)
                        .unwrap_or(false) =>
            {
                let targs: Vec<Ty> = ctx
                    .checked
                    .call_instantiations
                    .get(&c.span.start)
                    .map(|i| i.type_args.clone())
                    .unwrap_or_else(|| {
                        c.type_args
                            .iter()
                            .filter_map(|t| type_ref_to_ty(t, ctx))
                            .collect()
                    });
                mono_key(&id.name, &targs)
            }
            Expr::Ident(id)
                if ctx
                    .checked
                    .ast
                    .functions
                    .iter()
                    .any(|f| f.name.name == id.name) =>
            {
                let targs: Vec<Ty> = ctx
                    .checked
                    .call_instantiations
                    .get(&c.span.start)
                    .map(|i| i.type_args.clone())
                    .unwrap_or_else(|| {
                        c.type_args
                            .iter()
                            .filter_map(|t| type_ref_to_ty(t, ctx))
                            .collect()
                    });
                if let Some(f) = ctx
                    .checked
                    .ast
                    .functions
                    .iter()
                    .find(|f| f.name.name == id.name)
                {
                    let params: Vec<String> =
                        f.type_params.iter().map(|p| p.name.name.clone()).collect();
                    if let Some(rt) = &f.return_type {
                        return type_ref_local_key(rt, &params, &targs);
                    }
                }
                "Unit".into()
            }
            Expr::Field(fe) => {
                if let Some(mono) = resolve_class_of_expr(&fe.object, ctx) {
                    let base = mono_base_name(mono, ctx.checked).unwrap_or(mono);
                    if let Some(m) = ctx
                        .checked
                        .ast
                        .classes
                        .iter()
                        .find(|c| c.name.name == base)
                        .and_then(|c| c.methods.iter().find(|m| m.name.name == fe.field.name))
                    {
                        if let Some(rt) = &m.return_type {
                            // substitute class type args from mono key is hard; use name only for primitives
                            return type_ref_local_key(rt, &[], &[]);
                        }
                    }
                }
                "Int".into()
            }
            _ => "Int".into(),
        },
        Expr::Field(f) => {
            if let Some(mono) = resolve_class_of_expr(&f.object, ctx) {
                let base = mono_base_name(mono, ctx.checked).unwrap_or(mono);
                if (base == "Array" || mono.starts_with("Array_")) && f.field.name == "len" {
                    return "Int".into();
                }
                if let Some(field) = ctx
                    .checked
                    .ast
                    .classes
                    .iter()
                    .find(|c| c.name.name == base)
                    .and_then(|c| c.fields.iter().find(|x| x.name.name == f.field.name))
                {
                    return type_ref_local_key(&field.ty, &[], &[]);
                }
            }
            "String".into()
        }
        Expr::Ident(i) => ctx
            .lookup_local(&i.name)
            .unwrap_or("Int")
            .to_string(),
        Expr::This(_) => ctx.method_class.unwrap_or("Int").to_string(),
        Expr::Group(inner, _) => infer_type_name(inner, ctx),
        Expr::Assign(a) => infer_type_name(&a.value, ctx),
        Expr::Unary(UnaryExpr { op: UnOp::Not, .. }) => "Bool".into(),
        Expr::ForceUnwrap(f) => infer_type_name(&f.expr, ctx),
        Expr::Binary(BinaryExpr {
            op: BinOp::Lt
                | BinOp::Le
                | BinOp::Gt
                | BinOp::Ge
                | BinOp::Eq
                | BinOp::Ne
                | BinOp::And
                | BinOp::Or,
            ..
        }) => "Bool".into(),
        _ => "Int".into(),
    }
}




pub(crate) fn emit_expr(expr: &Expr, ctx: &EmitCtx<'_>) -> String {
    match expr {
        Expr::Ident(i) => {
            // Inside method: bare field names → this->field
            if let Some(class) = ctx.method_class {
                if let Some(cl) = ctx.checked.ast.classes.iter().find(|c| c.name.name == class) {
                    if cl.fields.iter().any(|f| f.name.name == i.name) {
                        return format!("this->{}", mangle_ident(&i.name));
                    }
                }
            }
            mangle_ident(&i.name)
        }
        Expr::This(_) => "(*this)".into(),
        Expr::Int(n) => format!("INT64_C({})", n.value),
        Expr::Bool(b) => {
            if b.value {
                "true".into()
            } else {
                "false".into()
            }
        }
        Expr::String(s) => format!("\"{}\"", escape_c_string(&s.value)),
        Expr::Null(_) => "NULL".into(),
        Expr::Group(inner, _) => format!("({})", emit_expr(inner, ctx)),
        Expr::Unary(u) => {
            let op = match u.op {
                UnOp::Neg => "-",
                UnOp::Not => "!",
            };
            format!("({op}{})", emit_expr(&u.expr, ctx))
        }
        Expr::ForceUnwrap(f) => {
            // C: just the pointer/value; null is a runtime fault (MVP).
            emit_expr(&f.expr, ctx)
        }
        Expr::Binary(b) => {
            let op = match b.op {
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "/",
                BinOp::Rem => "%",
                BinOp::Eq => "==",
                BinOp::Ne => "!=",
                BinOp::Lt => "<",
                BinOp::Le => "<=",
                BinOp::Gt => ">",
                BinOp::Ge => ">=",
                BinOp::And => "&&",
                BinOp::Or => "||",
            };
            format!(
                "({} {} {})",
                emit_expr(&b.left, ctx),
                op,
                emit_expr(&b.right, ctx)
            )
        }
        Expr::Assign(a) => {
            // field assign in method for bare field name
            let lhs = if let Some(class) = ctx.method_class {
                if let Some(cl) = ctx.checked.ast.classes.iter().find(|c| c.name.name == class) {
                    if cl.fields.iter().any(|f| f.name.name == a.name.name) {
                        format!("this->{}", mangle_ident(&a.name.name))
                    } else {
                        mangle_ident(&a.name.name)
                    }
                } else {
                    mangle_ident(&a.name.name)
                }
            } else {
                mangle_ident(&a.name.name)
            };
            format!("({lhs} = {})", emit_expr(&a.value, ctx))
        }
        Expr::Field(f) => {
            let obj = emit_expr(&f.object, ctx);
            // this.name already becomes (*this).name if object is This
            format!("({obj}).{}", mangle_ident(&f.field.name))
        }
        Expr::Call(c) => emit_call(c, ctx),
    }
}

pub(crate) fn mono_base_name<'a>(mono: &'a str, checked: &'a CheckedFile) -> Option<&'a str> {
    mono_split(mono, checked).map(|(n, _)| n)
}

/// Resolve monomorphized key `Holder_User` → (`Holder`, `[User]`), or plain class/enum → `(Name, [])`.
pub(crate) fn mono_split<'a>(mono: &'a str, checked: &'a CheckedFile) -> Option<(&'a str, &'a [Ty])> {
    if checked.ast.classes.iter().any(|c| c.name.name == mono)
        || checked.ast.enums.iter().any(|e| e.name.name == mono)
    {
        return Some((mono, &[]));
    }
    for (name, args) in &checked.mono_classes {
        if mono_key(name, args) == mono {
            return Some((name.as_str(), args.as_slice()));
        }
    }
    for (name, args) in &checked.mono_enums {
        if mono_key(name, args) == mono {
            return Some((name.as_str(), args.as_slice()));
        }
    }
    None
}

pub(crate) fn type_ref_to_ty(t: &TypeRef, ctx: &EmitCtx<'_>) -> Option<Ty> {
    if !t.type_args.is_empty() {
        let args: Vec<Ty> = t
            .type_args
            .iter()
            .filter_map(|a| type_ref_to_ty(a, ctx))
            .collect();
        return Some(Ty::ClassApp {
            name: t.name.name.clone(),
            args,
        });
    }
    match t.name.name.as_str() {
        "Int" => Some(Ty::Int),
        "Bool" => Some(Ty::Bool),
        "String" => Some(Ty::String),
        "Unit" => Some(Ty::Unit),
        name if ctx.checked.ast.classes.iter().any(|c| c.name.name == name) => {
            Some(Ty::Class(name.into()))
        }
        name if ctx
            .checked
            .ast
            .interfaces
            .iter()
            .any(|i| i.name.name == name) =>
        {
            Some(Ty::Interface(name.into()))
        }
        _ => None,
    }
}

/// If `expr` has class type `from` and expected is interface, emit upcast.
pub(crate) fn coerce_expr(expr: &Expr, expected_ty: &str, ctx: &EmitCtx<'_>) -> String {
    let actual = resolve_type_name(expr, ctx);
    let code = emit_expr(expr, ctx);
    if let Some(from) = actual {
        if from != expected_ty
            && ctx
                .checked
                .ast
                .interfaces
                .iter()
                .any(|i| i.name.name == expected_ty)
            && ctx
                .checked
                .ast
                .classes
                .iter()
                .any(|c| c.name.name == from && c.implements.iter().any(|i| i.name == expected_ty))
        {
            return format!("{}({code})", c_upcast_name(&from, expected_ty));
        }
    }
    // Constructor expr Greeter(...) inferred as class, expected interface
    if let Expr::Call(c) = expr {
        if let Expr::Ident(id) = c.callee.as_ref() {
            if ctx
                .checked
                .ast
                .classes
                .iter()
                .any(|cl| {
                    cl.name.name == id.name
                        && cl.implements.iter().any(|i| i.name == expected_ty)
                })
                && ctx
                    .checked
                    .ast
                    .interfaces
                    .iter()
                    .any(|i| i.name.name == expected_ty)
            {
                return format!("{}({code})", c_upcast_name(&id.name, expected_ty));
            }
        }
    }
    code
}

pub(crate) fn resolve_type_name(expr: &Expr, ctx: &EmitCtx<'_>) -> Option<String> {
    match expr {
        Expr::Ident(id) => ctx.lookup_local(&id.name).map(|s| s.to_string()),
        Expr::This(_) => ctx.method_class.map(|s| s.to_string()),
        Expr::Call(c) => {
            if let Expr::Ident(id) = c.callee.as_ref() {
                if let Some(inst) = ctx.checked.call_instantiations.get(&c.span.start) {
                    if inst.is_constructor {
                        return Some(mono_key(&inst.name, &inst.type_args));
                    }
                }
                if id.name == "Array" {
                    let targs: Vec<Ty> = c
                        .type_args
                        .iter()
                        .filter_map(|t| type_ref_to_ty(t, ctx))
                        .collect();
                    if !targs.is_empty() {
                        return Some(mono_key("Array", &targs));
                    }
                }
                if ctx
                    .checked
                    .ast
                    .classes
                    .iter()
                    .any(|x| x.name.name == id.name)
                {
                    return Some(id.name.clone());
                }
                if let Some(f) = ctx
                    .checked
                    .ast
                    .functions
                    .iter()
                    .find(|f| f.name.name == id.name)
                {
                    return f.return_type.as_ref().map(|t| t.name.name.clone());
                }
            }
            if let Expr::Field(fe) = c.callee.as_ref() {
                // method return
                if let Some(recv) = resolve_type_name(&fe.object, ctx) {
                    if let Some(m) = ctx
                        .checked
                        .ast
                        .classes
                        .iter()
                        .find(|c| c.name.name == recv)
                        .and_then(|c| c.methods.iter().find(|m| m.name.name == fe.field.name))
                    {
                        return m.return_type.as_ref().map(|t| t.name.name.clone());
                    }
                    if let Some(m) = ctx
                        .checked
                        .ast
                        .interfaces
                        .iter()
                        .find(|i| i.name.name == recv)
                        .and_then(|i| i.methods.iter().find(|m| m.name.name == fe.field.name))
                    {
                        return m.return_type.as_ref().map(|t| t.name.name.clone());
                    }
                }
            }
            None
        }
        Expr::Field(f) => {
            let parent = resolve_type_name(&f.object, ctx)?;
            if (parent.starts_with("Array_") || parent == "Array") && f.field.name == "len" {
                return Some("Int".into());
            }
            let (base, args) = mono_split(&parent, ctx.checked)?;
            if base == "Array" && f.field.name == "len" {
                return Some("Int".into());
            }
            let class = ctx
                .checked
                .ast
                .classes
                .iter()
                .find(|c| c.name.name == base)?;
            let field = class
                .fields
                .iter()
                .find(|x| x.name.name == f.field.name)?;
            let params: Vec<String> = class
                .type_params
                .iter()
                .map(|p| p.name.name.clone())
                .collect();
            // When `this` is the open mono instance, args come from mono_split;
            // empty args on a generic class falls back to the current emit substitution.
            let (ps, as_) = if args.is_empty() && !params.is_empty() && !ctx.type_args.is_empty()
            {
                (ctx.type_params.clone(), ctx.type_args.clone())
            } else {
                (params, args.to_vec())
            };
            Some(type_ref_local_key(&field.ty, &ps, &as_))
        }
        Expr::Group(inner, _) => resolve_type_name(inner, ctx),
        Expr::String(_) => Some("String".into()),
        Expr::Int(_) => Some("Int".into()),
        Expr::Bool(_) => Some("Bool".into()),
        _ => None,
    }
}

/// Best-effort class name for method receiver (for mangling).
/// Returns mono class key for a receiver expression.
pub(crate) fn resolve_class_of_expr<'a>(expr: &Expr, ctx: &'a EmitCtx<'_>) -> Option<&'a str> {
    match expr {
        Expr::This(_) => ctx.method_class,
        Expr::Ident(id) => {
            let ty = ctx.lookup_local(&id.name)?;
            if ty.starts_with("Array_")
                || ctx.checked.ast.classes.iter().any(|c| c.name.name == ty)
                || ctx
                    .checked
                    .mono_classes
                    .iter()
                    .any(|(n, a)| mono_key(n, a) == ty)
            {
                // Need to return ref into locals or mono list - use leak-free approach:
                // return from mono_classes storage
                return ctx.lookup_local(&id.name);
            }
            None
        }
        Expr::Call(c) => {
            if let Expr::Ident(id) = c.callee.as_ref() {
                if ctx
                    .checked
                    .ast
                    .classes
                    .iter()
                    .any(|x| x.name.name == id.name)
                {
                    // mono from type args — stored not as ref; fall back base name for non-generic
                    if c.type_args.is_empty() {
                        return ctx
                            .checked
                            .ast
                            .classes
                            .iter()
                            .find(|x| x.name.name == id.name)
                            .map(|x| x.name.name.as_str());
                    }
                    // For generic ctor, resolve_type_name is better
                    return None;
                }
            }
            None
        }
        Expr::Group(inner, _) => resolve_class_of_expr(inner, ctx),
        _ => None,
    }
}
