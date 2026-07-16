//! Expression emission.

use aura_ast::*;
use aura_sema::{nominal_key, CheckedFile, Ty};

use crate::ctx::EmitCtx;
use crate::call_emit::emit_call;
use crate::names::*;

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
                // C4k: resolve receiver via type name (handles field chains like this.item).
                let mono = resolve_type_name(&fe.object, ctx)
                    .or_else(|| resolve_class_of_expr(&fe.object, ctx).map(|s| s.to_string()));
                if let Some(mono) = mono {
                    let base = mono_base_name(&mono, ctx.checked).unwrap_or(mono.as_str());
                    if let Some(m) = ctx
                        .checked
                        .ast
                        .classes
                        .iter()
                        .find(|c| c.name.name == base)
                        .and_then(|c| c.methods.iter().find(|m| m.name.name == fe.field.name))
                    {
                        if let Some(rt) = &m.return_type {
                            let (ps, as_) = if let Some((_, args)) =
                                mono_split(&mono, ctx.checked)
                            {
                                let params: Vec<String> = ctx
                                    .checked
                                    .ast
                                    .classes
                                    .iter()
                                    .find(|c| c.name.name == base)
                                    .map(|c| {
                                        c.type_params
                                            .iter()
                                            .map(|p| p.name.name.clone())
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                (params, args.to_vec())
                            } else {
                                (Vec::new(), Vec::new())
                            };
                            return type_ref_local_key(rt, &ps, &as_);
                        }
                    }
                    // Interface method return type
                    if let Some(m) = ctx
                        .checked
                        .ast
                        .interfaces
                        .iter()
                        .find(|i| i.name.name == base || iface_mono(i, ctx.checked) == mono)
                        .and_then(|i| i.methods.iter().find(|m| m.name.name == fe.field.name))
                    {
                        if let Some(rt) = &m.return_type {
                            return type_ref_local_key(rt, &[], &[]);
                        }
                    }
                }
                "Int".into()
            }
            _ => "Int".into(),
        },
        Expr::Field(f) => {
            if f.field.name == "len" {
                let recv = resolve_type_name(&f.object, ctx);
                if matches!(recv.as_deref(), Some("String"))
                    || matches!(f.object.as_ref(), Expr::String(_))
                {
                    return "Int".into();
                }
            }
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
                    // Substitute class type params (e.g. T on Box_String methods).
                    let (ps, as_) = if !ctx.type_args.is_empty() {
                        (ctx.type_params.clone(), ctx.type_args.clone())
                    } else if let Some((_, args)) = mono_split(mono, ctx.checked) {
                        let params: Vec<String> = ctx
                            .checked
                            .ast
                            .classes
                            .iter()
                            .find(|c| c.name.name == base)
                            .map(|c| {
                                c.type_params
                                    .iter()
                                    .map(|p| p.name.name.clone())
                                    .collect()
                            })
                            .unwrap_or_default();
                        (params, args.to_vec())
                    } else {
                        (vec![], vec![])
                    };
                    return type_ref_local_key(&field.ty, &ps, &as_);
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
        Expr::Binary(BinaryExpr {
            op: BinOp::Coalesce,
            right,
            ..
        }) => infer_type_name(right, ctx),
        Expr::If(i) => match i.then_block.stmts.last() {
            Some(Stmt::Expr(e)) => infer_type_name(e, ctx),
            _ => "Int".into(),
        },
        _ => "Int".into(),
    }
}




