//! Call-expression emission.

use aura_ast::*;
use aura_sema::Ty;

use crate::array_emit::is_array_type_key;
use crate::ctx::EmitCtx;
use crate::expr::{
    coerce_expr, emit_expr, infer_type_name, mono_base_name, mono_split, resolve_class_of_expr,
    resolve_type_name, type_ref_to_ty,
};
use crate::names::*;

/// C6b: after a call that moved Array owner args into params, zero sources.
fn wrap_array_arg_moves(
    call: String,
    move_srcs: &[String],
    ret_c: &str,
    ctx: &mut EmitCtx<'_>,
) -> String {
    if move_srcs.is_empty() {
        return call;
    }
    let mut zeros = String::new();
    for src in move_srcs {
        let s = mangle_ident(src);
        zeros.push_str(&format!("{s}.data = NULL; {s}.len = 0; {s}.cap = 0; "));
        ctx.unmark_array_owner(src);
    }
    if ret_c == "void" {
        format!("({{ {call}; {zeros}}})")
    } else {
        format!("({{ {ret_c} __am = ({call}); {zeros}__am; }})")
    }
}

/// After a call that moved Fun owner args into params, zero source envs.
fn wrap_fun_arg_moves(
    call: String,
    move_srcs: &[String],
    ret_c: &str,
    ctx: &mut EmitCtx<'_>,
) -> String {
    if move_srcs.is_empty() {
        return call;
    }
    let mut zeros = String::new();
    for src in move_srcs {
        let s = mangle_ident(src);
        zeros.push_str(&format!("{s}.env = NULL; "));
        ctx.unmark_fun_owner(src);
    }
    if ret_c == "void" {
        format!("({{ {call}; {zeros}}})")
    } else {
        format!("({{ {ret_c} __fm = ({call}); {zeros}__fm; }})")
    }
}

/// Collect Array owner idents that should move into matching Array params.
fn array_move_srcs_from_args(
    args: &[Expr],
    param_keys: &[String],
    ctx: &EmitCtx<'_>,
) -> Vec<String> {
    let mut move_srcs = Vec::new();
    for (a, expected) in args.iter().zip(param_keys.iter()) {
        if !is_array_type_key(expected) {
            continue;
        }
        if let Expr::Ident(id) = a {
            if ctx.is_array_owner(&id.name) && !move_srcs.contains(&id.name) {
                move_srcs.push(id.name.clone());
            }
        }
    }
    move_srcs
}

fn fun_move_srcs_from_args(args: &[Expr], param_keys: &[String], ctx: &EmitCtx<'_>) -> Vec<String> {
    let mut move_srcs = Vec::new();
    for (a, expected) in args.iter().zip(param_keys.iter()) {
        if !is_fun_type_key(expected) {
            continue;
        }
        if let Expr::Ident(id) = a {
            if ctx.is_fun_owner(&id.name) && !move_srcs.contains(&id.name) {
                move_srcs.push(id.name.clone());
            }
        }
    }
    move_srcs
}

/// Move Array + Fun owner args into params (zero sources after call).
fn wrap_owner_arg_moves(
    call: String,
    args: &[Expr],
    param_keys: &[String],
    ret_c: &str,
    ctx: &mut EmitCtx<'_>,
) -> String {
    let array_srcs = array_move_srcs_from_args(args, param_keys, ctx);
    let fun_srcs = fun_move_srcs_from_args(args, param_keys, ctx);
    let call = wrap_array_arg_moves(call, &array_srcs, ret_c, ctx);
    wrap_fun_arg_moves(call, &fun_srcs, ret_c, ctx)
}

