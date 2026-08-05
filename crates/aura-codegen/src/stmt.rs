//! Statement and block emission.

use std::fmt::Write as _;

use aura_ast::*;
use aura_sema::{CheckedFile, Ty};
// Ty used in type_ref_local_key_checked

use crate::class_emit::ownership_fields;
use crate::ctx::EmitCtx;
use crate::expr::{
    array_field_move_out_lvalue, coerce_expr, emit_expr, full_type_mono, infer_type_name,
    mono_base_name, mono_split, owned_string_copy_expr, resolve_type_name,
    string_expr_is_owned_temp,
};
use crate::names::*;

/// Resolve interface mono id + decl + type args for for-in iterable key (C6c/C8c).
fn resolve_iface_for_iter<'a>(
    iter_key: &str,
    checked: &'a CheckedFile,
) -> (String, Option<&'a InterfaceDecl>, Vec<Ty>) {
    let imono = resolve_iface_mono_key(iter_key, checked);
    let (iface, args) = resolve_iface_decl_and_args(iter_key, checked);
    if iface.is_some() {
        return (imono, iface, args);
    }
    // Retry with full mono key if local key was simple (`Iterable_Int`).
    let (iface2, args2) = resolve_iface_decl_and_args(&imono, checked);
    (imono, iface2, args2)
}

/// Local type key with C3v package mono when the TypeRef is qualified or unique.
fn type_ref_local_key_checked(t: &TypeRef, ctx: &EmitCtx<'_>) -> String {
    // C9f: expand type aliases first.
    if ctx
        .checked
        .ast
        .type_aliases
        .iter()
        .any(|a| a.name.name == t.name.name)
    {
        return type_ref_local_key_expand(t, &ctx.type_params, &ctx.type_args, ctx.checked);
    }
    if is_primitive_name(&t.name.name) {
        return type_ref_local_key(t, &ctx.type_params, &ctx.type_args);
    }
    // C4c: Array mono must package-qualify class element types (match emit_array_mono).
    if t.name.name == "Array" {
        let targs: Vec<Ty> = t
            .type_args
            .iter()
            .filter_map(|a| crate::expr::type_ref_to_ty(a, ctx))
            .collect();
        if !targs.is_empty() {
            return mono_key("Array", &targs);
        }
        return type_ref_local_key(t, &ctx.type_params, &ctx.type_args);
    }
    let targs: Vec<Ty> = t
        .type_args
        .iter()
        .filter_map(|a| crate::expr::type_ref_to_ty(a, ctx))
        .collect();
    if let Some(q) = &t.qualifier {
        if let Some(imp) = ctx
            .checked
            .ast
            .imports
            .iter()
            .find(|i| i.alias.as_ref().map(|a| a.name == q.name).unwrap_or(false))
        {
            return type_mono(&imp.path.display(), &t.name.name, &targs);
        }
    }
    // Unique class/enum in unit → package mono.
    let matches: Vec<_> = ctx
        .checked
        .ast
        .classes
        .iter()
        .filter(|c| c.name.name == t.name.name)
        .collect();
    if matches.len() == 1 {
        let pkg = class_decl_package(matches[0], ctx.checked);
        return type_mono(&pkg, &t.name.name, &targs);
    }
    let ematches: Vec<_> = ctx
        .checked
        .ast
        .enums
        .iter()
        .filter(|e| e.name.name == t.name.name)
        .collect();
    if ematches.len() == 1 {
        let pkg = enum_decl_package(ematches[0], ctx.checked);
        return type_mono(&pkg, &t.name.name, &targs);
    }
    type_ref_local_key(t, &ctx.type_params, &ctx.type_args)
}

pub(crate) fn emit_return_fallback(
    out: &mut String,
    ret: &Option<TypeRef>,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
) {
    match ret {
        Some(t) if t.name.name != "Unit" || !t.type_args.is_empty() => {
            let ct = c_type_ref_subst(t, checked, params, args);
            if ct == "void" {
                return;
            }
            if ct == "int64_t" {
                out.push_str("  return 0; /* fallback */\n");
            } else if ct == "bool" {
                out.push_str("  return false; /* fallback */\n");
            } else if ct == "const char *" {
                out.push_str("  return \"\"; /* fallback */\n");
            } else if ct == "aura_opt_i64"
                || ct == "aura_opt_bool"
                || ct.starts_with("aura_cls_")
                || ct.starts_with("aura_iface_")
            {
                let _ = writeln!(out, "  return ({ct}){{0}}; /* fallback */");
            } else {
                let _ = writeln!(out, "  return ({ct}){{0}}; /* fallback */");
            }
        }
        _ => {}
    }
}

pub(crate) fn emit_block(out: &mut String, block: &Block, indent: usize, ctx: &mut EmitCtx<'_>) {
    ctx.push_scope();
    for stmt in &block.stmts {
        emit_stmt(out, stmt, indent, ctx);
    }
    // C6e: unregister Array-of-class element roots before free/pop.
    emit_remove_array_gc_roots(out, indent, &ctx.array_gc_roots_current());
    // C5g: unregister GC roots for heap-class locals in this scope.
    emit_remove_gc_roots(out, indent, &ctx.gc_roots_current());
    // C3t: free Array buffers owned by this block before leaving the scope.
    emit_free_array_owners(out, indent, ctx, &ctx.array_owners_current());
    // Free Fun capture envs owned by this block.
    emit_free_fun_owners(out, indent, ctx, &ctx.fun_owners_current());
    emit_free_string_owners(out, indent, &ctx.string_owners_current());
    emit_destroy_channel_owners(out, indent, &ctx.channel_owners_current());
    emit_free_task_result_owners(out, indent, ctx, &ctx.task_result_owners_current());
    emit_free_shared_outcome_owners(out, indent, ctx);
    emit_release_task_handle_owners(out, indent, ctx, &ctx.task_handle_owners_current());
    // C12m: release by-ref boxes owned by this block (after Fun envs drop their retains).
    emit_release_box_locals(out, indent, ctx, &ctx.box_owners_current());
    ctx.pop_scope();
}

fn emit_free_shared_outcome_owners(out: &mut String, indent: usize, ctx: &EmitCtx<'_>) {
    let mut owners = Vec::new();
    for scope in ctx.locals.iter().rev() {
        for (name, key) in scope {
            if is_shared_outcome_error_owner_key(key) {
                owners.push((name.clone(), key.clone()));
            }
        }
    }
    owners.sort_by(|left, right| left.0.cmp(&right.0));
    for (name, key) in owners {
        crate::emit::emit_owned_value_cleanup(out, indent, &mangle_ident(&name), &key, ctx.checked);
    }
}

/// Free heap buffer of a local `Array` (null-safe; zeros fields).
/// C8f: if elements are Array, free each element's buffer first.
/// C13d: if elements are String, free each owned `const char *` first.
pub(crate) fn emit_free_array_local(
    out: &mut String,
    indent: usize,
    name: &str,
    ty_key: &str,
    checked: &CheckedFile,
) {
    let n = mangle_ident(name);
    crate::array_emit::emit_array_contents_free_checked(out, indent, &n, ty_key, checked);
}

pub(crate) fn emit_free_array_owners(
    out: &mut String,
    indent: usize,
    ctx: &EmitCtx<'_>,
    owners: &[String],
) {
    for name in owners {
        let ty = ctx.lookup_local(name).unwrap_or("Array");
        emit_free_array_local(out, indent, name, ty, ctx.checked);
    }
}

/// Free capture env of a Fun local (`env` may be NULL for non-capturing).
/// C12k: uses `aura_fun_env_free` so class capture GC roots are unregistered.
pub(crate) fn emit_free_fun_local(out: &mut String, indent: usize, name: &str) {
    let p = pad(indent);
    let n = mangle_ident(name);
    let _ = writeln!(
        out,
        "{p}if ({n}.env != NULL) {{ aura_fun_env_free({n}.env); {n}.env = NULL; }}"
    );
}

pub(crate) fn emit_free_fun_owners(
    out: &mut String,
    indent: usize,
    _ctx: &EmitCtx<'_>,
    owners: &[String],
) {
    for name in owners {
        emit_free_fun_local(out, indent, name);
    }
}

/// Free heap-allocated String values returned by runtime/codegen calls.
pub(crate) fn emit_free_string_owners(out: &mut String, indent: usize, owners: &[String]) {
    for name in owners {
        let p = pad(indent);
        let n = mangle_ident(name);
        let _ = writeln!(out, "{p}free((void *){n}); {n} = NULL;");
    }
}

pub(crate) fn emit_destroy_channel_owners(out: &mut String, indent: usize, owners: &[String]) {
    for name in owners {
        let p = pad(indent);
        let n = mangle_ident(name);
        let _ = writeln!(out, "{p}aura_task_channel_destroy({n}); {n} = NULL;");
    }
}

pub(crate) fn is_task_result_owner_key(key: &str) -> bool {
    key.starts_with("std_io_Result_") && key.ends_with("_std_io_TaskError")
}

fn is_task_result_string_owner_key(key: &str) -> bool {
    matches!(
        key.strip_prefix("std_io_Result_")
            .and_then(|rest| rest.strip_suffix("_std_io_TaskError")),
        Some("String" | "Opt_String")
    )
}

pub(crate) fn is_shared_outcome_error_owner_key(key: &str) -> bool {
    ((key.starts_with("std_error_Outcome_") || key.starts_with("Outcome_"))
        && key.ends_with("_std_error_Error"))
        || key == "Outcome_String_Error"
}

fn is_task_result_shared_outcome_error_owner_key(key: &str) -> bool {
    key.starts_with("std_io_Result_std_error_Outcome_")
        && key.ends_with("_std_error_Error_std_io_TaskError")
}

fn task_result_array_owner_key(key: &str) -> Option<&str> {
    key.strip_prefix("std_io_Result_")
        .and_then(|rest| rest.strip_suffix("_std_io_TaskError"))
        .filter(|payload| is_array_type_key(crate::expr::task_payload_repr_key(payload).as_str()))
}

fn task_result_class_owner_key<'a>(key: &'a str, ctx: &EmitCtx<'_>) -> Option<&'a str> {
    let payload = key
        .strip_prefix("std_io_Result_")
        .and_then(|rest| rest.strip_suffix("_std_io_TaskError"))?;
    is_heap_class_mono(&crate::expr::task_payload_repr_key(payload), ctx.checked).then_some(payload)
}

