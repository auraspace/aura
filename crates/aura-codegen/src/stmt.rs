//! Statement and block emission.

use std::fmt::Write as _;

use aura_ast::*;
use aura_sema::{CheckedFile, Ty};

use crate::ctx::EmitCtx;
use crate::expr::{coerce_expr, emit_expr, infer_type_name, mono_base_name};
use crate::names::*;

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
            } else if ct.starts_with("aura_cls_") {
                let _ = writeln!(out, "  return ({ct}){{0}}; /* fallback */");
            } else if ct.starts_with("aura_iface_") {
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
    ctx.pop_scope();
}

pub(crate) fn pad(n: usize) -> String {
    "  ".repeat(n)
}

pub(crate) fn emit_stmt(out: &mut String, stmt: &Stmt, indent: usize, ctx: &mut EmitCtx<'_>) {
    let p = pad(indent);
    match stmt {
        Stmt::Var(v) => {
            let ty_name = v
                .ty
                .as_ref()
                .map(|t| {
                    type_ref_local_key(t, &ctx.type_params, &ctx.type_args)
                })
                .unwrap_or_else(|| infer_type_name(&v.init, ctx));
            let ty = v
                .ty
                .as_ref()
                .map(|t| {
                    c_type_ref_subst(t, ctx.checked, &ctx.type_params, &ctx.type_args)
                })
                .unwrap_or_else(|| local_key_to_c(&ty_name, ctx.checked));
            ctx.define_local(&v.name.name, ty_name.clone());
            let init = coerce_expr(&v.init, &ty_name, ctx);
            let _ = writeln!(
                out,
                "{p}{ty} {} = {init};",
                mangle_ident(&v.name.name)
            );
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
            ctx.pop_scope();
            let _ = writeln!(out, "{p}  }}");
            let _ = writeln!(out, "{p}}}");
        }
        Stmt::ForIn(f) => {
            // for (x in arr) → index loop + Array_get (C3k).
            let iter_key = infer_type_name(&f.iterable, ctx);
            let mono = if iter_key.starts_with("Array_") {
                iter_key.clone()
            } else if iter_key == "Array" {
                // Should not happen after sema; fall back.
                "Array_Int".into()
            } else {
                iter_key.clone()
            };
            let elem_key = mono
                .strip_prefix("Array_")
                .unwrap_or("Int")
                .to_string();
            let arr_c = local_key_to_c(&mono, ctx.checked);
            let elem_c = local_key_to_c(&elem_key, ctx.checked);
            let iter_e = emit_expr(&f.iterable, ctx);
            let it_tmp = format!("__for_it_{}", f.span.start);
            let idx_tmp = format!("__for_i_{}", f.span.start);
            let bind = mangle_ident(&f.name.name);
            let get_fn = c_method_name(&mono, "get");
            let _ = writeln!(out, "{p}{{");
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
            ctx.pop_scope();
            let _ = writeln!(out, "{p}  }}");
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
                    // C3g: class/struct value — heap-copy then throw pointer + type name.
                    let c_ty = local_key_to_c(other, ctx.checked);
                    let tmp = format!("__throw_v_{}", t.span.start);
                    let ptr = format!("__throw_p_{}", t.span.start);
                    let _ = writeln!(out, "{p}{{");
                    let _ = writeln!(out, "{p}  {c_ty} {tmp} = {val};");
                    let _ = writeln!(
                        out,
                        "{p}  {c_ty} *{ptr} = ({c_ty} *)malloc(sizeof({c_ty}));"
                    );
                    let _ = writeln!(out, "{p}  *{ptr} = {tmp};");
                    // Match key uses the Aura type name (mono key), not C typedef.
                    let _ = writeln!(out, "{p}  aura_throw_obj(\"{other}\", {ptr});");
                    let _ = writeln!(out, "{p}}}");
                }
            }
        }
        Stmt::Try(t) => emit_try(out, t, indent, ctx),
        Stmt::Return(r) => match &r.value {
            None => {
                let _ = writeln!(out, "{p}return;");
            }
            Some(e) => {
                let _ = writeln!(out, "{p}return {};", emit_expr(e, ctx));
            }
        },
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
        n if checked.ast.interfaces.iter().any(|i| i.name.name == n) => c_iface_type(n),
        n if is_enum_name(checked, n) => c_enum_type(n),
        n if checked
            .mono_enums
            .iter()
            .any(|(name, args)| mono_key(name, args) == n) =>
        {
            c_enum_type(n)
        }
        n => c_class_type(n),
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
        let catch_key = type_ref_local_key(
            &c.ty,
            &ctx.type_params,
            &ctx.type_args,
        );
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
                let c_ty = local_key_to_c(other, ctx.checked);
                let _ = writeln!(
                    out,
                    "{p}      {c_ty} {bind} = *({c_ty} *)aura_ex_as_obj();"
                );
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

    let enum_decl = ctx
        .checked
        .ast
        .enums
        .iter()
        .find(|e| e.name.name == ename);

    for arm in &m.arms {
        let Pattern::Variant {
            name,
            bindings,
            ..
        } = &arm.pattern;
        let tag = enum_decl
            .and_then(|e| e.variants.iter().position(|v| v.name.name == name.name))
            .unwrap_or(0);
        let _ = writeln!(out, "{p}  case {tag}: {{");
        ctx.push_scope();
        if let Some(e) = enum_decl {
            if let Some(v) = e.variants.iter().find(|v| v.name.name == name.name) {
                let params: Vec<String> =
                    e.type_params.iter().map(|p| p.name.name.clone()).collect();
                let targs: Vec<Ty> = ctx
                    .checked
                    .mono_enums
                    .iter()
                    .find(|(n, a)| mono_key(n, a) == scrut_key)
                    .map(|(_, a)| a.clone())
                    .unwrap_or_default();
                for (bind, field) in bindings.iter().zip(v.fields.iter()) {
                    let fty = type_ref_local_key(&field.ty, &params, &targs);
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
