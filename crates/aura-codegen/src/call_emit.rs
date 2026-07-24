//! Call-expression emission.

use aura_ast::*;
use aura_sema::Ty;

use crate::array_emit::is_array_type_key;
use crate::ctx::EmitCtx;
use crate::expr::{
    coerce_expr, emit_expr, infer_type_name, mono_base_name, mono_split, resolve_class_of_expr,
    resolve_type_name, string_expr_is_owned_temp, type_ref_to_ty,
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
                let subst = aura_sema::type_subst_map(&ctx.type_params, &ctx.type_args);
                let targs: Vec<Ty> = inst
                    .map(|i| {
                        i.type_args
                            .iter()
                            .map(|t| aura_sema::subst_ty(t, &subst))
                            .collect()
                    })
                    .unwrap_or_default();
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
                if let Some(foreign) =
                    ctx.checked.ast.foreign_functions.iter().find(|f| {
                        f.name.name == *name && foreign_decl_package(f, ctx.checked) == pkg
                    })
                {
                    return emit_foreign_call(foreign, c, ctx);
                }
                return format!("{}({args})", c_fun_name(pkg, name, &targs));
            }
        }

        // C13b: prefer resolve_type_name; fall back to infer so call-result receivers
        // (e.g. Array.get → String) dispatch to String/Array methods, not Unknown.
        let obj_ty = resolve_type_name(&fe.object, ctx).or_else(|| {
            let t = crate::expr::infer_type_name(&fe.object, ctx);
            if t == "Unit" || t.is_empty() {
                None
            } else {
                Some(t)
            }
        });
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

        // C13c: builtin Int.toString() → aura_i64_to_string (malloc'd decimal).
        if (mono_raw == "Int"
            || matches!(fe.object.as_ref(), Expr::Int(_))
            || matches!(obj_ty.as_deref(), Some("Int")))
            && fe.field.name == "toString"
        {
            return format!("aura_i64_to_string({obj})");
        }

        // Compiler-backed Hashable implementation for Int.
        if (mono_raw == "Int" || matches!(obj_ty.as_deref(), Some("Int")))
            && fe.field.name == "hash"
        {
            return format!("({obj})");
        }

        // C4v/C4w: builtin String methods.
        if mono_raw == "String"
            || matches!(fe.object.as_ref(), Expr::String(_))
            || matches!(obj_ty.as_deref(), Some("String"))
        {
            if fe.field.name == "hash" {
                return format!("aura_hash_string({obj})");
            }
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
                let start = if !c.args.is_empty() {
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
            // C12i: toInt() — full-string decimal parse → Int? (aura_opt_i64).
            // No auto-trim; optional leading +/-; empty/invalid/overflow → null.
            if fe.field.name == "toInt" {
                let none = null_opt_prim("Opt_Int");
                let call = format!(
                    "({{ const char *__s = ({obj}); \
                     if (__s == NULL) __s = \"\"; \
                     aura_opt_i64 __out = {none}; \
                     size_t __i = 0; \
                     if (__s[0] == '+' || __s[0] == '-') __i = 1; \
                     if (__s[__i] != '\\0') {{ \
                       int __ok = 1; \
                       for (size_t __j = __i; __s[__j]; __j++) {{ \
                         if (__s[__j] < '0' || __s[__j] > '9') {{ __ok = 0; break; }} \
                       }} \
                       if (__ok) {{ \
                         errno = 0; \
                         char *__end = NULL; \
                         long long __v = strtoll(__s, &__end, 10); \
                         if (errno != ERANGE && __end != NULL && *__end == '\\0') {{ \
                           __out = ((aura_opt_i64){{ .has = true, .value = (int64_t)__v }}); \
                         }} \
                       }} \
                     }} \
                     __out; }})"
                );
                if fe.safe {
                    return format!("(({obj}) == NULL ? {none} : {call})");
                }
                return call;
            }
            // C12h: trim / trimStart / trimEnd — ASCII whitespace (' ','\t','\n','\r').
            // Fresh malloc copy of the kept span (same ownership MVP as substring).
            if matches!(fe.field.name.as_str(), "trim" | "trimStart" | "trimEnd") {
                let mname = fe.field.name.as_str();
                let (do_start, do_end) = match mname {
                    "trim" => (true, true),
                    "trimStart" => (true, false),
                    "trimEnd" => (false, true),
                    _ => unreachable!(),
                };
                let start_loop = if do_start {
                    "while (__i < __n && (__s[__i] == ' ' || __s[__i] == '\\t' || __s[__i] == '\\n' || __s[__i] == '\\r')) __i++;"
                } else {
                    ""
                };
                let end_loop = if do_end {
                    "while (__j > __i && (__s[__j - 1] == ' ' || __s[__j - 1] == '\\t' || __s[__j - 1] == '\\n' || __s[__j - 1] == '\\r')) __j--;"
                } else {
                    ""
                };
                let call = format!(
                    "({{ const char *__s = ({obj}); \
                     if (__s == NULL) __s = \"\"; \
                     size_t __n = strlen(__s); \
                     size_t __i = 0; \
                     size_t __j = __n; \
                     {start_loop} \
                     {end_loop} \
                     size_t __len = __j - __i; \
                     char *__r = (char *)malloc(__len + 1); \
                     if (__r == NULL) aura_throw_string(\"String {mname} out of memory\"); \
                     if (__len > 0) memcpy(__r, __s + __i, __len); \
                     __r[__len] = '\\0'; \
                     (const char *)__r; }})"
                );
                if fe.safe {
                    return format!("(({obj}) == NULL ? NULL : {call})");
                }
                return call;
            }
            // C13m: toLower / toUpper — ASCII A–Z/a–z only; other bytes (incl. UTF-8
            // multi-byte sequences) copied unchanged. Fresh malloc copy.
            if matches!(fe.field.name.as_str(), "toLower" | "toUpper") {
                let mname = fe.field.name.as_str();
                let map_byte = if mname == "toLower" {
                    "if (__c >= 'A' && __c <= 'Z') __c = (char)(__c + ('a' - 'A'));"
                } else {
                    "if (__c >= 'a' && __c <= 'z') __c = (char)(__c - ('a' - 'A'));"
                };
                let call = format!(
                    "({{ const char *__s = ({obj}); \
                     if (__s == NULL) __s = \"\"; \
                     size_t __n = strlen(__s); \
                     char *__r = (char *)malloc(__n + 1); \
                     if (__r == NULL) aura_throw_string(\"String {mname} out of memory\"); \
                     for (size_t __i = 0; __i < __n; __i++) {{ \
                       char __c = __s[__i]; \
                       {map_byte} \
                       __r[__i] = __c; \
                     }} \
                     __r[__n] = '\\0'; \
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
            let mut owned_string_temps: Vec<(usize, String)> = Vec::new();
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
                    if elem_key == "String" && string_expr_is_owned_temp(a, ctx) {
                        let temp = format!("__aura_array_string_arg_{}", a.span().start);
                        owned_string_temps.push((param_keys.len(), temp.clone()));
                        param_keys.push(elem_key.to_string());
                        args.push(temp);
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
            if (fe.field.name == "push" || fe.field.name == "set")
                && elem_key == "String"
                && !owned_string_temps.is_empty()
            {
                let mut prefix = String::new();
                let mut suffix = String::new();
                for (arg_index, temp) in owned_string_temps {
                    let a = &c.args[arg_index];
                    let value = emit_expr(a, ctx);
                    prefix.push_str(&format!("const char *{temp} = ({value}); "));
                    suffix.push_str(&format!("free((void *){temp}); "));
                }
                let wrapped = format!("({{ {prefix}{call}; {suffix}}})");
                return wrapped;
            }
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
                    let f = if ctx.is_box_local(&id.name) {
                        format!(
                            "(*({} *)aura_box_ptr_get({}))",
                            c_fun_typedef(key),
                            mangle_ident(&id.name)
                        )
                    } else {
                        mangle_ident(&id.name)
                    };
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
                    let subst = aura_sema::type_subst_map(&ctx.type_params, &ctx.type_args);
                    inst.type_args
                        .iter()
                        .map(|t| aura_sema::subst_ty(t, &subst))
                        .collect()
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
                    let subst = aura_sema::type_subst_map(&ctx.type_params, &ctx.type_args);
                    inst.type_args
                        .iter()
                        .map(|t| aura_sema::subst_ty(t, &subst))
                        .collect()
                } else {
                    c.type_args
                        .iter()
                        .filter_map(|t| type_ref_to_ty(t, ctx))
                        .collect()
                };
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
            if let Some(foreign) = ctx.checked.ast.foreign_functions.iter().find(|f| {
                f.name.name == id.name
                    && (pkg.is_empty() || foreign_decl_package(f, ctx.checked) == pkg)
            }) {
                return emit_foreign_call(foreign, c, ctx);
            }
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

/// F2: foreign calls use the declared C symbol verbatim. String arguments are
/// borrowed `const char *` handles; a foreign String result is also borrowed,
/// so it is deliberately not added to codegen ownership tracking.
fn emit_foreign_call(foreign: &ForeignDecl, call: &CallExpr, ctx: &mut EmitCtx<'_>) -> String {
    let pinned: Vec<usize> = foreign
        .params
        .iter()
        .enumerate()
        .filter(|(_, param)| param.ty.name.name == "ForeignHandle")
        .map(|(index, _)| index)
        .collect();
    if !pinned.is_empty() {
        // FFI-001/002: ForeignHandle parameters are borrowed for exactly the
        // C call.  A TASK pin is the ABI's checked async-capable ownership
        // class even though this call itself is synchronous; it prevents
        // release/destruction during the call and remains compatible with an
        // async caller.  Aura does not silently pin Task, TaskHandle,
        // Channel, or any unproven value across an await.
        let ret = crate::names::c_type_from_opt(&foreign.return_type, ctx.checked, &[], &[]);
        let async_frame = ctx.async_frame.clone();
        let mut out = String::from("({ ");
        for (slot, index) in pinned.iter().enumerate() {
            let arg = call
                .args
                .get(*index)
                .map(|arg| emit_expr(arg, ctx))
                .unwrap_or_else(|| "NULL".into());
            if let Some(frame) = &async_frame {
                let _ = std::fmt::Write::write_fmt(
                    &mut out,
                    format_args!(
                        "AuraFfiOpaqueHandle *__aura_ffi_handle_{slot} = (AuraFfiOpaqueHandle *)({arg}); if (__aura_ffi_handle_{slot} != NULL && aura_task_frame_pin_foreign_handle({frame}, __aura_ffi_handle_{slot}, AURA_FFI_BOUNDARY_TASK) != AURA_FFI_OK) abort(); "
                    ),
                );
            } else {
                let _ = std::fmt::Write::write_fmt(
                    &mut out,
                    format_args!(
                        "AuraFfiOpaqueHandle *__aura_ffi_handle_{slot} = (AuraFfiOpaqueHandle *)({arg}); AuraFfiHandlePin __aura_ffi_pin_{slot} = {{0}}; if (__aura_ffi_handle_{slot} != NULL && aura_ffi_handle_pin_for_boundary(__aura_ffi_handle_{slot}, AURA_FFI_BOUNDARY_TASK, &__aura_ffi_pin_{slot}) != AURA_FFI_OK) abort(); "
                    ),
                );
            }
        }
        let call_args = call
            .args
            .iter()
            .zip(foreign.params.iter())
            .enumerate()
            .map(|(index, (arg, param))| {
                if let Some(slot) = pinned.iter().position(|p| *p == index) {
                    format!("__aura_ffi_handle_{slot}")
                } else {
                    let expected = type_ref_local_key(&param.ty, &[], &[]);
                    coerce_expr(arg, &expected, ctx)
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let pinned_call = format!("{}({call_args})", foreign.name.name);
        if ret == "void" {
            let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{pinned_call}; "));
        } else {
            let _ = std::fmt::Write::write_fmt(
                &mut out,
                format_args!("{ret} __aura_ffi_result = ({pinned_call}); "),
            );
        }
        if async_frame.is_none() {
            for slot in pinned.iter().enumerate().map(|(slot, _)| slot) {
                let _ = std::fmt::Write::write_fmt(
                    &mut out,
                    format_args!("if (__aura_ffi_pin_{slot}.handle != NULL) (void)aura_ffi_handle_unpin(&__aura_ffi_pin_{slot}); "),
                );
            }
        }
        if ret != "void" {
            out.push_str("__aura_ffi_result; ");
        }
        out.push_str("})");
        return out;
    }
    let args = call
        .args
        .iter()
        .zip(foreign.params.iter())
        .map(|(arg, param)| {
            let expected = type_ref_local_key(&param.ty, &[], &[]);
            coerce_expr(arg, &expected, ctx)
        })
        .collect::<Vec<_>>()
        .join(", ");
    let call = format!("{}({args})", foreign.name.name);
    if foreign.failure.as_deref() == Some("status") {
        // F2: an explicitly declared status-returning primitive is normalized
        // to the bounded Aura outcome code.  It remains an Int, not an
        // implicit exception or callback result.
        format!("((int64_t)aura_ffi_map_error((int32_t)({call})))")
    } else {
        call
    }
}

fn foreign_decl_package(foreign: &ForeignDecl, checked: &aura_sema::CheckedFile) -> String {
    if foreign.origin_package.is_empty() {
        checked.package.clone()
    } else {
        foreign.origin_package.clone()
    }
}
