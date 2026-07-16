//! Class/struct typedefs, constructors, and methods.

use std::collections::HashMap;
use std::fmt::Write as _;

use aura_ast::*;
use aura_sema::{CheckedFile, Ty};

use crate::ctx::EmitCtx;
use crate::iface::emit_upcast;
use crate::names::*;
use crate::stmt::{emit_block, emit_return_fallback};

pub(crate) fn emit_class_typedef(out: &mut String, checked: &CheckedFile, c: &ClassDecl, args: &[Ty]) {
    let params: Vec<String> = c.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = class_decl_package(c, checked);
    let mono = type_mono(&pkg, &c.name.name, args);
    // Body only — incomplete `typedef struct X X` may already exist (C4u forwards).
    let _ = writeln!(out, "struct {} {{", c_class_type(&mono));
    for f in &c.fields {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&f.ty, checked, &params, args),
            mangle_ident(&f.name.name)
        );
    }
    if c.fields.is_empty() {
        out.push_str("  char _pad;\n");
    }
    out.push_str("};\n\n");
}

pub(crate) fn emit_class_forwards(out: &mut String, checked: &CheckedFile, c: &ClassDecl, args: &[Ty]) {
    let params: Vec<String> = c.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = class_decl_package(c, checked);
    let mono = type_mono(&pkg, &c.name.name, args);
    let _ = writeln!(out, "{};", c_ctor_signature_mono(c, checked, &params, args, &mono));
    for m in &c.methods {
        let _ = writeln!(
            out,
            "{};",
            c_method_signature_mono(c, m, checked, &params, args, &mono)
        );
    }
    if args.is_empty() {
        for iface_id in &c.implements {
            if let Some(iface) = checked
                .ast
                .interfaces
                .iter()
                .find(|i| i.name.name == iface_id.name)
            {
                let imono = iface_mono(iface, checked);
                let param_ty = if is_heap_class_decl(c) {
                    format!("{} *", c_class_type(&mono))
                } else {
                    c_class_type(&mono)
                };
                let _ = writeln!(
                    out,
                    "{} {}({param_ty} v);",
                    c_iface_type(&imono),
                    c_upcast_name(&mono, &imono),
                );
            }
        }
    }
}

pub(crate) fn emit_class_defs(out: &mut String, checked: &CheckedFile, c: &ClassDecl, args: &[Ty]) {
    let params: Vec<String> = c.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = class_decl_package(c, checked);
    let mono = type_mono(&pkg, &c.name.name, args);
    emit_ctor_mono(out, c, checked, &params, args, &mono);
    out.push('\n');
    for m in &c.methods {
        emit_method_mono(out, c, m, checked, &params, args, &mono);
        out.push('\n');
    }
    if args.is_empty() {
        for iface_id in &c.implements {
            if let Some(iface) = checked
                .ast
                .interfaces
                .iter()
                .find(|i| i.name.name == iface_id.name)
            {
                emit_upcast(out, checked, c, iface);
                out.push('\n');
            }
        }
    }
}

pub(crate) fn c_ctor_signature_mono(
    c: &ClassDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
) -> String {
    let ret = if is_heap_class_decl(c) {
        format!("{} *", c_class_type(mono))
    } else {
        c_class_type(mono)
    };
    let ps = if c.fields.is_empty() {
        "void".into()
    } else {
        c.fields
            .iter()
            .map(|f| {
                format!(
                    "{} {}",
                    c_type_ref_subst(&f.ty, checked, params, args),
                    mangle_ident(&f.name.name)
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!("{ret} {}({ps})", c_ctor_name(mono))
}

pub(crate) fn c_method_signature_mono(
    c: &ClassDecl,
    m: &FunDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
) -> String {
    let _ = c;
    let ret = c_type_from_opt(&m.return_type, checked, params, args);
    let mut ps = vec![format!("{} *this", c_class_type(mono))];
    for p in &m.params {
        ps.push(format!(
            "{} {}",
            c_type_ref_subst(&p.ty, checked, params, args),
            mangle_ident(&p.name.name)
        ));
    }
    format!("{ret} {}({})", c_method_name(mono, &m.name.name), ps.join(", "))
}

pub(crate) fn c_fun_signature(f: &FunDecl, checked: &CheckedFile, args: &[Ty]) -> String {
    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let ret = c_type_from_opt(&f.return_type, checked, &params, args);
    let ps = if f.params.is_empty() {
        "void".into()
    } else {
        f.params
            .iter()
            .map(|p| {
                format!(
                    "{} {}",
                    c_type_ref_subst(&p.ty, checked, &params, args),
                    mangle_ident(&p.name.name)
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let pkg = fun_decl_package(f, checked);
    format!("{ret} {}({ps})", c_fun_name(&pkg, &f.name.name, args))
}

pub(crate) fn emit_ctor_mono(
    out: &mut String,
    c: &ClassDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
) {
    let _ = writeln!(
        out,
        "{} {{",
        c_ctor_signature_mono(c, checked, params, args, mono)
    );
    let cty = c_class_type(mono);
    if is_heap_class_decl(c) {
        // C3y: allocate class instance on GC heap.
        let _ = writeln!(
            out,
            "  {cty} *self = ({cty} *)aura_gc_alloc(sizeof({cty}));"
        );
        for f in &c.fields {
            let n = mangle_ident(&f.name.name);
            let _ = writeln!(out, "  self->{n} = {n};");
        }
        out.push_str("  return self;\n}\n");
    } else {
        let _ = writeln!(out, "  {cty} self;");
        for f in &c.fields {
            let n = mangle_ident(&f.name.name);
            let _ = writeln!(out, "  self.{n} = {n};");
        }
        out.push_str("  return self;\n}\n");
    }
}

pub(crate) fn emit_method_mono(
    out: &mut String,
    c: &ClassDecl,
    m: &FunDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
) {
    let _ = writeln!(
        out,
        "{} {{",
        c_method_signature_mono(c, m, checked, params, args, mono)
    );
    let mut ctx = EmitCtx {
        checked,
        method_class: Some(mono),
        type_params: params.to_vec(),
        type_args: args.to_vec(),
        locals: vec![HashMap::new()],
        array_owners: vec![std::collections::HashSet::new()],
        gc_roots: vec![std::collections::HashSet::new()],
    };
    for f in &c.fields {
        ctx.define_local(&f.name.name, type_ref_local_key(&f.ty, params, args));
    }
    for p in &m.params {
        let key = type_ref_local_key(&p.ty, params, args);
        ctx.define_local(&p.name.name, key.clone());
        let mono = crate::expr::full_type_mono(&key, checked);
        if is_heap_class_mono(&mono, checked) {
            ctx.mark_gc_root(&p.name.name);
            let n = mangle_ident(&p.name.name);
            let _ = writeln!(out, "  aura_gc_add_root((void **)&{n});");
        }
    }
    // `this` is a heap pointer for classes — root it for the method body.
    if is_heap_class_mono(mono, checked) {
        ctx.mark_gc_root("this");
        out.push_str("  aura_gc_add_root((void **)&this);\n");
    }
    emit_block(out, &m.body, 1, &mut ctx);
    for name in ctx.gc_roots_all() {
        let n = if name == "this" {
            "this".to_string()
        } else {
            mangle_ident(&name)
        };
        let _ = writeln!(out, "  aura_gc_remove_root((void **)&{n});");
    }
    emit_return_fallback(out, &m.return_type, checked, params, args);
    out.push_str("}\n");
}
