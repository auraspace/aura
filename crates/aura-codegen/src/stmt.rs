//! Statement and block emission.

use std::fmt::Write as _;

use aura_ast::*;
use aura_sema::{CheckedFile, Ty};
// Ty used in type_ref_local_key_checked

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
    // C12m: release by-ref boxes owned by this block (after Fun envs drop their retains).
    emit_release_box_locals(out, indent, ctx, &ctx.box_owners_current());
    ctx.pop_scope();
}

/// Free heap buffer of a local `Array` (null-safe; zeros fields).
/// C8f: if elements are Array, free each element's buffer first.
/// C13d: if elements are String, free each owned `const char *` first.
pub(crate) fn emit_free_array_local(out: &mut String, indent: usize, name: &str, ty_key: &str) {
    let n = mangle_ident(name);
    crate::array_emit::emit_array_contents_free(out, indent, &n, ty_key);
}

pub(crate) fn emit_free_array_owners(
    out: &mut String,
    indent: usize,
    ctx: &EmitCtx<'_>,
    owners: &[String],
) {
    for name in owners {
        let ty = ctx.lookup_local(name).unwrap_or("Array");
        emit_free_array_local(out, indent, name, ty);
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

fn emit_add_array_gc_root(out: &mut String, indent: usize, name: &str) {
    let p = pad(indent);
    let n = mangle_ident(name);
    let _ = writeln!(
        out,
        "{p}aura_gc_add_array_root((void **)&{n}.data, &{n}.len);"
    );
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
    // Do not infer ownership from a String return type alone: user functions
    // and foreign helpers may return borrowed/static storage.  Only the
    // concrete allocating primitives below establish transfer ownership.
    if infer_type_name(e, ctx) == "String"
        && ctx
            .checked
            .call_instantiations
            .get(&call.span.start)
            .is_some_and(|inst| !inst.type_args.is_empty())
    {
        return false;
    }
    if let Expr::Ident(id) = call.callee.as_ref() {
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
                        get_receiver.starts_with("Array_String")
                    }
                    _ => false,
                },
                _ => false,
            };
            let receiver = resolve_type_name(&field.object, ctx)
                .or_else(|| Some(infer_type_name(&field.object, ctx)))
                .unwrap_or_default();
            (receiver_is_array_string_get
                || (receiver.starts_with("Array_String") && field.field.name == "get"))
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
            let ty_name =
                v.ty.as_ref()
                    .map(|t| type_ref_local_key_checked(t, ctx))
                    .unwrap_or_else(|| infer_type_name(&v.init, ctx));
            let ty =
                v.ty.as_ref()
                    .map(|t| c_type_ref_subst(t, ctx.checked, &ctx.type_params, &ctx.type_args))
                    .unwrap_or_else(|| local_key_to_c(&ty_name, ctx.checked));
            // Store package mono key so method dispatch picks the right C symbol (C3v).
            ctx.define_local(&v.name.name, full_type_mono(&ty_name, ctx.checked));
            // C22l: make bindings visible to a later bounded spawn in the same
            // lexical scope. `bounded_spawn_captures` still filters the actual
            // capture set to the supported owned types.
            if v.ty.is_some() {
                ctx.spawn_params.insert(v.name.name.clone());
            }
            // C12m/C13f: `var` Int/Bool/String that is by-ref captured → heap box local.
            let captured_by_ref =
                v.mutable && ctx.checked.by_ref_capture_names().contains(&v.name.name);
            // C21d: `ref Array<T>` is a scoped header view. It never owns or
            // moves the backing buffer, even when the source is an owning local.
            let borrow_binding = v.ty.as_ref().is_some_and(|t| t.reference);
            let needs_box =
                captured_by_ref && (ty_name == "Int" || ty_name == "Bool" || ty_name == "String");
            let needs_ptr_box = captured_by_ref
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
            if !needs_box && (string_owned_init || string_move_src.is_some() || string_copy_init) {
                ctx.mark_string_owner(&v.name.name);
            }
            if ty_name.starts_with("Channel_")
                && matches!(&v.init, Expr::Async(AsyncExpr::ChannelCreate(_)))
            {
                ctx.mark_channel_owner(&v.name.name);
            }
            // C3t: locals from `Array(...)` own the heap buffer.
            // C6d: call/return results that are Array also transfer ownership to the binding.
            // C8e: `arr.get(i)` of nested Array is a shallow view — do not own (avoids double-free).
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
            if !needs_ptr_box
                && !borrow_binding
                && is_array_type_key(&ty_name)
                && (is_array_ctor_expr(&v.init)
                    || (matches!(&v.init, Expr::Call(_)) && !from_array_get))
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
            let raw_init = coerce_expr(&v.init, &ty_name, ctx);
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
                            " aura_gc_add_array_root((void **)&{payload}->data, &{payload}->len);"
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
            if crate::array_emit::is_array_of_heap_class(&mono, ctx.checked) {
                ctx.mark_array_gc_root(&v.name.name);
                emit_add_array_gc_root(out, indent, &v.name.name);
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
                                for field in &class.fields {
                                    if type_ref_local_key(&field.ty, &params, &[]) != "String" {
                                        continue;
                                    }
                                    let field_name = mangle_ident(&field.name.name);
                                    let copy = format!(
                                        "__throw_string_{}_{}",
                                        t.span.start, field.span.start
                                    );
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
                    let _ = writeln!(out, "{p}return;");
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
                    let owners: Vec<String> = ctx
                        .array_owners_all()
                        .into_iter()
                        .filter(|n| skip != Some(n.as_str()))
                        .collect();
                    let fun_owners: Vec<String> = ctx
                        .fun_owners_all()
                        .into_iter()
                        .filter(|n| skip_fun != Some(n.as_str()))
                        .collect();
                    let string_owners: Vec<String> = ctx
                        .string_owners_all()
                        .into_iter()
                        .filter(|n| skip_string != Some(n.as_str()))
                        .collect();
                    if ret_key == "Unit" {
                        let _ = writeln!(out, "{p}{};", emit_expr(e, ctx));
                        emit_remove_array_gc_roots(out, indent, &ctx.array_gc_roots_all());
                        emit_remove_gc_roots(out, indent, &ctx.gc_roots_all());
                        emit_free_array_owners(out, indent, ctx, &owners);
                        emit_free_fun_owners(out, indent, ctx, &fun_owners);
                        emit_free_string_owners(out, indent, &string_owners);
                        emit_destroy_channel_owners(out, indent, &ctx.channel_owners_all());
                        emit_release_box_locals(out, indent, ctx, &ctx.box_owners_all());
                        let _ = writeln!(out, "{p}return;");
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
                        let _ = writeln!(out, "{p}{c_ty} {tmp} = {val};");
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
                        emit_release_box_locals(out, indent, ctx, &ctx.box_owners_all());
                        let _ = writeln!(out, "{p}return {tmp};");
                    }
                }
            }
        }
        Stmt::Expr(e) => {
            let _ = writeln!(out, "{p}{};", emit_expr(e, ctx));
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
            } else {
                c_class_local_type(&mono, checked)
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
                            for field in &class.fields {
                                if type_ref_local_key(&field.ty, &params, &[]) != "String" {
                                    continue;
                                }
                                let field_name = mangle_ident(&field.name.name);
                                let src = format!("(({base_c} *)aura_ex_as_obj())->{field_name}");
                                let copy =
                                    format!("__catch_string_{}_{}", t.span.start, field.span.start);
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
        ctx.define_local(&c.name.name, catch_key);
        for stmt in &c.body.stmts {
            emit_stmt(out, stmt, indent + 3, ctx);
        }
        ctx.pop_scope();
        let _ = writeln!(out, "{p}    }} else {{");
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
    let _ = writeln!(
        out,
        "{p}{{ {scrut_c} {tmp} = {};",
        emit_expr(&m.scrutinee, ctx)
    );
    let _ = writeln!(out, "{p}  switch ({tmp}.tag) {{");

    let ename = mono_base_name(&scrut_key, ctx.checked)
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
                let targs: Vec<Ty> = mono_split(&scrut_key, ctx.checked)
                    .map(|(_, a)| a.to_vec())
                    .or_else(|| {
                        ctx.checked
                            .mono_enums
                            .iter()
                            .find(|(n, a)| mono_key(n, a) == scrut_key)
                            .map(|(_, a)| a.clone())
                    })
                    .unwrap_or_default();
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
        ctx.pop_scope();
        let _ = writeln!(out, "{p}    break;\n{p}  }}");
    }
    let _ = writeln!(out, "{p}  }}\n{p}}}");
}