fn task_result_enum_owner_key<'a>(key: &'a str, ctx: &EmitCtx<'_>) -> Option<&'a str> {
    let payload = key
        .strip_prefix("std_io_Result_")
        .and_then(|rest| rest.strip_suffix("_std_io_TaskError"))?;
    let repr = crate::expr::task_payload_repr_key(payload);
    crate::expr::mono_split(&repr, ctx.checked)
        .is_some_and(|(base, _)| ctx.checked.ast.enums.iter().any(|e| e.name.name == base))
        .then_some(payload)
}

fn task_result_struct_owner_key<'a>(key: &'a str, ctx: &EmitCtx<'_>) -> Option<&'a str> {
    let payload = key
        .strip_prefix("std_io_Result_")
        .and_then(|rest| rest.strip_suffix("_std_io_TaskError"))?;
    let repr = crate::expr::task_payload_repr_key(payload);
    let base = crate::expr::mono_base_name(&repr, ctx.checked)?;
    ctx.checked
        .ast
        .classes
        .iter()
        .any(|class| class.kind == aura_ast::NominalKind::Struct && class.name.name == base)
        .then_some(payload)
}

pub(crate) fn task_result_fun_owner_key(key: &str) -> Option<&str> {
    let payload = key
        .strip_prefix("std_io_Result_")
        .and_then(|rest| rest.strip_suffix("_std_io_TaskError"))?;
    is_fun_type_key(&crate::expr::task_payload_repr_key(payload)).then_some(payload)
}

pub(crate) fn emit_free_task_result_owners(
    out: &mut String,
    indent: usize,
    ctx: &EmitCtx<'_>,
    owners: &[String],
) {
    for name in owners {
        let key = ctx.lookup_local(name).unwrap_or_default();
        if !is_task_result_owner_key(key) {
            continue;
        }
        let p = pad(indent);
        let n = mangle_ident(name);
        let ok_cleanup = if is_task_result_string_owner_key(key) {
            format!(
                "if ({n}.tag == 0 && {n}.data.Ok.owned) {{ free((void *){n}.data.Ok.value); {n}.data.Ok.value = NULL; {n}.data.Ok.owned = false; }} "
            )
        } else if let Some(array_key) = task_result_array_owner_key(key) {
            let array_key = crate::expr::task_payload_repr_key(array_key);
            let mut cleanup = String::new();
            crate::array_emit::emit_array_contents_free(
                &mut cleanup,
                0,
                &format!("{n}.data.Ok.value"),
                &array_key,
            );
            format!("if ({n}.tag == 0) {{ {cleanup} }} ")
        } else if task_result_foreign_handle_owner_key(key).is_some() {
            format!(
                "if ({n}.tag == 0 && {n}.data.Ok.owned && {n}.data.Ok.value != NULL) {{ (void)aura_ffi_handle_drop(&{n}.data.Ok.value); {n}.data.Ok.owned = false; }}"
            )
        } else if task_result_class_owner_key(key, ctx).is_some() {
            format!(
                "if ({n}.tag == 0 && {n}.data.Ok.owned && {n}.data.Ok.value != NULL) {{ aura_gc_remove_root((void **)&{n}.data.Ok.value); {n}.data.Ok.value = NULL; {n}.data.Ok.owned = false; }} "
            )
        } else if is_task_result_shared_outcome_error_owner_key(key) {
            format!(
                "if ({n}.tag == 0 && {n}.data.Ok.owned) {{ if ({n}.data.Ok.value.tag == 0 && {n}.data.Ok.value.data.OutcomeOk.owned && {n}.data.Ok.value.data.OutcomeOk.value != NULL) {{ free((void *){n}.data.Ok.value.data.OutcomeOk.value); {n}.data.Ok.value.data.OutcomeOk.value = NULL; {n}.data.Ok.value.data.OutcomeOk.owned = false; }} if ({n}.data.Ok.value.tag == 1 && {n}.data.Ok.value.data.OutcomeErr.owned && {n}.data.Ok.value.data.OutcomeErr.error != NULL) {{ aura_gc_remove_root((void **)&{n}.data.Ok.value.data.OutcomeErr.error); {n}.data.Ok.value.data.OutcomeErr.error = NULL; {n}.data.Ok.value.data.OutcomeErr.owned = false; }} {n}.data.Ok.owned = false; }} "
            )
        } else if task_result_enum_owner_key(key, ctx).is_some() {
            let payload = task_result_enum_owner_key(key, ctx).unwrap();
            let payload_cty =
                local_key_to_c(&crate::expr::task_payload_repr_key(payload), ctx.checked);
            format!(
                "if ({n}.tag == 0 && {n}.data.Ok.owned) {{ {payload_cty}_drop(&{n}.data.Ok.value); {n}.data.Ok.owned = false; }} "
            )
        } else if task_result_struct_owner_key(key, ctx).is_some() {
            let payload = task_result_struct_owner_key(key, ctx).unwrap();
            let payload_cty =
                local_key_to_c(&crate::expr::task_payload_repr_key(payload), ctx.checked);
            format!(
                "if ({n}.tag == 0 && {n}.data.Ok.owned) {{ {payload_cty}_drop(&{n}.data.Ok.value); {n}.data.Ok.owned = false; }} "
            )
        } else if task_result_fun_owner_key(key).is_some() {
            format!(
                "if ({n}.tag == 0 && {n}.data.Ok.owned && {n}.data.Ok.value.env != NULL) {{ aura_fun_env_free({n}.data.Ok.value.env); {n}.data.Ok.value.env = NULL; {n}.data.Ok.owned = false; }} "
            )
        } else if task_result_scheduler_owner_key(key).is_some() {
            let payload = task_result_scheduler_owner_key(key).unwrap();
            if crate::expr::task_payload_repr_key(payload).starts_with("Channel") {
                format!(
                    "if ({n}.tag == 0 && {n}.data.Ok.owned && {n}.data.Ok.value != NULL) {{ aura_task_channel_destroy({n}.data.Ok.value); {n}.data.Ok.value = NULL; {n}.data.Ok.owned = false; }} "
                )
            } else {
                format!(
                    "if ({n}.tag == 0 && {n}.data.Ok.owned && {n}.data.Ok.value != NULL && __aura_task_executor != NULL) {{ (void)aura_task_executor_release_payload(__aura_task_executor, &{n}.data.Ok.value); {n}.data.Ok.owned = false; }} "
                )
            }
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "{p}{ok_cleanup}if ({n}.tag == 1 && {n}.data.Err.error.tag == 0) {{ if ({n}.data.Err.error.data.Failed.owned) {{ free((void *){n}.data.Err.error.data.Failed.error); {n}.data.Err.error.data.Failed.error = NULL; {n}.data.Err.error.data.Failed.owned = false; }} if ({n}.data.Err.error.data.Failed.type_name_owned) {{ free((void *){n}.data.Err.error.data.Failed.type_name); {n}.data.Err.error.data.Failed.type_name = NULL; {n}.data.Err.error.data.Failed.type_name_owned = false; }} }}"
        );
    }
}

fn task_result_scheduler_owner_key(key: &str) -> Option<&str> {
    let payload = key
        .strip_prefix("std_io_Result_")
        .and_then(|rest| rest.strip_suffix("_std_io_TaskError"))?;
    let repr = crate::expr::task_payload_repr_key(payload);
    (repr == "Task"
        || repr.starts_with("Task_")
        || repr == "TaskHandle"
        || repr.starts_with("TaskHandle_")
        || repr == "Channel"
        || repr.starts_with("Channel_")
        || repr.starts_with("std_io_Task")
        || repr.starts_with("std_io_Channel"))
    .then_some(payload)
}

fn task_result_foreign_handle_owner_key(key: &str) -> Option<&str> {
    let payload = key
        .strip_prefix("std_io_Result_")
        .and_then(|rest| rest.strip_suffix("_std_io_TaskError"))?;
    crate::expr::task_payload_repr_key(payload)
        .starts_with("ForeignHandle_")
        .then_some(payload)
}

pub(crate) fn emit_release_task_handle_owners(
    out: &mut String,
    indent: usize,
    ctx: &EmitCtx<'_>,
    owners: &[String],
) {
    for name in owners {
        let key = ctx.lookup_local(name).unwrap_or_default();
        let p = pad(indent);
        let n = mangle_ident(name);
        if key == "ForeignHandle" || key.starts_with("ForeignHandle_") {
            let _ = writeln!(out, "{p}if ({n} != NULL) (void)aura_ffi_handle_drop(&{n});");
        } else if key == "Task"
            || key.starts_with("Task_")
            || key == "TaskHandle"
            || key.starts_with("TaskHandle_")
        {
            let _ = writeln!(
                out,
                "{p}if (__aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &{n});"
            );
        }
    }
}

/// C12m/C13f: release a refcounted by-ref capture box (Int/Bool/String).
pub(crate) fn emit_release_box_local(
    out: &mut String,
    indent: usize,
    name: &str,
    ty_key: &str,
    ptr_box: bool,
) {
    let p = pad(indent);
    let n = mangle_ident(name);
    if is_array_type_key(ty_key) || is_fun_type_key(ty_key) || ptr_box {
        let _ = writeln!(out, "{p}aura_box_ptr_release({n}); {n} = NULL;");
    } else {
        let rel = box_release_fn(ty_key);
        let _ = writeln!(out, "{p}{rel}({n}); {n} = NULL;");
    }
}

pub(crate) fn emit_release_box_locals(
    out: &mut String,
    indent: usize,
    ctx: &EmitCtx<'_>,
    names: &[String],
) {
    for name in names {
        let ty = ctx.lookup_local(name).unwrap_or("Int");
        emit_release_box_local(out, indent, name, ty, is_heap_class_mono(ty, ctx.checked));
    }
}

/// C name of a GC root local. Method `this` is emitted as the C param `this` (not mangled).
fn gc_root_c_name(name: &str) -> String {
    if name == "this" {
        "this".into()
    } else {
        mangle_ident(name)
    }
}

fn emit_remove_gc_roots(out: &mut String, indent: usize, names: &[String]) {
    let p = pad(indent);
    for name in names {
        let n = gc_root_c_name(name);
        let _ = writeln!(out, "{p}aura_gc_remove_root((void **)&{n});");
    }
}

