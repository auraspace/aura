//! Expression emission.

use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;

use aura_ast::*;
use aura_sema::{nominal_key, CheckedFile, Ty};

use crate::array_emit::is_array_type_key;
use crate::call_emit::emit_call;
use crate::ctx::EmitCtx;
use crate::names::*;

pub(crate) fn infer_type_name(e: &Expr, ctx: &EmitCtx<'_>) -> String {
    match e {
        Expr::Int(_) => "Int".into(),
        Expr::Bool(_) => "Bool".into(),
        Expr::String(_) => "String".into(),
        Expr::Call(c) => match c.callee.as_ref() {
            // Free-function / Alias.fun via call_instantiations (return type from FunSig).
            Expr::Field(_) | Expr::Ident(_)
                if ctx
                    .checked
                    .call_instantiations
                    .get(&c.span.start)
                    .is_some_and(|i| !i.is_constructor && i.variant.is_none()) =>
            {
                let Some(inst) = ctx.checked.call_instantiations.get(&c.span.start) else {
                    return "Unit".into();
                };
                if let Some(f) = ctx.checked.ast.functions.iter().find(|f| {
                    f.name.name == inst.name
                        && (inst.package.is_empty()
                            || fun_decl_package(f, ctx.checked) == inst.package)
                }) {
                    let params: Vec<String> =
                        f.type_params.iter().map(|p| p.name.name.clone()).collect();
                    if let Some(rt) = &f.return_type {
                        return type_ref_local_key(rt, &params, &inst.type_args);
                    }
                    return "Unit".into();
                }
                if let Some(f) = ctx.checked.ast.foreign_functions.iter().find(|f| {
                    f.name.name == inst.name
                        && (inst.package.is_empty()
                            || foreign_decl_package(f, ctx.checked) == inst.package)
                }) {
                    if let Some(rt) = &f.return_type {
                        return type_ref_local_key(rt, &[], &inst.type_args);
                    }
                    return "Unit".into();
                }
                // Fall through to other match arms when decl not found.
                if let Expr::Ident(id) = c.callee.as_ref() {
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
                            return type_ref_local_key(rt, &params, &inst.type_args);
                        }
                    }
                }
                "Unit".into()
            }
            Expr::Ident(id)
                if ctx
                    .checked
                    .call_instantiations
                    .get(&c.span.start)
                    .and_then(|i| i.variant.as_ref())
                    .is_some() =>
            {
                let Some(inst) = ctx.checked.call_instantiations.get(&c.span.start) else {
                    return "Unit".into();
                };
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
                // Fall back to infer so arithmetic receivers work: (0-1).toString().
                let mono = resolve_type_name(&fe.object, ctx)
                    .or_else(|| resolve_class_of_expr(&fe.object, ctx).map(|s| s.to_string()))
                    .or_else(|| {
                        let t = infer_type_name(&fe.object, ctx);
                        if t == "Unit" || t.is_empty() {
                            None
                        } else {
                            Some(t)
                        }
                    });
                if let Some(mono) = mono {
                    let base = mono_base_name(&mono, ctx.checked).unwrap_or(mono.as_str());
                    // Builtin Array.get / Array.pop return element type T (C3j/C6g).
                    if (base == "Array" || mono.starts_with("Array_"))
                        && (fe.field.name == "get" || fe.field.name == "pop")
                    {
                        if let Some(elem) = array_elem_local_key(&mono, ctx.checked) {
                            return elem;
                        }
                    }
                    // C9c: Array.clone() returns same Array mono (owning copy).
                    if (base == "Array" || mono.starts_with("Array_")) && fe.field.name == "clone" {
                        return mono;
                    }
                    // Builtin String methods (return type for locals / assert_eq).
                    if mono == "String" || base == "String" {
                        match fe.field.name.as_str() {
                            "isEmpty" | "startsWith" | "contains" | "endsWith" => {
                                return "Bool".into();
                            }
                            "charAt" | "indexOf" | "len" | "hash" => return "Int".into(),
                            "substring" | "trim" | "trimStart" | "trimEnd" | "toLower"
                            | "toUpper" => {
                                return "String".into();
                            }
                            // C12g: split(sep) → Array<String>
                            "split" => return mono_key("Array", &[Ty::String]),
                            // C12i: toInt() → Int?
                            "toInt" => return "Opt_Int".into(),
                            _ => {}
                        }
                    }
                    // C13c: Int.toString() → String
                    if (mono == "Int" || base == "Int") && fe.field.name == "toString" {
                        return "String".into();
                    }
                    if (mono == "Int" || base == "Int") && fe.field.name == "hash" {
                        return "Int".into();
                    }
                    if let Some(m) = ctx
                        .checked
                        .ast
                        .classes
                        .iter()
                        .find(|c| c.name.name == base)
                        .and_then(|c| c.methods.iter().find(|m| m.name.name == fe.field.name))
                    {
                        if let Some(rt) = &m.return_type {
                            let (ps, as_) = if let Some((_, args)) = mono_split(&mono, ctx.checked)
                            {
                                let params: Vec<String> = ctx
                                    .checked
                                    .ast
                                    .classes
                                    .iter()
                                    .find(|c| c.name.name == base)
                                    .map(|c| {
                                        c.type_params.iter().map(|p| p.name.name.clone()).collect()
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
            // Prefer resolve_type_name so field chains (this.keys.len) and Array/String
            // `.len` resolve correctly (C6f).
            if let Some(t) = resolve_type_name(e, ctx) {
                return t;
            }
            if f.field.name == "len" {
                let recv = resolve_type_name(&f.object, ctx);
                if matches!(recv.as_deref(), Some("String"))
                    || matches!(f.object.as_ref(), Expr::String(_))
                {
                    return "Int".into();
                }
                if let Some(r) = recv.as_deref() {
                    if r == "Array" || r.starts_with("Array_") {
                        return "Int".into();
                    }
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
                            .map(|c| c.type_params.iter().map(|p| p.name.name.clone()).collect())
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
        Expr::Ident(i) => {
            if let Some(t) = ctx.lookup_local(&i.name) {
                return t.to_string();
            }
            // C9g: const type from annotation (via full_type / TypeRef).
            if let Some(c) = ctx
                .checked
                .ast
                .consts
                .iter()
                .find(|c| c.name.name == i.name)
            {
                return type_ref_local_key(&c.ty, &[], &[]);
            }
            "Int".into()
        }
        Expr::This(_) => ctx.method_class.unwrap_or("Int").to_string(),
        Expr::Group(inner, _) => infer_type_name(inner, ctx),
        Expr::Assign(a) => infer_type_name(&a.value, ctx),
        Expr::Unary(UnaryExpr { op: UnOp::Not, .. }) => "Bool".into(),
        Expr::Is(_) => "Bool".into(),
        Expr::ForceUnwrap(f) => {
            let inner = infer_type_name(&f.expr, ctx);
            // C7a: !! on Opt_Int/Opt_Bool yields the bare primitive key.
            if let Some(rest) = inner.strip_prefix("Opt_") {
                rest.to_string()
            } else {
                inner
            }
        }
        Expr::Binary(BinaryExpr {
            op:
                BinOp::Lt
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
        // C9d: String + String; C13c: String + Int / Int + String.
        // Int + Int must stay Int (do not treat as concat).
        Expr::Binary(BinaryExpr {
            op: BinOp::Add,
            left,
            right,
            ..
        }) => {
            let lt = infer_type_name(left, ctx);
            let rt = infer_type_name(right, ctx);
            if lt == "String" || rt == "String" {
                "String".into()
            } else {
                "Int".into()
            }
        }
        Expr::Lambda(l) => {
            if let Some(ty) = ctx.checked.lambda_tys.get(&l.span.start) {
                return ty.mono_suffix();
            }
            "Int".into()
        }
        Expr::If(i) => match i.then_block.stmts.last() {
            Some(Stmt::Expr(e)) => infer_type_name(e, ctx),
            _ => "Int".into(),
        },
        Expr::Async(AsyncExpr::Spawn(_)) => "TaskHandle_Unit".into(),
        Expr::Async(AsyncExpr::Join(j)) => async_inner_key_for_infer(&j.handle, ctx),
        Expr::Async(AsyncExpr::Cancel(_)) => "Unit".into(),
        Expr::Async(AsyncExpr::Await(a)) => async_inner_key_for_infer(&a.operand, ctx),
        Expr::Async(AsyncExpr::ChannelCreate(c)) => {
            format!("Channel_{}", type_ref_local_key(&c.element_type, &[], &[]))
        }
        Expr::Async(AsyncExpr::ChannelSend(_)) | Expr::Async(AsyncExpr::ChannelClose(_)) => {
            "Unit".into()
        }
        Expr::Async(AsyncExpr::ChannelReceive(r)) => {
            let channel = resolve_type_name(&r.channel, ctx)
                .unwrap_or_else(|| infer_type_name(&r.channel, ctx));
            let inner = channel_inner_key(&channel).unwrap_or("Unit");
            if inner == "Int" {
                "Opt_Int".into()
            } else {
                inner.to_string()
            }
        }
        _ => "Int".into(),
    }
}

fn foreign_decl_package(foreign: &ForeignDecl, checked: &CheckedFile) -> String {
    if foreign.origin_package.is_empty() {
        checked.package.clone()
    } else {
        foreign.origin_package.clone()
    }
}

/// Bounded C22l lowering: a spawn body may contain only calls with literal
/// arguments, or copied `Int`/`String`/heap-class/primitive-Array parameters
/// from the enclosing function, and an optional unit return. String captures
/// are copied into owned boxes, class captures are rooted, and bounded Array
/// captures are cloned by the frame emitter.
pub(crate) fn bounded_spawn_captures(
    body: &Block,
    available: &HashMap<String, String>,
    checked: &CheckedFile,
) -> Option<Vec<(String, String)>> {
    if body.stmts.iter().any(
        |stmt| matches!(stmt, Stmt::Var(v) if matches!(&v.init, Expr::Async(AsyncExpr::Await(_)))),
    ) && bounded_spawn_await_shape(body, checked).is_none()
    {
        return None;
    }
    let mut returned = false;
    let mut captures = BTreeSet::new();
    for stmt in &body.stmts {
        if returned {
            return None;
        }
        match stmt {
            Stmt::Expr(Expr::Call(call))
                if matches!(call.callee.as_ref(), Expr::Ident(_))
                    && call
                        .args
                        .iter()
                        .all(|arg| bounded_spawn_value(arg, available, &mut captures, checked)) => {
            }
            Stmt::Return(ret) if ret.value.is_none() => returned = true,
            Stmt::Var(v)
                if matches!(&v.init, Expr::Async(AsyncExpr::Await(_)))
                    && v.ty
                        .as_ref()
                        .map(|ty| type_ref_local_key_expand(ty, &[], &[], checked) == "Int")
                        .unwrap_or(false) => {}
            _ => return None,
        }
    }
    Some(
        captures
            .into_iter()
            .filter_map(|name| available.get(&name).map(|ty| (name, ty.clone())))
            .collect(),
    )
}

/// Bounded spawn suspension shape: the first statement awaits an `Int` task,
/// and the remaining body is effect-only. Captures are therefore copied into
/// the frame before submission and materialized only after the child reaches
/// a terminal state, so temporary Array/Fun clones never span a pending poll.
pub(crate) fn bounded_spawn_await_shape<'a>(
    body: &'a Block,
    checked: &CheckedFile,
) -> Option<&'a AwaitExpr> {
    let Stmt::Var(await_var) = body.stmts.first()? else {
        return None;
    };
    let Expr::Async(AsyncExpr::Await(await_expr)) = &await_var.init else {
        return None;
    };
    if await_var
        .ty
        .as_ref()
        .map(|ty| type_ref_local_key_expand(ty, &[], &[], checked) == "Int")
        .unwrap_or(false)
        && body.stmts[1..].iter().all(|stmt| match stmt {
            Stmt::Expr(Expr::Call(_)) => true,
            Stmt::Return(ret) => ret.value.is_none(),
            _ => false,
        })
    {
        Some(await_expr)
    } else {
        None
    }
}

fn bounded_spawn_value(
    expr: &Expr,
    available: &HashMap<String, String>,
    captures: &mut BTreeSet<String>,
    checked: &CheckedFile,
) -> bool {
    match expr {
        Expr::Int(_) | Expr::Bool(_) | Expr::String(_) | Expr::Null(_) => true,
        Expr::Ident(id) => {
            if available.get(&id.name).is_some_and(|ty| {
                ty == "Int"
                    || ty == "String"
                    || is_heap_class_mono(ty, checked)
                    || ty == "Array_Int"
                    || ty == "Array_String"
                    || is_fun_type_key(ty)
            }) {
                captures.insert(id.name.clone());
                true
            } else {
                false
            }
        }
        Expr::Group(inner, _) => bounded_spawn_value(inner, available, captures, checked),
        Expr::Unary(unary) => bounded_spawn_value(&unary.expr, available, captures, checked),
        Expr::Binary(binary) => {
            bounded_spawn_value(&binary.left, available, captures, checked)
                && bounded_spawn_value(&binary.right, available, captures, checked)
        }
        _ => false,
    }
}

pub(crate) fn bounded_spawn_poll_name(span: Span) -> String {
    format!("aura_spawn_poll_{}", span.start)
}

pub(crate) fn bounded_spawn_destroy_name(span: Span) -> String {
    format!("aura_spawn_destroy_{}", span.start)
}

fn race_read(value: String, lvalue: String, span: Span, ctx: &EmitCtx<'_>) -> String {
    if !ctx.detector {
        return value;
    }
    format!(
        "({{ aura_race_record_access((uintptr_t)&({lvalue}), UINT32_C({}), AURA_RACE_READ); {value}; }})",
        span.start
    )
}

pub(crate) fn race_write(code: String, lvalue: &str, span: Span, ctx: &EmitCtx<'_>) -> String {
    if !ctx.detector {
        return code;
    }
    format!(
        "({{ aura_race_record_access((uintptr_t)&({lvalue}), UINT32_C({}), AURA_RACE_WRITE); {code}; }})",
        span.start
    )
}

/// Whether evaluating this String expression produces a temporary allocation
/// that a consuming expression must release after copying or inspection.
pub(crate) fn string_expr_is_owned_temp(e: &Expr, ctx: &EmitCtx<'_>) -> bool {
    match e {
        Expr::Binary(b) => matches!(b.op, BinOp::Add),
        Expr::Call(_) => crate::stmt::string_call_owns_result(e, ctx),
        Expr::ForceUnwrap(f) => string_expr_is_owned_temp(&f.expr, ctx),
        Expr::Group(inner, _) => string_expr_is_owned_temp(inner, ctx),
        _ => false,
    }
}

/// Copy a borrowed/string-literal expression into an owned String value.
/// Mutable locals use this at declaration so later assignments can always
/// release the previous value without attempting to free static storage or a
/// borrowed function parameter.
pub(crate) fn owned_string_copy_expr(code: String, span: Span) -> String {
    let tmp = format!("__aura_string_init_{}", span.start);
    format!(
        "({{ const char *__s = {code}; size_t __n = __s ? strlen(__s) : 0; char *{tmp} = (char *)malloc(__n + 1); if ({tmp} == NULL) {{ fputs(\"aura: String copy OOM\\n\", stderr); abort(); }} if (__n > 0) memcpy({tmp}, __s, __n); {tmp}[__n] = '\\0'; (const char *){tmp}; }})"
    )
}

pub(crate) fn emit_expr(expr: &Expr, ctx: &mut EmitCtx<'_>) -> String {
    match expr {
        Expr::Ident(i) => {
            // Inside method: bare field names → this->field
            if let Some(class) = ctx.method_class {
                let base = mono_base_name(class, ctx.checked).unwrap_or(class);
                if let Some(cl) = ctx.checked.ast.classes.iter().find(|c| c.name.name == base) {
                    if cl.fields.iter().any(|f| f.name.name == i.name) {
                        let field = format!("this->{}", mangle_ident(&i.name));
                        return race_read(field.clone(), field, i.span, ctx);
                    }
                }
            }
            // C9g: top-level const → emit initializer expression.
            if let Some(c) = ctx
                .checked
                .ast
                .consts
                .iter()
                .find(|c| c.name.name == i.name)
            {
                return emit_expr(&c.value, ctx);
            }
            // C12m/C13f: by-ref boxed locals read through the box.
            // String: snapshot (heap copy) so later set does not invalidate escaped pointers.
            if ctx.is_box_local(&i.name) {
                let m = mangle_ident(&i.name);
                let key = ctx.lookup_local(&i.name).unwrap_or("Int");
                if key == "String" {
                    return format!("aura_box_str_get({m})");
                }
                if is_array_type_key(key) || is_fun_type_key(key) {
                    let cty = crate::stmt::local_key_to_c(key, ctx.checked);
                    return format!("(*({cty} *)aura_box_ptr_get({m}))");
                }
                if is_heap_class_mono(key, ctx.checked) {
                    return format!(
                        "((({})((aura_capture_obj_payload *)aura_box_ptr_get({m}))->value))",
                        crate::stmt::local_key_to_c(key, ctx.checked)
                    );
                }
                return format!("({m})->value");
            }
            let value = mangle_ident(&i.name);
            let local_type = ctx.lookup_local(&i.name);
            if local_type.is_some_and(|key| !key.starts_with("Array_")) {
                return race_read(value.clone(), value, i.span, ctx);
            }
            value
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
            let inner = emit_expr(&f.expr, ctx);
            let ty =
                resolve_type_name(&f.expr, ctx).unwrap_or_else(|| infer_type_name(&f.expr, ctx));
            if is_opt_prim_key(&ty) {
                // C7a: check `.has`, yield `.value`.
                let cty = opt_prim_c_type(&ty).unwrap_or("aura_opt_i64");
                return format!(
                    "({{ {cty} __u = ({inner}); if (!__u.has) aura_throw_string(\"force unwrap null\"); __u.value; }})"
                );
            }
            // Pointer-like T?: null is a runtime fault (MVP).
            inner
        }
        // C9i: `expr is Type` — interface tag check or class mono equality.
        Expr::Is(i) => {
            let val = emit_expr(&i.expr, ctx);
            let recv =
                resolve_type_name(&i.expr, ctx).unwrap_or_else(|| infer_type_name(&i.expr, ctx));
            let target_key = type_ref_local_key_expand(&i.ty, &[], &[], ctx.checked);
            let target_mono = full_type_mono(&target_key, ctx.checked);
            // Interface-typed receiver: compare runtime tag.
            if is_iface_type_key(&recv, ctx.checked) {
                // tag lives on the iface struct value.
                return format!("({val}).tag == AURA_TAG_{target_mono}");
            }
            // Class receiver: compile-time mono match (or false if different).
            let recv_mono = full_type_mono(&recv, ctx.checked);
            if recv_mono == target_mono {
                "true".into()
            } else {
                "false".into()
            }
        }
        Expr::Binary(b) => {
            let left = emit_expr(&b.left, ctx);
            let right = emit_expr(&b.right, ctx);
            let lt =
                resolve_type_name(&b.left, ctx).unwrap_or_else(|| infer_type_name(&b.left, ctx));
            let rt =
                resolve_type_name(&b.right, ctx).unwrap_or_else(|| infer_type_name(&b.right, ctx));
            // C9d: String + String → heap concat (const char *).
            // C13c: String + Int / Int + String — coerce Int via aura_i64_to_string.
            if matches!(b.op, BinOp::Add) {
                let left_str = lt == "String" || matches!(b.left.as_ref(), Expr::String(_));
                let right_str = rt == "String" || matches!(b.right.as_ref(), Expr::String(_));
                let left_int = lt == "Int" || matches!(b.left.as_ref(), Expr::Int(_));
                let right_int = rt == "Int" || matches!(b.right.as_ref(), Expr::Int(_));
                if left_str && right_str {
                    let free_left = if string_expr_is_owned_temp(&b.left, ctx) {
                        " free((void *)__a);"
                    } else {
                        ""
                    };
                    let free_right = if string_expr_is_owned_temp(&b.right, ctx) {
                        " free((void *)__b);"
                    } else {
                        ""
                    };
                    return format!(
                        "({{ const char *__a = ({left}); const char *__b = ({right}); \
                         size_t __la = __a ? strlen(__a) : 0; size_t __lb = __b ? strlen(__b) : 0; \
                         char *__r = (char *)malloc(__la + __lb + 1); \
                         if (__r == NULL) {{ fputs(\"aura: string concat OOM\\n\", stderr); abort(); }} \
                         if (__la) memcpy(__r, __a, __la); if (__lb) memcpy(__r + __la, __b, __lb); \
                         __r[__la + __lb] = '\\0';{free_left}{free_right} (const char *)__r; }})"
                    );
                }
                if left_str && right_int {
                    // Free the temporary decimal from toString after memcpy into result.
                    let free_left = if string_expr_is_owned_temp(&b.left, ctx) {
                        " free((void *)__a);"
                    } else {
                        ""
                    };
                    return format!(
                        "({{ const char *__a = ({left}); const char *__b = aura_i64_to_string({right}); \
                         size_t __la = __a ? strlen(__a) : 0; size_t __lb = __b ? strlen(__b) : 0; \
                         char *__r = (char *)malloc(__la + __lb + 1); \
                         if (__r == NULL) {{ fputs(\"aura: string concat OOM\\n\", stderr); abort(); }} \
                         if (__la) memcpy(__r, __a, __la); if (__lb) memcpy(__r + __la, __b, __lb); \
                         __r[__la + __lb] = '\\0'; free((void *)__b);{free_left} (const char *)__r; }})"
                    );
                }
                if left_int && right_str {
                    let free_right = if string_expr_is_owned_temp(&b.right, ctx) {
                        " free((void *)__b);"
                    } else {
                        ""
                    };
                    return format!(
                        "({{ const char *__a = aura_i64_to_string({left}); const char *__b = ({right}); \
                         size_t __la = __a ? strlen(__a) : 0; size_t __lb = __b ? strlen(__b) : 0; \
                         char *__r = (char *)malloc(__la + __lb + 1); \
                         if (__r == NULL) {{ fputs(\"aura: string concat OOM\\n\", stderr); abort(); }} \
                         if (__la) memcpy(__r, __a, __la); if (__lb) memcpy(__r + __la, __b, __lb); \
                         __r[__la + __lb] = '\\0'; free((void *)__a);{free_right} (const char *)__r; }})"
                    );
                }
            }
            // C4e: String content equality (null-safe strcmp); class stays pointer identity.
            if matches!(b.op, BinOp::Coalesce) {
                // C7a: optional primitives use `.has` / `.value`.
                if is_opt_prim_key(&lt) {
                    let cty = opt_prim_c_type(&lt).unwrap_or("aura_opt_i64");
                    return format!(
                        "({{ {cty} __c = ({left}); __c.has ? __c.value : ({right}); }})"
                    );
                }
                // C4m: pointer/string null-coalesce ternary.
                return format!("(({left}) != NULL ? ({left}) : ({right}))");
            }
            if matches!(b.op, BinOp::Eq | BinOp::Ne) {
                // C7a: `x == null` / `x != null` on Int?/Bool? → `.has`.
                let left_null = matches!(b.right.as_ref(), Expr::Null(_)) && is_opt_prim_key(&lt);
                let right_null = matches!(b.left.as_ref(), Expr::Null(_)) && is_opt_prim_key(&rt);
                if left_null {
                    return if matches!(b.op, BinOp::Eq) {
                        format!("(!({left}).has)")
                    } else {
                        format!("(({left}).has)")
                    };
                }
                if right_null {
                    return if matches!(b.op, BinOp::Eq) {
                        format!("(!({right}).has)")
                    } else {
                        format!("(({right}).has)")
                    };
                }
                let is_string = lt == "String"
                    || rt == "String"
                    || matches!(
                        (&*b.left, &*b.right),
                        (Expr::String(_), _) | (_, Expr::String(_))
                    );
                if is_string {
                    // Both non-null and equal content, or both null.  String
                    // helpers (notably Array<String>.get) return owned copies;
                    // bind each side once so comparisons neither leak nor
                    // evaluate an allocating expression twice.
                    let free_left = if string_expr_is_owned_temp(&b.left, ctx) {
                        " free((void *)__a);"
                    } else {
                        ""
                    };
                    let free_right = if string_expr_is_owned_temp(&b.right, ctx) {
                        " free((void *)__b);"
                    } else {
                        ""
                    };
                    let cmp =
                        "(__a == NULL ? __b == NULL : (__b != NULL && strcmp(__a, __b) == 0))";
                    let result = if matches!(b.op, BinOp::Ne) {
                        format!("!{cmp}")
                    } else {
                        cmp.into()
                    };
                    return format!(
                        "({{ const char *__a = ({left}); const char *__b = ({right}); bool __eq = {result};{free_left}{free_right} __eq; }})"
                    );
                }
            }
            // C7a: after flow narrowing, Opt_* locals still hold tagged structs — use `.value`.
            let left_v = if is_opt_prim_key(&lt) {
                format!("({left}).value")
            } else {
                left.clone()
            };
            let right_v = if is_opt_prim_key(&rt) {
                format!("({right}).value")
            } else {
                right.clone()
            };
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
                    format!("{left_v} {op} {right_v}")
                }
                _ => format!("({left_v} {op} {right_v})"),
            }
        }
        Expr::Assign(a) => {
            // field assign in method for bare field name
            let dst_is_field = ctx.method_class.is_some_and(|class| {
                let base = mono_base_name(class, ctx.checked).unwrap_or(class);
                ctx.checked
                    .ast
                    .classes
                    .iter()
                    .find(|c| c.name.name == base)
                    .is_some_and(|cl| cl.fields.iter().any(|f| f.name.name == a.name.name))
            });
            let lhs = if dst_is_field {
                format!("this->{}", mangle_ident(&a.name.name))
            } else if ctx.is_box_local(&a.name.name) {
                // C12m: assign through shared mutable box.
                let key = ctx.lookup_local(&a.name.name).unwrap_or("Int");
                if is_array_type_key(key) || is_fun_type_key(key) {
                    let cty = crate::stmt::local_key_to_c(key, ctx.checked);
                    format!(
                        "(*({cty} *)aura_box_ptr_get({}))",
                        mangle_ident(&a.name.name)
                    )
                } else if is_heap_class_mono(key, ctx.checked) {
                    format!(
                        "(((aura_capture_obj_payload *)aura_box_ptr_get({}))->value)",
                        mangle_ident(&a.name.name)
                    )
                } else {
                    format!("({})->value", mangle_ident(&a.name.name))
                }
            } else {
                mangle_ident(&a.name.name)
            };
            let dst_name = &a.name.name;
            let dst_ty = ctx.lookup_local(dst_name).map(|s| s.to_string());
            let rhs = if let Some(ref dt) = dst_ty {
                coerce_expr(&a.value, dt, ctx)
            } else {
                emit_expr(&a.value, ctx)
            };
            // C13f: String by-ref box owns a heap copy; set frees the previous value.
            if ctx.is_box_local(dst_name) && dst_ty.as_deref() == Some("String") {
                let box_ptr = mangle_ident(dst_name);
                return format!("aura_box_str_set({box_ptr}, {rhs})");
            }
            if ctx.is_box_local(dst_name)
                && dst_ty.as_deref().is_some_and(|k| {
                    is_array_type_key(k) || is_fun_type_key(k) || is_heap_class_mono(k, ctx.checked)
                })
            {
                let key = dst_ty.as_deref().unwrap_or("Int");
                let cty = crate::stmt::local_key_to_c(key, ctx.checked);
                let payload = format!("__capture_set_{}", a.span.start);
                let drop = if is_array_type_key(key) {
                    format!("aura_capture_drop_{key}")
                } else if is_fun_type_key(key) {
                    "aura_capture_drop_fun".into()
                } else {
                    "aura_capture_drop_obj".into()
                };
                if is_heap_class_mono(key, ctx.checked) {
                    return format!(
                        "({{ aura_capture_obj_payload *__p = (aura_capture_obj_payload *)malloc(sizeof(aura_capture_obj_payload)); __p->value = (void *)({rhs}); aura_gc_add_root(&__p->value); aura_box_ptr_set({}, __p, {drop}); ({})((aura_capture_obj_payload *)aura_box_ptr_get({}))->value; }})",
                        mangle_ident(dst_name),
                        crate::stmt::local_key_to_c(key, ctx.checked),
                        mangle_ident(dst_name)
                    );
                }
                return format!(
                    "({{ {cty} *{payload} = ({cty} *)malloc(sizeof({cty})); *{payload} = {rhs}; aura_box_ptr_set({}, {payload}, {drop}); *({cty} *)aura_box_ptr_get({}); }})",
                    mangle_ident(dst_name),
                    mangle_ident(dst_name)
                );
            }
            // Move ownership for heap-backed String locals.  Array and Fun have
            // analogous move paths below; without this branch `dst = src`
            // leaves both locals pointing at the same allocation while the
            // source is released at scope exit.
            if dst_ty.as_deref() == Some("String") {
                if let Expr::Ident(src) = a.value.as_ref() {
                    if ctx.is_string_owner(&src.name) && src.name != *dst_name {
                        let source = mangle_ident(&src.name);
                        let free_dst = if ctx.is_string_owner(dst_name) {
                            format!("if ({lhs} != NULL) {{ free((void *){lhs}); {lhs} = NULL; }} ")
                        } else {
                            String::new()
                        };
                        ctx.unmark_string_owner(&src.name);
                        ctx.mark_string_owner(dst_name);
                        return format!("({{ {free_dst}{lhs} = {rhs}; {source} = NULL; }})");
                    }
                }
                if let Expr::ForceUnwrap(inner) = a.value.as_ref() {
                    if let Expr::Ident(src) = inner.expr.as_ref() {
                        if ctx.is_string_owner(&src.name) && src.name != *dst_name {
                            let source = mangle_ident(&src.name);
                            let free_dst = if ctx.is_string_owner(dst_name) {
                                format!(
                                    "if ({lhs} != NULL) {{ free((void *){lhs}); {lhs} = NULL; }} "
                                )
                            } else {
                                String::new()
                            };
                            ctx.unmark_string_owner(&src.name);
                            ctx.mark_string_owner(dst_name);
                            return format!("({{ {free_dst}{lhs} = {rhs}; {source} = NULL; }})");
                        }
                    }
                }
                if matches!(a.value.as_ref(), Expr::Binary(_))
                    || crate::stmt::string_call_owns_result(&a.value, ctx)
                {
                    // Evaluate the RHS first. In `s = f(s)`, freeing `s`
                    // before calling `f` turns the argument into a dangling
                    // pointer and can later surface as a double-free.
                    let rhs_tmp = format!("__aura_string_rhs_{}", a.span.start);
                    let free_dst = if ctx.is_string_owner(dst_name) {
                        format!("if ({lhs} != NULL) {{ free((void *){lhs}); {lhs} = NULL; }} ")
                    } else {
                        String::new()
                    };
                    ctx.mark_string_owner(dst_name);
                    return format!(
                        "({{ const char *{rhs_tmp} = {rhs}; {free_dst}{lhs} = {rhs_tmp}; }})"
                    );
                }
                // Borrowed identifiers and literals must be copied before
                // assigning into an owning String destination. Otherwise the
                // destination becomes responsible for static/borrowed storage
                // and its scope drop calls free() on an invalid pointer.
                let copied_rhs = owned_string_copy_expr(rhs, a.value.span());
                let rhs_tmp = format!("__aura_string_rhs_{}", a.span.start);
                let free_dst = if ctx.is_string_owner(dst_name) {
                    format!("if ({lhs} != NULL) {{ free((void *){lhs}); {lhs} = NULL; }} ")
                } else {
                    String::new()
                };
                ctx.mark_string_owner(dst_name);
                return format!(
                    "({{ const char *{rhs_tmp} = {copied_rhs}; {free_dst}{lhs} = {rhs_tmp}; }})"
                );
            }
            let dst_is_array = dst_ty
                .as_deref()
                .map(|t| t == "Array" || t.starts_with("Array_"))
                .unwrap_or(false);
            // C4r/C6d: free previous Array buffer when reassigning an owning local from
            // Array(...) or any call that transfers an Array result.
            // C6i: Array fields always own — free old field buffer on reassignment.
            let is_array_call = matches!(a.value.as_ref(), Expr::Call(_));
            if dst_is_array && is_array_call {
                let free_dst = if dst_is_field || ctx.is_array_owner(dst_name) {
                    crate::array_emit::array_contents_free_expr(
                        &lhs,
                        dst_ty.as_deref().unwrap_or("Array"),
                    )
                } else {
                    String::new()
                };
                if !dst_is_field {
                    ctx.mark_array_owner(dst_name);
                }
                let rhs_tmp = format!("__aura_array_rhs_{}", a.span.start);
                let cty =
                    crate::stmt::local_key_to_c(dst_ty.as_deref().unwrap_or("Array"), ctx.checked);
                return format!("({{ {cty} {rhs_tmp} = {rhs}; {free_dst}({lhs} = {rhs_tmp}); }})");
            }
            // C5e/C6i: move ownership on `b = a` / `field = a` when `a` owns an Array buffer.
            if dst_is_array {
                if let Expr::Ident(src) = a.value.as_ref() {
                    if ctx.is_array_owner(&src.name) && src.name != *dst_name {
                        let s = mangle_ident(&src.name);
                        // Free old dst if local owner or field (field always owns).
                        let free_dst = if dst_is_field || ctx.is_array_owner(dst_name) {
                            crate::array_emit::array_contents_free_expr(
                                &lhs,
                                dst_ty.as_deref().unwrap_or("Array"),
                            )
                        } else {
                            String::new()
                        };
                        if !dst_is_field {
                            ctx.mark_array_owner(dst_name);
                        }
                        ctx.unmark_array_owner(&src.name);
                        return format!(
                            "({{ {free_dst}({lhs} = {rhs}); {s}.data = NULL; {s}.len = 0; {s}.cap = 0; }})"
                        );
                    }
                }
                // C8j: Array field assign is a non-owning shallow copy (no move-out).
                // Deep ownership transfer remains on return (C7c) and local-to-local moves.
            }
            let dst_is_fun = dst_ty.as_deref().map(is_fun_type_key).unwrap_or(false);
            let free_fun_lvalue = |lv: &str| -> String {
                format!(
                    "if (({lv}).env != NULL) {{ aura_fun_env_free(({lv}).env); ({lv}).env = NULL; }} "
                )
            };
            // Reassign Fun from capturing lambda or call: free old env first.
            let is_fun_new = matches!(a.value.as_ref(), Expr::Lambda(_) | Expr::Call(_));
            if dst_is_fun && is_fun_new && !dst_is_field {
                let free_dst = if ctx.is_fun_owner(dst_name) {
                    free_fun_lvalue(&lhs)
                } else {
                    String::new()
                };
                // Capturing lambda / call result owns env.
                let owns = match a.value.as_ref() {
                    Expr::Lambda(l) => ctx
                        .checked
                        .lambda_captures
                        .get(&l.span.start)
                        .map(|c| !c.is_empty())
                        .unwrap_or(false),
                    Expr::Call(_) => true,
                    _ => false,
                };
                if owns {
                    ctx.mark_fun_owner(dst_name);
                } else {
                    ctx.unmark_fun_owner(dst_name);
                }
                return format!("({{ {free_dst}({lhs} = {rhs}); }})");
            }
            // Move Fun owner local-to-local.
            if dst_is_fun && !dst_is_field {
                if let Expr::Ident(src) = a.value.as_ref() {
                    if ctx.is_fun_owner(&src.name) && src.name != *dst_name {
                        let s = mangle_ident(&src.name);
                        let free_dst = if ctx.is_fun_owner(dst_name) {
                            free_fun_lvalue(&lhs)
                        } else {
                            String::new()
                        };
                        ctx.mark_fun_owner(dst_name);
                        ctx.unmark_fun_owner(&src.name);
                        return format!("({{ {free_dst}({lhs} = {rhs}); {s}.env = NULL; }})");
                    }
                }
            }
            race_write(format!("({lhs} = {rhs})"), &lhs, a.span, ctx)
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
            let access = field_access_c(&obj, f, ctx);
            if f.safe {
                format!("(({obj}) == NULL ? NULL : {access})")
            } else {
                access
            }
        }
        Expr::Call(c) => emit_call(c, ctx),
        // C10e/h: lambda → fat pointer `{ .env, .fn }`; env heap-alloc when capturing.
        Expr::Lambda(l) => {
            let id = ctx.lambda_ids.get(&l.span.start).copied().unwrap_or(0);
            let fun_ty = ctx
                .checked
                .lambda_tys
                .get(&l.span.start)
                .cloned()
                .unwrap_or(Ty::Fun {
                    params: Vec::new(),
                    ret: Box::new(Ty::Int),
                });
            let fp = c_fun_typedef(&fun_ty.mono_suffix());
            let captures = ctx
                .checked
                .lambda_captures
                .get(&l.span.start)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            if captures.is_empty() {
                format!("(({fp}){{ .env = NULL, .fn = aura_lambda_{id} }})")
            } else {
                // GNU statement-expr: allocate env, set drop+refs, fill captures, root GC slots.
                // C12l: Array fields are header copies (view); outer scope still owns the buffer.
                // C12m: by-ref slots store box pointers and retain.
                // C13e: Fun slots copy fat pointer and retain nested env (shared RC).
                let mut fill = String::new();
                let _ = writeln!(fill, "  __e->__drop = aura_lenv_{id}_drop;");
                let _ = writeln!(fill, "  __e->__refs = 1;");
                for cap in captures {
                    let m = mangle_ident(&cap.name);
                    // Capture from enclosing scope local of the same name.
                    let _ = writeln!(fill, "  __e->{m} = {m};");
                    if cap.by_ref {
                        if crate::names::is_array_capture_ty(&cap.ty)
                            || crate::names::is_fun_capture_ty(&cap.ty)
                            || crate::names::is_heap_class_capture_ty(&cap.ty, ctx.checked)
                        {
                            let _ = writeln!(fill, "  aura_box_ptr_retain(__e->{m});");
                        } else {
                            let ret = crate::names::box_retain_fn(&cap.ty.mono_suffix());
                            let _ = writeln!(fill, "  {ret}(__e->{m});");
                        }
                    } else if crate::names::is_fun_capture_ty(&cap.ty) {
                        let _ = writeln!(fill, "  aura_fun_env_retain(__e->{m}.env);");
                    } else if crate::names::is_heap_class_capture_ty(&cap.ty, ctx.checked) {
                        let _ = writeln!(fill, "  aura_gc_add_root((void **)&__e->{m});");
                    }
                }
                format!(
                    "({{ aura_lenv_{id} *__e = (aura_lenv_{id} *)malloc(sizeof(aura_lenv_{id}));{fill} ({fp}){{ .env = __e, .fn = aura_lambda_{id} }}; }})"
                )
            }
        }
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
        // C22 plumbing: lowering is owned by the C22 codegen task.
        Expr::Async(async_expr) => emit_async_expr(async_expr, ctx),
    }
}

fn task_inner_key(key: &str) -> Option<&str> {
    key.strip_prefix("TaskHandle_")
        .or_else(|| key.strip_prefix("Task_"))
}

fn channel_inner_key(key: &str) -> Option<&str> {
    key.strip_prefix("Channel_")
}

fn async_inner_key_for_infer(expr: &Expr, ctx: &EmitCtx<'_>) -> String {
    let key = infer_type_name(expr, ctx);
    task_inner_key(&key).unwrap_or("Unit").to_string()
}

pub(crate) fn async_inner_key(expr: &Expr, ctx: &EmitCtx<'_>) -> String {
    let key = infer_type_name(expr, ctx);
    task_inner_key(&key).unwrap_or("Unit").to_string()
}

fn emit_async_expr(expr: &AsyncExpr, ctx: &mut EmitCtx<'_>) -> String {
    match expr {
        AsyncExpr::Spawn(s) => {
            if s.body.stmts.is_empty() {
                let source = s.span.start;
                return format!("({{ AuraTaskFrame *__spawn = aura_task_frame_new(0, aura_task_poll_unit, NULL); if (__spawn != NULL) aura_task_frame_set_race_source_id(__spawn, UINT32_C({source})); if (__spawn != NULL && (__aura_task_executor == NULL || !aura_task_executor_submit(__aura_task_executor, __spawn))) {{ aura_task_frame_destroy(__spawn); __spawn = NULL; }} __spawn; }})");
            }
            let available = ctx.spawn_capture_types();
            let Some(captures) = bounded_spawn_captures(&s.body, &available, ctx.checked) else {
                return "({ fputs(\"aura: non-empty spawn body requires C22l state-machine lowering\\n\", stderr); abort(); (AuraTaskFrame *)NULL; })".to_string();
            };
            let poll = bounded_spawn_poll_name(s.span);
            let source = s.span.start;
            let has_await = bounded_spawn_await_shape(&s.body, ctx.checked).is_some();
            let data_size = if captures.is_empty() && !has_await {
                "0".to_string()
            } else {
                format!("sizeof(aura_spawn_data_{})", s.span.start)
            };
            let init = if captures.is_empty() {
                String::new()
            } else {
                let assignments = captures
                    .iter()
                    .map(|(name, key)| {
                        let n = mangle_ident(name);
                        if key == "String" {
                            format!("__spawn_data->{n} = aura_box_str_new({n});")
                        } else if is_heap_class_mono(key, ctx.checked) {
                            format!("__spawn_data->{n} = {n}; aura_gc_add_root((void **)&__spawn_data->{n});")
                        } else if key == "Array_Int" || key == "Array_String" {
                            format!("__spawn_data->{n} = {}(&{n});", crate::names::c_method_name(key, "clone"))
                        } else if is_fun_type_key(key) {
                            format!("__spawn_data->{n} = {n}; if (__spawn_data->{n}.env != NULL) aura_fun_env_retain(__spawn_data->{n}.env);")
                        } else {
                            format!("__spawn_data->{n} = {n};")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                format!(
                    "aura_spawn_data_{} *__spawn_data = (aura_spawn_data_{0} *)aura_task_frame_data(__spawn); {assignments}",
                    s.span.start
                )
            };
            let destroy = if captures.is_empty() {
                "NULL".to_string()
            } else {
                bounded_spawn_destroy_name(s.span)
            };
            format!("({{ AuraTaskFrame *__spawn = aura_task_frame_new({data_size}, {poll}, {destroy}); if (__spawn != NULL) {{ aura_task_frame_set_race_source_id(__spawn, UINT32_C({source})); {init} }} if (__spawn != NULL && (__aura_task_executor == NULL || !aura_task_executor_submit(__aura_task_executor, __spawn))) {{ aura_task_frame_destroy(__spawn); __spawn = NULL; }} __spawn; }})")
        }
        AsyncExpr::Join(j) => {
            let handle = emit_expr(&j.handle, ctx);
            let inner = async_inner_key(&j.handle, ctx);
            let cty = crate::stmt::local_key_to_c(&inner, ctx.checked);
            let mut out = String::new();
            out.push_str("({ AuraTaskFrame *__join = (");
            out.push_str(&handle);
            out.push_str(&format!("); aura_race_set_source_id(UINT32_C({})); AuraTaskOutcome __join_outcome = aura_task_executor_join_outcome(__aura_task_executor, __join); AuraTaskPollState __join_state = __join_outcome.state; AuraTaskResult __join_result = __join_outcome.result; AuraTaskResult __join_error = __join_outcome.error; aura_race_set_source_id(0); ", j.span.start));
            out.push_str("if (__join_state == AURA_TASK_FAILED) { fputs(\"aura: joined task failed\\n\", stderr); abort(); } ");
            out.push_str("if (__join_state != AURA_TASK_COMPLETE && __join_state != AURA_TASK_CANCELLED) { fputs(\"aura: joined task is pending\\n\", stderr); abort(); } ");
            if inner == "Unit" {
                out.push_str("(void)0; ");
            } else {
                out.push_str(&format!(
                    "(__join_result.data != NULL ? *(({cty} *)__join_result.data) : ({cty}){{0}}); "
                ));
            }
            out.push_str("})");
            out
        }
        AsyncExpr::Cancel(c) => {
            let handle = emit_expr(&c.handle, ctx);
            format!(
                "({{ AuraTaskFrame *__cancel = ({handle}); if (__cancel != NULL && __aura_task_executor != NULL) (void)aura_task_executor_cancel(__aura_task_executor, __cancel); }})"
            )
        }
        AsyncExpr::Await(a) => emit_await(a, ctx),
        AsyncExpr::ChannelCreate(c) => {
            let capacity = emit_expr(&c.capacity, ctx);
            format!("aura_task_channel_new((size_t)({capacity}))")
        }
        AsyncExpr::ChannelSend(s) => emit_channel_send(s, ctx),
        AsyncExpr::ChannelReceive(r) => emit_channel_receive(r, ctx),
        AsyncExpr::ChannelClose(c) => {
            let channel = emit_expr(&c.channel, ctx);
            format!("({{ aura_race_set_source_id(UINT32_C({})); (void)aura_task_channel_close({channel}); aura_race_set_source_id(0); (void)0; }})", c.span.start)
        }
    }
}

fn emit_await(a: &AwaitExpr, ctx: &mut EmitCtx<'_>) -> String {
    let task = emit_expr(&a.operand, ctx);
    let inner = async_inner_key(&a.operand, ctx);
    let cty = crate::stmt::local_key_to_c(&inner, ctx.checked);
    let mut out = String::new();
    out.push_str("({ AuraTaskFrame *__await = (");
    out.push_str(&task);
    out.push_str(&format!(
        "); aura_race_set_source_id(UINT32_C({})); AuraTaskPollState __await_state = ",
        a.span.start
    ));
    out.push_str("__await == NULL ? AURA_TASK_FAILED : aura_task_frame_state(__await); ");
    out.push_str("if (__await_state == AURA_TASK_READY) __await_state = aura_task_frame_poll_once(__await); ");
    out.push_str("if (__await_state == AURA_TASK_PENDING && __aura_task_executor != NULL) { ");
    out.push_str(
        "while (__await_state == AURA_TASK_PENDING && !aura_task_frame_is_waiting(__await)) { ",
    );
    out.push_str("(void)aura_task_executor_wake(__aura_task_executor, __await); ");
    out.push_str("if (aura_task_executor_run_one(__aura_task_executor) == 0) break; ");
    out.push_str("__await_state = aura_task_frame_state(__await); } } ");
    out.push_str("if (__await_state != AURA_TASK_COMPLETE) { fputs(\"aura: awaited task failed or was cancelled\\n\", stderr); abort(); } ");
    out.push_str("aura_race_set_source_id(0); ");
    if inner == "Unit" {
        out.push_str("(void)0; ");
    } else {
        out.push_str(&format!("{cty} __await_value = ({cty}){{0}}; "));
        out.push_str(&format!(
            "AuraTaskResult __await_result = aura_task_frame_result(__await); if (__await_result.data != NULL) __await_value = *(({cty} *)__await_result.data); __await_value; "
        ));
    }
    out.push_str("})");
    out
}

fn channel_payload_kind(key: &str, ctx: &EmitCtx<'_>) -> Option<&'static str> {
    if key == "Int" {
        Some("int")
    } else if key == "String" {
        Some("free")
    } else if is_heap_class_mono(key, ctx.checked) {
        Some("class")
    } else {
        None
    }
}

fn emit_channel_send(s: &ChannelSendExpr, ctx: &mut EmitCtx<'_>) -> String {
    let channel = emit_expr(&s.channel, ctx);
    let value = emit_expr(&s.value, ctx);
    let channel_key =
        resolve_type_name(&s.channel, ctx).unwrap_or_else(|| infer_type_name(&s.channel, ctx));
    let inner = channel_inner_key(&channel_key).unwrap_or("Unit");
    let Some(kind) = channel_payload_kind(inner, ctx) else {
        return "({ fputs(\"aura: Channel payload type is unsupported by C22o\\n\", stderr); abort(); (void)0; })".into();
    };
    let (alloc, destroy) = match kind {
        "int" => (format!("int64_t *__p = (int64_t *)malloc(sizeof(*__p)); if (__p == NULL) abort(); *__p = (int64_t)({value});"), "aura_task_channel_value_destroy_free"),
        "free" => (format!("const char *__s = ({value}); size_t __n = __s == NULL ? 0 : strlen(__s); char *__p = (char *)malloc(__n + 1); if (__p == NULL) abort(); if (__n != 0) memcpy(__p, __s, __n); __p[__n] = '\\0';"), "aura_task_channel_value_destroy_free"),
        "class" => (format!("void *__obj = (void *)({value}); void **__p = (void **)malloc(sizeof(*__p)); if (__p == NULL) abort(); *__p = __obj; aura_gc_add_root(__p);"), "aura_task_channel_value_destroy_class"),
        _ => unreachable!(),
    };
    format!("({{ {alloc} AuraTaskChannelValue __v = {{ __p, sizeof(*__p), {destroy} }}; aura_race_set_source_id(UINT32_C({})); AuraTaskChannelStatus __s = aura_task_channel_send({channel}, NULL, __v); aura_race_set_source_id(0); if (__s == AURA_CHANNEL_PENDING || __s == AURA_CHANNEL_ERROR) {destroy}(__v.data, __v.size); (void)__s; (void)0; }})", s.span.start)
}

fn emit_channel_receive(r: &ChannelReceiveExpr, ctx: &mut EmitCtx<'_>) -> String {
    let channel = emit_expr(&r.channel, ctx);
    let channel_key =
        resolve_type_name(&r.channel, ctx).unwrap_or_else(|| infer_type_name(&r.channel, ctx));
    let inner = channel_inner_key(&channel_key).unwrap_or("Unit");
    let Some(kind) = channel_payload_kind(inner, ctx) else {
        return "({ fputs(\"aura: Channel payload type is unsupported by C22o\\n\", stderr); abort(); (void *)0; })".into();
    };
    let destroy = if kind == "class" {
        "aura_task_channel_value_destroy_class"
    } else {
        "aura_task_channel_value_destroy_free"
    };
    match kind {
        "int" => format!("({{ aura_opt_i64 __r = {{ false, 0 }}; AuraTaskChannelValue __v = {{0}}; aura_race_set_source_id(UINT32_C({})); if (aura_task_channel_receive({channel}, NULL, &__v) == AURA_CHANNEL_OK) {{ __r = (aura_opt_i64){{ true, *((int64_t *)__v.data) }}; {destroy}(__v.data, __v.size); }} aura_race_set_source_id(0); __r; }})", r.span.start),
        "free" => format!("({{ const char *__r = NULL; AuraTaskChannelValue __v = {{0}}; aura_race_set_source_id(UINT32_C({})); if (aura_task_channel_receive({channel}, NULL, &__v) == AURA_CHANNEL_OK) {{ __r = (const char *)__v.data; __v.data = NULL; {destroy}(__v.data, __v.size); }} aura_race_set_source_id(0); __r; }})", r.span.start),
        "class" => format!("({{ void *__r = NULL; AuraTaskChannelValue __v = {{0}}; aura_race_set_source_id(UINT32_C({})); if (aura_task_channel_receive({channel}, NULL, &__v) == AURA_CHANNEL_OK) {{ __r = *((void **)__v.data); {destroy}(__v.data, __v.size); }} aura_race_set_source_id(0); __r; }})", r.span.start),
        _ => unreachable!(),
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
pub(crate) fn mono_split<'a>(
    mono: &'a str,
    checked: &'a CheckedFile,
) -> Option<(&'a str, &'a [Ty])> {
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

/// Local key for `Array_<elem>` element type (primitives, class/struct/enum mono).
pub(crate) fn array_elem_local_key(array_mono: &str, checked: &CheckedFile) -> Option<String> {
    let mono = full_type_mono(array_mono, checked);
    if let Some(("Array", args)) = mono_split(&mono, checked) {
        if let Some(elem) = args.first() {
            return Some(match elem {
                Ty::Int => "Int".into(),
                Ty::Bool => "Bool".into(),
                Ty::String => "String".into(),
                Ty::Unit => "Unit".into(),
                other => other.mono_suffix(),
            });
        }
    }
    mono.strip_prefix("Array_").map(|s| s.to_string())
}

/// Full C mono id for a local/type key (simple name or already-mangled mono).
pub(crate) fn full_type_mono(key: &str, checked: &CheckedFile) -> String {
    if key == "Array" {
        return key.to_string();
    }
    // C4c/C6g: upgrade `Array_Box` / `Array_Color` / short generic keys → package mono.
    if let Some(elem) = key.strip_prefix("Array_") {
        if elem == "Int" || elem == "Bool" || elem == "String" {
            return key.to_string();
        }
        // Prefer recorded mono (covers `Array_Result_Int_String` → package-qualified).
        for (name, args) in &checked.mono_classes {
            if name == "Array" {
                let full = mono_key(name, args);
                if full == key {
                    return full;
                }
            }
        }
        // Simple class/enum name: `Array_Box` → `Array_demo_pkg_Box`.
        if let Some(c) = checked.ast.classes.iter().find(|c| c.name.name == elem) {
            let pkg = class_decl_package(c, checked);
            return mono_key("Array", &[Ty::Class(nominal_key(&pkg, elem))]);
        }
        if let Some(e) = checked.ast.enums.iter().find(|e| e.name.name == elem) {
            let pkg = enum_decl_package(e, checked);
            return mono_key("Array", &[Ty::Enum(nominal_key(&pkg, elem))]);
        }
        // Short generic enum/class mono: match unique Array mono whose suffix ends with elem.
        let mut match_full: Option<String> = None;
        for (name, args) in &checked.mono_classes {
            if name != "Array" || args.is_empty() {
                continue;
            }
            let full = mono_key(name, args);
            if full.ends_with(elem) || full == format!("Array_{elem}") {
                if match_full.is_some() {
                    match_full = None; // ambiguous
                    break;
                }
                match_full = Some(full);
            }
        }
        if let Some(full) = match_full {
            return full;
        }
        // Leave fully-mangled keys as-is.
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
    if t.type_args.is_empty() {
        if let Some(idx) = ctx.type_params.iter().position(|p| p == &t.name.name) {
            if let Some(arg) = ctx.type_args.get(idx) {
                return Some(if t.nullable {
                    Ty::Nullable(Box::new(arg.clone()))
                } else {
                    arg.clone()
                });
            }
        }
    }
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
        } else if let Some(e) = ctx
            .checked
            .ast
            .enums
            .iter()
            .find(|e| e.name.name == t.name.name)
        {
            // C6g: generic enums (e.g. Result<T,E>) as Array elements.
            enum_decl_package(e, ctx.checked)
        } else {
            String::new()
        };
        // Prefer enum over class when only one matches (names are unique in unit).
        if ctx
            .checked
            .ast
            .enums
            .iter()
            .any(|e| e.name.name == t.name.name)
        {
            return Some(Ty::EnumApp {
                name: nominal_key(&pkg, &t.name.name),
                args,
            });
        }
        // C8c: generic interface type args.
        if let Some(i) = ctx
            .checked
            .ast
            .interfaces
            .iter()
            .find(|i| i.name.name == t.name.name)
        {
            let ipkg = iface_decl_package(i, ctx.checked);
            return Some(Ty::InterfaceApp {
                name: nominal_key(&ipkg, &t.name.name),
                args,
            });
        }
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
            if let Some(c) = ctx.checked.ast.classes.iter().find(|c| c.name.name == name) {
                let pkg = class_decl_package(c, ctx.checked);
                return Some(Ty::Class(nominal_key(&pkg, name)));
            }
            if let Some(e) = ctx.checked.ast.enums.iter().find(|e| e.name.name == name) {
                let pkg = enum_decl_package(e, ctx.checked);
                return Some(Ty::Enum(nominal_key(&pkg, name)));
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
    // expected may be simple name, mono (incl. Iterable_Int), or local key.
    let im = iface_mono_from_key(expected_ty, checked);
    if checked
        .ast
        .interfaces
        .iter()
        .any(|i| iface_mono(i, checked) == im || i.name.name == expected_ty)
    {
        return Some(im);
    }
    // C8c: mono_interfaces e.g. Iterable_Int
    for (name, args) in &checked.mono_interfaces {
        if let Some(i) = checked.ast.interfaces.iter().find(|i| i.name.name == *name) {
            let m = iface_mono_args(i, checked, args);
            if m == expected_ty
                || m == im
                || format!(
                    "{name}_{}",
                    args.iter()
                        .map(|t| t.mono_suffix())
                        .collect::<Vec<_>>()
                        .join("_")
                ) == expected_ty
            {
                return Some(m);
            }
        }
    }
    // already a full mono id matching an interface prefix
    if expected_ty.starts_with("aura_iface_") {
        return None;
    }
    if checked.ast.interfaces.iter().any(|i| {
        let base = iface_mono(i, checked);
        expected_ty == base || expected_ty.starts_with(&format!("{base}_"))
    }) {
        return Some(expected_ty.to_string());
    }
    None
}

/// If `expr` has class type `from` and expected is interface, emit upcast.
pub(crate) fn coerce_expr(expr: &Expr, expected_ty: &str, ctx: &mut EmitCtx<'_>) -> String {
    let actual = resolve_type_name(expr, ctx).unwrap_or_else(|| infer_type_name(expr, ctx));
    let code = emit_expr(expr, ctx);

    // C7a: null → empty optional primitive; Int/Bool → wrap into Opt_*.
    if is_opt_prim_key(expected_ty) {
        if matches!(expr, Expr::Null(_)) {
            return null_opt_prim(expected_ty);
        }
        if is_opt_prim_key(&actual) {
            return code;
        }
        // Wrap non-null primitive (literal, narrowed value, or expression).
        if matches!(actual.as_str(), "Int" | "Bool") || matches!(expr, Expr::Int(_) | Expr::Bool(_))
        {
            return wrap_opt_prim(expected_ty, &code);
        }
        // Fallback: treat as value to wrap (e.g. arithmetic result inferred Int).
        if expected_ty == "Opt_Int" || expected_ty == "Opt_Bool" {
            return wrap_opt_prim(expected_ty, &code);
        }
    }
    // Opt_* → bare primitive (e.g. println expects nothing; rare).
    if matches!(expected_ty, "Int" | "Bool") && is_opt_prim_key(&actual) {
        return format!("({code}).value");
    }

    let Some(imono) = expected_iface_mono(expected_ty, ctx.checked) else {
        return code;
    };
    let actual = Some(actual);
    // Resolve simple iface name for non-generic or mono (`Iterable_Int` → Iterable).
    let iface_simple_owned: String = ctx
        .checked
        .ast
        .interfaces
        .iter()
        .find(|i| {
            let base = iface_mono(i, ctx.checked);
            base == imono || imono.starts_with(&format!("{base}_")) || i.name.name == expected_ty
        })
        .map(|i| i.name.name.clone())
        .or_else(|| {
            ctx.checked
                .mono_interfaces
                .iter()
                .find(|(n, args)| {
                    ctx.checked
                        .ast
                        .interfaces
                        .iter()
                        .find(|i| i.name.name == *n)
                        .map(|i| iface_mono_args(i, ctx.checked, args) == imono)
                        .unwrap_or(false)
                })
                .map(|(n, _)| n.clone())
        })
        .unwrap_or_else(|| expected_ty.to_string());
    let iface_simple = iface_simple_owned.as_str();

    if let Some(from) = actual {
        let class_mono = full_type_mono(&from, ctx.checked);
        let base = mono_base_name(&class_mono, ctx.checked).unwrap_or(from.as_str());
        if class_mono != imono
            && ctx.checked.ast.classes.iter().any(|c| {
                c.name.name == base && c.implements.iter().any(|i| i.name.name == iface_simple)
            })
        {
            return format!("{}({code})", c_upcast_name(&class_mono, &imono));
        }
        // C8c: also match mono iface simple via ClassSig implements display/mono
        if class_mono != imono {
            if let Some(cs) = ctx.checked.classes.iter().find(|cs| {
                let pkg = &cs.package;
                type_mono(pkg, &cs.name, &[]) == class_mono || cs.name == base
            }) {
                if cs.implements.iter().any(|imp| {
                    let m = imp.mono_suffix();
                    m == imono
                        || format!("{}_{}", iface_simple, /* fallthrough */ "") == imono
                        || m.ends_with(&imono)
                        || imono.ends_with(&m)
                        || imp.iface_name() == Some(iface_simple)
                }) {
                    // Prefer exact mono match for upcast name
                    return format!("{}({code})", c_upcast_name(&class_mono, &imono));
                }
            }
        }
    }
    // Constructor expr Greeter(...) inferred as class, expected interface
    if let Expr::Call(c) = expr {
        if let Expr::Ident(id) = c.callee.as_ref() {
            if let Some(cl) = ctx.checked.ast.classes.iter().find(|cl| {
                cl.name.name == id.name && cl.implements.iter().any(|i| i.name.name == iface_simple)
            }) {
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
        Expr::ForceUnwrap(f) => {
            let inner =
                resolve_type_name(&f.expr, ctx).unwrap_or_else(|| infer_type_name(&f.expr, ctx));
            Some(if let Some(rest) = inner.strip_prefix("Opt_") {
                rest.to_string()
            } else {
                inner
            })
        }
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
                    let targs: Vec<Ty> = inst.map(|i| i.type_args.clone()).unwrap_or_else(|| {
                        c.type_args
                            .iter()
                            .filter_map(|t| type_ref_to_ty(t, ctx))
                            .collect()
                    });
                    let pkg = inst
                        .map(|i| i.package.as_str())
                        .filter(|p| !p.is_empty())
                        .unwrap_or({
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
                // Fall back to infer for arithmetic receivers: (0-1).toString().
                if let Some(recv) = resolve_type_name(&fe.object, ctx)
                    .or_else(|| resolve_class_of_expr(&fe.object, ctx).map(|s| s.to_string()))
                    .or_else(|| {
                        let t = infer_type_name(&fe.object, ctx);
                        if t == "Unit" || t.is_empty() {
                            None
                        } else {
                            Some(t)
                        }
                    })
                {
                    let base = mono_base_name(&recv, ctx.checked).unwrap_or(recv.as_str());
                    // C13b: Array.get/pop element type (needed for a.get(i).trim() chains).
                    // Must live in resolve_type_name (not only infer_type_name) so method
                    // dispatch on call-result receivers sees String/class, not Unknown.
                    if (base == "Array" || recv.starts_with("Array_"))
                        && (fe.field.name == "get" || fe.field.name == "pop")
                    {
                        if let Some(elem) = array_elem_local_key(&recv, ctx.checked) {
                            return Some(elem);
                        }
                    }
                    // Builtin String methods (needed for chains like s.trim().toInt()).
                    if recv == "String" || base == "String" {
                        match fe.field.name.as_str() {
                            "isEmpty" | "startsWith" | "contains" | "endsWith" => {
                                return Some("Bool".into());
                            }
                            "charAt" | "indexOf" => return Some("Int".into()),
                            "substring" | "trim" | "trimStart" | "trimEnd" | "toLower"
                            | "toUpper" => {
                                return Some("String".into());
                            }
                            "split" => return Some(mono_key("Array", &[Ty::String])),
                            "toInt" => return Some("Opt_Int".into()),
                            _ => {}
                        }
                    }
                    // C13c: Int.toString() → String
                    if (recv == "Int" || base == "Int") && fe.field.name == "toString" {
                        return Some("String".into());
                    }
                    // C9c: builtin Array.clone returns same mono key.
                    if (base == "Array" || recv.starts_with("Array_")) && fe.field.name == "clone" {
                        return Some(recv);
                    }
                    if let Some(class) =
                        ctx.checked.ast.classes.iter().find(|c| c.name.name == base)
                    {
                        if let Some(m) = class.methods.iter().find(|m| m.name.name == fe.field.name)
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
                                        && class.name.name == ctx.method_class.unwrap_or("")
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
            let field = class.fields.iter().find(|x| x.name.name == f.field.name)?;
            let params: Vec<String> = class
                .type_params
                .iter()
                .map(|p| p.name.name.clone())
                .collect();
            // When `this` is the open mono instance, args come from mono_split;
            // empty args on a generic class falls back to the current emit substitution.
            let (ps, as_) = if args.is_empty() && !params.is_empty() && !ctx.type_args.is_empty() {
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

/// C7c: C lvalue of an Array **field** being moved out (return / binding / assign).
/// Returns `None` for non-Array fields, safe-call (`?.`), or owning locals/params.
pub(crate) fn array_field_move_out_lvalue(e: &Expr, ctx: &mut EmitCtx<'_>) -> Option<String> {
    use crate::array_emit::is_array_type_key;
    match e {
        Expr::Field(f) if !f.safe => {
            let key = resolve_type_name(e, ctx).unwrap_or_else(|| infer_type_name(e, ctx));
            if !is_array_type_key(&key) {
                return None;
            }
            let obj = emit_expr(&f.object, ctx);
            Some(field_access_c(&obj, f, ctx))
        }
        Expr::Ident(i) => {
            // Owning local/param: normal Array move path (C5b/C6d), not field.
            if ctx.is_array_owner(&i.name) {
                return None;
            }
            let class = ctx.method_class?;
            let base = mono_base_name(class, ctx.checked).unwrap_or(class);
            let cl = ctx
                .checked
                .ast
                .classes
                .iter()
                .find(|c| c.name.name == base)?;
            if !cl.fields.iter().any(|f| f.name.name == i.name) {
                return None;
            }
            let key = ctx.lookup_local(&i.name)?;
            if !is_array_type_key(key) {
                return None;
            }
            Some(format!("this->{}", mangle_ident(&i.name)))
        }
        Expr::Group(inner, _) => array_field_move_out_lvalue(inner, ctx),
        _ => None,
    }
}

fn field_access_c(obj: &str, f: &FieldExpr, ctx: &EmitCtx<'_>) -> String {
    // C3y: heap class receivers use -> ; structs/Array/This use .
    // `this` is already a pointer; emit This as (*this) so `.` still works.
    let use_arrow = match f.object.as_ref() {
        Expr::This(_) => false,
        _ => {
            resolve_type_name(&f.object, ctx)
                .map(|t| is_heap_class_mono(&full_type_mono(&t, ctx.checked), ctx.checked))
                .unwrap_or(false)
                || f.safe
        }
    };
    if use_arrow {
        format!("({obj})->{}", mangle_ident(&f.field.name))
    } else {
        format!("({obj}).{}", mangle_ident(&f.field.name))
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
