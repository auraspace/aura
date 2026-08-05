//! Expression emission.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;

use aura_ast::*;
use aura_sema::{nominal_key, CheckedFile, Ty};

use crate::array_emit::is_array_type_key;
use crate::call_emit::emit_call;
use crate::class_emit::{direct_superclass, has_field_in_hierarchy, method_owner};
use crate::ctx::EmitCtx;
use crate::names::*;

pub(crate) fn infer_type_name(e: &Expr, ctx: &EmitCtx<'_>) -> String {
    match e {
        Expr::Int(_) => "Int".into(),
        Expr::Bool(_) => "Bool".into(),
        Expr::String(_) => "String".into(),
        Expr::Call(c) => {
            // Channel operations stay as ordinary calls until sema. Recover
            // their typed result here for inferred locals (notably
            // Channel<Class?>.receive()).
            if let Expr::Field(field) = c.callee.as_ref() {
                let receiver_ty = ctx
                    .checked
                    .expr_tys
                    .get(&(field.object.span().start, field.object.span().end));
                if matches!(receiver_ty, Some(Ty::Channel(_)))
                    && matches!(field.field.name.as_str(), "send" | "receive" | "close")
                {
                    if let Some(ty) = ctx.checked.expr_tys.get(&(c.span.start, c.span.end)) {
                        return match ty {
                            Ty::Nullable(inner) if **inner == Ty::Int => "Opt_Int".into(),
                            Ty::Nullable(inner) if **inner == Ty::Bool => "Opt_Bool".into(),
                            Ty::Nullable(inner) => inner.mono_suffix(),
                            other => other.mono_suffix(),
                        };
                    }
                }
            }
            // Method-call result types are resolved by the same path used for
            // chained receivers. Keep free-function handling below separate.
            if matches!(c.callee.as_ref(), Expr::Field(_))
                && ctx
                    .checked
                    .call_instantiations
                    .get(&c.span.start)
                    .is_some_and(|inst| !inst.is_static && !inst.is_constructor)
            {
                if let Some(ty) = resolve_type_name(e, ctx) {
                    return ty;
                }
            }
            if let (Expr::Field(fe), Some(inst)) = (
                c.callee.as_ref(),
                ctx.checked.call_instantiations.get(&c.span.start),
            ) {
                if inst.is_static {
                    let class_name = match fe.object.as_ref() {
                        Expr::Ident(id) => id.name.as_str(),
                        _ => "",
                    };
                    if let Some(class) = ctx
                        .checked
                        .ast
                        .classes
                        .iter()
                        .find(|x| x.name.name == class_name)
                    {
                        if let Some(method) =
                            class.methods.iter().find(|m| m.name.name == fe.field.name)
                        {
                            if let Some(rt) = &method.return_type {
                                let params = class
                                    .type_params
                                    .iter()
                                    .map(|p| p.name.name.clone())
                                    .chain(method.type_params.iter().map(|p| p.name.name.clone()))
                                    .collect::<Vec<_>>();
                                let subst =
                                    aura_sema::type_subst_map(&ctx.type_params, &ctx.type_args);
                                let args = inst
                                    .type_args
                                    .iter()
                                    .chain(inst.method_type_args.iter())
                                    .map(|t| aura_sema::subst_ty(t, &subst))
                                    .collect::<Vec<_>>();
                                return type_ref_local_key(rt, &params, &args);
                            }
                            return "Unit".into();
                        }
                    }
                }
            }
            if let Expr::Ident(id) = c.callee.as_ref() {
                match id.name.as_str() {
                    "exception_cause_count"
                    | "exception_source_span_start"
                    | "exception_source_span_end"
                    | "exception_cause_span_start"
                    | "exception_cause_span_end" => return "Int".into(),
                    "exception_cause_type" => return "String".into(),
                    "exception_add_cause" => return "Unit".into(),
                    _ => {}
                }
            }
            match c.callee.as_ref() {
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
                    if let Some(f) = ctx.checked.ast.async_functions.iter().find(|f| {
                        f.name.name == inst.name
                            && (inst.package.is_empty()
                                || async_fun_decl_package(f, ctx.checked) == inst.package)
                    }) {
                        if let Some(rt) = &f.return_type {
                            if let Some(result) = type_ref_to_ty(rt, ctx) {
                                return format!("Task_{}", ty_full_mono(&result));
                            }
                            let result = type_ref_local_key(rt, &[], &inst.type_args);
                            return format!("Task_{}", full_type_mono(&result, ctx.checked));
                        }
                        return "Task_Unit".into();
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
                    let substitutions = aura_sema::type_subst_map(&ctx.type_params, &ctx.type_args);
                    let targs: Vec<Ty> = ctx
                        .checked
                        .call_instantiations
                        .get(&c.span.start)
                        .map(|i| {
                            i.type_args
                                .iter()
                                .map(|ty| aura_sema::subst_ty(ty, &substitutions))
                                .collect()
                        })
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
                        if (base == "Array" || mono.starts_with("Array_"))
                            && fe.field.name == "clone"
                        {
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
                                let (ps, as_) =
                                    if let Some((_, args)) = mono_split(&mono, ctx.checked) {
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
                        // Interface method return type, including methods inherited from parents.
                        if let Some(i) = ctx
                            .checked
                            .ast
                            .interfaces
                            .iter()
                            .find(|i| i.name.name == base || iface_mono(i, ctx.checked) == mono)
                        {
                            let (_, iargs) = resolve_iface_decl_and_args(&mono, ctx.checked);
                            if let Some((m, owner, owner_args)) =
                                crate::iface::interface_method_decls_with_parents(
                                    ctx.checked,
                                    i,
                                    &iargs,
                                )
                                .into_iter()
                                .find(|(m, _, _)| m.name.name == fe.field.name)
                            {
                                if let Some(rt) = &m.return_type {
                                    let owner_params = owner
                                        .type_params
                                        .iter()
                                        .map(|p| p.name.name.clone())
                                        .collect::<Vec<_>>();
                                    return type_ref_local_key(rt, &owner_params, &owner_args);
                                }
                            }
                        }
                    }
                    "Int".into()
                }
                _ => "Int".into(),
            }
        }
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
                    return type_ref_local_key_expand(&field.ty, &ps, &as_, ctx.checked);
                }
            }
            "String".into()
        }
        Expr::Ident(i) => {
            if i.name == "super" {
                return ctx
                    .method_class
                    .and_then(|class| {
                        let base = mono_base_name(class, ctx.checked).unwrap_or(class);
                        crate::class_emit::direct_superclass(
                            ctx.checked,
                            ctx.checked
                                .ast
                                .classes
                                .iter()
                                .find(|c| c.name.name == base)?,
                        )
                    })
                    .map(|parent| parent.name.name.clone())
                    .unwrap_or_else(|| "Int".into());
            }
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
        Expr::Async(AsyncExpr::Spawn(s)) => {
            format!(
                "TaskHandle_{}",
                full_type_mono(&spawn_result_key(&s.body, ctx), ctx.checked)
            )
        }
        Expr::Async(AsyncExpr::Join(j)) => {
            let inner = async_inner_key_for_infer(&j.handle, ctx);
            format!(
                "std_io_Result_{}_std_io_TaskError",
                full_type_mono(&inner, ctx.checked)
            )
        }
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
            let raw_inner = channel_inner_key(&channel).unwrap_or("Unit");
            let inner = task_payload_repr_key(raw_inner);
            match inner.as_str() {
                "Int" => "Opt_Int".into(),
                "Bool" => "Opt_Bool".into(),
                _ => inner,
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

/// Bounded C22l lowering: a spawn body may contain synchronous statements and
/// the current first-await shape, with copied `Int`/`Bool`/`String`/heap-class/
/// Array/Fun parameters from the enclosing function. String captures are copied
/// into owned boxes, class captures are rooted, and Array captures are cloned by
/// the frame emitter.
#[derive(Debug, Clone)]
pub(crate) struct BoundedSpawnCapture {
    pub(crate) name: String,
    pub(crate) key: String,
    pub(crate) boxed: bool,
}

/// Return the lexical names referenced by a spawn body. The emitter uses this
/// for both the bounded lowering and the general CFG lowering; keeping the
/// discovery in one place prevents inferred captures from diverging between
/// the two frame layouts.
pub(crate) fn spawn_capture_names(body: &Block) -> Vec<String> {
    let mut names = BTreeSet::new();
    spawn_body_capture_refs_scoped(body, &mut names);
    names.into_iter().collect()
}

pub(crate) fn general_spawn_captures(
    body: &Block,
    available: &HashMap<String, String>,
    checked: &CheckedFile,
    mutable_captures: &HashSet<String>,
) -> Option<Vec<BoundedSpawnCapture>> {
    let names = spawn_capture_names(body);
    if names.iter().any(|name| {
        available.get(name).is_some_and(|key| {
            !spawn_capture_type_supported(key, checked)
                || (mutable_captures.contains(name) && !mutable_capture_box_supported(key, checked))
        })
    }) {
        return None;
    }
    Some(
        names
            .into_iter()
            .filter_map(|name| {
                available.get(&name).map(|key| {
                    let boxed = mutable_captures.contains(&name);
                    BoundedSpawnCapture {
                        name,
                        key: key.clone(),
                        boxed,
                    }
                })
            })
            .collect(),
    )
}

fn mutable_capture_box_supported(key: &str, checked: &CheckedFile) -> bool {
    matches!(key, "Int" | "Bool" | "String")
        || is_array_type_key(key)
        || is_fun_type_key(key)
        || is_heap_class_mono(key, checked)
}

pub(crate) fn general_spawn_function_name(span: Span, package: &str) -> String {
    c_fun_name(package, &format!("__spawn_cfg_{}", span.start), &[])
}

pub(crate) fn bounded_capture_box_kind(capture: &BoundedSpawnCapture) -> &'static str {
    if !capture.boxed {
        return "none";
    }
    match capture.key.as_str() {
        "String" => "string",
        "Int" => "i64",
        "Bool" => "bool",
        _ => "ptr",
    }
}

pub(crate) fn bounded_spawn_captures(
    body: &Block,
    available: &HashMap<String, String>,
    checked: &CheckedFile,
    mutable_captures: &HashSet<String>,
) -> Option<Vec<BoundedSpawnCapture>> {
    let has_await = spawn_body_contains_await(body);
    if has_await
        && bounded_spawn_await_shape(body, checked).is_none()
        && !bounded_spawn_discard_await_shape(body)
    {
        return None;
    }
    if !has_await {
        let mut names = BTreeSet::new();
        spawn_body_capture_refs_scoped(body, &mut names);
        // A referenced outer value with an unsupported ownership shape must
        // reject the spawn lowering; silently dropping it would emit a poller
        // that reads an uninitialized or undeclared C value.
        if names.iter().any(|name| {
            available.get(name).is_some_and(|ty| {
                !spawn_capture_type_supported(ty, checked)
                    || (mutable_captures.contains(name) && scheduler_owned_key(ty))
            })
        }) {
            return None;
        }
        return Some(
            names
                .into_iter()
                .filter_map(|name| {
                    let ty = available.get(&name)?;
                    if !spawn_capture_type_supported(ty, checked)
                        || (mutable_captures.contains(&name) && scheduler_owned_key(ty))
                    {
                        return None;
                    }
                    Some(BoundedSpawnCapture {
                        boxed: mutable_captures.contains(&name),
                        name,
                        key: ty.clone(),
                    })
                })
                .collect(),
        );
    }
    let mut captures = BTreeSet::new();
    spawn_body_capture_refs_scoped(body, &mut captures);
    if captures.iter().any(|name| {
        available.get(name).is_some_and(|ty| {
            !spawn_capture_type_supported(ty, checked)
                || (mutable_captures.contains(name) && scheduler_owned_key(ty))
        })
    }) {
        return None;
    }
    Some(
        captures
            .into_iter()
            .filter_map(|name| {
                available
                    .get(&name)
                    .filter(|ty| !mutable_captures.contains(&name) || !scheduler_owned_key(ty))
                    .map(|ty| BoundedSpawnCapture {
                        boxed: mutable_captures.contains(&name),
                        name,
                        key: ty.clone(),
                    })
            })
            .collect(),
    )
}

fn scheduler_owned_key(key: &str) -> bool {
    key == "Task"
        || key.starts_with("Task_")
        || key == "TaskHandle"
        || key.starts_with("TaskHandle_")
        || key == "Channel"
        || key.starts_with("Channel_")
}

/// A spawn may intentionally discard the result of an awaited `Unit` task.
/// This is still a bounded shape: the generic spawn poller can execute the
/// await expression synchronously, while preserving cancellation checks and
/// the normal task outcome/result cleanup.  Previously this valid form was
/// rejected before code generation, even though no suspended value needed a
/// frame slot.
pub(crate) fn bounded_spawn_discard_await_shape(body: &Block) -> bool {
    let Some(Stmt::Expr(Expr::Async(AsyncExpr::Await(await_expr)))) = body.stmts.first() else {
        return false;
    };
    let _ = await_expr;
    !spawn_body_contains_await(&Block {
        stmts: body.stmts[1..].to_vec(),
        span: body.span,
    })
}

fn spawn_capture_type_supported(key: &str, checked: &CheckedFile) -> bool {
    key == "Int"
        || key == "Bool"
        || key == "Opt_Int"
        || key == "Opt_Bool"
        || key == "String"
        || key == "ForeignHandle"
        || key.starts_with("ForeignHandle_")
        || key == "Task"
        || key.starts_with("Task_")
        || key == "TaskHandle"
        || key.starts_with("TaskHandle_")
        || key == "Channel"
        || key.starts_with("Channel_")
        || is_array_type_key(key)
        || is_fun_type_key(key)
        || is_heap_class_mono(key, checked)
        || is_iface_type_key(key, checked)
        || is_enum_mono(key, checked)
        || is_value_struct_mono(key, checked)
}

fn spawn_body_contains_await(block: &Block) -> bool {
    fn expr_has_await(expr: &Expr) -> bool {
        match expr {
            Expr::Async(AsyncExpr::Await(_)) => true,
            Expr::Call(c) => expr_has_await(&c.callee) || c.args.iter().any(expr_has_await),
            Expr::Field(f) => expr_has_await(&f.object),
            Expr::Assign(a) => expr_has_await(&a.value),
            Expr::Binary(b) => expr_has_await(&b.left) || expr_has_await(&b.right),
            Expr::Unary(u) => expr_has_await(&u.expr),
            Expr::ForceUnwrap(f) => expr_has_await(&f.expr),
            Expr::Is(i) => expr_has_await(&i.expr),
            Expr::Group(e, _) => expr_has_await(e),
            Expr::If(i) => {
                expr_has_await(&i.cond)
                    || block_has_await(&i.then_block)
                    || block_has_await(&i.else_block)
            }
            Expr::Lambda(l) => match &l.body {
                LambdaBody::Expr(e) => expr_has_await(e),
                LambdaBody::Block(b) => block_has_await(b),
            },
            Expr::Async(a) => match a {
                AsyncExpr::Spawn(s) => block_has_await(&s.body),
                AsyncExpr::Join(j) => expr_has_await(&j.handle),
                AsyncExpr::Cancel(c) => expr_has_await(&c.handle),
                AsyncExpr::ChannelCreate(c) => expr_has_await(&c.capacity),
                AsyncExpr::ChannelSend(c) => expr_has_await(&c.channel) || expr_has_await(&c.value),
                AsyncExpr::ChannelReceive(c) => expr_has_await(&c.channel),
                AsyncExpr::ChannelClose(c) => expr_has_await(&c.channel),
                AsyncExpr::Await(_) => true,
            },
            Expr::Ident(_)
            | Expr::This(_)
            | Expr::Int(_)
            | Expr::Bool(_)
            | Expr::String(_)
            | Expr::Null(_) => false,
        }
    }
    fn block_has_await(block: &Block) -> bool {
        block.stmts.iter().any(|stmt| match stmt {
            Stmt::Var(v) => expr_has_await(&v.init),
            Stmt::If(i) => {
                expr_has_await(&i.cond)
                    || block_has_await(&i.then_block)
                    || i.else_block.as_ref().is_some_and(block_has_await)
            }
            Stmt::While(w) => expr_has_await(&w.cond) || block_has_await(&w.body),
            Stmt::ForRange(f) => {
                expr_has_await(&f.start) || expr_has_await(&f.end) || block_has_await(&f.body)
            }
            Stmt::ForIn(f) => expr_has_await(&f.iterable) || block_has_await(&f.body),
            Stmt::Match(m) => {
                expr_has_await(&m.scrutinee) || m.arms.iter().any(|a| block_has_await(&a.body))
            }
            Stmt::Try(t) => {
                block_has_await(&t.try_block)
                    || t.catch.as_ref().is_some_and(|c| block_has_await(&c.body))
                    || t.finally.as_ref().is_some_and(block_has_await)
            }
            Stmt::Throw(t) => expr_has_await(&t.value),
            Stmt::Return(r) => r.value.as_ref().is_some_and(expr_has_await),
            Stmt::Expr(e) => expr_has_await(e),
            Stmt::Break(_) | Stmt::Continue(_) => false,
        })
    }
    block_has_await(block)
}

/// Collect captures with lexical scope awareness so inner bindings cannot
/// accidentally capture an outer local with the same spelling.
fn spawn_body_capture_refs_scoped(block: &Block, captures: &mut BTreeSet<String>) {
    fn expr_refs(expr: &Expr, locals: &BTreeSet<String>, captures: &mut BTreeSet<String>) {
        match expr {
            Expr::Ident(id) => {
                if !locals.contains(&id.name) {
                    captures.insert(id.name.clone());
                }
            }
            Expr::Call(call) => {
                expr_refs(&call.callee, locals, captures);
                for arg in &call.args {
                    expr_refs(arg, locals, captures);
                }
            }
            Expr::Field(field) => expr_refs(&field.object, locals, captures),
            Expr::Assign(assign) => {
                if !locals.contains(&assign.name.name) {
                    captures.insert(assign.name.name.clone());
                }
                expr_refs(&assign.value, locals, captures);
            }
            Expr::Binary(binary) => {
                expr_refs(&binary.left, locals, captures);
                expr_refs(&binary.right, locals, captures);
            }
            Expr::Unary(unary) => expr_refs(&unary.expr, locals, captures),
            Expr::ForceUnwrap(force) => expr_refs(&force.expr, locals, captures),
            Expr::Is(is_expr) => expr_refs(&is_expr.expr, locals, captures),
            Expr::Group(inner, _) => expr_refs(inner, locals, captures),
            Expr::If(if_expr) => {
                expr_refs(&if_expr.cond, locals, captures);
                block_refs(&if_expr.then_block, locals, captures);
                block_refs(&if_expr.else_block, locals, captures);
            }
            Expr::Lambda(lambda) => {
                let mut lambda_locals = locals.clone();
                lambda_locals.extend(lambda.params.iter().map(|param| param.name.name.clone()));
                match &lambda.body {
                    LambdaBody::Expr(body) => expr_refs(body, &lambda_locals, captures),
                    LambdaBody::Block(body) => block_refs(body, &lambda_locals, captures),
                }
            }
            Expr::Async(async_expr) => match async_expr {
                AsyncExpr::Spawn(spawn) => block_refs(&spawn.body, locals, captures),
                AsyncExpr::Await(await_expr) => expr_refs(&await_expr.operand, locals, captures),
                AsyncExpr::Join(join) => expr_refs(&join.handle, locals, captures),
                AsyncExpr::Cancel(cancel) => expr_refs(&cancel.handle, locals, captures),
                AsyncExpr::ChannelCreate(create) => expr_refs(&create.capacity, locals, captures),
                AsyncExpr::ChannelSend(send) => {
                    expr_refs(&send.channel, locals, captures);
                    expr_refs(&send.value, locals, captures);
                }
                AsyncExpr::ChannelReceive(receive) => expr_refs(&receive.channel, locals, captures),
                AsyncExpr::ChannelClose(close) => expr_refs(&close.channel, locals, captures),
            },
            Expr::This(_) | Expr::Int(_) | Expr::Bool(_) | Expr::String(_) | Expr::Null(_) => {}
        }
    }

    fn block_refs(block: &Block, inherited: &BTreeSet<String>, captures: &mut BTreeSet<String>) {
        let mut locals = inherited.clone();
        for stmt in &block.stmts {
            match stmt {
                Stmt::Var(var) => {
                    expr_refs(&var.init, &locals, captures);
                    locals.insert(var.name.name.clone());
                }
                Stmt::If(if_stmt) => {
                    expr_refs(&if_stmt.cond, &locals, captures);
                    block_refs(&if_stmt.then_block, &locals, captures);
                    if let Some(else_block) = &if_stmt.else_block {
                        block_refs(else_block, &locals, captures);
                    }
                }
                Stmt::While(while_stmt) => {
                    expr_refs(&while_stmt.cond, &locals, captures);
                    block_refs(&while_stmt.body, &locals, captures);
                }
                Stmt::ForRange(range) => {
                    expr_refs(&range.start, &locals, captures);
                    expr_refs(&range.end, &locals, captures);
                    let mut body_locals = locals.clone();
                    body_locals.insert(range.name.name.clone());
                    block_refs(&range.body, &body_locals, captures);
                }
                Stmt::ForIn(for_in) => {
                    expr_refs(&for_in.iterable, &locals, captures);
                    let mut body_locals = locals.clone();
                    body_locals.insert(for_in.name.name.clone());
                    block_refs(&for_in.body, &body_locals, captures);
                }
                Stmt::Match(match_stmt) => {
                    expr_refs(&match_stmt.scrutinee, &locals, captures);
                    for arm in &match_stmt.arms {
                        let mut arm_locals = locals.clone();
                        let Pattern::Variant { bindings, .. } = &arm.pattern;
                        arm_locals.extend(bindings.iter().map(|binding| binding.name.clone()));
                        block_refs(&arm.body, &arm_locals, captures);
                    }
                }
                Stmt::Try(try_stmt) => {
                    block_refs(&try_stmt.try_block, &locals, captures);
                    if let Some(catch) = &try_stmt.catch {
                        let mut catch_locals = locals.clone();
                        catch_locals.insert(catch.name.name.clone());
                        block_refs(&catch.body, &catch_locals, captures);
                    }
                    if let Some(finally) = &try_stmt.finally {
                        block_refs(finally, &locals, captures);
                    }
                }
                Stmt::Throw(throw_stmt) => expr_refs(&throw_stmt.value, &locals, captures),
                Stmt::Return(return_stmt) => {
                    if let Some(value) = &return_stmt.value {
                        expr_refs(value, &locals, captures);
                    }
                }
                Stmt::Expr(expr) => expr_refs(expr, &locals, captures),
                Stmt::Break(_) | Stmt::Continue(_) => {}
            }
        }
    }

    block_refs(block, &BTreeSet::new(), captures);
}

/// Return mutable locals that are referenced from a spawn body. The sema
/// capture model intentionally remains separate from task lowering, so this
/// conservative codegen pass identifies the names that need shared boxes.
pub(crate) fn mutable_spawn_capture_names(block: &Block) -> HashSet<String> {
    let mut mutable = HashSet::new();
    let mut spawned = HashSet::new();

    fn block_decls(block: &Block, mutable: &mut HashSet<String>) {
        for stmt in &block.stmts {
            match stmt {
                Stmt::Var(v) => {
                    if v.mutable {
                        mutable.insert(v.name.name.clone());
                    }
                    expr_decls(&v.init, mutable);
                }
                Stmt::If(i) => {
                    expr_decls(&i.cond, mutable);
                    block_decls(&i.then_block, mutable);
                    if let Some(block) = &i.else_block {
                        block_decls(block, mutable);
                    }
                }
                Stmt::While(w) => {
                    expr_decls(&w.cond, mutable);
                    block_decls(&w.body, mutable);
                }
                Stmt::ForRange(f) => {
                    expr_decls(&f.start, mutable);
                    expr_decls(&f.end, mutable);
                    block_decls(&f.body, mutable);
                }
                Stmt::ForIn(f) => {
                    expr_decls(&f.iterable, mutable);
                    block_decls(&f.body, mutable);
                }
                Stmt::Match(m) => {
                    expr_decls(&m.scrutinee, mutable);
                    for arm in &m.arms {
                        block_decls(&arm.body, mutable);
                    }
                }
                Stmt::Try(t) => {
                    block_decls(&t.try_block, mutable);
                    if let Some(catch) = &t.catch {
                        block_decls(&catch.body, mutable);
                    }
                    if let Some(finally) = &t.finally {
                        block_decls(finally, mutable);
                    }
                }
                Stmt::Throw(t) => expr_decls(&t.value, mutable),
                Stmt::Return(r) => {
                    if let Some(value) = &r.value {
                        expr_decls(value, mutable);
                    }
                }
                Stmt::Expr(e) => expr_decls(e, mutable),
                Stmt::Break(_) | Stmt::Continue(_) => {}
            }
        }
    }

    fn expr_decls(expr: &Expr, mutable: &mut HashSet<String>) {
        match expr {
            Expr::Call(c) => {
                expr_decls(&c.callee, mutable);
                for arg in &c.args {
                    expr_decls(arg, mutable);
                }
            }
            Expr::Field(f) => expr_decls(&f.object, mutable),
            Expr::Assign(a) => expr_decls(&a.value, mutable),
            Expr::Binary(b) => {
                expr_decls(&b.left, mutable);
                expr_decls(&b.right, mutable);
            }
            Expr::Unary(u) => expr_decls(&u.expr, mutable),
            Expr::ForceUnwrap(f) => expr_decls(&f.expr, mutable),
            Expr::Is(i) => expr_decls(&i.expr, mutable),
            Expr::Group(e, _) => expr_decls(e, mutable),
            Expr::If(i) => {
                expr_decls(&i.cond, mutable);
                block_decls(&i.then_block, mutable);
                block_decls(&i.else_block, mutable);
            }
            Expr::Lambda(l) => match &l.body {
                LambdaBody::Expr(e) => expr_decls(e, mutable),
                LambdaBody::Block(b) => block_decls(b, mutable),
            },
            Expr::Async(a) => match a {
                AsyncExpr::Await(a) => expr_decls(&a.operand, mutable),
                AsyncExpr::Spawn(s) => block_decls(&s.body, mutable),
                AsyncExpr::Join(j) => expr_decls(&j.handle, mutable),
                AsyncExpr::Cancel(c) => expr_decls(&c.handle, mutable),
                AsyncExpr::ChannelCreate(c) => expr_decls(&c.capacity, mutable),
                AsyncExpr::ChannelSend(c) => {
                    expr_decls(&c.channel, mutable);
                    expr_decls(&c.value, mutable);
                }
                AsyncExpr::ChannelReceive(c) => expr_decls(&c.channel, mutable),
                AsyncExpr::ChannelClose(c) => expr_decls(&c.channel, mutable),
            },
            Expr::Ident(_)
            | Expr::This(_)
            | Expr::Int(_)
            | Expr::Bool(_)
            | Expr::String(_)
            | Expr::Null(_) => {}
        }
    }

    fn block_spawn_refs(block: &Block, spawned: &mut HashSet<String>) {
        for stmt in &block.stmts {
            match stmt {
                Stmt::Var(v) if matches!(v.init, Expr::Async(AsyncExpr::Spawn(_))) => {
                    expr_spawn_refs(&v.init, spawned)
                }
                Stmt::Var(_) => {}
                Stmt::If(i) => {
                    if matches!(i.cond, Expr::Async(AsyncExpr::Spawn(_))) {
                        expr_spawn_refs(&i.cond, spawned);
                    }
                    block_spawn_refs(&i.then_block, spawned);
                    if let Some(block) = &i.else_block {
                        block_spawn_refs(block, spawned);
                    }
                }
                Stmt::While(w) => {
                    if matches!(w.cond, Expr::Async(AsyncExpr::Spawn(_))) {
                        expr_spawn_refs(&w.cond, spawned);
                    }
                    block_spawn_refs(&w.body, spawned);
                }
                Stmt::ForRange(f) => {
                    if matches!(f.start, Expr::Async(AsyncExpr::Spawn(_))) {
                        expr_spawn_refs(&f.start, spawned);
                    }
                    if matches!(f.end, Expr::Async(AsyncExpr::Spawn(_))) {
                        expr_spawn_refs(&f.end, spawned);
                    }
                    block_spawn_refs(&f.body, spawned);
                }
                Stmt::ForIn(f) => {
                    if matches!(f.iterable, Expr::Async(AsyncExpr::Spawn(_))) {
                        expr_spawn_refs(&f.iterable, spawned);
                    }
                    block_spawn_refs(&f.body, spawned);
                }
                Stmt::Match(m) => {
                    if matches!(m.scrutinee, Expr::Async(AsyncExpr::Spawn(_))) {
                        expr_spawn_refs(&m.scrutinee, spawned);
                    }
                    for arm in &m.arms {
                        block_spawn_refs(&arm.body, spawned);
                    }
                }
                Stmt::Try(t) => {
                    block_spawn_refs(&t.try_block, spawned);
                    if let Some(catch) = &t.catch {
                        block_spawn_refs(&catch.body, spawned);
                    }
                    if let Some(finally) = &t.finally {
                        block_spawn_refs(finally, spawned);
                    }
                }
                Stmt::Throw(t) if matches!(t.value, Expr::Async(AsyncExpr::Spawn(_))) => {
                    expr_spawn_refs(&t.value, spawned)
                }
                Stmt::Throw(_) => {}
                Stmt::Return(r) => {
                    if let Some(value) = &r.value {
                        if matches!(value, Expr::Async(AsyncExpr::Spawn(_))) {
                            expr_spawn_refs(value, spawned);
                        }
                    }
                }
                Stmt::Expr(e) if matches!(e, Expr::Async(AsyncExpr::Spawn(_))) => {
                    expr_spawn_refs(e, spawned)
                }
                Stmt::Expr(_) => {}
                Stmt::Break(_) | Stmt::Continue(_) => {}
            }
        }
    }

    fn block_capture_refs(block: &Block, captured: &mut HashSet<String>) {
        for stmt in &block.stmts {
            match stmt {
                Stmt::Var(v) => expr_capture_refs(&v.init, captured),
                Stmt::If(i) => {
                    expr_capture_refs(&i.cond, captured);
                    block_capture_refs(&i.then_block, captured);
                    if let Some(block) = &i.else_block {
                        block_capture_refs(block, captured);
                    }
                }
                Stmt::While(w) => {
                    expr_capture_refs(&w.cond, captured);
                    block_capture_refs(&w.body, captured);
                }
                Stmt::ForRange(f) => {
                    expr_capture_refs(&f.start, captured);
                    expr_capture_refs(&f.end, captured);
                    block_capture_refs(&f.body, captured);
                }
                Stmt::ForIn(f) => {
                    expr_capture_refs(&f.iterable, captured);
                    block_capture_refs(&f.body, captured);
                }
                Stmt::Match(m) => {
                    expr_capture_refs(&m.scrutinee, captured);
                    for arm in &m.arms {
                        block_capture_refs(&arm.body, captured);
                    }
                }
                Stmt::Try(t) => {
                    block_capture_refs(&t.try_block, captured);
                    if let Some(catch) = &t.catch {
                        block_capture_refs(&catch.body, captured);
                    }
                    if let Some(finally) = &t.finally {
                        block_capture_refs(finally, captured);
                    }
                }
                Stmt::Throw(t) => expr_capture_refs(&t.value, captured),
                Stmt::Return(r) => {
                    if let Some(value) = &r.value {
                        expr_capture_refs(value, captured);
                    }
                }
                Stmt::Expr(e) => expr_capture_refs(e, captured),
                Stmt::Break(_) | Stmt::Continue(_) => {}
            }
        }
    }

    fn expr_capture_refs(expr: &Expr, captured: &mut HashSet<String>) {
        match expr {
            Expr::Ident(id) => {
                captured.insert(id.name.clone());
            }
            Expr::Call(c) => {
                expr_capture_refs(&c.callee, captured);
                for arg in &c.args {
                    expr_capture_refs(arg, captured);
                }
            }
            Expr::Field(f) => expr_capture_refs(&f.object, captured),
            Expr::Assign(a) => expr_capture_refs(&a.value, captured),
            Expr::Binary(b) => {
                expr_capture_refs(&b.left, captured);
                expr_capture_refs(&b.right, captured);
            }
            Expr::Unary(u) => expr_capture_refs(&u.expr, captured),
            Expr::ForceUnwrap(f) => expr_capture_refs(&f.expr, captured),
            Expr::Is(i) => expr_capture_refs(&i.expr, captured),
            Expr::Group(e, _) => expr_capture_refs(e, captured),
            Expr::If(i) => {
                expr_capture_refs(&i.cond, captured);
                block_capture_refs(&i.then_block, captured);
                block_capture_refs(&i.else_block, captured);
            }
            Expr::Lambda(l) => match &l.body {
                LambdaBody::Expr(e) => expr_capture_refs(e, captured),
                LambdaBody::Block(b) => block_capture_refs(b, captured),
            },
            Expr::Async(a) => match a {
                AsyncExpr::Spawn(s) => block_capture_refs(&s.body, captured),
                AsyncExpr::Await(a) => expr_capture_refs(&a.operand, captured),
                AsyncExpr::Join(j) => expr_capture_refs(&j.handle, captured),
                AsyncExpr::Cancel(c) => expr_capture_refs(&c.handle, captured),
                AsyncExpr::ChannelCreate(c) => expr_capture_refs(&c.capacity, captured),
                AsyncExpr::ChannelSend(c) => {
                    expr_capture_refs(&c.channel, captured);
                    expr_capture_refs(&c.value, captured);
                }
                AsyncExpr::ChannelReceive(c) => expr_capture_refs(&c.channel, captured),
                AsyncExpr::ChannelClose(c) => expr_capture_refs(&c.channel, captured),
            },
            Expr::This(_) | Expr::Int(_) | Expr::Bool(_) | Expr::String(_) | Expr::Null(_) => {}
        }
    }

    fn expr_spawn_refs(expr: &Expr, spawned: &mut HashSet<String>) {
        match expr {
            Expr::Ident(id) => {
                spawned.insert(id.name.clone());
            }
            Expr::Call(c) => {
                expr_spawn_refs(&c.callee, spawned);
                for arg in &c.args {
                    expr_spawn_refs(arg, spawned);
                }
            }
            Expr::Field(f) => expr_spawn_refs(&f.object, spawned),
            Expr::Assign(a) => expr_spawn_refs(&a.value, spawned),
            Expr::Binary(b) => {
                expr_spawn_refs(&b.left, spawned);
                expr_spawn_refs(&b.right, spawned);
            }
            Expr::Unary(u) => expr_spawn_refs(&u.expr, spawned),
            Expr::ForceUnwrap(f) => expr_spawn_refs(&f.expr, spawned),
            Expr::Is(i) => expr_spawn_refs(&i.expr, spawned),
            Expr::Group(e, _) => expr_spawn_refs(e, spawned),
            Expr::If(i) => {
                expr_spawn_refs(&i.cond, spawned);
                block_spawn_refs(&i.then_block, spawned);
                block_spawn_refs(&i.else_block, spawned);
            }
            Expr::Lambda(l) => match &l.body {
                LambdaBody::Expr(e) => expr_spawn_refs(e, spawned),
                LambdaBody::Block(b) => block_spawn_refs(b, spawned),
            },
            Expr::Async(a) => match a {
                AsyncExpr::Spawn(s) => block_capture_refs(&s.body, spawned),
                AsyncExpr::Await(a) => expr_spawn_refs(&a.operand, spawned),
                AsyncExpr::Join(j) => expr_spawn_refs(&j.handle, spawned),
                AsyncExpr::Cancel(c) => expr_spawn_refs(&c.handle, spawned),
                AsyncExpr::ChannelCreate(c) => expr_spawn_refs(&c.capacity, spawned),
                AsyncExpr::ChannelSend(c) => {
                    expr_spawn_refs(&c.channel, spawned);
                    expr_spawn_refs(&c.value, spawned);
                }
                AsyncExpr::ChannelReceive(c) => expr_spawn_refs(&c.channel, spawned),
                AsyncExpr::ChannelClose(c) => expr_spawn_refs(&c.channel, spawned),
            },
            Expr::This(_) | Expr::Int(_) | Expr::Bool(_) | Expr::String(_) | Expr::Null(_) => {}
        }
    }

    block_decls(block, &mut mutable);
    block_spawn_refs(block, &mut spawned);
    mutable.intersection(&spawned).cloned().collect()
}

/// Bounded spawn suspension shape: the first statement awaits a primitive task,
/// and the remaining body is synchronous. Captures are copied into
/// the frame before submission and materialized only after the child reaches
/// a terminal state, so temporary Array/Fun clones never span a pending poll.
pub(crate) fn bounded_spawn_await_shape<'a>(
    body: &'a Block,
    checked: &CheckedFile,
) -> Option<(&'a VarStmt, &'a AwaitExpr)> {
    let Stmt::Var(await_var) = body.stmts.first()? else {
        return None;
    };
    let Expr::Async(AsyncExpr::Await(await_expr)) = &await_var.init else {
        return None;
    };
    if await_var
        .ty
        .as_ref()
        .map(|ty| {
            let key = type_ref_local_key_expand(ty, &[], &[], checked);
            matches!(
                key.as_str(),
                "Int" | "Bool" | "String" | "Opt_Int" | "Opt_Bool"
            ) || is_array_type_key(&key)
                || is_heap_class_mono(&key, checked)
                || is_enum_mono(&key, checked)
                || is_value_struct_mono(&key, checked)
        })
        .unwrap_or(false)
        && !spawn_body_contains_await(&Block {
            stmts: body.stmts[1..].to_vec(),
            span: body.span,
        })
    {
        Some((await_var, await_expr))
    } else {
        None
    }
}