fn emit_remove_array_gc_roots(out: &mut String, indent: usize, names: &[String]) {
    let p = pad(indent);
    for name in names {
        let n = mangle_ident(name);
        let _ = writeln!(out, "{p}aura_gc_remove_array_root((void **)&{n}.data);");
    }
}

fn emit_add_array_gc_root(
    out: &mut String,
    indent: usize,
    name: &str,
    key: &str,
    checked: &CheckedFile,
) {
    let p = pad(indent);
    let n = mangle_ident(name);
    let root = crate::array_emit::array_gc_root_add_call(
        &format!("{n}.data"),
        &format!("{n}.len"),
        key,
        checked,
    );
    let _ = writeln!(out, "{p}{root}");
}

fn is_array_type_key(key: &str) -> bool {
    crate::array_emit::is_array_type_key(key)
}

fn is_array_ctor_expr(e: &Expr) -> bool {
    match e {
        Expr::Call(c) => matches!(c.callee.as_ref(), Expr::Ident(id) if id.name == "Array"),
        _ => false,
    }
}

pub(crate) fn string_call_owns_result(e: &Expr, ctx: &EmitCtx<'_>) -> bool {
    let Expr::Call(call) = e else {
        return false;
    };
    // JSON traversal helpers return freshly allocated strings, including the
    // optional values that are force-unwrapped by `Value` methods.
    let is_json_owned = ctx
        .checked
        .call_instantiations
        .get(&call.span.start)
        .is_some_and(|inst| {
            inst.package == "std.json"
                && matches!(
                    inst.name.as_str(),
                    "jsonObjectGet"
                        | "jsonArrayAt"
                        | "jsonObjectKeys"
                        | "jsonDecodeString"
                        | "jsonDuplicateKey"
                )
        });
    if is_json_owned {
        return true;
    }
    let is_crypto_owned = ctx
        .checked
        .call_instantiations
        .get(&call.span.start)
        .is_some_and(|inst| {
            inst.package == "std.crypto" && matches!(inst.name.as_str(), "randomBytes")
        });
    if is_crypto_owned {
        return true;
    }
    let is_compress_owned = ctx
        .checked
        .call_instantiations
        .get(&call.span.start)
        .is_some_and(|inst| {
            inst.package == "std.compress"
                && matches!(inst.name.as_str(), "compress" | "decompress")
        });
    if is_compress_owned {
        return true;
    }
    // Do not infer ownership from a String return type alone: user functions
    // and foreign helpers may return borrowed/static storage.  Only the
    // concrete allocating primitives below establish transfer ownership.
    let is_array_string_get = match call.callee.as_ref() {
        Expr::Field(field) if field.field.name == "get" => {
            let receiver = resolve_type_name(&field.object, ctx)
                .or_else(|| Some(infer_type_name(&field.object, ctx)))
                .unwrap_or_default();
            receiver == "Array_String" || receiver.ends_with("_Array_String") || receiver == "Array"
        }
        _ => false,
    };
    if infer_type_name(e, ctx) == "String"
        && ctx
            .checked
            .call_instantiations
            .get(&call.span.start)
            .is_some_and(|inst| !inst.type_args.is_empty())
        && !is_array_string_get
    {
        return false;
    }
    if let Expr::Ident(id) = call.callee.as_ref() {
        if id.name == "exception_cause_type" {
            return true;
        }
        if ctx
            .checked
            .ast
            .foreign_functions
            .iter()
            .any(|foreign| foreign.name.name == id.name)
        {
            return false;
        }
        // User-defined String functions return owned values by convention.
        if infer_type_name(e, ctx) == "String" {
            return true;
        }
    }
    match call.callee.as_ref() {
        Expr::Ident(id) => matches!(id.name.as_str(), "readFile" | "tryReadFile"),
        Expr::Field(field) => {
            if matches!(field.field.name.as_str(), "httpResponse" | "loopbackEcho") {
                return false;
            }
            if matches!(field.object.as_ref(), Expr::Ident(id) if ctx.checked.ast.imports.iter().any(|imp| imp.alias.as_ref().is_some_and(|alias| alias.name == id.name)))
                && matches!(field.field.name.as_str(), "readFile" | "tryReadFile")
            {
                return true;
            }
            let receiver_is_array_string_get = match field.object.as_ref() {
                Expr::Call(inner) => match inner.callee.as_ref() {
                    Expr::Field(get_field) if get_field.field.name == "get" => {
                        let get_receiver = resolve_type_name(&get_field.object, ctx)
                            .or_else(|| Some(infer_type_name(&get_field.object, ctx)))
                            .unwrap_or_default();
                        get_receiver == "Array_String" || get_receiver.ends_with("_Array_String")
                    }
                    _ => false,
                },
                _ => false,
            };
            let receiver = resolve_type_name(&field.object, ctx)
                .or_else(|| Some(infer_type_name(&field.object, ctx)))
                .unwrap_or_default();
            // Array<String>.get() always returns a heap copy.  Keep this
            // ownership rule based on the expression's String result rather
            // than one exact monomorph spelling: local type resolution may
            // expose `Array`, a qualified mono, or a package-qualified mono
            // depending on the call site.
            let array_string_get = field.field.name == "get"
                && infer_type_name(e, ctx) == "String"
                && (receiver == "Array"
                    || receiver.starts_with("Array_")
                    || receiver.ends_with("_Array_String"));
            (receiver_is_array_string_get || array_string_get)
                || (receiver == "Int" && field.field.name == "toString")
                || (receiver == "String"
                    && matches!(
                        field.field.name.as_str(),
                        "substring" | "trim" | "trimStart" | "trimEnd" | "toLower"
                    ))
        }
        _ => false,
    }
}

pub(crate) fn pad(n: usize) -> String {
    "  ".repeat(n)
}