pub(crate) fn emit_call(c: &CallExpr, ctx: &mut EmitCtx<'_>) -> String {
    // Method call: obj.method(args)
    if let Expr::Field(fe) = c.callee.as_ref() {
        // C3n: package alias qualified free function `Math.square(...)`.
        if let Expr::Ident(id) = fe.object.as_ref() {
            let is_alias = ctx.checked.ast.imports.iter().any(|imp| {
                imp.alias
                    .as_ref()
                    .map(|a| a.name == id.name)
                    .unwrap_or(false)
            });
            if is_alias {
                let name = &fe.field.name;
                let inst = ctx.checked.call_instantiations.get(&c.span.start);
                let targs: Vec<Ty> = inst.map(|i| i.type_args.clone()).unwrap_or_default();
                let pkg = inst.map(|i| i.package.as_str()).unwrap_or("");
                let args = c
                    .args
                    .iter()
                    .map(|a| emit_expr(a, ctx))
                    .collect::<Vec<_>>()
                    .join(", ");
                // C3u: `Alias.Type(...)` constructor vs `Alias.fun(...)`.
                if inst.map(|i| i.is_constructor).unwrap_or(false) {
                    let mono = type_mono(pkg, name, &targs);
                    // C6i: move Array owner args into ctor fields when class is known.
                    if let Some(class) = ctx.checked.ast.classes.iter().find(|x| {
                        x.name.name == *name
                            && (pkg.is_empty() || class_decl_package(x, ctx.checked) == pkg)
                    }) {
                        let tparams: Vec<String> = class
                            .type_params
                            .iter()
                            .map(|p| p.name.name.clone())
                            .collect();
                        let mut field_keys = Vec::new();
                        let args = c
                            .args
                            .iter()
                            .zip(class.fields.iter())
                            .map(|(a, f)| {
                                let expected = type_ref_local_key(&f.ty, &tparams, &targs);
                                field_keys.push(expected.clone());
                                coerce_expr(a, &expected, ctx)
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        let move_srcs = array_move_srcs_from_args(&c.args, &field_keys, ctx);
                        let ret_c = if is_heap_class_decl(class) {
                            format!("{} *", c_class_type(&mono))
                        } else {
                            c_class_type(&mono)
                        };
                        let call = format!("{}({args})", c_ctor_name(&mono));
                        return wrap_array_arg_moves(call, &move_srcs, &ret_c, ctx);
                    }
                    return format!("{}({args})", c_ctor_name(&mono));
                }
                return format!("{}({args})", c_fun_name(pkg, name, &targs));
            }
        }

        let obj_ty = resolve_type_name(&fe.object, ctx);
        let obj = emit_expr(&fe.object, ctx);

        // Interface method (C4d package mono; C8c mono args e.g. Boxable_Int)
        if let Some(iface_key) = obj_ty
            .as_ref()
            .filter(|t| is_iface_type_key(t, ctx.checked))
        {
            let imono = resolve_iface_mono_key(iface_key, ctx.checked);
            let mut args = vec![format!("&({obj})")];
            let (iface_decl, iargs) = resolve_iface_decl_and_args(iface_key, ctx.checked);
            if let Some(i) = iface_decl {
                let tparams: Vec<String> =
                    i.type_params.iter().map(|p| p.name.name.clone()).collect();
                if let Some(m) = i.methods.iter().find(|m| m.name.name == fe.field.name) {
                    for (a, p) in c.args.iter().zip(m.params.iter()) {
                        let expected = type_ref_local_key(&p.ty, &tparams, &iargs);
                        args.push(coerce_expr(a, &expected, ctx));
                    }
                } else {
                    for a in &c.args {
                        args.push(emit_expr(a, ctx));
                    }
                }
            } else {
                for a in &c.args {
                    args.push(emit_expr(a, ctx));
                }
            }
            return format!(
                "{}({})",
                c_iface_method_name(&imono, &fe.field.name),
                args.join(", ")
            );
        }

        // Class method (obj_ty is mono key e.g. Box_String, demo_t_User, or User)
        // C4k: also resolve field chains (this.item) via resolve_type_name.
        let mono_from_ty = resolve_type_name(&fe.object, ctx);
        let mono_from_cls = resolve_class_of_expr(&fe.object, ctx).map(|s| s.to_string());
        let mono_owned = obj_ty
            .clone()
            .or(mono_from_ty)
            .or(mono_from_cls)
            .unwrap_or_else(|| "Unknown".into());
        let mono_raw = mono_owned.as_str();
        let base = mono_base_name(mono_raw, ctx.checked).unwrap_or(mono_raw);
        let mono = crate::expr::full_type_mono(mono_raw, ctx.checked);

        // C4v/C4w: builtin String methods.
        if mono_raw == "String"
            || matches!(fe.object.as_ref(), Expr::String(_))
            || matches!(obj_ty.as_deref(), Some("String"))
        {
            if fe.field.name == "isEmpty" {
                // UTF-8 byte length via strlen; null-safe → true when null (empty-ish MVP).
                let call = format!("(({obj}) == NULL || ({obj})[0] == '\\0')");
                if fe.safe {
                    return format!("(({obj}) == NULL ? true : {call})");
                }
                return call;
            }
            if fe.field.name == "charAt" {
                // C4w: byte at index as int64_t; OOB / null throws.
                let idx = if c.args.len() == 1 {
                    emit_expr(&c.args[0], ctx)
                } else {
                    "0".into()
                };
                let call = format!(
                    "({{ const char *__s = ({obj}); int64_t __i = ({idx}); \
                     if (__s == NULL) aura_throw_string(\"String charAt on null\"); \
                     size_t __n = strlen(__s); \
                     if (__i < 0 || (size_t)__i >= __n) aura_throw_string(\"String charAt out of bounds\"); \
                     (int64_t)(unsigned char)__s[__i]; }})"
                );
                if fe.safe {
                    return format!("(({obj}) == NULL ? INT64_C(0) : {call})");
                }
                return call;
            }
            // C5h: startsWith — prefix match via strncmp.
            if fe.field.name == "startsWith" {
                let pref = if c.args.len() == 1 {
                    emit_expr(&c.args[0], ctx)
                } else {
                    "\"\"".into()
                };
                let call = format!(
                    "({{ const char *__s = ({obj}); const char *__p = ({pref}); \
                     if (__s == NULL) __s = \"\"; if (__p == NULL) __p = \"\"; \
                     size_t __pl = strlen(__p); \
                     (strncmp(__s, __p, __pl) == 0); }})"
                );
                if fe.safe {
                    return format!("(({obj}) == NULL ? false : {call})");
                }
                return call;
            }
            // C5i: contains — strstr.
            if fe.field.name == "contains" {
                let sub = if c.args.len() == 1 {
                    emit_expr(&c.args[0], ctx)
                } else {
                    "\"\"".into()
                };
                let call = format!(
                    "({{ const char *__s = ({obj}); const char *__n = ({sub}); \
                     if (__s == NULL) __s = \"\"; if (__n == NULL) __n = \"\"; \
                     (strstr(__s, __n) != NULL); }})"
                );
                if fe.safe {
                    return format!("(({obj}) == NULL ? false : {call})");
                }
                return call;
            }
            // C12f: indexOf — byte index of first strstr match; -1 if missing; empty sub → 0.
            if fe.field.name == "indexOf" {
                let sub = if c.args.len() == 1 {
                    emit_expr(&c.args[0], ctx)
                } else {
                    "\"\"".into()
                };
                let call = format!(
                    "({{ const char *__s = ({obj}); const char *__n = ({sub}); \
                     if (__s == NULL) __s = \"\"; if (__n == NULL) __n = \"\"; \
                     const char *__p = strstr(__s, __n); \
                     (__p == NULL ? (int64_t)-1 : (int64_t)(__p - __s)); }})"
                );
                if fe.safe {
                    return format!("(({obj}) == NULL ? INT64_C(0) : {call})");
                }
                return call;
            }
            // C12g: split(sep) → Array<String>. Empty sep throws; consecutive/trailing seps
            // yield empty segments; each segment is a freshly malloc'd copy.
            if fe.field.name == "split" {
                let sep = if c.args.len() == 1 {
                    emit_expr(&c.args[0], ctx)
                } else {
                    "\"\"".into()
                };
                let arr_ty = c_class_type("Array_String");
                let ctor = c_ctor_name("Array_String");
                let call = format!(
                    "({{ const char *__s = ({obj}); const char *__sep = ({sep}); \
                     if (__s == NULL) __s = \"\"; if (__sep == NULL) __sep = \"\"; \
                     size_t __seplen = strlen(__sep); \
                     if (__seplen == 0) aura_throw_string(\"String split empty separator\"); \
                     size_t __n = 1; \
                     const char *__scan = __s; \
                     while ((__scan = strstr(__scan, __sep)) != NULL) {{ \
                       __n++; \
                       __scan += __seplen; \
                     }} \
                     {arr_ty} __a = {ctor}((int64_t)__n); \
                     const char *__start = __s; \
                     int64_t __i = 0; \
                     for (;;) {{ \
                       const char *__found = strstr(__start, __sep); \
                       size_t __len = __found ? (size_t)(__found - __start) : strlen(__start); \
                       char *__copy = (char *)malloc(__len + 1); \
                       if (__copy == NULL) aura_throw_string(\"String split out of memory\"); \
                       if (__len > 0) memcpy(__copy, __start, __len); \
                       __copy[__len] = '\\0'; \
                       __a.data[__i++] = (const char *)__copy; \
                       if (__found == NULL) break; \
                       __start = __found + __seplen; \
                     }} \
                     __a; }})"
                );
                if fe.safe {
                    return format!("(({obj}) == NULL ? {ctor}(0) : {call})");
                }
                return call;
            }
            // C5j: endsWith — compare suffix bytes.
            if fe.field.name == "endsWith" {
                let suf = if c.args.len() == 1 {
                    emit_expr(&c.args[0], ctx)
                } else {
                    "\"\"".into()
                };
                let call = format!(
                    "({{ const char *__s = ({obj}); const char *__u = ({suf}); \
                     if (__s == NULL) __s = \"\"; if (__u == NULL) __u = \"\"; \
                     size_t __sl = strlen(__s), __ul = strlen(__u); \
                     (__ul <= __sl && strcmp(__s + (__sl - __ul), __u) == 0); }})"
                );
                if fe.safe {
                    return format!("(({obj}) == NULL ? false : {call})");
                }
                return call;
            }
            // C11d: substring(start, end) exclusive end; malloc copy; OOB throws.
            if fe.field.name == "substring" {
                let start = if c.args.len() >= 1 {
                    emit_expr(&c.args[0], ctx)
                } else {
                    "0".into()
                };
                let end = if c.args.len() >= 2 {
                    emit_expr(&c.args[1], ctx)
                } else {
                    "0".into()
                };
                let call = format!(
                    "({{ const char *__s = ({obj}); int64_t __a = ({start}); int64_t __b = ({end}); \
                     if (__s == NULL) aura_throw_string(\"String substring on null\"); \
                     size_t __n = strlen(__s); \
                     if (__a < 0 || __b < __a || (size_t)__b > __n) aura_throw_string(\"String substring out of bounds\"); \
                     size_t __len = (size_t)(__b - __a); \
                     char *__r = (char *)malloc(__len + 1); \
                     if (__r == NULL) aura_throw_string(\"String substring out of memory\"); \
                     if (__len > 0) memcpy(__r, __s + (size_t)__a, __len); \
                     __r[__len] = '\\0'; \
                     (const char *)__r; }})"
                );
                if fe.safe {
                    return format!("(({obj}) == NULL ? NULL : {call})");
                }
                return call;
            }
        }

        // Builtin Array methods
        if base == "Array" || mono.starts_with("Array_") {
            let mut args = vec![format!("&({obj})")];
            // C8e: push/set of Array-valued elems move from owner args (nested Array).
            let elem_key = mono.strip_prefix("Array_").unwrap_or("");
            let mut param_keys = Vec::new();
            for a in &c.args {
                if fe.field.name == "push" || fe.field.name == "set" {
                    // set(i, v): first arg Int, second elem; push(v): one elem arg
                    if fe.field.name == "set" && param_keys.is_empty() {
                        param_keys.push("Int".into());
                        args.push(emit_expr(a, ctx));
                        continue;
                    }
                    if is_array_type_key(elem_key) {
                        param_keys.push(elem_key.to_string());
                        args.push(emit_expr(a, ctx));
                        continue;
                    }
                }
                args.push(emit_expr(a, ctx));
                param_keys.push(String::new());
            }
            let call = format!(
                "{}({})",
                c_method_name(&mono, &fe.field.name),
                args.join(", ")
            );
            if (fe.field.name == "push" || fe.field.name == "set") && is_array_type_key(elem_key) {
                let move_srcs = array_move_srcs_from_args(&c.args, &param_keys, ctx);
                return wrap_array_arg_moves(call, &move_srcs, "void", ctx);
            }
            return call;
        }

        // C3y: heap classes are already pointers; structs/Array need &.
        // `this` emits as `(*this)` for field `.` access — method recv must stay the pointer.
        let this_arg = if is_heap_class_mono(&mono, ctx.checked) {
            if matches!(fe.object.as_ref(), Expr::This(_)) {
                "this".into()
            } else {
                format!("({obj})")
            }
        } else {
            format!("&({obj})")
        };
        let mut args = vec![this_arg];
        if let Some(class) = ctx.checked.ast.classes.iter().find(|c| c.name.name == base) {
            if let Some(m) = class.methods.iter().find(|m| m.name.name == fe.field.name) {
                // C4u: substitute class type params for method parameter expected types.
                let params: Vec<String> = class
                    .type_params
                    .iter()
                    .map(|p| p.name.name.clone())
                    .collect();
                let targs: Vec<Ty> = mono_split(mono_raw, ctx.checked)
                    .map(|(_, a)| a.to_vec())
                    .unwrap_or_default();
                let mut param_keys = Vec::new();
                for (a, p) in c.args.iter().zip(m.params.iter()) {
                    let expected = type_ref_local_key(&p.ty, &params, &targs);
                    param_keys.push(expected.clone());
                    args.push(coerce_expr(a, &expected, ctx));
                }
                let ret_c = c_type_from_opt(&m.return_type, ctx.checked, &params, &targs);
                let call = format!(
                    "{}({})",
                    c_method_name(&mono, &fe.field.name),
                    args.join(", ")
                );
                let call = wrap_owner_arg_moves(call, &c.args, &param_keys, &ret_c, ctx);
                // C4s: `?.` short-circuit to NULL when receiver is null (pointer-like results).
                if fe.safe {
                    return format!("(({obj}) == NULL ? NULL : {call})");
                }
                return call;
            } else {
                for a in &c.args {
                    args.push(emit_expr(a, ctx));
                }
            }
        } else {
            for a in &c.args {
                args.push(emit_expr(a, ctx));
            }
        }
        let call = format!(
            "{}({})",
            c_method_name(&mono, &fe.field.name),
            args.join(", ")
        );
        // C4s: `?.` short-circuit to NULL when receiver is null (pointer-like results).
        if fe.safe {
            return format!("(({obj}) == NULL ? NULL : {call})");
        }
        return call;
    }

    match c.callee.as_ref() {
        Expr::Ident(id) => {
            // C10e/h: call through a local function-value (fat pointer).
            if let Some(key) = ctx.lookup_local(&id.name) {
                if is_fun_type_key(key) {
                    let f = mangle_ident(&id.name);
                    let mut parts = vec![format!("{f}.env")];
                    for a in &c.args {
                        parts.push(emit_expr(a, ctx));
                    }
                    return format!("{f}.fn({})", parts.join(", "));
                }
            }

            // Prefer type args resolved by sema (explicit or inferred)
            let inst = ctx.checked.call_instantiations.get(&c.span.start);

            // Builtin Array constructor
            if id.name == "Array" {
                let targs: Vec<Ty> = if let Some(inst) = inst {
                    inst.type_args.clone()
                } else {
                    c.type_args
                        .iter()
                        .filter_map(|t| type_ref_to_ty(t, ctx))
                        .collect()
                };
                let mono = mono_key("Array", &targs);
                let args = c
                    .args
                    .iter()
                    .map(|a| emit_expr(a, ctx))
                    .collect::<Vec<_>>()
                    .join(", ");
                return format!("{}({args})", c_ctor_name(&mono));
            }

            // Constructor (optional type args)
            if let Some(class) = ctx
                .checked
                .ast
                .classes
                .iter()
                .find(|x| x.name.name == id.name)
            {
                let targs: Vec<Ty> = if let Some(inst) = inst {
                    inst.type_args.clone()
                } else {
                    c.type_args
                        .iter()
                        .filter_map(|t| type_ref_to_ty(t, ctx))
                        .collect()
                };
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
                let mono = type_mono(pkg, &id.name, &targs);
                let params: Vec<String> = class
                    .type_params
                    .iter()
                    .map(|p| p.name.name.clone())
                    .collect();
                // C6i: Array primary-ctor fields own the buffer — move from owner idents.
                let mut field_keys = Vec::new();
                let args = c
                    .args
                    .iter()
                    .zip(class.fields.iter())
                    .map(|(a, f)| {
                        let expected = type_ref_local_key(&f.ty, &params, &targs);
                        field_keys.push(expected.clone());
                        coerce_expr(a, &expected, ctx)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let move_srcs = array_move_srcs_from_args(&c.args, &field_keys, ctx);
                let ret_c = if is_heap_class_decl(class) {
                    format!("{} *", c_class_type(&mono))
                } else {
                    c_class_type(&mono)
                };
                let call = format!("{}({args})", c_ctor_name(&mono));
                return wrap_array_arg_moves(call, &move_srcs, &ret_c, ctx);
            }
            // Enum variant constructor: Ok(...), Err(...), Red()
            if let Some(inst) = inst {
                if let Some(vname) = &inst.variant {
                    let mono = type_mono(&inst.package, &inst.name, &inst.type_args);
                    if let Some(e) = ctx
                        .checked
                        .ast
                        .enums
                        .iter()
                        .find(|e| e.name.name == inst.name)
                    {
                        if let Some(v) = e.variants.iter().find(|v| v.name.name == *vname) {
                            let params: Vec<String> =
                                e.type_params.iter().map(|p| p.name.name.clone()).collect();
                            let args = c
                                .args
                                .iter()
                                .zip(v.fields.iter())
                                .map(|(a, f)| {
                                    let expected =
                                        type_ref_local_key(&f.ty, &params, &inst.type_args);
                                    coerce_expr(a, &expected, ctx)
                                })
                                .collect::<Vec<_>>()
                                .join(", ");
                            return format!("{}({args})", c_variant_ctor_name(&mono, vname));
                        }
                    }
                    return format!("{}()", c_variant_ctor_name(&mono, vname));
                }
            }
            // Builtins: assert / assert_eq
            if id.name == "assert" && c.args.len() == 1 {
                return format!("aura_assert({})", emit_expr(&c.args[0], ctx));
            }
            if id.name == "assert_eq" && c.args.len() == 2 {
                let ta = infer_type_name(&c.args[0], ctx);
                let a = emit_expr(&c.args[0], ctx);
                let b = emit_expr(&c.args[1], ctx);
                // C7a: after null-narrow, Opt_* still stores a tagged struct — compare values.
                let a_v = if is_opt_prim_key(&ta) {
                    format!("({a}).value")
                } else {
                    a
                };
                let tb = infer_type_name(&c.args[1], ctx);
                let b_v = if is_opt_prim_key(&tb) {
                    format!("({b}).value")
                } else {
                    b
                };
                let kind = if is_opt_prim_key(&ta) {
                    ta.strip_prefix("Opt_").unwrap_or(ta.as_str())
                } else {
                    ta.as_str()
                };
                return match kind {
                    "String" => format!("aura_assert_eq_string({a_v}, {b_v})"),
                    "Bool" => format!("aura_assert_eq_bool({a_v}, {b_v})"),
                    _ => format!("aura_assert_eq_int({a_v}, {b_v})"),
                };
            }
            if id.name == "print" && c.args.len() == 1 {
                return format!("aura_print({})", coerce_expr(&c.args[0], "String", ctx));
            }
            if id.name == "println" && c.args.len() == 1 {
                return format!("aura_println({})", coerce_expr(&c.args[0], "String", ctx));
            }
            if id.name == "eprint" && c.args.len() == 1 {
                return format!("aura_eprint({})", coerce_expr(&c.args[0], "String", ctx));
            }
            if id.name == "eprintln" && c.args.len() == 1 {
                return format!("aura_eprintln({})", coerce_expr(&c.args[0], "String", ctx));
            }
            // C5m: builtin STW GC collect.
            if id.name == "gc_collect" && c.args.is_empty() {
                return "aura_gc_collect()".into();
            }
            // Free function
            let targs: Vec<Ty> = if let Some(inst) = inst {
                inst.type_args.clone()
            } else {
                c.type_args
                    .iter()
                    .filter_map(|t| type_ref_to_ty(t, ctx))
                    .collect()
            };
            let pkg = inst.map(|i| i.package.as_str()).unwrap_or("");
            if let Some(f) = ctx.checked.ast.functions.iter().find(|f| {
                f.name.name == id.name
                    && (pkg.is_empty() || fun_decl_package(f, ctx.checked) == pkg)
            }) {
                let params: Vec<String> =
                    f.type_params.iter().map(|p| p.name.name.clone()).collect();
                let mut param_keys = Vec::new();
                let args = c
                    .args
                    .iter()
                    .zip(f.params.iter())
                    .map(|(a, p)| {
                        let expected = type_ref_local_key(&p.ty, &params, &targs);
                        param_keys.push(expected.clone());
                        coerce_expr(a, &expected, ctx)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let ret_c = c_type_from_opt(&f.return_type, ctx.checked, &params, &targs);
                let fpkg = fun_decl_package(f, ctx.checked);
                let call = format!("{}({args})", c_fun_name(&fpkg, &id.name, &targs));
                return wrap_owner_arg_moves(call, &c.args, &param_keys, &ret_c, ctx);
            }
            let args = c
                .args
                .iter()
                .map(|a| emit_expr(a, ctx))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({args})", c_fun_name(pkg, &id.name, &[]))
        }
        // C10e/h: call a lambda / fun value (fat pointer: .fn(.env, args…)).
        other => {
            let callee = emit_expr(other, ctx);
            let mut parts = vec![format!("({callee}).env")];
            for a in &c.args {
                parts.push(emit_expr(a, ctx));
            }
            let args = parts.join(", ");
            format!("({callee}).fn({args})")
        }
    }
}
