//! Call-expression emission.

use aura_ast::*;
use aura_sema::{subst_ty, type_subst_map, Ty};

use crate::array_emit::is_array_type_key;
use crate::class_emit::{class_tag, method_owner, virtual_overrides};
use crate::ctx::EmitCtx;
use crate::expr::{
    array_field_move_out_lvalue, coerce_expr, emit_channel_receive, emit_channel_send, emit_expr,
    full_type_mono, infer_type_name, is_value_struct_mono, mono_base_name, mono_split,
    owned_string_copy_expr, resolve_class_of_expr, resolve_type_name, string_expr_is_owned_temp,
    type_ref_to_ty,
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

/// A mutable Array capture is shared through a box.  Owning call parameters
/// must receive a clone so the callee cannot free the box's live buffer.
pub(crate) fn coerce_owner_arg_expr(
    expr: &Expr,
    expected_ty: &str,
    ctx: &mut EmitCtx<'_>,
) -> String {
    // Outcome values carry owned payloads.  Passing one by value to a
    // predicate such as `isSuccess` must clone the payload so the callee's
    // parameter cleanup cannot consume the caller's live result.
    if crate::stmt::is_shared_outcome_error_owner_key(expected_ty) {
        let Expr::Ident(id) = expr else {
            return coerce_expr(expr, expected_ty, ctx);
        };
        let source = mangle_ident(&id.name);
        let cty = crate::stmt::local_key_to_c(expected_ty, ctx.checked);
        return format!("{cty}_clone(&({source}))");
    }
    if is_array_type_key(expected_ty)
        && matches!(expr, Expr::Ident(id) if ctx.is_box_local(&id.name))
    {
        let mono = resolve_type_name(expr, ctx).unwrap_or_else(|| expected_ty.to_string());
        let value = emit_expr(expr, ctx);
        return format!("{}(&({value}))", c_method_name(&mono, "clone"));
    }
    coerce_expr(expr, expected_ty, ctx)
}

/// Defaults are evaluated at the caller. String literals are static C storage,
/// while Aura String parameters are owned, so materialize a heap copy before
/// passing a literal (or another borrowed expression) into the callee.
fn coerce_default_arg_expr(expr: &Expr, expected_ty: &str, ctx: &mut EmitCtx<'_>) -> String {
    let value = coerce_owner_arg_expr(expr, expected_ty, ctx);
    if expected_ty == "String" && !string_expr_is_owned_temp(expr, ctx) {
        return owned_string_copy_expr(value, expr.span());
    }
    value
}

fn emit_vararg_array(args: &[Expr], element_key: &str, ctx: &mut EmitCtx<'_>) -> String {
    let array_key = format!("Array_{element_key}");
    let array_ty = crate::stmt::local_key_to_c(&array_key, ctx.checked);
    let ctor = c_ctor_name(&array_key);
    let set = c_method_name(&array_key, "set");
    let mut code = format!(
        "({{ {array_ty} __aura_vararg = {ctor}(INT64_C({})); ",
        args.len()
    );
    for (index, arg) in args.iter().enumerate() {
        let value = coerce_owner_arg_expr(arg, element_key, ctx);
        code.push_str(&format!(
            "{set}(&__aura_vararg, INT64_C({index}), {value}); ",
        ));
    }
    code.push_str("__aura_vararg; })");
    code
}

/// Return an lvalue for a nested `Array.get`/`pop` receiver. Array accessors
/// return aggregate values by ABI, but a subsequent Array method still needs
/// the original element storage as its mutable receiver.
fn array_element_lvalue(expr: &Expr, ctx: &mut EmitCtx<'_>) -> Option<String> {
    let Expr::Call(call) = expr else {
        return None;
    };
    let Expr::Field(field) = call.callee.as_ref() else {
        return None;
    };
    if field.field.name != "get" && field.field.name != "pop" {
        return None;
    }
    let receiver_ty = resolve_type_name(&field.object, ctx)?;
    if !is_array_type_key(&receiver_ty) || call.args.len() != 1 {
        return None;
    }
    let receiver = array_field_move_out_lvalue(&field.object, ctx)
        .unwrap_or_else(|| emit_expr(&field.object, ctx));
    let index = emit_expr(&call.args[0], ctx);
    Some(format!("({receiver}).data[{index}]"))
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
        // C10d: class fields can contain first-class function values too. They
        // use the same fat-pointer ABI as local function values.
        if resolve_type_name(&c.callee, ctx).is_some_and(|k| is_fun_type_key(&k)) {
            let f = emit_expr(&c.callee, ctx);
            let mut parts = vec![format!("({f}).env")];
            for arg in &c.args {
                parts.push(emit_expr(arg, ctx));
            }
            return format!("({f}).fn({})", parts.join(", "));
        }
        // C3n: package alias qualified free function `Math.square(...)`.
        if let Expr::Ident(id) = fe.object.as_ref() {
            if let Some(inst) = ctx
                .checked
                .call_instantiations
                .get(&c.span.start)
                .filter(|i| i.is_static)
            {
                let subst = aura_sema::type_subst_map(&ctx.type_params, &ctx.type_args);
                let class_args = inst
                    .type_args
                    .iter()
                    .map(|t| aura_sema::subst_ty(t, &subst))
                    .collect::<Vec<_>>();
                let method_args = inst
                    .method_type_args
                    .iter()
                    .map(|t| aura_sema::subst_ty(t, &subst))
                    .collect::<Vec<_>>();
                let mono = type_mono(&inst.package, &id.name, &class_args);
                let selected_span = ctx
                    .checked
                    .call_instantiations
                    .get(&c.span.start)
                    .and_then(|i| i.declaration_span);
                let selected_class = ctx
                    .checked
                    .ast
                    .classes
                    .iter()
                    .find(|class| class.name.name == id.name);
                let class_params = selected_class
                    .map(|class| {
                        class
                            .type_params
                            .iter()
                            .map(|param| param.name.name.clone())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let selected_method = selected_class.and_then(|class| {
                    class.methods.iter().find(|method| {
                        method.name.name == fe.field.name
                            && selected_span.is_none_or(|span| method.span == span)
                    })
                });
                let args = selected_method
                    .map(|method| {
                        method
                            .params
                            .iter()
                            .enumerate()
                            .map(|(index, param)| {
                                c.args
                                    .get(index)
                                    .map(|arg| emit_expr(arg, ctx))
                                    .or_else(|| {
                                        param.default.as_ref().map(|expr| emit_expr(expr, ctx))
                                    })
                                    .unwrap_or_default()
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_else(|| {
                        c.args
                            .iter()
                            .map(|a| emit_expr(a, ctx))
                            .collect::<Vec<_>>()
                            .join(", ")
                    });
                return format!(
                    "{}({args})",
                    c_generic_method_name_with_params(
                        &mono,
                        &fe.field.name,
                        &method_args,
                        &selected_method
                            .expect("resolved static method declaration")
                            .params
                            .iter()
                            .map(|param| param_local_key_expand(
                                param,
                                &class_params,
                                &class_args,
                                ctx.checked
                            ))
                            .collect::<Vec<_>>(),
                        selected_class.is_some_and(|class| {
                            class
                                .methods
                                .iter()
                                .filter(|candidate| candidate.name.name == fe.field.name)
                                .count()
                                > 1
                        }),
                    )
                );
            }
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
                let imported_pkg = ctx
                    .checked
                    .ast
                    .imports
                    .iter()
                    .find(|imp| {
                        imp.alias
                            .as_ref()
                            .is_some_and(|alias| alias.name == id.name)
                    })
                    .map(|imp| {
                        imp.path
                            .segments
                            .iter()
                            .map(|segment| segment.name.as_str())
                            .collect::<Vec<_>>()
                            .join(".")
                    });
                let pkg = inst
                    .map(|i| i.package.as_str())
                    .filter(|package| !package.is_empty())
                    .or(imported_pkg.as_deref())
                    .unwrap_or("");
                let function = ctx.checked.ast.functions.iter().find(|function| {
                    function.name.name == *name && fun_decl_package(function, ctx.checked) == pkg
                });
                let args = if let Some(function) = function {
                    let params = function
                        .type_params
                        .iter()
                        .map(|p| p.name.name.clone())
                        .collect::<Vec<_>>();
                    let mut emitted = Vec::new();
                    for (index, arg) in c.args.iter().enumerate() {
                        if let Some(param) = function.params.get(index) {
                            let expected =
                                type_ref_local_key_expand(&param.ty, &params, &targs, ctx.checked);
                            emitted.push(coerce_owner_arg_expr(arg, &expected, ctx));
                        } else {
                            emitted.push(emit_expr(arg, ctx));
                        }
                    }
                    emitted.join(", ")
                } else {
                    c.args
                        .iter()
                        .map(|a| emit_expr(a, ctx))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                // C3u: `Alias.Type(...)` constructor vs `Alias.fun(...)`.
                if inst.map(|i| i.is_constructor).unwrap_or(false) {
                    let mono = type_mono(pkg, name, &targs);
                    let ctor_index = inst.and_then(|i| i.constructor_index).unwrap_or(0);
                    if ctor_index > 0 {
                        let selected_span = inst.and_then(|i| i.declaration_span);
                        let args =
                            ctx.checked
                                .ast
                                .classes
                                .iter()
                                .find(|class| class.name.name == *name)
                                .and_then(|class| {
                                    class.constructors.iter().find(|ctor| {
                                        selected_span.is_none_or(|span| ctor.span == span)
                                    })
                                })
                                .map(|ctor| {
                                    ctor.params
                                        .iter()
                                        .enumerate()
                                        .map(|(index, param)| {
                                            c.args
                                                .get(index)
                                                .map(|arg| emit_expr(arg, ctx))
                                                .or_else(|| {
                                                    param
                                                        .default
                                                        .as_ref()
                                                        .map(|expr| emit_expr(expr, ctx))
                                                })
                                                .unwrap_or_default()
                                        })
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                })
                                .unwrap_or_else(|| {
                                    c.args
                                        .iter()
                                        .map(|a| emit_expr(a, ctx))
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                });
                        return format!("{}({args})", c_ctor_name_index(&mono, ctor_index));
                    }
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
                        let args = class
                            .fields
                            .iter()
                            .enumerate()
                            .map(|(index, f)| {
                                let expected =
                                    type_ref_local_key_expand(&f.ty, &tparams, &targs, ctx.checked);
                                field_keys.push(expected.clone());
                                c.args
                                    .get(index)
                                    .map(|a| coerce_owner_arg_expr(a, &expected, ctx))
                                    .or_else(|| {
                                        f.default.as_ref().map(|default| {
                                            coerce_default_arg_expr(default, &expected, ctx)
                                        })
                                    })
                                    .unwrap_or_default()
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
                if let Expr::Call(inner) = fe.object.as_ref() {
                    if let Expr::Field(qualified) = inner.callee.as_ref() {
                        if let Expr::Ident(alias) = qualified.object.as_ref() {
                            if let Some(import) = ctx.checked.ast.imports.iter().find(|import| {
                                import
                                    .alias
                                    .as_ref()
                                    .is_some_and(|candidate| candidate.name == alias.name)
                            }) {
                                let package = import
                                    .path
                                    .segments
                                    .iter()
                                    .map(|segment| segment.name.as_str())
                                    .collect::<Vec<_>>()
                                    .join(".");
                                ctx.checked
                                    .ast
                                    .functions
                                    .iter()
                                    .find(|function| {
                                        function.name.name == qualified.field.name
                                            && fun_decl_package(function, ctx.checked) == package
                                    })
                                    .and_then(|function| {
                                        function.return_type.as_ref().map(|ty| {
                                            type_ref_local_key_expand(ty, &[], &[], ctx.checked)
                                        })
                                    })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                Some(t)
            }
        });

        let obj_ty = obj_ty.or_else(|| {
            let ty = ctx
                .checked
                .expr_tys
                .get(&(fe.object.span().start, fe.object.span().end))?;
            match ty {
                Ty::Channel(_) => Some(ty.mono_suffix()),
                _ => None,
            }
        });

        // Channel operations are represented as ordinary calls until semantic
        // typing confirms the receiver is Channel<T>. Emit the same intrinsic
        // operations used by the legacy parser lowering in that case.
        if obj_ty
            .as_deref()
            .is_some_and(|key| key.starts_with("Channel_"))
        {
            match fe.field.name.as_str() {
                "send" if c.args.len() == 1 => {
                    return emit_channel_send(
                        &ChannelSendExpr {
                            channel: fe.object.clone(),
                            value: Box::new(c.args[0].clone()),
                            span: c.span,
                        },
                        ctx,
                    );
                }
                "receive" if c.args.is_empty() => {
                    return emit_channel_receive(
                        &ChannelReceiveExpr {
                            channel: fe.object.clone(),
                            span: c.span,
                        },
                        ctx,
                    );
                }
                "close" if c.args.is_empty() => {
                    let channel = emit_expr(&fe.object, ctx);
                    return format!(
                        "({{ aura_race_set_source_id(UINT32_C({})); (void)aura_task_channel_close({channel}); aura_race_set_source_id(0); (void)0; }})",
                        c.span.start
                    );
                }
                _ => {}
            }
        }

        // Array fields are mutable receivers; keep the direct lvalue so the
        // generated `&receiver` is valid instead of taking the address of a
        // race-instrumented rvalue expression.
        let obj = if let (Expr::Ident(id), Some(struct_key)) =
            (fe.object.as_ref(), obj_ty.as_deref())
        {
            if is_value_struct_mono(struct_key, ctx.checked) {
                mangle_ident(&id.name)
            } else if ctx.is_box_local(&id.name) && is_array_type_key(struct_key) {
                let cty = crate::stmt::local_key_to_c(struct_key, ctx.checked);
                format!("(*({cty} *)aura_box_ptr_get({}))", mangle_ident(&id.name))
            } else {
                array_field_move_out_lvalue(&fe.object, ctx)
                    .unwrap_or_else(|| emit_expr(&fe.object, ctx))
            }
        } else if let (Expr::Ident(id), Some(array_key)) = (fe.object.as_ref(), obj_ty.as_deref()) {
            if ctx.is_box_local(&id.name) && is_array_type_key(array_key) {
                let cty = crate::stmt::local_key_to_c(array_key, ctx.checked);
                format!("(*({cty} *)aura_box_ptr_get({}))", mangle_ident(&id.name))
            } else {
                array_field_move_out_lvalue(&fe.object, ctx)
                    .unwrap_or_else(|| emit_expr(&fe.object, ctx))
            }
        } else {
            array_field_move_out_lvalue(&fe.object, ctx)
                .unwrap_or_else(|| emit_expr(&fe.object, ctx))
        };

        // Interface method (C4d package mono; C8c mono args e.g. Boxable_Int)
        if let Some(iface_key) = obj_ty
            .as_ref()
            .filter(|t| is_iface_type_key(t, ctx.checked))
        {
            let imono = resolve_iface_mono_key(iface_key, ctx.checked);
            let mut args = Vec::new();
            let (iface_decl, iargs) = resolve_iface_decl_and_args(iface_key, ctx.checked);
            let selected_span = ctx
                .checked
                .call_instantiations
                .get(&c.span.start)
                .and_then(|inst| inst.declaration_span);
            let mut selected_method = None;
            let mut selected_param_keys = Vec::new();
            let mut interface_overloaded = false;
            if let Some(i) = iface_decl {
                let inherited =
                    crate::iface::interface_method_decls_with_parents(ctx.checked, i, &iargs);
                interface_overloaded = inherited
                    .iter()
                    .filter(|(method, _, _)| method.name.name == fe.field.name)
                    .count()
                    > 1;
                if let Some((m, owner, owner_args)) = inherited.into_iter().find(|(m, _, _)| {
                    m.name.name == fe.field.name && selected_span.is_none_or(|span| m.span == span)
                }) {
                    selected_method = Some(m);
                    let tparams = owner
                        .type_params
                        .iter()
                        .map(|p| p.name.name.clone())
                        .collect::<Vec<_>>();
                    selected_param_keys = m
                        .params
                        .iter()
                        .map(|param| {
                            param_local_key_expand(param, &tparams, &owner_args, ctx.checked)
                        })
                        .collect();
                    for (index, p) in m.params.iter().enumerate() {
                        if p.is_vararg {
                            let expected =
                                param_local_key_expand(p, &tparams, &owner_args, ctx.checked);
                            let element = expected
                                .strip_prefix("Array_")
                                .unwrap_or(expected.as_str())
                                .to_string();
                            args.push(emit_vararg_array(&c.args[index..], &element, ctx));
                            break;
                        }
                        if let Some(a) = c.args.get(index) {
                            let expected =
                                param_local_key_expand(p, &tparams, &owner_args, ctx.checked);
                            args.push(coerce_owner_arg_expr(a, &expected, ctx));
                        } else if let Some(default) = &p.default {
                            let expected = param_local_key_expand(p, &tparams, &iargs, ctx.checked);
                            args.push(coerce_default_arg_expr(default, &expected, ctx));
                        }
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
            let receiver = format!("__aura_iface_recv_{}", fe.object.span().start);
            let cty = c_iface_type(&imono);
            let call_args = if args.is_empty() {
                format!("&{receiver}")
            } else {
                format!("&{receiver}, {}", args.join(", "))
            };
            return format!(
                "({{ {cty} {receiver} = ({obj}); {}({call_args}); }})",
                c_iface_method_name_with_params(
                    &imono,
                    &fe.field.name,
                    &selected_method
                        .map(|_| selected_param_keys)
                        .unwrap_or_default(),
                    interface_overloaded,
                ),
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
        if (mono_raw == "Int" || matches!(obj_ty.as_deref(), Some("Int")))
            && fe.field.name == "toFloat"
        {
            return format!("((double)({obj}))");
        }
        if mono_raw == "Float" || matches!(obj_ty.as_deref(), Some("Float")) {
            if fe.field.name == "toInt" {
                return format!("((int64_t)({obj}))");
            }
            if fe.field.name == "toString" {
                return format!("aura_f64_to_string({obj})");
            }
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
                let free_owned_receiver = if string_expr_is_owned_temp(&fe.object, ctx) {
                    " free((void *)__s);"
                } else {
                    ""
                };
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
                     {free_owned_receiver} __out; }})"
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
            // A method receiver returned by a free function is an aggregate
            // rvalue; bind it before passing the mutable Array receiver.
            let array_temp = if matches!(fe.object.as_ref(), Expr::Call(_))
                && array_element_lvalue(&fe.object, ctx).is_none()
            {
                let name = format!("__aura_array_receiver_{}", fe.object.span().start);
                let cty = crate::stmt::local_key_to_c(&mono, ctx.checked);
                Some((name, cty, obj.clone()))
            } else {
                None
            };
            let force_array = match fe.object.as_ref() {
                Expr::ForceUnwrap(force) => {
                    let key = resolve_type_name(&fe.object, ctx)
                        .unwrap_or_else(|| infer_type_name(&fe.object, ctx));
                    is_array_type_key(&key).then(|| {
                        let name = format!("__aura_force_array_{}", fe.object.span().start);
                        let cty = crate::stmt::local_key_to_c(&key, ctx.checked);
                        (name, cty, emit_expr(&force.expr, ctx))
                    })
                }
                _ => None,
            };
            let receiver = array_temp
                .as_ref()
                .map(|(name, _, _)| format!("&{name}"))
                .or_else(|| {
                    array_element_lvalue(&fe.object, ctx).map(|lvalue| format!("&({lvalue})"))
                })
                .or_else(|| force_array.as_ref().map(|(name, _, _)| format!("&{name}")))
                .unwrap_or_else(|| format!("&({obj})"));
            let mut args = vec![receiver];
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
                    if !elem_key.is_empty() {
                        param_keys.push(elem_key.to_string());
                        args.push(coerce_owner_arg_expr(a, elem_key, ctx));
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
            let wrap_force_array = |call: String| {
                let Some((name, cty, value)) = force_array.as_ref() else {
                    return call;
                };
                let ret_c = match fe.field.name.as_str() {
                    "get" | "pop" => crate::stmt::local_key_to_c(elem_key, ctx.checked),
                    "clone" => crate::stmt::local_key_to_c(&mono, ctx.checked),
                    "isEmpty" => "bool".into(),
                    _ => "void".into(),
                };
                if ret_c == "void" {
                    format!(
                        "({{ {cty} {name} = ({value}); {call}; (void)0; }})",
                        cty = cty,
                        name = name,
                        value = value,
                        call = call
                    )
                } else {
                    format!(
                        "({{ {cty} {name} = ({value}); {ret_c} __aura_force_result = ({call}); __aura_force_result; }})",
                        cty = cty,
                        name = name,
                        value = value,
                        ret_c = ret_c,
                        call = call
                    )
                }
            };
            let wrap_array_temp = |call: String| {
                let Some((name, cty, value)) = array_temp.as_ref() else {
                    return call;
                };
                let ret_c = match fe.field.name.as_str() {
                    "get" | "pop" => crate::stmt::local_key_to_c(elem_key, ctx.checked),
                    "clone" => crate::stmt::local_key_to_c(&mono, ctx.checked),
                    "isEmpty" => "bool".into(),
                    "len" | "capacity" => "int64_t".into(),
                    _ => "void".into(),
                };
                let cleanup = crate::array_emit::array_contents_free_expr(name, &mono);
                if ret_c == "void" {
                    format!("({{ {cty} {name} = ({value}); {call}; {cleanup} (void)0; }})")
                } else {
                    format!("({{ {cty} {name} = ({value}); {ret_c} __aura_array_result = ({call}); {cleanup} __aura_array_result; }})")
                }
            };
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
                return wrap_array_temp(wrap_force_array(wrapped));
            }
            if (fe.field.name == "push" || fe.field.name == "set") && is_array_type_key(elem_key) {
                let move_srcs = array_move_srcs_from_args(&c.args, &param_keys, ctx);
                return wrap_array_temp(wrap_force_array(wrap_array_arg_moves(
                    call, &move_srcs, "void", ctx,
                )));
            }
            return wrap_array_temp(wrap_force_array(call));
        }

        let current_class = ctx.checked.ast.classes.iter().find(|c| {
            c.kind == NominalKind::Class
                && (c.name.name == base
                    || type_mono(&class_decl_package(c, ctx.checked), &c.name.name, &[]) == mono)
        });
        let owner =
            current_class.and_then(|class| method_owner(ctx.checked, class, &fe.field.name));
        let owner_mono = owner
            .map(|class| {
                let owner_args = if class.type_params.is_empty() {
                    Vec::new()
                } else {
                    mono_split(mono_raw, ctx.checked)
                        .map(|(_, args)| args.to_vec())
                        .unwrap_or_default()
                };
                type_mono(
                    &class_decl_package(class, ctx.checked),
                    &class.name.name,
                    &owner_args,
                )
            })
            .unwrap_or_else(|| mono.clone());
        let owner_mono = full_type_mono(&owner_mono, ctx.checked);

        // C3y: heap classes are already pointers; structs/Array need &.
        // Inherited methods receive a pointer to the child prefix layout.
        let is_super = matches!(fe.object.as_ref(), Expr::Ident(id) if id.name == "super");
        let this_arg = if is_super || owner.is_some_and(|class| class.name.name != base) {
            format!("(({} *)({obj}))", c_class_type(&owner_mono))
        } else if is_heap_class_mono(&mono, ctx.checked) {
            if matches!(fe.object.as_ref(), Expr::This(_)) {
                "this".into()
            } else {
                format!("({obj})")
            }
        } else {
            format!("&({obj})")
        };
        let mut args = vec![this_arg];
        if let Some(class) = owner {
            let selected_span = ctx
                .checked
                .call_instantiations
                .get(&c.span.start)
                .and_then(|inst| inst.declaration_span);
            if let Some(m) = class.methods.iter().find(|m| {
                m.name.name == fe.field.name && selected_span.is_none_or(|span| m.span == span)
            }) {
                // C4u: substitute class type params for method parameter expected types.
                let params: Vec<String> = class
                    .type_params
                    .iter()
                    .map(|p| p.name.name.clone())
                    .collect();
                let targs: Vec<Ty> = if class.type_params.is_empty() {
                    Vec::new()
                } else {
                    mono_split(mono_raw, ctx.checked)
                        .map(|(_, a)| a.to_vec())
                        .unwrap_or_default()
                };
                let mut param_keys = Vec::new();
                for (index, p) in m.params.iter().enumerate() {
                    if p.is_vararg {
                        let expected = param_local_key_expand(p, &params, &targs, ctx.checked);
                        let element = expected
                            .strip_prefix("Array_")
                            .unwrap_or(expected.as_str())
                            .to_string();
                        param_keys.push(expected);
                        args.push(emit_vararg_array(&c.args[index..], &element, ctx));
                        break;
                    }
                    if let Some(a) = c.args.get(index) {
                        let expected =
                            type_ref_local_key_expand(&p.ty, &params, &targs, ctx.checked);
                        param_keys.push(expected.clone());
                        args.push(coerce_owner_arg_expr(a, &expected, ctx));
                    } else if let Some(default) = &p.default {
                        let expected =
                            type_ref_local_key_expand(&p.ty, &params, &targs, ctx.checked);
                        args.push(coerce_default_arg_expr(default, &expected, ctx));
                    }
                }
                let ret_c = c_type_from_opt(&m.return_type, ctx.checked, &params, &targs);
                let method_args = ctx
                    .checked
                    .call_instantiations
                    .get(&c.span.start)
                    .map(|inst| {
                        let subst = aura_sema::type_subst_map(&ctx.type_params, &ctx.type_args);
                        inst.method_type_args
                            .iter()
                            .map(|ty| aura_sema::subst_ty(ty, &subst))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let call = format!(
                    "{}({})",
                    c_generic_method_name_with_params(
                        &owner_mono,
                        &fe.field.name,
                        &method_args,
                        &m.params
                            .iter()
                            .map(|param| param_local_key_expand(
                                param,
                                &params,
                                &targs,
                                ctx.checked
                            ))
                            .collect::<Vec<_>>(),
                        class
                            .methods
                            .iter()
                            .filter(|candidate| candidate.name.name == m.name.name)
                            .count()
                            > 1,
                    ),
                    args.join(", ")
                );
                let call = if let Some(static_class) = current_class {
                    let is_virtual = !is_super
                        && m.modifiers.contains(&aura_ast::Modifier::Open)
                        && is_heap_class_decl(static_class);
                    let children = if is_virtual {
                        virtual_overrides(ctx.checked, static_class, &fe.field.name)
                    } else {
                        Vec::new()
                    };
                    if children.is_empty() {
                        call
                    } else {
                        let tail = args[1..].join(", ");
                        let mut dispatch = call;
                        for child in children.into_iter().rev() {
                            let child_mono = if child.type_params.is_empty() {
                                type_mono(
                                    &class_decl_package(child, ctx.checked),
                                    &child.name.name,
                                    &[],
                                )
                            } else if let Some((_, args)) =
                                ctx.checked.mono_classes.iter().find(|(name, args)| {
                                    name == &child.name.name
                                        && crate::class_emit::class_mono_extends(
                                            ctx.checked,
                                            &type_mono(
                                                &class_decl_package(child, ctx.checked),
                                                name,
                                                args,
                                            ),
                                            &owner_mono,
                                        )
                                })
                            {
                                type_mono(
                                    &class_decl_package(child, ctx.checked),
                                    &child.name.name,
                                    args,
                                )
                            } else {
                                continue;
                            };
                            let child_call = format!(
                                "{}(({} *)({obj}){}{})",
                                c_method_name(&child_mono, &fe.field.name),
                                c_class_type(&child_mono),
                                if tail.is_empty() { "" } else { ", " },
                                tail
                            );
                            dispatch = format!(
                                "((({obj})->__aura_class_tag == UINT32_C({})) ? {} : {})",
                                class_tag(ctx.checked, child),
                                child_call,
                                dispatch
                            );
                        }
                        dispatch
                    }
                } else {
                    call
                };
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
        let selected_span = ctx
            .checked
            .call_instantiations
            .get(&c.span.start)
            .and_then(|inst| inst.declaration_span);
        let method_matches = |method: &FunDecl| {
            method.name.name == fe.field.name
                && (selected_span.is_some_and(|span| method.span == span)
                    || method.params.iter().any(|param| param.is_vararg)
                    || c.args.len() <= method.params.len())
        };
        let mono_base = mono_base_name(&mono, ctx.checked).unwrap_or(mono.as_str());
        let fallback_class = current_class
            .or_else(|| {
                ctx.checked.ast.classes.iter().find(|class| {
                    class.name.name == mono_base && class.methods.iter().any(method_matches)
                })
            })
            .or_else(|| {
                ctx.checked
                    .ast
                    .classes
                    .iter()
                    .find(|class| class.methods.iter().any(method_matches))
            });
        let fallback_method = fallback_class
            .and_then(|class| class.methods.iter().find(|method| method_matches(method)));
        if let (Some(class), Some(method)) = (fallback_class, fallback_method) {
            let params = class
                .type_params
                .iter()
                .map(|param| param.name.name.clone())
                .collect::<Vec<_>>();
            let targs = mono_split(&mono, ctx.checked)
                .map(|(_, args)| args.to_vec())
                .unwrap_or_default();
            args.truncate(1);
            for (index, param) in method.params.iter().enumerate() {
                if param.is_vararg {
                    let expected = param_local_key_expand(param, &params, &targs, ctx.checked);
                    let element = expected
                        .strip_prefix("Array_")
                        .unwrap_or(expected.as_str())
                        .to_string();
                    args.push(emit_vararg_array(&c.args[index..], &element, ctx));
                    break;
                }
                if let Some(arg) = c.args.get(index) {
                    let expected =
                        type_ref_local_key_expand(&param.ty, &params, &targs, ctx.checked);
                    args.push(coerce_owner_arg_expr(arg, &expected, ctx));
                }
            }
        }
        let fallback_params = fallback_method
            .map(|method| {
                let params = fallback_class
                    .map(|class| {
                        class
                            .type_params
                            .iter()
                            .map(|param| param.name.name.clone())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                method
                    .params
                    .iter()
                    .map(|param| param_local_key_expand(param, &params, &[], ctx.checked))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let fallback_overloaded = fallback_class.is_some_and(|class| {
            class
                .methods
                .iter()
                .filter(|candidate| candidate.name.name == fe.field.name)
                .count()
                > 1
        });
        let call = format!(
            "{}({})",
            c_generic_method_name_with_params(
                &mono,
                &fe.field.name,
                &ctx.checked
                    .call_instantiations
                    .get(&c.span.start)
                    .map(|i| i.method_type_args.clone())
                    .unwrap_or_default(),
                &fallback_params,
                fallback_overloaded,
            ),
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
            // Nested calls can share a start offset; never apply another call's
            // instantiation metadata to this callee.
            let inst = ctx
                .checked
                .call_instantiations
                .get(&c.span.start)
                .filter(|inst| {
                    inst.name == id.name || inst.variant.as_deref() == Some(id.name.as_str())
                });

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
                let ctor_index = inst
                    .and_then(|i| i.constructor_index)
                    .or_else(|| {
                        let primary_required = class
                            .fields
                            .iter()
                            .filter(|field| field.default.is_none())
                            .count();
                        if c.args.len() < primary_required || c.args.len() > class.fields.len() {
                            class
                                .constructors
                                .iter()
                                .enumerate()
                                .find(|(_, ctor)| {
                                    let required = ctor
                                        .params
                                        .iter()
                                        .filter(|param| param.default.is_none())
                                        .count();
                                    let is_vararg = ctor.params.iter().any(|param| param.is_vararg);
                                    c.args.len() >= required
                                        && (is_vararg || c.args.len() <= ctor.params.len())
                                })
                                .map(|(index, _)| index + 1)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);
                if ctor_index > 0 {
                    let selected_span = inst.and_then(|i| i.declaration_span);
                    let args = class
                        .constructors
                        .iter()
                        .find(|ctor| selected_span.is_none_or(|span| ctor.span == span))
                        .map(|ctor| {
                            ctor.params
                                .iter()
                                .enumerate()
                                .map(|(index, param)| {
                                    c.args
                                        .get(index)
                                        .map(|arg| emit_expr(arg, ctx))
                                        .or_else(|| {
                                            param.default.as_ref().map(|expr| emit_expr(expr, ctx))
                                        })
                                        .unwrap_or_default()
                                })
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_else(|| {
                            c.args
                                .iter()
                                .map(|a| emit_expr(a, ctx))
                                .collect::<Vec<_>>()
                                .join(", ")
                        });
                    return format!("{}({args})", c_ctor_name_index(&mono, ctor_index));
                }
                let params: Vec<String> = class
                    .type_params
                    .iter()
                    .map(|p| p.name.name.clone())
                    .collect();
                // C6i: Array primary-ctor fields own the buffer — move from owner idents.
                let mut field_keys = Vec::new();
                let mut owned_string_temps = Vec::new();
                let args = class
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(index, f)| {
                        let expected =
                            type_ref_local_key_expand(&f.ty, &params, &targs, ctx.checked);
                        field_keys.push(expected.clone());
                        let Some(a) = c.args.get(index) else {
                            return f
                                .default
                                .as_ref()
                                .map(|default| coerce_default_arg_expr(default, &expected, ctx))
                                .unwrap_or_default();
                        };
                        if expected == "String" && string_expr_is_owned_temp(a, ctx) {
                            let temp = format!("__aura_ctor_string_{}", a.span().start);
                            owned_string_temps.push((temp.clone(), emit_expr(a, ctx)));
                            return temp;
                        }
                        coerce_owner_arg_expr(a, &expected, ctx)
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
                if !owned_string_temps.is_empty() {
                    let prefix = owned_string_temps
                        .iter()
                        .map(|(name, value)| format!("const char *{name} = ({value}); "))
                        .collect::<String>();
                    let suffix = owned_string_temps
                        .iter()
                        .map(|(name, _)| format!("free((void *){name}); "))
                        .collect::<String>();
                    return format!("({{ {prefix}{ret_c} __ctor = ({call}); {suffix} __ctor; }})");
                }
                return wrap_array_arg_moves(call, &move_srcs, &ret_c, ctx);
            }
            // Enum variant constructor: Ok(...), Err(...), Red()
            if let Some(inst) = inst {
                if let Some(vname) = &inst.variant {
                    // Generic enum constructors inside a generic function carry
                    // open `T`/`E` arguments in the call instance. Resolve them
                    // against the current function instantiation before naming
                    // the concrete C constructor.
                    let subst = type_subst_map(&ctx.type_params, &ctx.type_args);
                    let resolved_type_args = inst
                        .type_args
                        .iter()
                        .map(|arg| subst_ty(arg, &subst))
                        .collect::<Vec<_>>();
                    let mono = type_mono(&inst.package, &inst.name, &resolved_type_args);
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
                                    let expected = type_ref_local_key_expand(
                                        &f.ty,
                                        &params,
                                        &resolved_type_args,
                                        ctx.checked,
                                    );
                                    coerce_owner_arg_expr(a, &expected, ctx)
                                })
                                .collect::<Vec<_>>()
                                .join(", ");
                            let ctor_name = if inst.package == "std.error"
                                && inst.name == "Outcome"
                                && vname == "OutcomeOk"
                                && resolved_type_args
                                    .first()
                                    .is_some_and(|ty| matches!(ty, Ty::String))
                            {
                                c_variant_ctor_name(&mono, "OutcomeOkOwned")
                            } else {
                                c_variant_ctor_name(&mono, vname)
                            };
                            return format!("{}({args})", ctor_name);
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
                    "Float" => format!("aura_assert_eq_float({a_v}, {b_v})"),
                    "Bool" => format!("aura_assert_eq_bool({a_v}, {b_v})"),
                    _ => format!("aura_assert_eq_int({a_v}, {b_v})"),
                };
            }
            if id.name == "print" && c.args.len() == 1 {
                let arg = coerce_expr(&c.args[0], "String", ctx);
                if string_expr_is_owned_temp(&c.args[0], ctx) {
                    return format!(
                        "({{ const char *__s = ({arg}); aura_print(__s); free((void *)__s); }})"
                    );
                }
                return format!("aura_print({arg})");
            }
            if id.name == "println" && c.args.len() == 1 {
                let arg = coerce_expr(&c.args[0], "String", ctx);
                if string_expr_is_owned_temp(&c.args[0], ctx) {
                    return format!(
                        "({{ const char *__s = ({arg}); aura_println(__s); free((void *)__s); }})"
                    );
                }
                return format!("aura_println({arg})");
            }
            if id.name == "eprint" && c.args.len() == 1 {
                let arg = coerce_expr(&c.args[0], "String", ctx);
                if string_expr_is_owned_temp(&c.args[0], ctx) {
                    return format!(
                        "({{ const char *__s = ({arg}); aura_eprint(__s); free((void *)__s); }})"
                    );
                }
                return format!("aura_eprint({arg})");
            }
            if id.name == "eprintln" && c.args.len() == 1 {
                let arg = coerce_expr(&c.args[0], "String", ctx);
                if string_expr_is_owned_temp(&c.args[0], ctx) {
                    return format!(
                        "({{ const char *__s = ({arg}); aura_eprintln(__s); free((void *)__s); }})"
                    );
                }
                return format!("aura_eprintln({arg})");
            }
            // C5m: builtin STW GC collect.
            if id.name == "gc_collect" && c.args.is_empty() {
                return "aura_gc_collect_executor(__aura_task_executor)".into();
            }
            // RUNTIME-003: expose the active cause chain without leaking the
            // runtime's borrowed type-name storage into Aura String values.
            match id.name.as_str() {
                "exception_cause_count" if c.args.is_empty() => {
                    return "((int64_t)aura_ex_cause_count())".into();
                }
                "exception_source_span_start" if c.args.is_empty() => {
                    return "((int64_t)aura_ex_source_span_start())".into();
                }
                "exception_source_span_end" if c.args.is_empty() => {
                    return "((int64_t)aura_ex_source_span_end())".into();
                }
                "exception_cause_type" if c.args.len() == 1 => {
                    return format!(
                        "aura_ex_cause_type_copy((size_t)({}))",
                        emit_expr(&c.args[0], ctx)
                    );
                }
                "exception_cause_span_start" if c.args.len() == 1 => {
                    return format!(
                        "((int64_t)aura_ex_cause_span_start((size_t)({})))",
                        emit_expr(&c.args[0], ctx)
                    );
                }
                "exception_cause_span_end" if c.args.len() == 1 => {
                    return format!(
                        "((int64_t)aura_ex_cause_span_end((size_t)({})))",
                        emit_expr(&c.args[0], ctx)
                    );
                }
                "exception_add_cause" if c.args.len() == 3 => {
                    return format!(
                        "((void)aura_ex_add_cause({}, (uint32_t)({}), (uint32_t)({})))",
                        emit_expr(&c.args[0], ctx),
                        emit_expr(&c.args[1], ctx),
                        emit_expr(&c.args[2], ctx)
                    );
                }
                _ => {}
            }
            // Free function
            let targs: Vec<Ty> = if !c.type_args.is_empty() {
                let subst = aura_sema::type_subst_map(&ctx.type_params, &ctx.type_args);
                c.type_args
                    .iter()
                    .filter_map(|t| type_ref_to_ty(t, ctx))
                    .map(|ty| aura_sema::subst_ty(&ty, &subst))
                    .collect()
            } else if let Some(inst) = inst {
                let subst = aura_sema::type_subst_map(&ctx.type_params, &ctx.type_args);
                inst.type_args
                    .iter()
                    .map(|t| aura_sema::subst_ty(t, &subst))
                    .collect()
            } else {
                Vec::new()
            };
            let pkg = inst.map(|i| i.package.as_str()).unwrap_or("");
            // Open generic bodies are emitted with erased type parameters and
            // are not runtime call targets. Typed JSON encoding is closed at
            // each concrete monomorph; keep the open fallback compilable.
            if pkg == "std.json"
                && matches!(id.name.as_str(), "encode" | "stringify")
                && targs.iter().any(Ty::is_open)
            {
                return "NULL".into();
            }
            if pkg == "std.task" && id.name == "isCancelled" && c.args.is_empty() {
                return ctx
                    .async_frame
                    .as_deref()
                    .map(|frame| format!("aura_task_frame_cancel_requested({frame}) != 0"))
                    .unwrap_or_else(|| "false".into());
            }
            if let Some(foreign) = ctx.checked.ast.foreign_functions.iter().find(|f| {
                f.name.name == id.name
                    && (pkg.is_empty() || foreign_decl_package(f, ctx.checked) == pkg)
            }) {
                return emit_foreign_call(foreign, c, ctx);
            }
            if let Some(f) = ctx.checked.ast.functions.iter().find(|f| {
                f.name.name == id.name
                    && (pkg.is_empty() || fun_decl_package(f, ctx.checked) == pkg)
                    && inst
                        .and_then(|selected| selected.declaration_span)
                        .is_none_or(|span| f.span == span)
            }) {
                let params: Vec<String> =
                    f.type_params.iter().map(|p| p.name.name.clone()).collect();
                let mut param_keys = Vec::new();
                let mut owned_string_args = Vec::new();
                let mut emitted_args = Vec::new();
                for (index, param) in f.params.iter().enumerate() {
                    if param.is_vararg {
                        let expected = param_local_key_expand(param, &params, &targs, ctx.checked);
                        let element = expected
                            .strip_prefix("Array_")
                            .unwrap_or(expected.as_str())
                            .to_string();
                        param_keys.push(expected);
                        emitted_args.push(emit_vararg_array(&c.args[index..], &element, ctx));
                        break;
                    }
                    if let Some(arg) = c.args.get(index) {
                        let expected =
                            type_ref_local_key_expand(&param.ty, &params, &targs, ctx.checked);
                        param_keys.push(expected.clone());
                        let value = coerce_owner_arg_expr(arg, &expected, ctx);
                        if expected == "String" && string_expr_is_owned_temp(arg, ctx) {
                            let temp = format!("__aura_string_arg_{index}");
                            owned_string_args.push((temp.clone(), value));
                            emitted_args.push(temp);
                        } else {
                            emitted_args.push(value);
                        }
                    } else if let Some(default) = &param.default {
                        let expected =
                            type_ref_local_key_expand(&param.ty, &params, &targs, ctx.checked);
                        emitted_args.push(coerce_default_arg_expr(default, &expected, ctx));
                    }
                }
                let args = emitted_args.join(", ");
                let ret_c = c_type_from_opt(&f.return_type, ctx.checked, &params, &targs);
                let fpkg = fun_decl_package(f, ctx.checked);
                // Do not inherit method inference arguments when the free
                // function itself is non-generic (e.g. fields(...).get(...)).
                let call_name = if f.type_params.is_empty() {
                    let overload_keys = f
                        .params
                        .iter()
                        .map(|param| {
                            type_ref_local_key_expand(&param.ty, &params, &[], ctx.checked)
                        })
                        .collect::<Vec<_>>();
                    let overloaded = ctx
                        .checked
                        .ast
                        .functions
                        .iter()
                        .filter(|candidate| {
                            candidate.name.name == id.name
                                && fun_decl_package(candidate, ctx.checked) == fpkg
                        })
                        .count()
                        > 1;
                    c_fun_name_with_params(&fpkg, &id.name, &[], &overload_keys, overloaded)
                } else {
                    c_fun_name(&fpkg, &id.name, &targs)
                };
                let call = format!("{}({args})", call_name);
                let call = wrap_owner_arg_moves(call, &c.args, &param_keys, &ret_c, ctx);
                if owned_string_args.is_empty() {
                    return call;
                }
                let prelude = owned_string_args
                    .iter()
                    .map(|(name, value)| format!("const char *{name} = ({value});"))
                    .collect::<String>();
                let cleanup = owned_string_args
                    .iter()
                    .map(|(name, _)| format!("free((void *){name});"))
                    .collect::<String>();
                if ret_c == "void" {
                    return format!("({{ {prelude} {call}; {cleanup} }})");
                }
                return format!(
                    "({{ {prelude} {ret_c} __aura_call_result = ({call}); {cleanup} __aura_call_result; }})"
                );
            }
            if let Some(f) = ctx.checked.ast.async_functions.iter().find(|f| {
                f.name.name == id.name
                    && (pkg.is_empty() || async_fun_decl_package(f, ctx.checked) == pkg)
                    && inst
                        .and_then(|selected| selected.declaration_span)
                        .is_none_or(|span| f.span == span)
            }) {
                let params: Vec<String> =
                    f.type_params.iter().map(|p| p.name.name.clone()).collect();
                let mut args = Vec::new();
                let mut prelude = String::new();
                let mut cleanup = String::new();
                for (index, p) in f.params.iter().enumerate() {
                    if p.is_vararg {
                        let expected = param_local_key_expand(p, &params, &targs, ctx.checked);
                        let element = expected.strip_prefix("Array_").unwrap_or(expected.as_str());
                        args.push(emit_vararg_array(&c.args[index..], element, ctx));
                        break;
                    }
                    let Some(a) = c.args.get(index) else {
                        if let Some(default) = &p.default {
                            let expected =
                                type_ref_local_key_expand(&p.ty, &params, &targs, ctx.checked);
                            args.push(coerce_default_arg_expr(default, &expected, ctx));
                        }
                        continue;
                    };
                    let expected = type_ref_local_key_expand(&p.ty, &params, &targs, ctx.checked);
                    let value = coerce_owner_arg_expr(a, &expected, ctx);
                    if expected == "String" && string_expr_is_owned_temp(a, ctx) {
                        let name = format!("__aura_async_string_{}_{}", c.span.start, index);
                        prelude.push_str(&format!("const char *{name} = ({value}); "));
                        cleanup.push_str(&format!("free((void *){name}); "));
                        args.push(name);
                    } else if (expected == "ForeignHandle"
                        || expected.starts_with("ForeignHandle_"))
                        && matches!(a, Expr::Call(_))
                    {
                        let name = format!("__aura_async_handle_{}_{}", c.span.start, index);
                        prelude.push_str(&format!(
                            "AuraFfiOpaqueHandle *{name} = (AuraFfiOpaqueHandle *)({value}); "
                        ));
                        cleanup.push_str(&format!(
                            "if ({name} != NULL) (void)aura_ffi_handle_drop(&{name}); "
                        ));
                        args.push(name);
                    } else {
                        args.push(value);
                    }
                }
                let call_name = if f.type_params.is_empty() {
                    let fpkg = async_fun_decl_package(f, ctx.checked);
                    let overload_keys = f
                        .params
                        .iter()
                        .map(|param| {
                            type_ref_local_key_expand(&param.ty, &params, &[], ctx.checked)
                        })
                        .collect::<Vec<_>>();
                    let overloaded = ctx
                        .checked
                        .ast
                        .async_functions
                        .iter()
                        .filter(|candidate| {
                            candidate.name.name == id.name
                                && async_fun_decl_package(candidate, ctx.checked) == fpkg
                        })
                        .count()
                        > 1;
                    c_fun_name_with_params(&fpkg, &id.name, &[], &overload_keys, overloaded)
                } else {
                    c_fun_name(&async_fun_decl_package(f, ctx.checked), &id.name, &targs)
                };
                let call = format!("{}({})", call_name, args.join(", "));
                if prelude.is_empty() {
                    return call;
                }
                return format!(
                    "({{ {prelude} AuraTaskFrame *__aura_async_result = ({call}); {cleanup} __aura_async_result; }})"
                );
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
        // async caller.  A failed pin is passed through to the C shim as the
        // original handle so native code can return a typed closed-handle
        // error instead of forcing a process abort. Aura does not silently pin Task, TaskHandle,
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
                        "AuraFfiOpaqueHandle *__aura_ffi_handle_{slot} = (AuraFfiOpaqueHandle *)({arg}); if (__aura_ffi_handle_{slot} != NULL) (void)aura_task_frame_pin_foreign_handle({frame}, __aura_ffi_handle_{slot}, AURA_FFI_BOUNDARY_TASK); "
                    ),
                );
            } else {
                let _ = std::fmt::Write::write_fmt(
                    &mut out,
                    format_args!(
                        "AuraFfiOpaqueHandle *__aura_ffi_handle_{slot} = (AuraFfiOpaqueHandle *)({arg}); AuraFfiHandlePin __aura_ffi_pin_{slot} = {{0}}; if (__aura_ffi_handle_{slot} != NULL) (void)aura_ffi_handle_pin_for_boundary(__aura_ffi_handle_{slot}, AURA_FFI_BOUNDARY_TASK, &__aura_ffi_pin_{slot}); "
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
                    let expected = type_ref_local_key_expand(&param.ty, &[], &[], ctx.checked);
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
            let expected = type_ref_local_key_expand(&param.ty, &[], &[], ctx.checked);
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
