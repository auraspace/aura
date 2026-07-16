//! Call-expression emission.

use aura_ast::*;
use aura_sema::Ty;

use crate::ctx::EmitCtx;
use crate::expr::{
    coerce_expr, emit_expr, infer_type_name, mono_base_name, mono_split, resolve_class_of_expr,
    resolve_type_name, type_ref_to_ty,
};
use crate::names::*;

pub(crate) fn emit_call(c: &CallExpr, ctx: &EmitCtx<'_>) -> String {
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
                    return format!("{}({args})", c_ctor_name(&mono));
                }
                return format!("{}({args})", c_fun_name(pkg, name, &targs));
            }
        }

        let obj_ty = resolve_type_name(&fe.object, ctx);
        let obj = emit_expr(&fe.object, ctx);

        // Interface method (C4d: package-prefixed mono)
        if let Some(iface_key) = obj_ty.as_ref().filter(|t| {
            let im = iface_mono_from_key(t, ctx.checked);
            ctx.checked.ast.interfaces.iter().any(|i| {
                iface_mono(i, ctx.checked) == im || i.name.name == **t
            })
        }) {
            let imono = iface_mono_from_key(iface_key, ctx.checked);
            let mut args = vec![format!("&({obj})")];
            if let Some(m) = ctx
                .checked
                .ast
                .interfaces
                .iter()
                .find(|i| iface_mono(i, ctx.checked) == imono || i.name.name == *iface_key)
                .and_then(|i| i.methods.iter().find(|m| m.name.name == fe.field.name))
            {
                for (a, p) in c.args.iter().zip(m.params.iter()) {
                    args.push(coerce_expr(a, &p.ty.name.name, ctx));
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

        // Builtin Array methods
        if base == "Array" || mono.starts_with("Array_") {
            let mut args = vec![format!("&({obj})")];
            for a in &c.args {
                args.push(emit_expr(a, ctx));
            }
            return format!(
                "{}({})",
                c_method_name(&mono, &fe.field.name),
                args.join(", ")
            );
        }

        // C3y: heap classes are already pointers; structs/Array need &.
        let this_arg = if is_heap_class_mono(&mono, ctx.checked) {
            format!("({obj})")
        } else {
            format!("&({obj})")
        };
        let mut args = vec![this_arg];
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
                // C4u: substitute class type params for method parameter expected types.
                let params: Vec<String> = class
                    .type_params
                    .iter()
                    .map(|p| p.name.name.clone())
                    .collect();
                let targs: Vec<Ty> = mono_split(mono_raw, ctx.checked)
                    .map(|(_, a)| a.to_vec())
                    .unwrap_or_default();
                for (a, p) in c.args.iter().zip(m.params.iter()) {
                    let expected = type_ref_local_key(&p.ty, &params, &targs);
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
                let params: Vec<String> =
                    class.type_params.iter().map(|p| p.name.name.clone()).collect();
                let args = c
                    .args
                    .iter()
                    .zip(class.fields.iter())
                    .map(|(a, f)| {
                        let expected = type_ref_local_key(&f.ty, &params, &targs);
                        coerce_expr(a, &expected, ctx)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                return format!("{}({args})", c_ctor_name(&mono));
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
                            let params: Vec<String> = e
                                .type_params
                                .iter()
                                .map(|p| p.name.name.clone())
                                .collect();
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
                return match ta.as_str() {
                    "String" => format!("aura_assert_eq_string({a}, {b})"),
                    "Bool" => format!("aura_assert_eq_bool({a}, {b})"),
                    _ => format!("aura_assert_eq_int({a}, {b})"),
                };
            }
            if id.name == "println" && c.args.len() == 1 {
                return format!("aura_println({})", coerce_expr(&c.args[0], "String", ctx));
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
                let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
                let args = c
                    .args
                    .iter()
                    .zip(f.params.iter())
                    .map(|(a, p)| {
                        let expected = type_ref_local_key(&p.ty, &params, &targs);
                        coerce_expr(a, &expected, ctx)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let fpkg = fun_decl_package(f, ctx.checked);
                return format!("{}({args})", c_fun_name(&fpkg, &id.name, &targs));
            }
            let args = c
                .args
                .iter()
                .map(|a| emit_expr(a, ctx))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({args})", c_fun_name(pkg, &id.name, &[]))
        }
        _ => "/* bad call */(0)".into(),
    }
}