pub(crate) fn bounded_spawn_layout_suffix(captures: &[BoundedSpawnCapture]) -> String {
    let suffix = captures
        .iter()
        .map(|capture| capture.key.as_str())
        .collect::<Vec<_>>()
        .join("_");
    if suffix.is_empty() {
        String::new()
    } else {
        format!("_{suffix}")
    }
}

pub(crate) fn bounded_spawn_data_name(span: Span, suffix: &str) -> String {
    format!("aura_spawn_data_{}{}", span.start, suffix)
}

pub(crate) fn bounded_spawn_poll_name_with_suffix(span: Span, suffix: &str) -> String {
    format!("aura_spawn_poll_{}{}", span.start, suffix)
}

pub(crate) fn bounded_spawn_destroy_name_with_suffix(span: Span, suffix: &str) -> String {
    format!("aura_spawn_destroy_{}{}", span.start, suffix)
}

pub(crate) fn bounded_spawn_gc_mark_name(span: Span, suffix: &str) -> String {
    format!("aura_spawn_gc_mark_{}{}", span.start, suffix)
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
        // Local strings remain owned by their slot; the assignment lowering
        // releases the previous value after the concat has been evaluated.
        Expr::Ident(_) => false,
        Expr::Binary(b) => matches!(b.op, BinOp::Add),
        Expr::Call(_) => crate::stmt::string_call_owns_result(e, ctx),
        Expr::Field(field) => {
            let receiver = resolve_type_name(&field.object, ctx)
                .or_else(|| Some(infer_type_name(&field.object, ctx)))
                .unwrap_or_default();
            receiver == "Int" && field.field.name == "toString"
        }
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
            if i.name == "super" {
                return "this".into();
            }
            // Inside method: bare field names → this->field
            if let Some(class) = ctx.method_class {
                let base = mono_base_name(class, ctx.checked).unwrap_or(class);
                if let Some(cl) = ctx.checked.ast.classes.iter().find(|c| c.name.name == base) {
                    if has_field_in_hierarchy(ctx.checked, cl, &i.name) {
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
            let value = ctx.local_c_name(&i.name);
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
                    .is_some_and(|cl| has_field_in_hierarchy(ctx.checked, cl, &a.name.name))
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
                if string_expr_is_owned_temp(&a.value, ctx) {
                    return format!(
                        "({{ const char *__s = ({rhs}); const char *__r = aura_box_str_set({box_ptr}, __s); free((void *)__s); __r; }})"
                    );
                }
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
                    let owned = string_expr_is_owned_temp(&f.object, ctx);
                    let len_e = if owned {
                        format!("({{ const char *__s = ({obj}); int64_t __n = (int64_t)strlen(__s); free((void *)__s); __n; }})")
                    } else {
                        format!("((int64_t)strlen({obj}))")
                    };
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
                // Immutable Array captures are cloned so the closure owns its snapshot.
                // C12m: by-ref slots store box pointers and retain.
                // C13e: Fun slots copy fat pointer and retain nested env (shared RC).
                let mut fill = String::new();
                let _ = writeln!(fill, "  __e->__drop = aura_lenv_{id}_drop;");
                let _ = writeln!(fill, "  __e->__refs = 1;");
                for cap in captures {
                    let m = mangle_ident(&cap.name);
                    let source_m = ctx.local_c_name(&cap.name);
                    // Capture from enclosing scope local of the same name.
                    if !cap.by_ref && crate::names::is_array_capture_ty(&cap.ty) {
                        let key = cap.ty.mono_suffix();
                        let clone = crate::names::c_method_name(&key, "clone");
                        let _ = writeln!(fill, "  __e->{m} = {clone}(&{source_m});");
                        if crate::array_emit::is_array_of_heap_class(&key, ctx.checked) {
                            let root = crate::array_emit::array_gc_root_add_call(
                                &format!("__e->{m}.data"),
                                &format!("__e->{m}.len"),
                                &key,
                                ctx.checked,
                            );
                            let _ = writeln!(fill, "  {root}");
                        }
                    } else {
                        let _ = writeln!(fill, "  __e->{m} = {source_m};");
                    }
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
    if let Some(key) = semantic_async_inner_key(expr, ctx) {
        return key;
    }
    let key = infer_type_name(expr, ctx);
    task_inner_key(&key).unwrap_or("Unit").to_string()
}

pub(crate) fn spawn_result_key(body: &Block, ctx: &EmitCtx<'_>) -> String {
    body.stmts
        .iter()
        .rev()
        .find_map(|stmt| match stmt {
            Stmt::Return(ReturnStmt {
                value: Some(value), ..
            }) => {
                if let Some(ty) = ctx
                    .checked
                    .expr_tys
                    .get(&(value.span().start, value.span().end))
                {
                    return Some(ty.mono_suffix());
                }
                if let Expr::Ident(id) = value {
                    if let Some(Stmt::Var(local)) = body
                        .stmts
                        .iter()
                        .find(|stmt| matches!(stmt, Stmt::Var(local) if local.name.name == id.name))
                    {
                        if let Some(ty) = &local.ty {
                            return Some(type_ref_local_key_expand(ty, &[], &[], ctx.checked));
                        }
                    }
                }
                Some(infer_type_name(value, ctx))
            }
            _ => None,
        })
        .unwrap_or_else(|| "Unit".into())
}

pub(crate) fn async_inner_key(expr: &Expr, ctx: &EmitCtx<'_>) -> String {
    let key = if let Some(key) = semantic_async_inner_key(expr, ctx) {
        key
    } else {
        let inferred = infer_type_name(expr, ctx);
        task_inner_key(&inferred).unwrap_or("Unit").to_string()
    };
    key
}

/// Nullable reference-like payloads keep their semantic monomorph in generic
/// result layouts but use the underlying C representation for cloning and
/// runtime ownership operations.
pub(crate) fn task_payload_repr_key(key: &str) -> String {
    match key {
        "Opt_Int" | "Opt_Bool" => key.to_string(),
        _ => key.strip_prefix("Opt_").unwrap_or(key).to_string(),
    }
}

fn semantic_async_inner_key(expr: &Expr, ctx: &EmitCtx<'_>) -> Option<String> {
    let span = expr.span();
    let ty = ctx.checked.expr_tys.get(&(span.start, span.end))?;
    let ty = if ctx.type_params.is_empty() {
        ty.clone()
    } else {
        let substitutions = aura_sema::type_subst_map(&ctx.type_params, &ctx.type_args);
        aura_sema::subst_ty(ty, &substitutions)
    };
    match ty {
        Ty::Task(inner) | Ty::TaskHandle(inner) => Some(inner.mono_suffix()),
        _ => None,
    }
}

fn ty_full_mono(ty: &Ty) -> String {
    match ty {
        Ty::Class(name) | Ty::Enum(name) => mono_key(name, &[]),
        Ty::ClassApp { name, args } | Ty::EnumApp { name, args } => mono_key(name, args),
        other => other.mono_suffix(),
    }
}

fn emit_async_expr(expr: &AsyncExpr, ctx: &mut EmitCtx<'_>) -> String {
    match expr {
        AsyncExpr::Spawn(s) => {
            if s.body.stmts.is_empty() {
                let source = s.span.start;
                return format!("({{ AuraTaskFrame *__spawn = aura_task_frame_new(0, aura_task_poll_unit, NULL); if (__spawn != NULL) aura_task_frame_set_race_source_id(__spawn, UINT32_C({source})); if (__spawn != NULL && (__aura_task_executor == NULL || !aura_task_executor_submit(__aura_task_executor, __spawn))) {{ aura_task_frame_destroy(__spawn); __spawn = NULL; }} __spawn; }})");
            }
            let available = ctx.spawn_capture_types();
            let Some(captures) = bounded_spawn_captures(
                &s.body,
                &available,
                ctx.checked,
                &ctx.mutable_spawn_captures,
            ) else {
                if let Some(general_captures) = general_spawn_captures(
                    &s.body,
                    &available,
                    ctx.checked,
                    &ctx.mutable_spawn_captures,
                ) {
                    let function = general_spawn_function_name(s.span, &ctx.checked.package);
                    let args = general_captures
                        .iter()
                        .map(|capture| {
                            if capture.boxed {
                                mangle_ident(&capture.name)
                            } else {
                                emit_expr(
                                    &Expr::Ident(Ident {
                                        name: capture.name.clone(),
                                        span: s.span,
                                    }),
                                    ctx,
                                )
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    return format!("{function}({args})");
                }
                return "({ fputs(\"aura: non-empty spawn body requires C22l state-machine lowering\\n\", stderr); abort(); (AuraTaskFrame *)NULL; })".to_string();
            };
            let layout_suffix = bounded_spawn_layout_suffix(&captures);
            let poll = bounded_spawn_poll_name_with_suffix(s.span, &layout_suffix);
            let source = s.span.start;
            let has_await = bounded_spawn_await_shape(&s.body, ctx.checked).is_some()
                || bounded_spawn_discard_await_shape(&s.body);
            let data_size = if captures.is_empty() && !has_await {
                "0".to_string()
            } else {
                format!(
                    "sizeof({})",
                    bounded_spawn_data_name(s.span, &layout_suffix)
                )
            };
            let init = if captures.is_empty() {
                if has_await {
                    format!("{} *__spawn_data = ({0} *)aura_task_frame_data(__spawn); __spawn_data->await_task = NULL; __spawn_data->await_task_owned = false;", bounded_spawn_data_name(s.span, &layout_suffix))
                } else {
                    String::new()
                }
            } else {
                let assignments = captures
                    .iter()
                    .map(|capture| {
                        let name = &capture.name;
                        let key = &capture.key;
                        let n = mangle_ident(name);
                        if capture.boxed {
                            let retain = match bounded_capture_box_kind(capture) {
                                "string" => "aura_box_str_retain",
                                "i64" => "aura_box_i64_retain",
                                "bool" => "aura_box_bool_retain",
                                _ => "aura_box_ptr_retain",
                            };
                            format!("__spawn_data->{n} = {n}; {retain}(__spawn_data->{n});")
                        } else if key == "String" {
                            format!("__spawn_data->{n} = aura_box_str_new({n});")
                        } else if key == "ForeignHandle" || key.starts_with("ForeignHandle_") {
                            format!("__spawn_data->{n} = {n}; (void)aura_ffi_handle_retain(__spawn_data->{n});")
                        } else if scheduler_owned_key(key) {
                            if key == "Channel" || key.starts_with("Channel_") {
                                format!("__spawn_data->{n} = {n}; (void)aura_task_channel_retain(__spawn_data->{n});")
                            } else {
                                format!("__spawn_data->{n} = {n}; (void)aura_task_executor_retain_payload(__aura_task_executor, __spawn_data->{n});")
                            }
                        } else if is_heap_class_mono(key, ctx.checked) {
                            format!("__spawn_data->{n} = {n}; aura_gc_add_root((void **)&__spawn_data->{n});")
                        } else if is_iface_type_key(key, ctx.checked) {
                            let cty = crate::stmt::local_key_to_c(key, ctx.checked);
                            format!("__spawn_data->{n} = {cty}_clone(&{n});")
                        } else if is_array_type_key(key) {
                            let clone = crate::names::c_method_name(key, "clone");
                            let root = if crate::array_emit::is_array_of_heap_class(key, ctx.checked) {
                                format!(" {}", crate::array_emit::array_gc_root_add_call(
                                    &format!("__spawn_data->{n}.data"),
                                    &format!("__spawn_data->{n}.len"),
                                    key,
                                    ctx.checked,
                                ))
                            } else {
                                String::new()
                            };
                            format!("__spawn_data->{n} = {clone}(&{n});{root}")
                        } else if is_fun_type_key(key) {
                            format!("__spawn_data->{n} = {n}; if (__spawn_data->{n}.env != NULL) aura_fun_env_retain(__spawn_data->{n}.env);")
                        } else if is_enum_mono(key, ctx.checked) {
                            let cty = crate::stmt::local_key_to_c(key, ctx.checked);
                            format!("__spawn_data->{n} = {cty}_clone(&{n});")
                        } else if is_value_struct_mono(key, ctx.checked) {
                            let cty = crate::stmt::local_key_to_c(key, ctx.checked);
                            format!("__spawn_data->{n} = {cty}_clone(&{n});")
                        } else {
                            format!("__spawn_data->{n} = {n};")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                format!(
                    "{data_ty} *__spawn_data = ({data_ty} *)aura_task_frame_data(__spawn); {assignments} {extra}",
                    data_ty = bounded_spawn_data_name(s.span, &layout_suffix),
                    extra = if has_await {
                        "__spawn_data->await_task = NULL; __spawn_data->await_task_owned = false;"
                    } else {
                        ""
                    },
                )
            };
            let destroy = if captures.is_empty() && !has_await {
                "NULL".to_string()
            } else {
                bounded_spawn_destroy_name_with_suffix(s.span, &layout_suffix)
            };
            let gc_mark = bounded_spawn_gc_mark_name(s.span, &layout_suffix);
            let mark = if captures.is_empty() && !has_await {
                String::new()
            } else {
                format!("aura_task_frame_set_gc_mark(__spawn, {gc_mark}); ")
            };
            format!("({{ AuraTaskFrame *__spawn = aura_task_frame_new({data_size}, {poll}, {destroy}); if (__spawn != NULL) {{ aura_task_frame_set_race_source_id(__spawn, UINT32_C({source})); {mark}{init} }} if (__spawn != NULL && (__aura_task_executor == NULL || !aura_task_executor_submit(__aura_task_executor, __spawn))) {{ aura_task_frame_destroy(__spawn); __spawn = NULL; }} __spawn; }})")
        }
        AsyncExpr::Join(j) => emit_join(j, ctx, false),
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
            if ctx
                .checked
                .expr_tys
                .get(&(c.channel.span().start, c.channel.span().end))
                .is_some_and(|ty| matches!(ty, Ty::Class(name) if aura_sema::split_nominal(name) == ("Socket", "std.udp")))
            {
                return format!(
                    "({{ if ({channel} != NULL && {channel}->endpoint != NULL) (void)aura_udp_close({channel}->endpoint->host, {channel}->endpoint->port); (void)0; }})"
                );
            }
            if ctx
                .checked
                .expr_tys
                .get(&(c.channel.span().start, c.channel.span().end))
                .is_some_and(|ty| matches!(ty, Ty::Class(name) if aura_sema::split_nominal(name) == ("Connection", "std.websocket")))
            {
                return format!("({{ if ({channel} != NULL) (void)aura_ws_close({channel}->endpoint); (void)0; }})");
            }
            format!("({{ aura_race_set_source_id(UINT32_C({})); (void)aura_task_channel_close({channel}); aura_race_set_source_id(0); (void)0; }})", c.span.start)
        }
    }
}

pub(crate) fn emit_join_owned(j: &JoinExpr, ctx: &mut EmitCtx<'_>) -> String {
    emit_join(j, ctx, true)
}

fn emit_join(j: &JoinExpr, ctx: &mut EmitCtx<'_>, owned_error: bool) -> String {
    let handle = emit_expr(&j.handle, ctx);
    let inner_key = async_inner_key(&j.handle, ctx);
    let inner = full_type_mono(&task_payload_repr_key(&inner_key), ctx.checked);
    let cty = crate::stmt::local_key_to_c(&inner, ctx.checked);
    let task_error_mono = "std_io_TaskError";
    let semantic_inner = full_type_mono(&inner_key, ctx.checked);
    let result_inner = if is_iface_type_key(&inner, ctx.checked) {
        resolve_iface_mono_key(&inner, ctx.checked)
    } else {
        semantic_inner
    };
    let result_mono = format!("std_io_Result_{}_{}", result_inner, task_error_mono);
    let result_ty = format!("aura_enum_{result_mono}");
    let result_ok = format!("aura_var_{result_mono}_Ok");
    let result_ok_owned = format!("aura_var_{result_mono}_OkOwned");
    let result_err = format!("aura_var_{result_mono}_Err");
    let task_failed = if owned_error {
        "aura_var_std_io_TaskError_FailedOwned"
    } else {
        "aura_var_std_io_TaskError_Failed"
    };
    let task_cancelled = "aura_var_std_io_TaskError_Cancelled";
    let mut out = String::new();
    out.push_str("({ ");
    out.push_str(&result_ty);
    out.push_str(" __join_value; AuraTaskFrame *__join = (");
    out.push_str(&handle);
    out.push_str(&format!(
        "); aura_race_set_source_id(UINT32_C({})); AuraTaskOutcome __join_outcome = aura_task_executor_join_outcome(__aura_task_executor, __join); AuraTaskPollState __join_state = __join_outcome.state; AuraTaskResult __join_result = __join_outcome.result; const char *__join_error = (__join_outcome.error.data != NULL && __join_outcome.error.size != 0) ? (const char *)__join_outcome.error.data : \"joined task failed\"; aura_race_set_source_id(0); ",
        j.span.start
    ));
    if owned_error {
        out.push_str("char *__join_error_owned = NULL; if (__join_state == AURA_TASK_FAILED) { size_t __join_error_len = strlen(__join_error); __join_error_owned = (char *)malloc(__join_error_len + 1); if (__join_error_owned == NULL) abort(); memcpy(__join_error_owned, __join_error, __join_error_len + 1); } ");
    }
    out.push_str(&format!(
        "if (__join_state == AURA_TASK_FAILED) {{ __join_value = {result_err}({task_failed}({})); }} ",
        if owned_error { "__join_error_owned" } else { "__join_error" }
    ));
    out.push_str("if (__join_state == AURA_TASK_FAILED) { const char *__join_type = aura_task_frame_error_type_name(__join); if (__join_type != NULL) { size_t __join_type_len = strlen(__join_type); ");
    if owned_error {
        out.push_str("char *__join_type_owned = (char *)malloc(__join_type_len + 1); if (__join_type_owned == NULL) abort(); memcpy(__join_type_owned, __join_type, __join_type_len + 1); __join_value.data.Err.error.data.Failed.type_name = __join_type_owned; __join_value.data.Err.error.data.Failed.type_name_owned = true; ");
    } else {
        out.push_str("__join_value.data.Err.error.data.Failed.type_name = __join_type; __join_value.data.Err.error.data.Failed.type_name_owned = false; ");
    }
    out.push_str("__join_value.data.Err.error.data.Failed.source_id = aura_task_frame_error_source_id(__join); __join_value.data.Err.error.data.Failed.span_start = aura_task_frame_error_span_start(__join); __join_value.data.Err.error.data.Failed.span_end = aura_task_frame_error_span_end(__join); } } ");
    out.push_str(&format!(
        "else if (__join_state == AURA_TASK_CANCELLED) {{ __join_value = {result_err}({task_cancelled}()); }} "
    ));
    if owned_error {
        out.push_str(&format!(
            "else if (__join_state != AURA_TASK_COMPLETE) {{ __join_value = {result_err}({}((char *)\"joined task is pending\")); }} ",
            "aura_var_std_io_TaskError_Failed"
        ));
    } else {
        out.push_str(&format!(
            "else if (__join_state != AURA_TASK_COMPLETE) {{ __join_value = {result_err}({task_failed}(\"joined task is pending\")); }} "
        ));
    }
    if inner == "Unit" {
        out.push_str(&format!("else {{ __join_value = {result_ok}(); }} "));
    } else if owned_error && inner == "String" {
        out.push_str(&format!(
            "else {{ const char *__join_success = __join_result.data != NULL ? *((const char **)__join_result.data) : NULL; size_t __join_success_len = __join_success != NULL ? strlen(__join_success) : 0; char *__join_success_owned = (char *)malloc(__join_success_len + 1); if (__join_success_owned == NULL) abort(); if (__join_success_len != 0) memcpy(__join_success_owned, __join_success, __join_success_len); __join_success_owned[__join_success_len] = '\\0'; __join_value = {result_ok}Owned(__join_success_owned); }} "
        ));
    } else if owned_error && is_array_type_key(&inner) {
        let clone = crate::names::c_method_name(&full_type_mono(&inner, ctx.checked), "clone");
        out.push_str(&format!(
            "else {{ {cty} __join_array = __join_result.data != NULL ? *(({cty} *)__join_result.data) : ({cty}){{0}}; __join_value = {result_ok_owned}({clone}(&__join_array)); }} "
        ));
    } else if owned_error && is_heap_class_mono(&inner, ctx.checked) {
        out.push_str(&format!(
            "else {{ {cty} __join_class = __join_result.data != NULL ? *(({cty} *)__join_result.data) : NULL; __join_value = {result_ok_owned}(__join_class); }} "
        ));
    } else if owned_error && crate::stmt::is_shared_outcome_error_owner_key(&inner) {
        out.push_str(&format!(
            "else {{ {cty} __join_outcome = __join_result.data != NULL ? *(({cty} *)__join_result.data) : ({cty}){{0}}; if (__join_outcome.tag == 0) {{ const char *__src = __join_outcome.data.OutcomeOk.value; size_t __len = __src == NULL ? 0 : strlen(__src); char *__copy = (char *)malloc(__len + 1); if (__copy == NULL) abort(); if (__src != NULL) memcpy(__copy, __src, __len + 1); __join_outcome.data.OutcomeOk.value = __copy; __join_outcome.data.OutcomeOk.owned = true; }} __join_value = {result_ok}(__join_outcome); if (__join_value.data.Ok.value.tag == 1 && __join_value.data.Ok.value.data.OutcomeErr.error != NULL) {{ __join_value.data.Ok.value.data.OutcomeErr.owned = true; }} __join_value.data.Ok.owned = true; }} "
        ));
    } else if owned_error && (inner == "ForeignHandle" || inner.starts_with("ForeignHandle_")) {
        out.push_str(&format!(
            "else {{ {cty} __join_handle = __join_result.data != NULL ? *(({cty} *)__join_result.data) : NULL; if (__join_handle != NULL && aura_ffi_handle_retain(__join_handle) != AURA_FFI_OK) {{ const char *__msg = \"joined foreign handle could not be retained\"; size_t __len = strlen(__msg); char *__owned = (char *)malloc(__len + 1); if (__owned == NULL) abort(); memcpy(__owned, __msg, __len + 1); __join_value = {result_err}({task_failed}(__owned)); }} else {{ __join_value = {result_ok_owned}(__join_handle); }} }} "
        ));
    } else if owned_error
        && (inner == "Channel"
            || inner.starts_with("Channel_")
            || inner == "Task"
            || inner.starts_with("Task_")
            || inner == "TaskHandle"
            || inner.starts_with("TaskHandle_"))
    {
        if inner == "Channel" || inner.starts_with("Channel_") {
            out.push_str(&format!(
                "else {{ {cty} __join_channel = __join_result.data != NULL ? *(({cty} *)__join_result.data) : NULL; if (__join_channel != NULL && !aura_task_channel_retain(__join_channel)) {{ const char *__msg = \"joined channel could not be retained\"; size_t __len = strlen(__msg); char *__owned = (char *)malloc(__len + 1); if (__owned == NULL) abort(); memcpy(__owned, __msg, __len + 1); __join_value = {result_err}({task_failed}(__owned)); }} else {{ __join_value = {result_ok_owned}(__join_channel); }} }} "
            ));
        } else {
            out.push_str(&format!(
                "else {{ {cty} __join_task = __join_result.data != NULL ? *(({cty} *)__join_result.data) : NULL; if (__join_task != NULL && (__aura_task_executor == NULL || !aura_task_executor_retain_payload(__aura_task_executor, __join_task))) {{ const char *__msg = \"joined task handle could not be retained\"; size_t __len = strlen(__msg); char *__owned = (char *)malloc(__len + 1); if (__owned == NULL) abort(); memcpy(__owned, __msg, __len + 1); __join_value = {result_err}({task_failed}(__owned)); }} else {{ __join_value = {result_ok_owned}(__join_task); }} }} "
            ));
        }
    } else if owned_error && is_enum_mono(&inner, ctx.checked) {
        let clone_fn = format!("{cty}_clone");
        out.push_str(&format!(
            "else {{ {cty} __join_payload = __join_result.data != NULL ? *(({cty} *)__join_result.data) : ({cty}){{0}}; __join_value = {result_ok_owned}({clone_fn}(&__join_payload)); }} "
        ));
    } else if owned_error && is_value_struct_mono(&inner, ctx.checked) {
        let clone_fn = format!("{cty}_clone");
        out.push_str(&format!(
            "else {{ {cty} __join_payload = __join_result.data != NULL ? *(({cty} *)__join_result.data) : ({cty}){{0}}; __join_value = {result_ok_owned}({clone_fn}(&__join_payload)); }} "
        ));
    } else if owned_error && is_iface_type_key(&inner, ctx.checked) {
        let clone_fn = format!("{cty}_clone");
        out.push_str(&format!(
            "else {{ {cty} __join_payload = __join_result.data != NULL ? *(({cty} *)__join_result.data) : ({cty}){{0}}; __join_value = {result_ok_owned}({clone_fn}(&__join_payload)); }} "
        ));
    } else if owned_error && is_fun_type_key(&inner) {
        out.push_str(&format!(
            "else {{ {cty} __join_payload = __join_result.data != NULL ? *(({cty} *)__join_result.data) : ({cty}){{0}}; if (__join_payload.env != NULL) aura_fun_env_retain(__join_payload.env); __join_value = {result_ok_owned}(__join_payload); }} "
        ));
    } else if !owned_error || matches!(inner.as_str(), "Int" | "Bool" | "Opt_Int" | "Opt_Bool") {
        out.push_str(&format!(
            "else {{ __join_value = {result_ok}(__join_result.data != NULL ? *(({cty} *)__join_result.data) : ({cty}){{0}}); }} "
        ));
    } else {
        panic!("unsupported owned task outcome payload type: {inner}");
    }
    out.push_str("__join_value; })");
    out
}

pub(crate) fn is_enum_mono(key: &str, checked: &CheckedFile) -> bool {
    let Some((base, args)) = mono_split(key, checked) else {
        return false;
    };
    checked
        .ast
        .enums
        .iter()
        .any(|enum_decl| enum_decl.name.name == base)
        && (args.is_empty()
            || checked
                .mono_enums
                .iter()
                .any(|(name, candidate)| name == base && candidate.as_slice() == args))
}

pub(crate) fn is_value_struct_mono(key: &str, checked: &CheckedFile) -> bool {
    let Some(base) = mono_base_name(key, checked) else {
        return false;
    };
    checked
        .ast
        .classes
        .iter()
        .any(|class| class.kind == NominalKind::Struct && class.name.name == base)
}

fn emit_await(a: &AwaitExpr, ctx: &mut EmitCtx<'_>) -> String {
    let task = emit_expr(&a.operand, ctx);
    let semantic_inner = async_inner_key(&a.operand, ctx);
    let inner = full_type_mono(&task_payload_repr_key(&semantic_inner), ctx.checked);
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
    out.push_str("if (__await_state == AURA_TASK_CANCELLED) { if (__aura_task_executor != NULL) (void)aura_task_executor_release_terminal(__aura_task_executor, &__await); aura_throw_string(\"task cancelled\"); } if (__await_state == AURA_TASK_FAILED) { AuraTaskResult __await_error = aura_task_frame_error(__await); const char *__await_source = __await_error.data == NULL ? \"awaited task failed\" : (const char *)__await_error.data; size_t __await_length = strlen(__await_source); char *__await_copy = (char *)malloc(__await_length + 1); if (__await_copy != NULL) memcpy(__await_copy, __await_source, __await_length + 1); if (__aura_task_executor != NULL) (void)aura_task_executor_release_terminal(__aura_task_executor, &__await); if (__await_copy != NULL) aura_throw_string_owned(__await_copy); aura_throw_string(\"awaited task failed\"); } ");
    out.push_str("aura_race_set_source_id(0); ");
    if inner == "Unit" {
        out.push_str("(void)0; if (__aura_task_executor != NULL) (void)aura_task_executor_release_terminal(__aura_task_executor, &__await); ");
    } else {
        out.push_str(&format!("AuraTaskResult __await_result = aura_task_frame_result(__await); {cty} __await_value = ({cty}){{0}}; "));
        if inner == "String" {
            out.push_str("if (__await_result.data != NULL) { const char *__src = *((const char **)__await_result.data); size_t __len = __src == NULL ? 0 : strlen(__src); char *__copy = (char *)malloc(__len + 1); if (__copy == NULL) abort(); if (__src != NULL) memcpy(__copy, __src, __len + 1); else __copy[0] = '\\0'; __await_value = __copy; } ");
        } else if cty == "AuraTaskFrame *" {
            out.push_str(&format!(
                "if (__await_result.data != NULL) __await_value = *(({cty} *)__await_result.data); "
            ));
        } else if is_heap_class_mono(&inner, ctx.checked) {
            // Heap objects are transferred from the task result to the caller;
            // the receiving declaration roots the pointer immediately after
            // this expression is evaluated.
            out.push_str(&format!(
                "if (__await_result.data != NULL) __await_value = *(({cty} *)__await_result.data); "
            ));
        } else if matches!(inner.as_str(), "Int" | "Bool" | "Opt_Int" | "Opt_Bool") {
            out.push_str(&format!("if (__await_result.data != NULL) __await_value = *(({cty} *)__await_result.data); "));
        } else {
            let clone = c_method_name(&inner, "clone");
            out.push_str(&format!("if (__await_result.data != NULL) __await_value = {clone}(({cty} *)__await_result.data); "));
        }
        out.push_str("if (__aura_task_executor != NULL) (void)aura_task_executor_release_terminal(__aura_task_executor, &__await); __await_value; ");
    }
    out.push_str("})");
    out
}

fn channel_payload_kind(key: &str, ctx: &EmitCtx<'_>) -> Option<&'static str> {
    if key == "Unit" {
        Some("unit")
    } else if key == "Int" {
        Some("int")
    } else if key == "Bool" {
        Some("bool")
    } else if key == "String" {
        Some("free")
    } else if is_heap_class_mono(key, ctx.checked) {
        Some("class")
    } else if key == "ForeignHandle" || key.starts_with("ForeignHandle_") {
        Some("foreign_handle")
    } else if key == "Task"
        || key.starts_with("Task_")
        || key == "TaskHandle"
        || key.starts_with("TaskHandle_")
    {
        Some("task")
    } else if key == "Channel" || key.starts_with("Channel_") {
        Some("channel")
    } else if is_array_type_key(key)
        || is_enum_mono(key, ctx.checked)
        || is_value_struct_mono(key, ctx.checked)
        || is_iface_type_key(key, ctx.checked)
        || is_fun_type_key(key)
    {
        Some("typed")
    } else {
        None
    }
}

pub(crate) fn emit_channel_send(s: &ChannelSendExpr, ctx: &mut EmitCtx<'_>) -> String {
    let channel = emit_expr(&s.channel, ctx);
    let value = emit_expr(&s.value, ctx);
    if ctx.checked.expr_tys.get(&(s.channel.span().start, s.channel.span().end)).is_some_and(|ty| matches!(ty, Ty::Class(name) if aura_sema::split_nominal(name) == ("Connection", "std.websocket"))) {
        return format!("({{ if ({channel} == NULL || {value} == NULL || aura_ws_send({channel}->endpoint, (int64_t){value}->kind.tag, {value}->payload) < 0) aura_throw_string(\"std.websocket send failed\"); (void)0; }})");
    }
    let channel_key =
        resolve_type_name(&s.channel, ctx).unwrap_or_else(|| infer_type_name(&s.channel, ctx));
    let raw_inner = channel_inner_key(&channel_key).unwrap_or("Unit");
    let inner = task_payload_repr_key(raw_inner);
    let Some(kind) = channel_payload_kind(&inner, ctx) else {
        return "({ fputs(\"aura: Channel payload type is unsupported by C22o\\n\", stderr); abort(); (void)0; })".into();
    };
    if kind == "unit" {
        return format!(
            "({{ AuraTaskChannelValue __v = {{ NULL, 0, NULL }}; aura_race_set_source_id(UINT32_C({})); AuraTaskChannelStatus __status = aura_task_channel_send({}, NULL, __v); aura_race_set_source_id(0); (void)__status; (void)0; }})",
            s.span.start, channel
        );
    }
    let (alloc, destroy): (String, String) = match kind {
        "int" => (format!("int64_t *__p = (int64_t *)malloc(sizeof(*__p)); if (__p == NULL) abort(); *__p = (int64_t)({value});"), "aura_task_channel_value_destroy_free".into()),
        "bool" => (format!("bool *__p = (bool *)malloc(sizeof(*__p)); if (__p == NULL) abort(); *__p = (bool)({value});"), "aura_task_channel_value_destroy_free".into()),
        "free" => (format!("const char *__text = ({value}); size_t __n = __text == NULL ? 0 : strlen(__text); char *__p = (char *)malloc(__n + 1); if (__p == NULL) abort(); if (__n != 0) memcpy(__p, __text, __n); __p[__n] = '\\0';"), "aura_task_channel_value_destroy_free".into()),
        "class" => (format!("void *__obj = (void *)({value}); void **__p = (void **)malloc(sizeof(*__p)); if (__p == NULL) abort(); *__p = __obj; aura_gc_add_root(__p);"), "aura_task_channel_value_destroy_class".into()),
        "foreign_handle" => (format!("AuraFfiOpaqueHandle *__handle = (AuraFfiOpaqueHandle *)({value}); if (__handle != NULL && aura_ffi_handle_retain(__handle) != AURA_FFI_OK) abort(); AuraFfiOpaqueHandle **__p = (AuraFfiOpaqueHandle **)malloc(sizeof(*__p)); if (__p == NULL) abort(); *__p = __handle;"), "aura_task_channel_value_destroy_foreign_handle".into()),
        "task" => ("AuraTaskChannelValue __task_value = aura_task_channel_value_from_task(__aura_task_executor, (AuraTaskFrame *)".to_string() + &value + "); void *__p = __task_value.data;", "aura_task_channel_value_destroy_task".into()),
        "channel" => ("AuraTaskChannelValue __channel_value = aura_task_channel_value_from_channel((AuraTaskChannel *)".to_string() + &value + "); void *__p = __channel_value.data;", "aura_task_channel_value_destroy_channel".into()),
        "typed" => {
            let full = full_type_mono(&inner, ctx.checked);
            let cty = crate::stmt::local_key_to_c(&full, ctx.checked);
            let clone = if is_array_type_key(&full) {
                crate::names::c_method_name(&full, "clone")
            } else {
                format!("{cty}_clone")
            };
            let drop = crate::names::c_channel_drop_name(&full);
            (
                format!("{cty} __source = ({value}); {cty} *__p = ({cty} *)malloc(sizeof(*__p)); if (__p == NULL) abort(); *__p = {clone}(&__source);"),
                drop,
            )
        }
        _ => unreachable!(),
    };
    let transferred_value = if kind == "task" {
        "__task_value"
    } else if kind == "channel" {
        "__channel_value"
    } else {
        "(AuraTaskChannelValue){ __p, sizeof(*__p), "
    };
    if kind == "task" || kind == "channel" {
        format!("({{ {alloc} AuraTaskChannelValue __v = {transferred_value}; if (__v.data == NULL) abort(); aura_race_set_source_id(UINT32_C({})); AuraTaskChannelStatus __status = aura_task_channel_send({channel}, NULL, __v); aura_race_set_source_id(0); if (__status == AURA_CHANNEL_PENDING || __status == AURA_CHANNEL_ERROR) {destroy}(__v.data, __v.size); (void)__status; (void)0; }})", s.span.start)
    } else {
        format!("({{ {alloc} AuraTaskChannelValue __v = {{ __p, sizeof(*__p), {destroy} }}; aura_race_set_source_id(UINT32_C({})); AuraTaskChannelStatus __status = aura_task_channel_send({channel}, NULL, __v); aura_race_set_source_id(0); if (__status == AURA_CHANNEL_PENDING || __status == AURA_CHANNEL_ERROR) {destroy}(__v.data, __v.size); (void)__status; (void)0; }})", s.span.start)
    }
}

pub(crate) fn emit_channel_receive(r: &ChannelReceiveExpr, ctx: &mut EmitCtx<'_>) -> String {
    let channel = emit_expr(&r.channel, ctx);
    if ctx.checked.expr_tys.get(&(r.channel.span().start, r.channel.span().end)).is_some_and(|ty| matches!(ty, Ty::Class(name) if aura_sema::split_nominal(name) == ("Connection", "std.websocket"))) {
        return format!("({{ int64_t __kind = 0; const char *__payload = ({channel} == NULL) ? NULL : aura_ws_receive({channel}->endpoint, &__kind); if (__payload == NULL) return NULL; aura_enum_std_websocket_MessageKind __message_kind = __kind == 0 ? aura_var_std_websocket_MessageKind_Text() : __kind == 1 ? aura_var_std_websocket_MessageKind_Binary() : __kind == 2 ? aura_var_std_websocket_MessageKind_Ping() : __kind == 3 ? aura_var_std_websocket_MessageKind_Pong() : aura_var_std_websocket_MessageKind_Close(); aura_cls_std_websocket_Message *__message = aura_new_std_websocket_Message(__message_kind, __payload); free((void *)__payload); __message; }})");
    }
    let channel_key =
        resolve_type_name(&r.channel, ctx).unwrap_or_else(|| infer_type_name(&r.channel, ctx));
    let raw_inner = channel_inner_key(&channel_key).unwrap_or("Unit");
    let inner = task_payload_repr_key(raw_inner);
    let Some(kind) = channel_payload_kind(&inner, ctx) else {
        return "({ fputs(\"aura: Channel payload type is unsupported by C22o\\n\", stderr); abort(); (void *)0; })".into();
    };
    if kind == "unit" {
        return format!(
            "({{ AuraTaskChannelValue __v = {{0}}; aura_race_set_source_id(UINT32_C({})); (void)aura_task_channel_receive({}, NULL, &__v); aura_race_set_source_id(0); (void)0; }})",
            r.span.start, channel
        );
    }
    let destroy = if kind == "class" {
        "aura_task_channel_value_destroy_class"
    } else if kind == "foreign_handle" {
        "aura_task_channel_value_destroy_foreign_handle"
    } else if kind == "typed" {
        let full = full_type_mono(&inner, ctx.checked);
        let callback = crate::names::c_channel_drop_name(&full);
        // Keep the generated callback name in the expression; it is emitted
        // once with the rest of the aggregate ownership hooks.
        return emit_typed_channel_receive(r, ctx, channel, &full, &callback);
    } else {
        "aura_task_channel_value_destroy_free"
    };
    if kind == "task" {
        return format!("({{ AuraTaskFrame *__r = NULL; AuraTaskChannelValue __v = {{0}}; aura_race_set_source_id(UINT32_C({})); if (aura_task_channel_receive({channel}, NULL, &__v) == AURA_CHANNEL_OK) {{ __r = aura_task_channel_value_take_task(__v.data, __v.size); __v.data = NULL; }} aura_race_set_source_id(0); __r; }})", r.span.start);
    }
    if kind == "channel" {
        return format!("({{ AuraTaskChannel *__r = NULL; AuraTaskChannelValue __v = {{0}}; aura_race_set_source_id(UINT32_C({})); if (aura_task_channel_receive({channel}, NULL, &__v) == AURA_CHANNEL_OK) {{ __r = aura_task_channel_value_take_channel(__v.data, __v.size); __v.data = NULL; }} aura_race_set_source_id(0); __r; }})", r.span.start);
    }
    match kind {
        "int" => format!("({{ aura_opt_i64 __r = {{ false, 0 }}; AuraTaskChannelValue __v = {{0}}; aura_race_set_source_id(UINT32_C({})); if (aura_task_channel_receive({channel}, NULL, &__v) == AURA_CHANNEL_OK) {{ __r = (aura_opt_i64){{ true, *((int64_t *)__v.data) }}; {destroy}(__v.data, __v.size); }} aura_race_set_source_id(0); __r; }})", r.span.start),
        "bool" => format!("({{ aura_opt_bool __r = {{ false, false }}; AuraTaskChannelValue __v = {{0}}; aura_race_set_source_id(UINT32_C({})); if (aura_task_channel_receive({channel}, NULL, &__v) == AURA_CHANNEL_OK) {{ __r = (aura_opt_bool){{ true, *((bool *)__v.data) }}; {destroy}(__v.data, __v.size); }} aura_race_set_source_id(0); __r; }})", r.span.start),
        "free" => format!("({{ const char *__r = NULL; AuraTaskChannelValue __v = {{0}}; aura_race_set_source_id(UINT32_C({})); if (aura_task_channel_receive({channel}, NULL, &__v) == AURA_CHANNEL_OK) {{ __r = (const char *)__v.data; __v.data = NULL; {destroy}(__v.data, __v.size); }} aura_race_set_source_id(0); __r; }})", r.span.start),
        "class" => format!("({{ void *__r = NULL; AuraTaskChannelValue __v = {{0}}; aura_race_set_source_id(UINT32_C({})); if (aura_task_channel_receive({channel}, NULL, &__v) == AURA_CHANNEL_OK) {{ __r = *((void **)__v.data); {destroy}(__v.data, __v.size); }} aura_race_set_source_id(0); __r; }})", r.span.start),
        "foreign_handle" => format!("({{ AuraFfiOpaqueHandle *__r = NULL; AuraTaskChannelValue __v = {{0}}; aura_race_set_source_id(UINT32_C({})); if (aura_task_channel_receive({channel}, NULL, &__v) == AURA_CHANNEL_OK) {{ AuraFfiOpaqueHandle **__payload = (AuraFfiOpaqueHandle **)__v.data; if (__payload != NULL) {{ __r = *__payload; *__payload = NULL; }} {destroy}(__v.data, __v.size); }} aura_race_set_source_id(0); __r; }})", r.span.start),
        _ => unreachable!(),
    }
}

fn emit_typed_channel_receive(
    r: &ChannelReceiveExpr,
    _ctx: &mut EmitCtx<'_>,
    channel: String,
    full: &str,
    destroy: &str,
) -> String {
    let cty = crate::stmt::local_key_to_c(full, _ctx.checked);
    format!(
        "({{ {cty} __r = ({cty}){{0}}; AuraTaskChannelValue __v = {{0}}; aura_race_set_source_id(UINT32_C({})); if (aura_task_channel_receive({channel}, NULL, &__v) == AURA_CHANNEL_OK) {{ {cty} *__payload = ({cty} *)__v.data; if (__payload != NULL) {{ __r = *__payload; *__payload = ({cty}){{0}}; }} {destroy}(__v.data, __v.size); }} aura_race_set_source_id(0); __r; }})",
        r.span.start
    )
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
    if key.contains('@') {
        let (name, package) = aura_sema::split_nominal(key);
        if checked
            .ast
            .classes
            .iter()
            .any(|class| class.name.name == name && class_decl_package(class, checked) == package)
        {
            return type_mono(package, name, &[]);
        }
        if checked
            .ast
            .enums
            .iter()
            .any(|en| en.name.name == name && enum_decl_package(en, checked) == package)
        {
            return type_mono(package, name, &[]);
        }
    }
    // Recover package-qualified nested enum/class monomorphs from the short
    // keys used by local inference (for example `Outcome_String_Error`).
    for (name, args) in checked.mono_enums.iter().chain(checked.mono_classes.iter()) {
        let short = if args.is_empty() {
            name.clone()
        } else {
            format!(
                "{}_{}",
                name,
                args.iter().map(ty_short_mono).collect::<Vec<_>>().join("_")
            )
        };
        if short == key {
            let package = checked
                .ast
                .enums
                .iter()
                .find(|e| e.name.name == *name)
                .map(|e| enum_decl_package(e, checked))
                .or_else(|| {
                    checked
                        .ast
                        .classes
                        .iter()
                        .find(|c| c.name.name == *name)
                        .map(|c| class_decl_package(c, checked))
                })
                .unwrap_or_default();
            return type_mono(&package, name, args);
        }
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
    // Recover a short generic class key (for example `Box_Node` or
    // `Box_Array_Node`) when the sema mono table did not retain that nested
    // aggregate instantiation.
    if let Some((base, suffix)) = key.split_once('_') {
        if let Some(class) = checked
            .ast
            .classes
            .iter()
            .find(|c| c.name.name == base && c.type_params.len() == 1)
        {
            fn short_ty(key: &str, checked: &CheckedFile) -> Option<Ty> {
                match key {
                    "Int" => Some(Ty::Int),
                    "Bool" => Some(Ty::Bool),
                    "String" => Some(Ty::String),
                    "Unit" => Some(Ty::Unit),
                    suffix if suffix.starts_with("Array_") => {
                        let inner = short_ty(suffix.strip_prefix("Array_")?, checked)?;
                        Some(Ty::ClassApp {
                            name: "Array".into(),
                            args: vec![inner],
                        })
                    }
                    name => checked
                        .ast
                        .classes
                        .iter()
                        .find(|c| c.name.name == name)
                        .map(|c| Ty::Class(nominal_key(&class_decl_package(c, checked), name)))
                        .or_else(|| {
                            checked
                                .ast
                                .enums
                                .iter()
                                .find(|e| e.name.name == name)
                                .map(|e| {
                                    Ty::Enum(nominal_key(&enum_decl_package(e, checked), name))
                                })
                        }),
                }
            }
            if let Some(arg) = short_ty(suffix, checked) {
                return type_mono(&class_decl_package(class, checked), base, &[arg]);
            }
        }
    }
    key.to_string()
}

fn ty_short_mono(ty: &Ty) -> String {
    match ty {
        Ty::Class(name) | Ty::Enum(name) => aura_sema::split_nominal(name).0.to_string(),
        Ty::ClassApp { name, args } | Ty::EnumApp { name, args } => {
            let base = aura_sema::split_nominal(name).0;
            if args.is_empty() {
                base.to_string()
            } else {
                format!(
                    "{}_{}",
                    base,
                    args.iter().map(ty_short_mono).collect::<Vec<_>>().join("_")
                )
            }
        }
        other => other.mono_suffix(),
    }
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
    // `this` is a heap-class pointer in method bodies, while emit_expr(This)
    // dereferences it for ordinary field-style value access.
    if matches!(expr, Expr::This(_)) && is_heap_class_mono(expected_ty, ctx.checked) {
        return "this".into();
    }
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

    let actual_mono = full_type_mono(&actual, ctx.checked);
    if crate::class_emit::class_mono_extends(ctx.checked, &actual_mono, expected_ty) {
        return format!("({} *)({code})", c_class_type(expected_ty));
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
                c.name.name == base
                    && crate::iface::class_mono_implements_iface(
                        ctx.checked,
                        c,
                        &class_mono,
                        &imono,
                    )
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
                if ctx.checked.ast.classes.iter().any(|class| {
                    class.name.name == cs.name
                        && crate::iface::class_mono_implements_iface(
                            ctx.checked,
                            class,
                            &class_mono,
                            &imono,
                        )
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
                cl.name.name == id.name
                    && crate::iface::class_decl_implements_iface(ctx.checked, cl, iface_simple)
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
        Expr::Ident(id) if id.name == "super" => ctx
            .method_class
            .and_then(|class| {
                let base = mono_base_name(class, ctx.checked).unwrap_or(class);
                ctx.checked.ast.classes.iter().find(|c| c.name.name == base)
            })
            .and_then(|class| crate::class_emit::direct_superclass(ctx.checked, class))
            .map(|parent| parent.name.name.clone()),
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
                        if let Some(owner) = method_owner(ctx.checked, class, &fe.field.name) {
                            if let Some(m) =
                                owner.methods.iter().find(|m| m.name.name == fe.field.name)
                            {
                                if let Some(rt) = &m.return_type {
                                    let (ps, as_) =
                                        if let Some((_, args)) = mono_split(&recv, ctx.checked) {
                                            let params: Vec<String> = owner
                                                .type_params
                                                .iter()
                                                .map(|p| p.name.name.clone())
                                                .collect();
                                            (params, args.to_vec())
                                        } else if !ctx.type_args.is_empty()
                                            && owner.name.name == ctx.method_class.unwrap_or("")
                                        {
                                            (ctx.type_params.clone(), ctx.type_args.clone())
                                        } else {
                                            (Vec::new(), Vec::new())
                                        };
                                    let method_args = ctx
                                        .checked
                                        .call_instantiations
                                        .get(&c.span.start)
                                        .map(|i| i.method_type_args.clone())
                                        .unwrap_or_default();
                                    let mut all_ps = ps;
                                    all_ps
                                        .extend(m.type_params.iter().map(|p| p.name.name.clone()));
                                    let mut all_as = as_;
                                    all_as.extend(method_args);
                                    return Some(type_ref_local_key(rt, &all_ps, &all_as));
                                }
                            }
                        }
                    }
                    if let Some(i) = ctx.checked.ast.interfaces.iter().find(|i| {
                        i.name.name == recv
                            || iface_mono(i, ctx.checked) == recv
                            || i.name.name == base
                    }) {
                        let (_, iargs) = resolve_iface_decl_and_args(&recv, ctx.checked);
                        if let Some((m, owner, owner_args)) =
                            crate::iface::interface_method_decls_with_parents(
                                ctx.checked,
                                i,
                                &iargs,
                            )
                            .into_iter()
                            .find(|(m, _, _)| m.name.name == fe.field.name)
                        {
                            return m.return_type.as_ref().map(|t| {
                                let owner_params = owner
                                    .type_params
                                    .iter()
                                    .map(|p| p.name.name.clone())
                                    .collect::<Vec<_>>();
                                type_ref_local_key(t, &owner_params, &owner_args)
                            });
                        }
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
            let field_owner = if class.fields.iter().any(|x| x.name.name == f.field.name) {
                class
            } else {
                let mut current = direct_superclass(ctx.checked, class);
                let mut found = None;
                while let Some(candidate) = current {
                    if candidate.fields.iter().any(|x| x.name.name == f.field.name) {
                        found = Some(candidate);
                        break;
                    }
                    current = direct_superclass(ctx.checked, candidate);
                }
                found?
            };
            let field = field_owner
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
            let (ps, as_) = if args.is_empty() && !params.is_empty() && !ctx.type_args.is_empty() {
                (ctx.type_params.clone(), ctx.type_args.clone())
            } else {
                (params, args.to_vec())
            };
            Some(type_ref_local_key_expand(&field.ty, &ps, &as_, ctx.checked))
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
            if i.name == "super" {
                return Some("this".into());
            }
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