pub(crate) fn emit_expr(expr: &Expr, ctx: &mut EmitCtx<'_>) -> String {
    match expr {
        Expr::Ident(i) => {
            // Inside method: bare field names → this->field
            if let Some(class) = ctx.method_class {
                let base = mono_base_name(class, ctx.checked).unwrap_or(class);
                if let Some(cl) = ctx
                    .checked
                    .ast
                    .classes
                    .iter()
                    .find(|c| c.name.name == base)
                {
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
            let left = emit_expr(&b.left, ctx);
            let right = emit_expr(&b.right, ctx);
            // C4e: String content equality (null-safe strcmp); class stays pointer identity.
            if matches!(b.op, BinOp::Coalesce) {
                // C4m: pointer/string null-coalesce ternary.
                return format!("(({left}) != NULL ? ({left}) : ({right}))");
            }
            if matches!(b.op, BinOp::Eq | BinOp::Ne) {
                let lt = resolve_type_name(&b.left, ctx);
                let rt = resolve_type_name(&b.right, ctx);
                let is_string = matches!(lt.as_deref(), Some("String"))
                    || matches!(rt.as_deref(), Some("String"))
                    || matches!((&*b.left, &*b.right), (Expr::String(_), _) | (_, Expr::String(_)));
                if is_string {
                    // Both non-null and equal content, or both null.
                    let cmp = format!(
                        "(({left}) == NULL ? ({right}) == NULL : (({right}) != NULL && strcmp(({left}), ({right})) == 0))"
                    );
                    return if matches!(b.op, BinOp::Ne) {
                        format!("!({cmp})")
                    } else {
                        cmp
                    };
                }
            }
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
                BinOp::Coalesce => "?:", // handled above
            };
            // C3q: comparisons without outer parens so `if (x == y)` is not
            // `if ((x == y))` (clang -Wparentheses-equality). Arithmetic/logic
            // keep grouping parens for precedence.
            match b.op {
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                    format!("{left} {op} {right}")
                }
                _ => format!("({left} {op} {right})"),
            }
        }
        Expr::Assign(a) => {
            // field assign in method for bare field name
            let lhs = if let Some(class) = ctx.method_class {
                let base = mono_base_name(class, ctx.checked).unwrap_or(class);
                if let Some(cl) = ctx
                    .checked
                    .ast
                    .classes
                    .iter()
                    .find(|c| c.name.name == base)
                {
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
            let rhs = emit_expr(&a.value, ctx);
            let dst_name = &a.name.name;
            let dst_ty = ctx.lookup_local(dst_name);
            let dst_is_array = dst_ty
                .map(|t| t == "Array" || t.starts_with("Array_"))
                .unwrap_or(false);
            // C4r: free previous Array buffer when reassigning an owning local from Array(...).
            let is_ctor = matches!(
                a.value.as_ref(),
                Expr::Call(c) if matches!(c.callee.as_ref(), Expr::Ident(id) if id.name == "Array")
            );
            if ctx.is_array_owner(dst_name) && is_ctor {
                let n = mangle_ident(dst_name);
                return format!(
                    "({{ if (({n}).data != NULL) {{ free(({n}).data); ({n}).data = NULL; ({n}).len = 0; ({n}).cap = 0; }} ({lhs} = {rhs}); }})"
                );
            }
            // C5e: move ownership on `b = a` when `a` owns an Array buffer.
            if dst_is_array {
                if let Expr::Ident(src) = a.value.as_ref() {
                    if ctx.is_array_owner(&src.name) && src.name != *dst_name {
                        let n = mangle_ident(dst_name);
                        let s = mangle_ident(&src.name);
                        // Free old dst if it owned a buffer; then move; zero source.
                        let free_dst = if ctx.is_array_owner(dst_name) {
                            format!(
                                "if (({n}).data != NULL) {{ free(({n}).data); ({n}).data = NULL; ({n}).len = 0; ({n}).cap = 0; }} "
                            )
                        } else {
                            String::new()
                        };
                        ctx.mark_array_owner(dst_name);
                        ctx.unmark_array_owner(&src.name);
                        return format!(
                            "({{ {free_dst}({lhs} = {rhs}); {s}.data = NULL; {s}.len = 0; {s}.cap = 0; }})"
                        );
                    }
                }
            }
            format!("({lhs} = {rhs})")
        }
        Expr::Field(f) => {
            let obj = emit_expr(&f.object, ctx);
            // C4p: String.len → strlen (UTF-8 bytes).
            if f.field.name == "len" {
                let recv = resolve_type_name(&f.object, ctx);
                if matches!(recv.as_deref(), Some("String"))
                    || matches!(f.object.as_ref(), Expr::String(_))
                {
                    let len_e = format!("((int64_t)strlen({obj}))");
                    if f.safe {
                        // Nullable Int not representable; treat as 0 when null (MVP).
                        return format!("(({obj}) == NULL ? INT64_C(0) : {len_e})");
                    }
                    return len_e;
                }
            }
            // C3y: heap class receivers use -> ; structs/Array/This use .
            // `this` is already a pointer; emit This as (*this) so `.` still works.
            let use_arrow = match f.object.as_ref() {
                Expr::This(_) => false,
                _ => resolve_type_name(&f.object, ctx)
                    .map(|t| is_heap_class_mono(&full_type_mono(&t, ctx.checked), ctx.checked))
                    .unwrap_or(false)
                    || f.safe, // C4s: nullable class receivers are pointers
            };
            let access = if use_arrow {
                format!("({obj})->{}", mangle_ident(&f.field.name))
            } else {
                format!("({obj}).{}", mangle_ident(&f.field.name))
            };
            if f.safe {
                format!("(({obj}) == NULL ? NULL : {access})")
            } else {
                access
            }
        }
        Expr::Call(c) => emit_call(c, ctx),
        Expr::If(i) => {
            // C4t: GNU statement-expression; last expr of each branch is the value.
            // MVP: single-expression branches (no prefix statements).
            let cond = emit_expr(&i.cond, ctx);
            let then_v = block_last_expr_code(&i.then_block, ctx);
            let else_v = block_last_expr_code(&i.else_block, ctx);
            let ty_key = match i.then_block.stmts.last() {
                Some(Stmt::Expr(e)) => infer_type_name(e, ctx),
                _ => "Int".into(),
            };
            let c_ty = crate::stmt::local_key_to_c(&ty_key, ctx.checked);
            format!(
                "({{ {c_ty} __ifv; if ({cond}) {{ __ifv = ({then_v}); }} else {{ __ifv = ({else_v}); }} __ifv; }})"
            )
        }
    }
}

fn block_last_expr_code(block: &Block, ctx: &mut EmitCtx<'_>) -> String {
    match block.stmts.last() {
        Some(Stmt::Expr(e)) => emit_expr(e, ctx),
        _ => "0".into(),
    }
}

pub(crate) fn mono_base_name<'a>(mono: &'a str, checked: &'a CheckedFile) -> Option<&'a str> {
    mono_split(mono, checked).map(|(n, _)| n)
}

/// Resolve monomorphized key → (`SimpleName`, type args).
/// Understands C3v package-prefixed monos (`demo_counter_Counter`, `t_Box_String`).
pub(crate) fn mono_split<'a>(mono: &'a str, checked: &'a CheckedFile) -> Option<(&'a str, &'a [Ty])> {
    if mono == "Array" || mono.starts_with("Array_") {
        if mono == "Array" {
            return Some(("Array", &[]));
        }
        for (name, args) in &checked.mono_classes {
            if name == "Array" && mono_key(name, args) == mono {
                return Some((name.as_str(), args.as_slice()));
            }
        }
    }
    // Bare simple name
    if checked.ast.classes.iter().any(|c| c.name.name == mono)
        || checked.ast.enums.iter().any(|e| e.name.name == mono)
    {
        return Some((mono, &[]));
    }
    // Package-prefixed non-generic / mono: match type_mono(pkg, name, args)
    for c in &checked.ast.classes {
        let pkg = class_decl_package(c, checked);
        if type_mono(&pkg, &c.name.name, &[]) == mono {
            return Some((c.name.name.as_str(), &[]));
        }
    }
    for e in &checked.ast.enums {
        let pkg = enum_decl_package(e, checked);
        if type_mono(&pkg, &e.name.name, &[]) == mono {
            return Some((e.name.name.as_str(), &[]));
        }
    }
    for (name, args) in &checked.mono_classes {
        if mono_key(name, args) == mono {
            return Some((name.as_str(), args.as_slice()));
        }
        for c in checked.ast.classes.iter().filter(|c| c.name.name == *name) {
            let pkg = class_decl_package(c, checked);
            if type_mono(&pkg, name, args) == mono {
                return Some((name.as_str(), args.as_slice()));
            }
        }
    }
    for (name, args) in &checked.mono_enums {
        if mono_key(name, args) == mono {
            return Some((name.as_str(), args.as_slice()));
        }
        for e in checked.ast.enums.iter().filter(|e| e.name.name == *name) {
            let pkg = enum_decl_package(e, checked);
            if type_mono(&pkg, name, args) == mono {
                return Some((name.as_str(), args.as_slice()));
            }
        }
    }
    None
}

/// Full C mono id for a local/type key (simple name or already-mangled mono).
pub(crate) fn full_type_mono(key: &str, checked: &CheckedFile) -> String {
    if key == "Array" {
        return key.to_string();
    }
    // C4c: upgrade `Array_Box` → `Array_demo_pkg_Box` when element is a known class.
    if let Some(elem) = key.strip_prefix("Array_") {
        if elem == "Int" || elem == "Bool" || elem == "String" {
            return key.to_string();
        }
        // Already package-mangled (contains `_` from pkg_Name) or simple class name.
        if let Some(c) = checked.ast.classes.iter().find(|c| c.name.name == elem) {
            let pkg = class_decl_package(c, checked);
            return mono_key("Array", &[Ty::Class(nominal_key(&pkg, elem))]);
        }
        // Leave fully-mangled keys (Array_demo_gen_Box) as-is.
        return key.to_string();
    }
    if let Some((base, args)) = mono_split(key, checked) {
        if base == "Array" {
            return mono_key(base, args);
        }
        // Prefer class/enum matching this mono key.
        if let Some(c) = checked.ast.classes.iter().find(|c| {
            c.name.name == base
                && (type_mono(&class_decl_package(c, checked), base, args) == key
                    || key == base
                    || mono_key(base, args) == key)
        }) {
            return type_mono(&class_decl_package(c, checked), base, args);
        }
        if let Some(e) = checked.ast.enums.iter().find(|e| {
            e.name.name == base
                && (type_mono(&enum_decl_package(e, checked), base, args) == key
                    || key == base
                    || mono_key(base, args) == key)
        }) {
            return type_mono(&enum_decl_package(e, checked), base, args);
        }
        if let Some(c) = checked.ast.classes.iter().find(|c| c.name.name == base) {
            return type_mono(&class_decl_package(c, checked), base, args);
        }
        if let Some(e) = checked.ast.enums.iter().find(|e| e.name.name == base) {
            return type_mono(&enum_decl_package(e, checked), base, args);
        }
        return mono_key(base, args);
    }
    key.to_string()
}

pub(crate) fn type_ref_to_ty(t: &TypeRef, ctx: &EmitCtx<'_>) -> Option<Ty> {
    if !t.type_args.is_empty() {
        let args: Vec<Ty> = t
            .type_args
            .iter()
            .filter_map(|a| type_ref_to_ty(a, ctx))
            .collect();
        // C4c: package-qualify class type args so Array mono matches emit.
        if t.name.name == "Array" {
            return Some(Ty::ClassApp {
                name: "Array".into(),
                args,
            });
        }
        let pkg = if let Some(q) = &t.qualifier {
            ctx.checked
                .ast
                .imports
                .iter()
                .find(|i| i.alias.as_ref().map(|a| a.name == q.name).unwrap_or(false))
                .map(|i| i.path.display())
                .unwrap_or_default()
        } else if let Some(c) = ctx
            .checked
            .ast
            .classes
            .iter()
            .find(|c| c.name.name == t.name.name)
        {
            class_decl_package(c, ctx.checked)
        } else {
            String::new()
        };
        return Some(Ty::ClassApp {
            name: nominal_key(&pkg, &t.name.name),
            args,
        });
    }
    match t.name.name.as_str() {
        "Int" => Some(Ty::Int),
        "Bool" => Some(Ty::Bool),
        "String" => Some(Ty::String),
        "Unit" => Some(Ty::Unit),
        name => {
            if let Some(c) = ctx
                .checked
                .ast
                .classes
                .iter()
                .find(|c| c.name.name == name)
            {
                let pkg = class_decl_package(c, ctx.checked);
                return Some(Ty::Class(nominal_key(&pkg, name)));
            }
            if let Some(i) = ctx
                .checked
                .ast
                .interfaces
                .iter()
                .find(|i| i.name.name == name)
            {
                let pkg = iface_decl_package(i, ctx.checked);
                return Some(Ty::Interface(nominal_key(&pkg, name)));
            }
            None
        }
    }
}

fn expected_iface_mono(expected_ty: &str, checked: &CheckedFile) -> Option<String> {
    // expected may be simple name, mono, or local key.
    let im = iface_mono_from_key(expected_ty, checked);
    if checked.ast.interfaces.iter().any(|i| {
        iface_mono(i, checked) == im || i.name.name == expected_ty
    }) {
        Some(im)
    } else {
        None
    }
}

/// If `expr` has class type `from` and expected is interface, emit upcast.
pub(crate) fn coerce_expr(expr: &Expr, expected_ty: &str, ctx: &mut EmitCtx<'_>) -> String {
    let actual = resolve_type_name(expr, ctx);
    let code = emit_expr(expr, ctx);
    let Some(imono) = expected_iface_mono(expected_ty, ctx.checked) else {
        return code;
    };
    let iface_simple = ctx
        .checked
        .ast
        .interfaces
        .iter()
        .find(|i| iface_mono(i, ctx.checked) == imono)
        .map(|i| i.name.name.as_str())
        .unwrap_or(expected_ty);

    if let Some(from) = actual {
        let class_mono = full_type_mono(&from, ctx.checked);
        let base = mono_base_name(&class_mono, ctx.checked).unwrap_or(from.as_str());
        if class_mono != imono
            && ctx.checked.ast.classes.iter().any(|c| {
                c.name.name == base && c.implements.iter().any(|i| i.name == iface_simple)
            })
        {
            return format!("{}({code})", c_upcast_name(&class_mono, &imono));
        }
    }
    // Constructor expr Greeter(...) inferred as class, expected interface
    if let Expr::Call(c) = expr {
        if let Expr::Ident(id) = c.callee.as_ref() {
            if let Some(cl) = ctx
                .checked
                .ast
                .classes
                .iter()
                .find(|cl| {
                    cl.name.name == id.name
                        && cl.implements.iter().any(|i| i.name == iface_simple)
                })
            {
                let pkg = class_decl_package(cl, ctx.checked);
                let cmono = type_mono(&pkg, &id.name, &[]);
                return format!("{}({code})", c_upcast_name(&cmono, &imono));
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
                        return Some(type_mono(&inst.package, &inst.name, &inst.type_args));
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
                // Class constructor when instantiation missing `is_constructor` (defensive).
                if let Some(class) = ctx
                    .checked
                    .ast
                    .classes
                    .iter()
                    .find(|x| x.name.name == id.name)
                {
                    let inst = ctx.checked.call_instantiations.get(&c.span.start);
                    let targs: Vec<Ty> = inst
                        .map(|i| i.type_args.clone())
                        .unwrap_or_else(|| {
                            c.type_args
                                .iter()
                                .filter_map(|t| type_ref_to_ty(t, ctx))
                                .collect()
                        });
                    let pkg = inst
                        .map(|i| i.package.as_str())
                        .filter(|p| !p.is_empty())
                        .unwrap_or_else(|| {
                            if class.origin_package.is_empty() {
                                ctx.checked.package.as_str()
                            } else {
                                class.origin_package.as_str()
                            }
                        });
                    return Some(type_mono(pkg, &id.name, &targs));
                }
                // Free function return: substitute type params (C4u).
                if let Some(f) = ctx
                    .checked
                    .ast
                    .functions
                    .iter()
                    .find(|f| f.name.name == id.name)
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
                    let params: Vec<String> =
                        f.type_params.iter().map(|p| p.name.name.clone()).collect();
                    return f
                        .return_type
                        .as_ref()
                        .map(|t| type_ref_local_key(t, &params, &targs));
                }
            }
            if let Expr::Field(fe) = c.callee.as_ref() {
                // C4u: method return with mono type-arg substitution (mirror infer_type_name).
                if let Some(recv) = resolve_type_name(&fe.object, ctx)
                    .or_else(|| resolve_class_of_expr(&fe.object, ctx).map(|s| s.to_string()))
                {
                    let base = mono_base_name(&recv, ctx.checked).unwrap_or(recv.as_str());
                    if let Some(class) = ctx
                        .checked
                        .ast
                        .classes
                        .iter()
                        .find(|c| c.name.name == base)
                    {
                        if let Some(m) = class
                            .methods
                            .iter()
                            .find(|m| m.name.name == fe.field.name)
                        {
                            if let Some(rt) = &m.return_type {
                                let (ps, as_) =
                                    if let Some((_, args)) = mono_split(&recv, ctx.checked) {
                                        let params: Vec<String> = class
                                            .type_params
                                            .iter()
                                            .map(|p| p.name.name.clone())
                                            .collect();
                                        (params, args.to_vec())
                                    } else if !ctx.type_args.is_empty()
                                        && class.name.name
                                            == ctx.method_class.unwrap_or("")
                                    {
                                        (ctx.type_params.clone(), ctx.type_args.clone())
                                    } else {
                                        (Vec::new(), Vec::new())
                                    };
                                return Some(type_ref_local_key(rt, &ps, &as_));
                            }
                        }
                    }
                    if let Some(m) = ctx
                        .checked
                        .ast
                        .interfaces
                        .iter()
                        .find(|i| {
                            i.name.name == recv
                                || iface_mono(i, ctx.checked) == recv
                                || i.name.name == base
                        })
                        .and_then(|i| i.methods.iter().find(|m| m.name.name == fe.field.name))
                    {
                        return m
                            .return_type
                            .as_ref()
                            .map(|t| type_ref_local_key(t, &[], &[]));
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
                || ty == "Array"
                || mono_split(ty, ctx.checked).is_some()
                || ctx.checked.ast.classes.iter().any(|c| c.name.name == ty)
                || ctx
                    .checked
                    .mono_classes
                    .iter()
                    .any(|(n, a)| mono_key(n, a) == ty)
            {
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
