//! Call-expression emission.

use aura_ast::*;
use aura_sema::Ty;

use crate::ctx::EmitCtx;
use crate::expr::{
    coerce_expr, emit_expr, infer_type_name, mono_base_name, resolve_class_of_expr,
    resolve_type_name, type_ref_to_ty,
};
use crate::names::*;

pub(crate) fn emit_call(c: &CallExpr, ctx: &EmitCtx<'_>) -> String {
    // Method call: obj.method(args)
    if let Expr::Field(fe) = c.callee.as_ref() {
        let obj_ty = resolve_type_name(&fe.object, ctx);
        let obj = emit_expr(&fe.object, ctx);

        // Interface method
        if let Some(iface) = obj_ty.as_ref().filter(|t| {
            ctx.checked
                .ast
                .interfaces
                .iter()
                .any(|i| i.name.name == **t)
        }) {
            let mut args = vec![format!("&({obj})")];
            // param types from interface method
            if let Some(m) = ctx
                .checked
                .ast
                .interfaces
                .iter()
                .find(|i| i.name.name == *iface)
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
                c_iface_method_name(iface, &fe.field.name),
                args.join(", ")
            );
        }

        // Class method (obj_ty is mono key e.g. Box_String or User)
        let mono = obj_ty
            .as_deref()
            .or_else(|| resolve_class_of_expr(&fe.object, ctx))
            .unwrap_or("Unknown");
        let base = mono_base_name(mono, ctx.checked).unwrap_or(mono);
        let mut args = vec![format!("&({obj})")];
        if let Some(m) = ctx
            .checked
            .ast
            .classes
            .iter()
            .find(|c| c.name.name == base)
            .and_then(|c| c.methods.iter().find(|m| m.name.name == fe.field.name))
        {
            for (a, p) in c.args.iter().zip(m.params.iter()) {
                let expected = type_ref_local_key(&p.ty, &[], &[]);
                args.push(coerce_expr(a, &expected, ctx));
            }
        } else {
            for a in &c.args {
                args.push(emit_expr(a, ctx));
            }
        }
        return format!(
            "{}({})",
            c_method_name(mono, &fe.field.name),
            args.join(", ")
        );
    }

    match c.callee.as_ref() {
        Expr::Ident(id) => {
            // Prefer type args resolved by sema (explicit or inferred)
            let inst = ctx.checked.call_instantiations.get(&c.span.start);

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
                let mono = mono_key(&id.name, &targs);
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
                    let mono = mono_key(&inst.name, &inst.type_args);
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
            if let Some(f) = ctx
                .checked
                .ast
                .functions
                .iter()
                .find(|f| f.name.name == id.name)
            {
                let targs: Vec<Ty> = if let Some(inst) = inst {
                    inst.type_args.clone()
                } else {
                    c.type_args
                        .iter()
                        .filter_map(|t| type_ref_to_ty(t, ctx))
                        .collect()
                };
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
                return format!("{}({args})", c_fun_name(&id.name, &targs));
            }
            let args = c
                .args
                .iter()
                .map(|a| emit_expr(a, ctx))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({args})", c_fun_name(&id.name, &[]))
        }
        _ => "/* bad call */(0)".into(),
    }
}