pub(crate) fn emit_stmt(out: &mut String, stmt: &Stmt, indent: usize, ctx: &mut EmitCtx<'_>) {
    let p = pad(indent);
    match stmt {
        Stmt::Var(v) => {
            let raw_ty_name = if let Some(annotation) = v.ty.as_ref() {
                type_ref_local_key_checked(annotation, ctx)
            } else if let Some(semantic_ty) = ctx
                .checked
                .expr_tys
                .get(&(v.init.span().start, v.init.span().end))
            {
                // Generic method bodies retain open type parameters in the
                // semantic expression table. Substitute the enclosing mono
                // before deriving the C local type, or later method calls
                // fall back to AuraTypeErasedValue.
                if !semantic_ty.is_open() {
                    infer_type_name(&v.init, ctx)
                } else {
                    let substitutions = aura_sema::type_subst_map(&ctx.type_params, &ctx.type_args);
                    let substituted = aura_sema::subst_ty(semantic_ty, &substitutions);
                    if substituted.is_open() {
                        infer_type_name(&v.init, ctx)
                    } else {
                        full_type_mono(&substituted.mono_suffix(), ctx.checked)
                    }
                }
            } else {
                infer_type_name(&v.init, ctx)
            };
            let ty_name = crate::expr::task_payload_repr_key(&raw_ty_name);
            let ty =
                v.ty.as_ref()
                    .map(|t| c_type_ref_subst(t, ctx.checked, &ctx.type_params, &ctx.type_args))
                    .unwrap_or_else(|| local_key_to_c(&ty_name, ctx.checked));
            // Keep nullable generic arguments in enum context keys. Their C
            // field representation is unchanged, but the monomorph still
            // selects the correct Result/Outcome layout and match arms.
            let context_ty_name = if let Some(semantic_ty) = ctx
                .checked
                .expr_tys
                .get(&(v.init.span().start, v.init.span().end))
                .filter(|ty| {
                    matches!(
                        ty,
                        Ty::EnumApp { name, .. } | Ty::Enum(name)
                            if matches!(aura_sema::split_nominal(name).0, "Result" | "Outcome")
                    )
                }) {
                full_type_mono(&semantic_ty.mono_suffix(), ctx.checked)
            } else {
                full_type_mono(&ty_name, ctx.checked)
            };
            ctx.define_local(&v.name.name, context_ty_name.clone());
            let owns_task_result = is_task_result_owner_key(&context_ty_name)
                && matches!(v.init, Expr::Async(AsyncExpr::Join(_)));
            if owns_task_result {
                ctx.mark_task_result_owner(&v.name.name);
            }
            if ty_name == "Task"
                || ty_name.starts_with("Task_")
                || ty_name == "TaskHandle"
                || ty_name.starts_with("TaskHandle_")
            {
                ctx.mark_task_handle_owner(&v.name.name);
            }
            let owned_foreign_handle_init = ty_name.starts_with("ForeignHandle_")
                && matches!(&v.init, Expr::Call(call) if ctx.checked.ast.foreign_functions.iter().any(|foreign| {
                    matches!(call.callee.as_ref(), Expr::Ident(id) if id.name == foreign.name.name)
                }) || ctx.checked.call_instantiations.get(&call.span.start).is_some_and(|inst| {
                    inst.package == "std.io" && inst.name == "openFile"
                }))
                || (ty_name.starts_with("ForeignHandle_")
                    && matches!(v.init, Expr::Async(AsyncExpr::ChannelReceive(_))));
            if owned_foreign_handle_init {
                ctx.mark_task_handle_owner(&v.name.name);
            }
            let owned_string_channel_init =
                ty_name == "String" && matches!(v.init, Expr::Async(AsyncExpr::ChannelReceive(_)));
            // C22l: make bindings visible to a later bounded spawn in the same
            // lexical scope. `bounded_spawn_captures` still filters the actual
            // capture set to the supported owned types.
            ctx.spawn_params.insert(v.name.name.clone());
            // C12m/C13f: `var` Int/Bool/String that is by-ref captured → heap box local.
            let captured_by_ref =
                v.mutable && ctx.checked.by_ref_capture_names().contains(&v.name.name);
            // C21d: `ref Array<T>` is a scoped header view. It never owns or
            // moves the backing buffer, even when the source is an owning local.
            let borrow_binding = v.ty.as_ref().is_some_and(|t| t.reference);
            let needs_box = (captured_by_ref || ctx.mutable_spawn_captures.contains(&v.name.name))
                && (ty_name == "Int" || ty_name == "Bool" || ty_name == "String");
            let needs_ptr_box = (captured_by_ref
                || ctx.mutable_spawn_captures.contains(&v.name.name))
                && (is_array_type_key(&ty_name)
                    || is_fun_type_key(&ty_name)
                    || is_heap_class_mono(&ty_name, ctx.checked));
            if needs_box || needs_ptr_box {
                ctx.mark_box_owner(&v.name.name);
            }
            let string_owned_init = ty_name == "String" && string_expr_is_owned_temp(&v.init, ctx);
            let string_move_src = if ty_name == "String" {
                match &v.init {
                    Expr::Ident(id) if ctx.is_string_owner(&id.name) => Some(id.name.clone()),
                    Expr::ForceUnwrap(force) if matches!(force.expr.as_ref(), Expr::Ident(_)) => {
                        let Expr::Ident(id) = force.expr.as_ref() else {
                            unreachable!()
                        };
                        ctx.is_string_owner(&id.name).then(|| id.name.clone())
                    }
                    _ => None,
                }
            } else {
                None
            };
            // A mutable String must always own its initial value. Borrowed
            // parameters and literals are copied; owned expressions transfer
            // directly; an owned identifier is moved below after emission.
            let string_copy_init =
                ty_name == "String" && v.mutable && !string_owned_init && string_move_src.is_none();
            if !needs_box
                && (string_owned_init
                    || string_move_src.is_some()
                    || string_copy_init
                    || owned_string_channel_init)
            {
                ctx.mark_string_owner(&v.name.name);
            }
            if ty_name.starts_with("Channel_")
                && matches!(&v.init, Expr::Async(AsyncExpr::ChannelCreate(_)))
            {
                ctx.mark_channel_owner(&v.name.name);
            }
            // C3t: locals from `Array(...)` own the heap buffer.
            // C6d: call/return results that are Array also transfer ownership to the binding.
            // Array get is non-owning for pointer-like elements, but nested
            // arrays are deep-cloned by the generated get method.
            let from_array_get = if let Expr::Call(c) = &v.init {
                if let Expr::Field(fe) = c.callee.as_ref() {
                    if fe.field.name == "get" {
                        let obj_key = resolve_type_name(&fe.object, ctx)
                            .unwrap_or_else(|| infer_type_name(&fe.object, ctx));
                        is_array_type_key(&obj_key)
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };
            let array_get_owns_value = if from_array_get {
                if let Expr::Call(c) = &v.init {
                    if let Expr::Field(fe) = c.callee.as_ref() {
                        let obj_key = resolve_type_name(&fe.object, ctx)
                            .unwrap_or_else(|| infer_type_name(&fe.object, ctx));
                        obj_key.contains("Array_Array_")
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };
            if !needs_ptr_box
                && !borrow_binding
                && is_array_type_key(&ty_name)
                && (is_array_ctor_expr(&v.init)
                    || (matches!(&v.init, Expr::Call(_))
                        && (!from_array_get || array_get_owns_value)))
            {
                ctx.mark_array_owner(&v.name.name);
            }
            // Fun: capturing lambda, call result, or move from owner → own env.
            if !needs_ptr_box && is_fun_type_key(&ty_name) {
                match &v.init {
                    Expr::Lambda(l) => {
                        let has_caps = ctx
                            .checked
                            .lambda_captures
                            .get(&l.span.start)
                            .map(|c| !c.is_empty())
                            .unwrap_or(false);
                        if has_caps {
                            ctx.mark_fun_owner(&v.name.name);
                        }
                    }
                    Expr::Call(_) => {
                        ctx.mark_fun_owner(&v.name.name);
                    }
                    _ => {}
                }
            }
            // C5b: move ownership on `val b = a` when `a` owns an Array buffer.
            let moved_from = if is_array_type_key(&ty_name) && !borrow_binding {
                if let Expr::Ident(id) = &v.init {
                    if ctx.is_array_owner(&id.name) {
                        Some(id.name.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            let fun_moved_from = if is_fun_type_key(&ty_name) {
                if let Expr::Ident(id) = &v.init {
                    if ctx.is_fun_owner(&id.name) {
                        Some(id.name.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            // C8j: Array field bind is a non-owning view (no move-out). Return still moves (C7c).
            if moved_from.is_some() {
                ctx.mark_array_owner(&v.name.name);
            }
            if fun_moved_from.is_some() {
                ctx.mark_fun_owner(&v.name.name);
            }
            let raw_init = if owns_task_result {
                let Expr::Async(AsyncExpr::Join(join)) = &v.init else {
                    unreachable!()
                };
                crate::expr::emit_join_owned(join, ctx)
            } else {
                coerce_expr(&v.init, &ty_name, ctx)
            };
            let init = if needs_box && ty_name == "String" {
                raw_init.clone()
            } else {
                raw_init
            };
            let init = if string_copy_init && !(needs_box && ty_name == "String") {
                owned_string_copy_expr(init, v.init.span())
            } else {
                init
            };
            let dst = mangle_ident(&v.name.name);
            if needs_box {
                let (box_ty, new_fn) = match ty_name.as_str() {
                    "Bool" => ("aura_box_bool *", "aura_box_bool_new"),
                    "String" => ("aura_box_str *", "aura_box_str_new"),
                    _ => ("aura_box_i64 *", "aura_box_i64_new"),
                };
                if ty_name == "String" && string_expr_is_owned_temp(&v.init, ctx) {
                    let _ = writeln!(
                        out,
                        "{p}{box_ty} {dst} = ({{ const char *__s = ({init}); {box_ty} __b = {new_fn}(__s); free((void *)__s); __b; }});"
                    );
                } else {
                    let _ = writeln!(out, "{p}{box_ty} {dst} = {new_fn}({init});");
                }
            } else if needs_ptr_box {
                let payload = format!("{dst}__capture_value");
                let (payload_ty, drop, init_payload) = if is_array_type_key(&ty_name) {
                    let root = if crate::array_emit::is_array_of_heap_class(&ty_name, ctx.checked) {
                        format!(
                            " {}",
                            crate::array_emit::array_gc_root_add_call(
                                &format!("{payload}->data"),
                                &format!("{payload}->len"),
                                &ty_name,
                                ctx.checked,
                            )
                        )
                    } else {
                        String::new()
                    };
                    (
                        ty.clone(),
                        format!("aura_capture_drop_{ty_name}"),
                        format!("*{payload} = {init};{root}"),
                    )
                } else if is_fun_type_key(&ty_name) {
                    (
                        ty.clone(),
                        "aura_capture_drop_fun".into(),
                        format!("*{payload} = {init};"),
                    )
                } else {
                    (
                        "aura_capture_obj_payload".into(),
                        "aura_capture_drop_obj".into(),
                        format!("{payload}->value = (void *)({init}); aura_gc_add_root(&{payload}->value);"),
                    )
                };
                let _ = writeln!(
                    out,
                    "{p}{payload_ty} *{payload} = ({payload_ty} *)malloc(sizeof({payload_ty}));"
                );
                let _ = writeln!(out, "{p}{init_payload}");
                let _ = writeln!(
                    out,
                    "{p}aura_box_ptr *{dst} = aura_box_ptr_new({payload}, {drop});"
                );
            } else {
                let _ = writeln!(out, "{p}{ty} {dst} = {init};");
            }
            if owns_task_result && task_result_class_owner_key(&ty_name, ctx).is_some() {
                let _ = writeln!(
                    out,
                    "{p}if ({dst}.tag == 0 && {dst}.data.Ok.value != NULL) aura_gc_add_root((void **)&{dst}.data.Ok.value);"
                );
            }
            if is_shared_outcome_error_owner_key(&context_ty_name) {
                let _ = writeln!(
                    out,
                    "{p}if ({dst}.tag == 1 && {dst}.data.OutcomeErr.error != NULL) {{ aura_gc_add_root((void **)&{dst}.data.OutcomeErr.error); {dst}.data.OutcomeErr.owned = true; }}"
                );
            }
            if is_task_result_shared_outcome_error_owner_key(&context_ty_name) {
                let _ = writeln!(
                    out,
                    "{p}if ({dst}.tag == 0 && {dst}.data.Ok.value.tag == 1 && {dst}.data.Ok.value.data.OutcomeErr.error != NULL) {{ aura_gc_add_root((void **)&{dst}.data.Ok.value.data.OutcomeErr.error); {dst}.data.Ok.value.data.OutcomeErr.owned = true; {dst}.data.Ok.owned = true; }}"
                );
            }
            if let Some(src) = string_move_src {
                let source = mangle_ident(&src);
                let _ = writeln!(out, "{p}{source} = NULL;");
                ctx.unmark_string_owner(&src);
            }
            if ctx.detector {
                let _ = writeln!(
                    out,
                    "{p}aura_race_record_access((uintptr_t)&({dst}), UINT32_C({}), AURA_RACE_WRITE);",
                    v.span.start
                );
            }
            if let Some(src) = moved_from {
                let src_m = mangle_ident(&src);
                // Zero source so later free of src is a no-op; dst is the sole owner.
                let _ = writeln!(
                    out,
                    "{p}{src_m}.data = NULL; {src_m}.len = 0; {src_m}.cap = 0;"
                );
                ctx.unmark_array_owner(&src);
            }
            if let Some(src) = fun_moved_from {
                let src_m = mangle_ident(&src);
                let _ = writeln!(out, "{p}{src_m}.env = NULL;");
                ctx.unmark_fun_owner(&src);
            }
            // C5g/C21e: owning heap-class locals are GC roots until scope exit.
            // A scoped `ref` alias borrows an already-live iterator/source and
            // must not add an independent root or ownership edge.
            let mono = full_type_mono(&ty_name, ctx.checked);
            if is_heap_class_mono(&mono, ctx.checked) && !needs_ptr_box && !borrow_binding {
                ctx.mark_gc_root(&v.name.name);
                let _ = writeln!(out, "{p}aura_gc_add_root((void **)&{dst});");
            }
            // C6e: Array-of-class locals keep element GC pointers alive across collect.
            if !needs_ptr_box
                && !borrow_binding
                && !from_array_get
                && ctx.is_array_owner(&v.name.name)
                && crate::array_emit::is_array_of_heap_class(&mono, ctx.checked)
            {
                ctx.mark_array_gc_root(&v.name.name);
                emit_add_array_gc_root(out, indent, &v.name.name, &mono, ctx.checked);
            }
        }
        Stmt::If(i) => {
            let _ = writeln!(out, "{p}if ({}) {{", emit_expr(&i.cond, ctx));
            emit_block(out, &i.then_block, indent + 1, ctx);
            if let Some(else_b) = &i.else_block {
                let _ = writeln!(out, "{p}}} else {{");
                emit_block(out, else_b, indent + 1, ctx);
            }
            let _ = writeln!(out, "{p}}}");
        }
        Stmt::While(w) => {
            let _ = writeln!(out, "{p}while ({}) {{", emit_expr(&w.cond, ctx));
            emit_block(out, &w.body, indent + 1, ctx);
            let _ = writeln!(out, "{p}}}");
        }
        Stmt::ForRange(f) => {
            // Evaluate bounds once; `..` exclusive, `..=` inclusive (C3l).
            let start_e = emit_expr(&f.start, ctx);
            let end_e = emit_expr(&f.end, ctx);
            let bind = mangle_ident(&f.name.name);
            let end_tmp = format!("__for_end_{}", f.span.start);
            let cmp = if f.inclusive { "<=" } else { "<" };
            let _ = writeln!(out, "{p}{{");
            let _ = writeln!(out, "{p}  int64_t {end_tmp} = {end_e};");
            let _ = writeln!(
                out,
                "{p}  for (int64_t {bind} = {start_e}; {bind} {cmp} {end_tmp}; {bind}++) {{"
            );
            ctx.push_scope();
            ctx.define_local(&f.name.name, "Int".into());
            for stmt in &f.body.stmts {
                emit_stmt(out, stmt, indent + 2, ctx);
            }
            emit_remove_array_gc_roots(out, indent + 2, &ctx.array_gc_roots_current());
            emit_remove_gc_roots(out, indent + 2, &ctx.gc_roots_current());
            emit_free_array_owners(out, indent + 2, ctx, &ctx.array_owners_current());
            emit_free_fun_owners(out, indent + 2, ctx, &ctx.fun_owners_current());
            emit_free_string_owners(out, indent + 2, &ctx.string_owners_current());
            emit_free_task_result_owners(out, indent + 2, ctx, &ctx.task_result_owners_current());
            emit_release_task_handle_owners(
                out,
                indent + 2,
                ctx,
                &ctx.task_handle_owners_current(),
            );
            emit_release_box_locals(out, indent + 2, ctx, &ctx.box_owners_current());
            ctx.pop_scope();
            let _ = writeln!(out, "{p}  }}");
            let _ = writeln!(out, "{p}}}");
        }
        Stmt::ForIn(f) => {
            let iter_key = infer_type_name(&f.iterable, ctx);
            let iter_e = emit_expr(&f.iterable, ctx);
            let it_tmp = format!("__for_it_{}", f.span.start);
            let idx_tmp = format!("__for_i_{}", f.span.start);
            let bind = mangle_ident(&f.name.name);
            let _ = writeln!(out, "{p}{{");
            if iter_key == "String" {
                // C3w: for (b in s) over UTF-8 bytes as Int.
                let _ = writeln!(out, "{p}  const char *{it_tmp} = {iter_e};");
                let _ = writeln!(out, "{p}  if ({it_tmp} == NULL) {{ {it_tmp} = \"\"; }}");
                let len_tmp = format!("__for_len_{}", f.span.start);
                let _ = writeln!(out, "{p}  size_t {len_tmp} = strlen({it_tmp});");
                let _ = writeln!(
                    out,
                    "{p}  for (size_t {idx_tmp} = 0; {idx_tmp} < {len_tmp}; {idx_tmp}++) {{"
                );
                let _ = writeln!(
                    out,
                    "{p}    int64_t {bind} = (unsigned char){it_tmp}[{idx_tmp}];"
                );
                ctx.push_scope();
                ctx.define_local(&f.name.name, "Int".into());
                for stmt in &f.body.stmts {
                    emit_stmt(out, stmt, indent + 2, ctx);
                }
                emit_remove_array_gc_roots(out, indent + 2, &ctx.array_gc_roots_current());
                emit_remove_gc_roots(out, indent + 2, &ctx.gc_roots_current());
                emit_free_array_owners(out, indent + 2, ctx, &ctx.array_owners_current());
                emit_free_fun_owners(out, indent + 2, ctx, &ctx.fun_owners_current());
                emit_free_string_owners(out, indent + 2, &ctx.string_owners_current());
                emit_release_task_handle_owners(
                    out,
                    indent + 2,
                    ctx,
                    &ctx.task_handle_owners_current(),
                );
                emit_release_box_locals(out, indent + 2, ctx, &ctx.box_owners_current());
                ctx.pop_scope();
                let _ = writeln!(out, "{p}  }}");
            } else if iter_key == "Array"
                || iter_key.starts_with("Array_")
                || mono_base_name(&iter_key, ctx.checked) == Some("Array")
            {
                // for (x in arr) → index loop + Array_get (C3k).
                let mono = if iter_key == "Array" {
                    "Array_Int".into()
                } else {
                    full_type_mono(&iter_key, ctx.checked)
                };
                let elem_key = mono.strip_prefix("Array_").unwrap_or("Int").to_string();
                let arr_c = local_key_to_c(&mono, ctx.checked);
                let elem_c = local_key_to_c(&elem_key, ctx.checked);
                let get_fn = c_method_name(&mono, "get");
                let _ = writeln!(out, "{p}  {arr_c} {it_tmp} = {iter_e};");
                let _ = writeln!(
                    out,
                    "{p}  for (int64_t {idx_tmp} = 0; {idx_tmp} < {it_tmp}.len; {idx_tmp}++) {{"
                );
                let _ = writeln!(
                    out,
                    "{p}    {elem_c} {bind} = {get_fn}(&{it_tmp}, {idx_tmp});"
                );
                ctx.push_scope();
                ctx.define_local(&f.name.name, elem_key);
                for stmt in &f.body.stmts {
                    emit_stmt(out, stmt, indent + 2, ctx);
                }
                emit_remove_array_gc_roots(out, indent + 2, &ctx.array_gc_roots_current());
                emit_remove_gc_roots(out, indent + 2, &ctx.gc_roots_current());
                emit_free_array_owners(out, indent + 2, ctx, &ctx.array_owners_current());
                emit_free_fun_owners(out, indent + 2, ctx, &ctx.fun_owners_current());
                emit_free_string_owners(out, indent + 2, &ctx.string_owners_current());
                emit_release_task_handle_owners(
                    out,
                    indent + 2,
                    ctx,
                    &ctx.task_handle_owners_current(),
                );
                emit_release_box_locals(out, indent + 2, ctx, &ctx.box_owners_current());
                ctx.pop_scope();
                let _ = writeln!(out, "{p}  }}");
            } else if is_iface_type_key(&iter_key, ctx.checked) {
                // C6c/C8c: for-in over interface with len() + get(i) via iface dispatch.
                let (imono, iface, iargs) = resolve_iface_for_iter(&iter_key, ctx.checked);
                let tparams: Vec<String> = iface
                    .map(|i| i.type_params.iter().map(|p| p.name.name.clone()).collect())
                    .unwrap_or_default();
                let elem_key = iface
                    .and_then(|i| {
                        i.methods
                            .iter()
                            .find(|m| m.name.name == "get")
                            .and_then(|m| m.return_type.as_ref())
                            .map(|rt| type_ref_local_key(rt, &tparams, &iargs))
                    })
                    .unwrap_or_else(|| "Int".into());
                let recv_c = c_iface_type(&imono);
                let elem_c = local_key_to_c(&elem_key, ctx.checked);
                let len_fn = c_iface_method_name(&imono, "len");
                let get_fn = c_iface_method_name(&imono, "get");
                let _ = writeln!(out, "{p}  {recv_c} {it_tmp} = {iter_e};");
                let _ = writeln!(
                    out,
                    "{p}  for (int64_t {idx_tmp} = 0; {idx_tmp} < {len_fn}(&{it_tmp}); {idx_tmp}++) {{"
                );
                let _ = writeln!(
                    out,
                    "{p}    {elem_c} {bind} = {get_fn}(&{it_tmp}, {idx_tmp});"
                );
                ctx.push_scope();
                ctx.define_local(&f.name.name, elem_key);
                for stmt in &f.body.stmts {
                    emit_stmt(out, stmt, indent + 2, ctx);
                }
                emit_remove_array_gc_roots(out, indent + 2, &ctx.array_gc_roots_current());
                emit_remove_gc_roots(out, indent + 2, &ctx.gc_roots_current());
                emit_free_array_owners(out, indent + 2, ctx, &ctx.array_owners_current());
                emit_free_fun_owners(out, indent + 2, ctx, &ctx.fun_owners_current());
                emit_free_string_owners(out, indent + 2, &ctx.string_owners_current());
                emit_release_task_handle_owners(
                    out,
                    indent + 2,
                    ctx,
                    &ctx.task_handle_owners_current(),
                );
                emit_release_box_locals(out, indent + 2, ctx, &ctx.box_owners_current());
                ctx.pop_scope();
                let _ = writeln!(out, "{p}  }}");
            } else {
                // C4y: duck Iterable — class/struct with len field/method + get(i).
                let mono = full_type_mono(&iter_key, ctx.checked);
                let base = mono_base_name(&mono, ctx.checked).unwrap_or(mono.as_str());
                let class = ctx.checked.ast.classes.iter().find(|c| c.name.name == base);
                let has_len_field = class
                    .map(|c| c.fields.iter().any(|f| f.name.name == "len"))
                    .unwrap_or(false);
                let has_len_method = class
                    .map(|c| c.methods.iter().any(|m| m.name.name == "len"))
                    .unwrap_or(false);
                let elem_key = class
                    .and_then(|c| {
                        c.methods
                            .iter()
                            .find(|m| m.name.name == "get")
                            .and_then(|m| m.return_type.as_ref())
                            .map(|rt| {
                                let params: Vec<String> =
                                    c.type_params.iter().map(|p| p.name.name.clone()).collect();
                                let targs = mono_split(&mono, ctx.checked)
                                    .map(|(_, a)| a.to_vec())
                                    .unwrap_or_default();
                                type_ref_local_key(rt, &params, &targs)
                            })
                    })
                    .unwrap_or_else(|| "Int".into());
                let recv_c = local_key_to_c(&mono, ctx.checked);
                let elem_c = local_key_to_c(&elem_key, ctx.checked);
                let get_fn = c_method_name(&mono, "get");
                let len_fn = c_method_name(&mono, "len");
                let heap = is_heap_class_mono(&mono, ctx.checked);
                let this_arg = if heap {
                    format!("({it_tmp})")
                } else {
                    format!("&({it_tmp})")
                };
                let _ = writeln!(out, "{p}  {recv_c} {it_tmp} = {iter_e};");
                let len_expr = if has_len_field {
                    if heap {
                        format!("({it_tmp})->len")
                    } else {
                        format!("({it_tmp}).len")
                    }
                } else if has_len_method {
                    format!("{len_fn}({this_arg})")
                } else {
                    format!("({it_tmp}).len")
                };
                let _ = writeln!(
                    out,
                    "{p}  for (int64_t {idx_tmp} = 0; {idx_tmp} < {len_expr}; {idx_tmp}++) {{"
                );
                let _ = writeln!(
                    out,
                    "{p}    {elem_c} {bind} = {get_fn}({this_arg}, {idx_tmp});"
                );
                ctx.push_scope();
                ctx.define_local(&f.name.name, elem_key);
                for stmt in &f.body.stmts {
                    emit_stmt(out, stmt, indent + 2, ctx);
                }
                emit_remove_array_gc_roots(out, indent + 2, &ctx.array_gc_roots_current());
                emit_remove_gc_roots(out, indent + 2, &ctx.gc_roots_current());
                emit_free_array_owners(out, indent + 2, ctx, &ctx.array_owners_current());
                emit_free_fun_owners(out, indent + 2, ctx, &ctx.fun_owners_current());
                emit_free_string_owners(out, indent + 2, &ctx.string_owners_current());
                emit_release_task_handle_owners(
                    out,
                    indent + 2,
                    ctx,
                    &ctx.task_handle_owners_current(),
                );
                emit_release_box_locals(out, indent + 2, ctx, &ctx.box_owners_current());
                ctx.pop_scope();
                let _ = writeln!(out, "{p}  }}");
            }
            let _ = writeln!(out, "{p}}}");
        }
        Stmt::Break(_) => {
            let _ = writeln!(out, "{p}break;");
        }
        Stmt::Continue(_) => {
            let _ = writeln!(out, "{p}continue;");
        }
        Stmt::Match(m) => emit_match(out, m, indent, ctx),
        Stmt::Throw(t) => {
            let ty = infer_type_name(&t.value, ctx);
            let val = emit_expr(&t.value, ctx);
            let _ = writeln!(
                out,
                "{p}aura_ex_set_source_span({}, {});",
                t.span.start, t.span.end
            );
            match ty.as_str() {
                "String" => {
                    let _ = writeln!(out, "{p}aura_throw_string({val});");
                }
                "Int" => {
                    let _ = writeln!(out, "{p}aura_throw_int({val});");
                }
                "Bool" => {
                    let _ = writeln!(out, "{p}aura_throw_bool({val});");
                }
                other if other == "ForeignHandle" || other.starts_with("ForeignHandle_") => {
                    let payload = format!("__throw_foreign_payload_{}", t.span.start);
                    let _ = writeln!(out, "{p}{{ AuraFfiOpaqueHandle *__handle = {val}; if (__handle != NULL && aura_ffi_handle_retain(__handle) != AURA_FFI_OK) abort(); AuraFfiOpaqueHandle **{payload} = (AuraFfiOpaqueHandle **)malloc(sizeof(*{payload})); if ({payload} == NULL) abort(); *{payload} = __handle; aura_throw_obj_with_destructor(\"{other}\", {payload}, aura_destroy_foreign_handle_payload); }}");
                }
                other => {
                    // C3g/C3y: class/struct — malloc a payload copy for exception machinery.
                    let mono = full_type_mono(other, ctx.checked);
                    let base_c = c_class_type(&mono);
                    let tmp = format!("__throw_v_{}", t.span.start);
                    let ptr = format!("__throw_p_{}", t.span.start);
                    let _ = writeln!(out, "{p}{{");
                    if is_heap_class_mono(&mono, ctx.checked) {
                        // val is pointer; copy pointee into malloc payload.
                        let _ = writeln!(out, "{p}  {base_c} *{tmp} = {val};");
                        let _ = writeln!(
                            out,
                            "{p}  {base_c} *{ptr} = ({base_c} *)malloc(sizeof({base_c}));"
                        );
                        let _ = writeln!(out, "{p}  *{ptr} = *{tmp};");
                    } else {
                        let _ = writeln!(out, "{p}  {base_c} {tmp} = {val};");
                        let _ = writeln!(
                            out,
                            "{p}  {base_c} *{ptr} = ({base_c} *)malloc(sizeof({base_c}));"
                        );
                        let _ = writeln!(out, "{p}  *{ptr} = {tmp};");
                    }
                    if is_heap_class_mono(&mono, ctx.checked) {
                        if let Some(base) = mono_base_name(&mono, ctx.checked) {
                            if let Some(class) = ctx
                                .checked
                                .ast
                                .classes
                                .iter()
                                .find(|class| class.name.name == base)
                            {
                                let params: Vec<String> = class
                                    .type_params
                                    .iter()
                                    .map(|param| param.name.name.clone())
                                    .collect();
                                for (field_name, field_key) in
                                    ownership_fields(class, ctx.checked, &params, &[])
                                {
                                    let field_name = mangle_ident(&field_name);
                                    let full_key = full_type_mono(&field_key, ctx.checked);
                                    if field_key != "String"
                                        && !crate::array_emit::is_array_type_key(&full_key)
                                    {
                                        continue;
                                    }
                                    if crate::array_emit::is_array_type_key(&full_key) {
                                        let clone = c_method_name(&full_key, "clone");
                                        let _ = writeln!(out, "{p}  {ptr}->{field_name} = {clone}(&{tmp}->{field_name});");
                                        continue;
                                    }
                                    let copy =
                                        format!("__throw_string_{}_{}", t.span.start, field_name);
                                    let _ = writeln!(out, "{p}  {{");
                                    let _ = writeln!(
                                        out,
                                        "{p}    const char *__src = {tmp}->{field_name};"
                                    );
                                    let _ = writeln!(
                                        out,
                                        "{p}    size_t __len = __src ? strlen(__src) : 0;"
                                    );
                                    let _ = writeln!(
                                        out,
                                        "{p}    char *{copy} = (char *)malloc(__len + 1);"
                                    );
                                    let _ = writeln!(out, "{p}    if ({copy} == NULL) abort();");
                                    let _ = writeln!(
                                        out,
                                        "{p}    if (__len > 0) memcpy({copy}, __src, __len);"
                                    );
                                    let _ = writeln!(out, "{p}    {copy}[__len] = '\\0';");
                                    let _ = writeln!(
                                        out,
                                        "{p}    {ptr}->{field_name} = (const char *){copy};"
                                    );
                                    let _ = writeln!(out, "{p}  }}");
                                }
                            }
                        }
                    }
                    // Match key uses the Aura type name (mono key), not C typedef.
                    // Heap classes have a generated exception wrapper that
                    // releases owned fields and then the copied payload.
                    if is_heap_class_mono(&mono, ctx.checked) {
                        let dtor = format!("aura_ex_dtor_{mono}");
                        // `throw` unwinds through setjmp/longjmp, so lexical
                        // array roots must be removed before leaving the scope.
                        // The exception payload already owns cloned fields.
                        emit_remove_array_gc_roots(out, indent + 2, &ctx.array_gc_roots_all());
                        emit_free_array_owners(out, indent + 2, ctx, &ctx.array_owners_all());
                        emit_free_string_owners(out, indent + 2, &ctx.string_owners_all());
                        let _ = writeln!(
                            out,
                            "{p}  aura_throw_obj_with_destructor(\"{other}\", {ptr}, {dtor});"
                        );
                    } else {
                        let _ = writeln!(out, "{p}  aura_throw_obj(\"{other}\", {ptr});");
                    }
                    let _ = writeln!(out, "{p}}}");
                }
            }
        }
        Stmt::Try(t) => emit_try(out, t, indent, ctx),
        Stmt::Return(r) => {
            // C3t: evaluate return value first, free owned Arrays, then return
            // (so exprs like `return a.get(0)` stay valid).
            // C5g: drop GC roots before leaving so they do not dangle.
            match &r.value {
                None => {
                    emit_remove_array_gc_roots(out, indent, &ctx.array_gc_roots_all());
                    emit_remove_gc_roots(out, indent, &ctx.gc_roots_all());
                    emit_free_array_owners(out, indent, ctx, &ctx.array_owners_all());
                    emit_free_fun_owners(out, indent, ctx, &ctx.fun_owners_all());
                    emit_free_string_owners(out, indent, &ctx.string_owners_all());
                    emit_destroy_channel_owners(out, indent, &ctx.channel_owners_all());
                    emit_release_box_locals(out, indent, ctx, &ctx.box_owners_all());
                    let ret_stmt = if ctx.task_poller {
                        "return AURA_TASK_COMPLETE;"
                    } else {
                        "return;"
                    };
                    let _ = writeln!(out, "{p}{ret_stmt}");
                }
                Some(e) => {
                    let ret_key = infer_type_name(e, ctx);
                    let skip = match e {
                        // Returning a named Array local transfers ownership — do not free it.
                        Expr::Ident(id) if is_array_type_key(&ret_key) => Some(id.name.as_str()),
                        _ => None,
                    };
                    let skip_fun = match e {
                        Expr::Ident(id) if is_fun_type_key(&ret_key) => Some(id.name.as_str()),
                        _ => None,
                    };
                    let skip_string = match e {
                        Expr::Ident(id) if ret_key == "String" => Some(id.name.as_str()),
                        _ => None,
                    };
                    let moved_strings: Vec<String> = match e {
                        Expr::Call(call)
                            if matches!(call.callee.as_ref(), Expr::Ident(id) if matches!(id.name.as_str(), "Ok" | "Err" | "OutcomeOk" | "OutcomeErr"))
                                || ctx
                                    .checked
                                    .call_instantiations
                                    .get(&call.span.start)
                                    .and_then(|inst| inst.variant.as_ref())
                                    .is_some() =>
                        {
                            call.args
                                .iter()
                                .filter_map(|arg| match arg {
                                    Expr::Ident(id)
                                        if ctx.lookup_local(&id.name) == Some("String") =>
                                    {
                                        Some(id.name.clone())
                                    }
                                    Expr::ForceUnwrap(force)
                                        if matches!(force.expr.as_ref(), Expr::Ident(_)) =>
                                    {
                                        let Expr::Ident(id) = force.expr.as_ref() else {
                                            unreachable!()
                                        };
                                        (ctx.lookup_local(&id.name) == Some("String"))
                                            .then(|| id.name.clone())
                                    }
                                    _ => None,
                                })
                                .collect()
                        }
                        _ => Vec::new(),
                    };
                    let skip_foreign_handle = match e {
                        Expr::Ident(id) if ret_key.starts_with("ForeignHandle_") => {
                            Some(id.name.as_str())
                        }
                        _ => None,
                    };
                    let owners: Vec<String> = ctx
                        .array_owners_all()
                        .into_iter()
                        .filter(|n| skip != Some(n.as_str()))
                        .collect();
                    let task_handle_owners: Vec<String> = ctx
                        .task_handle_owners_all()
                        .into_iter()
                        .filter(|n| skip_foreign_handle != Some(n.as_str()))
                        .collect();
                    let fun_owners: Vec<String> = ctx
                        .fun_owners_all()
                        .into_iter()
                        .filter(|n| skip_fun != Some(n.as_str()))
                        .collect();
                    let string_owners: Vec<String> = ctx
                        .string_owners_all()
                        .into_iter()
                        .filter(|n| {
                            skip_string != Some(n.as_str()) && !moved_strings.iter().any(|m| m == n)
                        })
                        .collect();
                    if ret_key == "Unit" {
                        let _ = writeln!(out, "{p}{};", emit_expr(e, ctx));
                        emit_remove_array_gc_roots(out, indent, &ctx.array_gc_roots_all());
                        emit_remove_gc_roots(out, indent, &ctx.gc_roots_all());
                        emit_free_array_owners(out, indent, ctx, &owners);
                        emit_free_fun_owners(out, indent, ctx, &fun_owners);
                        emit_free_string_owners(out, indent, &string_owners);
                        emit_destroy_channel_owners(out, indent, &ctx.channel_owners_all());
                        emit_free_task_result_owners(
                            out,
                            indent,
                            ctx,
                            &ctx.task_result_owners_all(),
                        );
                        emit_release_task_handle_owners(out, indent, ctx, &task_handle_owners);
                        emit_release_box_locals(out, indent, ctx, &ctx.box_owners_all());
                        let ret_stmt = if ctx.task_poller {
                            "return AURA_TASK_COMPLETE;"
                        } else {
                            "return;"
                        };
                        let _ = writeln!(out, "{p}{ret_stmt}");
                    } else {
                        // Prefer declared return type for C7a opt coercion (`return 1` → Int?).
                        let expected = ctx.return_key.clone().unwrap_or_else(|| ret_key.clone());
                        let c_ty = local_key_to_c(&expected, ctx.checked);
                        let tmp = format!("__ret_{}", r.span.start);
                        // C7c: capture field lvalue before coerce re-emits the access.
                        let move_field =
                            if is_array_type_key(&expected) || is_array_type_key(&ret_key) {
                                array_field_move_out_lvalue(e, ctx)
                            } else {
                                None
                            };
                        let val = coerce_expr(e, &expected, ctx);
                        let val = if expected == "String"
                            && !matches!(e, Expr::Ident(_))
                            && !string_expr_is_owned_temp(e, ctx)
                        {
                            // String returns are owned at the ABI boundary; copy borrowed
                            // literals/expressions before the caller releases the result.
                            owned_string_copy_expr(val, e.span())
                        } else {
                            val
                        };
                        let _ = writeln!(out, "{p}{c_ty} {tmp} = {val};");
                        if is_shared_outcome_error_owner_key(&expected) {
                            // Returning an Outcome transfers its payload ownership to the result.
                            // Do not register a root on `tmp`: it is a stack temporary
                            // whose address becomes invalid when this function returns.
                            // The receiving local/result owner registers the payload root.
                            let _ = writeln!(
                                out,
                                "{p}if ({tmp}.tag == 0 && {tmp}.data.OutcomeOk.value != NULL) {tmp}.data.OutcomeOk.owned = true; if ({tmp}.tag == 1 && {tmp}.data.OutcomeErr.error != NULL) {tmp}.data.OutcomeErr.owned = true;"
                            );
                            if let Expr::Ident(id) = e {
                                let source = mangle_ident(&id.name);
                                let _ = writeln!(
                                    out,
                                    "{p}if ({source}.tag == 0) {source}.data.OutcomeOk.owned = false; else if ({source}.tag == 1) {{ if ({source}.data.OutcomeErr.error != NULL) aura_gc_remove_root((void **)&{source}.data.OutcomeErr.error); {source}.data.OutcomeErr.owned = false; }}"
                                );
                            }
                        }
                        emit_free_shared_outcome_owners(out, indent, ctx);
                        // C7c: zero source field so object no longer shares the buffer.
                        if let Some(lv) = move_field {
                            let _ =
                                writeln!(out, "{p}{lv}.data = NULL; {lv}.len = 0; {lv}.cap = 0;");
                        }
                        // Returning a named Fun owner: zero source after copy into tmp.
                        if let Some(src) = skip_fun {
                            let src_m = mangle_ident(src);
                            let _ = writeln!(out, "{p}{src_m}.env = NULL;");
                        }
                        emit_remove_array_gc_roots(out, indent, &ctx.array_gc_roots_all());
                        emit_remove_gc_roots(out, indent, &ctx.gc_roots_all());
                        emit_free_array_owners(out, indent, ctx, &owners);
                        emit_free_fun_owners(out, indent, ctx, &fun_owners);
                        emit_free_string_owners(out, indent, &string_owners);
                        emit_destroy_channel_owners(out, indent, &ctx.channel_owners_all());
                        emit_free_task_result_owners(
                            out,
                            indent,
                            ctx,
                            &ctx.task_result_owners_all(),
                        );
                        emit_release_task_handle_owners(out, indent, ctx, &task_handle_owners);
                        emit_release_box_locals(out, indent, ctx, &ctx.box_owners_all());
                        if ctx.task_poller {
                            let _ = writeln!(out, "{p}return AURA_TASK_COMPLETE;");
                        } else {
                            let _ = writeln!(out, "{p}return {tmp};");
                        }
                    }
                }
            }
        }
        Stmt::Expr(e) => {
            if let Expr::Async(AsyncExpr::Join(join)) = e {
                // A discarded join still produces an owned Result payload. Keep
                // it in a named temporary so generated enum/aggregate drop hooks
                // release nested error and success values immediately.
                let key = full_type_mono(&infer_type_name(e, ctx), ctx.checked);
                let value = crate::expr::emit_join_owned(join, ctx);
                if crate::expr::is_enum_mono(&key, ctx.checked)
                    || crate::expr::is_value_struct_mono(&key, ctx.checked)
                    || is_iface_type_key(&key, ctx.checked)
                {
                    let cty = local_key_to_c(&key, ctx.checked);
                    let drop = format!("{cty}_drop");
                    let _ = writeln!(
                        out,
                        "{p}{{ {cty} __join_discard = {value}; {drop}(&__join_discard); }}"
                    );
                } else {
                    let _ = writeln!(out, "{p}(void)({value});");
                }
            } else {
                let _ = writeln!(out, "{p}(void)({});", emit_expr(e, ctx));
            }
        }
    }
}

pub(crate) fn local_key_to_c(key: &str, checked: &CheckedFile) -> String {
    match key {
        "Int" => "int64_t".into(),
        "Bool" => "bool".into(),
        "String" => "const char *".into(),
        "Unit" => "void".into(),
        "Opt_Int" => "aura_opt_i64".into(),
        "Opt_Bool" => "aura_opt_bool".into(),
        // C22: all task/handle/channel monomorphs use opaque runtime pointers.
        n if n == "Task"
            || n.starts_with("Task_")
            || n == "TaskHandle"
            || n.starts_with("TaskHandle_") =>
        {
            "AuraTaskFrame *".into()
        }
        n if n == "Channel" || n.starts_with("Channel_") => "AuraTaskChannel *".into(),
        n if n == "ForeignHandle" || n.starts_with("ForeignHandle_") => {
            "AuraFfiOpaqueHandle *".into()
        }
        n if n == "Array" || n.starts_with("Array_") => {
            let mono = full_type_mono(n, checked);
            c_class_local_type(&mono, checked)
        }
        // C10e: function-type mono keys → typedef name.
        n if is_fun_type_key(n) => c_fun_typedef(n),
        n if checked
            .ast
            .interfaces
            .iter()
            .any(|i| i.name.name == n || iface_mono(i, checked) == n) =>
        {
            c_iface_type(&iface_mono_from_key(n, checked))
        }
        n => {
            let mono = full_type_mono(n, checked);
            let base = mono_base_name(&mono, checked).unwrap_or(n);
            if is_enum_name(checked, base)
                || checked.ast.enums.iter().any(|e| e.name.name == base)
                || checked.mono_enums.iter().any(|(name, _)| name == base)
            {
                c_enum_type(&mono)
            } else if checked.ast.classes.iter().any(|c| c.name.name == base) {
                c_class_local_type(&mono, checked)
            } else {
                // An unresolved generic parameter must never silently acquire
                // a nominal C class layout. Its operations travel through the
                // explicit type-erased payload ABI.
                "AuraTypeErasedValue".into()
            }
        }
    }
}

pub(crate) fn emit_try(out: &mut String, t: &TryStmt, indent: usize, ctx: &mut EmitCtx<'_>) {
    let p = pad(indent);
    let jb = format!("__jb_{}", t.span.start);
    // 0 = ok, 1 = caught, 2 = rethrow after finally (frame still on stack)
    let state = format!("__ex_state_{}", t.span.start);
    let _ = writeln!(out, "{p}{{");
    let _ = writeln!(out, "{p}  jmp_buf {jb};");
    let _ = writeln!(out, "{p}  int {state} = 0;");
    let _ = writeln!(out, "{p}  if (setjmp({jb}) == 0) {{");
    let _ = writeln!(out, "{p}    aura_try_enter(&{jb});");
    for stmt in &t.try_block.stmts {
        emit_stmt(out, stmt, indent + 2, ctx);
    }
    let _ = writeln!(out, "{p}    aura_try_leave();");
    let _ = writeln!(out, "{p}  }} else {{");
    if let Some(c) = &t.catch {
        // Local key for catch type (handles generics as mono key).
        let catch_key = type_ref_local_key(&c.ty, &ctx.type_params, &ctx.type_args);
        let _ = writeln!(out, "{p}    if (aura_ex_matches(\"{catch_key}\")) {{");
        let bind = mangle_ident(&c.name.name);
        match catch_key.as_str() {
            "String" => {
                let _ = writeln!(out, "{p}      const char *{bind} = aura_ex_as_string();");
            }
            "Int" => {
                let _ = writeln!(out, "{p}      int64_t {bind} = aura_ex_as_int();");
            }
            "Bool" => {
                let _ = writeln!(out, "{p}      bool {bind} = aura_ex_as_bool();");
            }
            other if other == "ForeignHandle" || other.starts_with("ForeignHandle_") => {
                let _ = writeln!(out, "{p}      AuraFfiOpaqueHandle **__payload = (AuraFfiOpaqueHandle **)aura_ex_take_obj(); AuraFfiOpaqueHandle *__source = __payload == NULL ? NULL : *__payload; if (__source != NULL && aura_ffi_handle_retain(__source) != AURA_FFI_OK) abort(); aura_destroy_foreign_handle_payload(__payload); AuraFfiOpaqueHandle *{bind} = __source;");
            }
            other => {
                let mono = full_type_mono(other, ctx.checked);
                let base_c = c_class_type(&mono);
                if is_heap_class_mono(&mono, ctx.checked) {
                    // Promote exception payload into GC heap pointer for the catch binding.
                    let _ = writeln!(
                        out,
                        "{p}      {base_c} *{bind} = ({base_c} *)aura_gc_alloc_full(sizeof({base_c}), aura_dtor_{mono}, NULL);"
                    );
                    let _ = writeln!(out, "{p}      *{bind} = *({base_c} *)aura_ex_as_obj();");
                    // Catch bindings outlive the exception frame.  Deep-copy
                    // owned String fields before aura_ex_clear disposes the
                    // throw payload; the binding's GC destructor owns the copy.
                    if let Some(base) = mono_base_name(&mono, ctx.checked) {
                        if let Some(class) = ctx
                            .checked
                            .ast
                            .classes
                            .iter()
                            .find(|class| class.name.name == base)
                        {
                            let params: Vec<String> = class
                                .type_params
                                .iter()
                                .map(|param| param.name.name.clone())
                                .collect();
                            for (field_name, field_key) in
                                ownership_fields(class, ctx.checked, &params, &[])
                            {
                                let field_name = mangle_ident(&field_name);
                                let full_key = full_type_mono(&field_key, ctx.checked);
                                if field_key != "String"
                                    && !crate::array_emit::is_array_type_key(&full_key)
                                {
                                    continue;
                                }
                                let src = format!("(({base_c} *)aura_ex_as_obj())->{field_name}");
                                if crate::array_emit::is_array_type_key(&full_key) {
                                    let clone = c_method_name(&full_key, "clone");
                                    let _ = writeln!(
                                        out,
                                        "{p}      {bind}->{field_name} = {clone}(&{src});"
                                    );
                                    continue;
                                }
                                let copy =
                                    format!("__catch_string_{}_{}", t.span.start, field_name);
                                let _ = writeln!(out, "{p}      {{");
                                let _ = writeln!(out, "{p}        const char *__src = {src};");
                                let _ = writeln!(
                                    out,
                                    "{p}        size_t __len = __src ? strlen(__src) : 0;"
                                );
                                let _ = writeln!(
                                    out,
                                    "{p}        char *{copy} = (char *)malloc(__len + 1);"
                                );
                                let _ = writeln!(out, "{p}        if ({copy} == NULL) abort();");
                                let _ = writeln!(
                                    out,
                                    "{p}        if (__len > 0) memcpy({copy}, __src, __len);"
                                );
                                let _ = writeln!(out, "{p}        {copy}[__len] = '\\0';");
                                let _ = writeln!(
                                    out,
                                    "{p}        {bind}->{field_name} = (const char *){copy};"
                                );
                                let _ = writeln!(out, "{p}      }}");
                            }
                        }
                    }
                } else {
                    let _ = writeln!(
                        out,
                        "{p}      {base_c} {bind} = *({base_c} *)aura_ex_as_obj();"
                    );
                }
            }
        }
        let _ = writeln!(out, "{p}      aura_ex_clear();");
        let _ = writeln!(out, "{p}      aura_try_leave();");
        let _ = writeln!(out, "{p}      {state} = 1;");
        ctx.push_scope();
        ctx.define_local(&c.name.name, catch_key.clone());
        for stmt in &c.body.stmts {
            emit_stmt(out, stmt, indent + 3, ctx);
        }
        ctx.pop_scope();
        let _ = writeln!(out, "{p}    }} else {{");
        // Record the compiler-visible nested boundary before rethrowing.
        let _ = writeln!(
            out,
            "{p}      (void)aura_ex_add_cause(\"{catch_key}\", {}, {});",
            t.span.start, t.span.end
        );
        // Keep frame for aura_ex_rethrow (do not leave).
        let _ = writeln!(out, "{p}      {state} = 2;");
        let _ = writeln!(out, "{p}    }}");
    } else {
        let _ = writeln!(out, "{p}    {state} = 2;");
    }
    let _ = writeln!(out, "{p}  }}");
    if let Some(fin) = &t.finally {
        for stmt in &fin.stmts {
            emit_stmt(out, stmt, indent + 1, ctx);
        }
    }
    let _ = writeln!(out, "{p}  if ({state} == 2) {{ aura_ex_rethrow(); }}");
    let _ = writeln!(out, "{p}}}");
}

pub(crate) fn emit_match(out: &mut String, m: &MatchStmt, indent: usize, ctx: &mut EmitCtx<'_>) {
    let p = pad(indent);
    let scrut_key = infer_type_name(&m.scrutinee, ctx);
    let scrut_c = local_key_to_c(&scrut_key, ctx.checked);
    let tmp = format!("__match_{}", m.span.start);
    let scrut_ty = ctx
        .checked
        .expr_tys
        .get(&(m.scrutinee.span().start, m.scrutinee.span().end));
    let _ = writeln!(
        out,
        "{p}{{ {scrut_c} {tmp} = {};",
        emit_expr(&m.scrutinee, ctx)
    );
    let _ = writeln!(out, "{p}  switch ({tmp}.tag) {{");

    let ename = mono_base_name(&scrut_key, ctx.checked)
        .or_else(|| match scrut_ty {
            Some(Ty::Enum(name) | Ty::EnumApp { name, .. }) => {
                Some(aura_sema::split_nominal(name).0)
            }
            _ => None,
        })
        .or_else(|| {
            if is_enum_name(ctx.checked, &scrut_key) {
                Some(scrut_key.as_str())
            } else {
                ctx.checked
                    .mono_enums
                    .iter()
                    .find(|(n, a)| mono_key(n, a) == scrut_key)
                    .map(|(n, _)| n.as_str())
            }
        })
        .unwrap_or(&scrut_key);

    let enum_decl = ctx.checked.ast.enums.iter().find(|e| e.name.name == ename);

    for arm in &m.arms {
        let Pattern::Variant { name, bindings, .. } = &arm.pattern;
        let tag = enum_decl
            .and_then(|e| e.variants.iter().position(|v| v.name.name == name.name))
            .unwrap_or(0);
        let _ = writeln!(out, "{p}  case {tag}: {{");
        ctx.push_scope();
        if let Some(e) = enum_decl {
            if let Some(v) = e.variants.iter().find(|v| v.name.name == name.name) {
                let params: Vec<String> =
                    e.type_params.iter().map(|p| p.name.name.clone()).collect();
                // Resolve package-prefixed mono (`demo_result_Result_Int_String`) via mono_split
                // so type params (T/E) substitute correctly in arm bindings.
                let targs: Vec<Ty> = match scrut_ty {
                    Some(Ty::EnumApp { args, .. }) => args.clone(),
                    _ => mono_split(&scrut_key, ctx.checked)
                        .map(|(_, a)| a.to_vec())
                        .or_else(|| {
                            ctx.checked
                                .mono_enums
                                .iter()
                                .find(|(n, a)| mono_key(n, a) == scrut_key)
                                .map(|(_, a)| a.clone())
                        })
                        .unwrap_or_default(),
                };
                for (bind, field) in bindings.iter().zip(v.fields.iter()) {
                    let fty = type_ref_local_key(&field.ty, &params, &targs);
                    if fty == "Unit" {
                        // Unit payloads are represented as an absent C value;
                        // the semantic binding remains valid but needs no C
                        // storage and cannot be read as a runtime value.
                        continue;
                    }
                    let ct = c_type_ref_subst(&field.ty, ctx.checked, &params, &targs);
                    ctx.define_local(&bind.name, fty);
                    let _ = writeln!(
                        out,
                        "{p}    {ct} {} = {tmp}.data.{}.{};",
                        mangle_ident(&bind.name),
                        mangle_ident(&v.name.name),
                        mangle_ident(&field.name.name)
                    );
                }
            }
        }
        for stmt in &arm.body.stmts {
            emit_stmt(out, stmt, indent + 2, ctx);
        }
        // Match bindings live in the arm scope.  Clean up every owner/root
        // registered there before the C block ends; otherwise a heap-class
        // binding such as `value` leaves a GC root pointing at a dead stack
        // slot and a later collection reads it after return from this poll.
        emit_remove_array_gc_roots(out, indent + 2, &ctx.array_gc_roots_current());
        emit_remove_gc_roots(out, indent + 2, &ctx.gc_roots_current());
        emit_free_array_owners(out, indent + 2, ctx, &ctx.array_owners_current());
        emit_free_fun_owners(out, indent + 2, ctx, &ctx.fun_owners_current());
        emit_free_string_owners(out, indent + 2, &ctx.string_owners_current());
        emit_destroy_channel_owners(out, indent + 2, &ctx.channel_owners_current());
        emit_free_task_result_owners(out, indent + 2, ctx, &ctx.task_result_owners_current());
        emit_release_task_handle_owners(out, indent + 2, ctx, &ctx.task_handle_owners_current());
        emit_release_box_locals(out, indent + 2, ctx, &ctx.box_owners_current());
        ctx.pop_scope();
        let _ = writeln!(out, "{p}    break;\n{p}  }}");
    }
    let _ = writeln!(out, "{p}  }}\n{p}}}");
}
