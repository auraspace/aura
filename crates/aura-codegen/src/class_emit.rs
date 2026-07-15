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
    let _ = writeln!(out, "typedef struct {} {{", c_class_type(&mono));
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
    let _ = writeln!(out, "}} {};\n", c_class_type(&mono));
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
        for iface in &c.implements {
            let _ = writeln!(
                out,
                "{} {}({} v);",
                c_iface_type(&iface.name),
                c_upcast_name(&c.name.name, &iface.name),
                c_class_type(&mono)
            );
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
        for iface in &c.implements {
            emit_upcast(out, checked, c, &iface.name);
            out.push('\n');
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
    let ret = c_class_type(mono);
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
    let _ = writeln!(out, "  {} self;", c_class_type(mono));
    for f in &c.fields {
        let n = mangle_ident(&f.name.name);
        let _ = writeln!(out, "  self.{n} = {n};");
    }
    out.push_str("  return self;\n}\n");
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
    };
    for f in &c.fields {
        ctx.define_local(&f.name.name, type_ref_local_key(&f.ty, params, args));
    }
    for p in &m.params {
        ctx.define_local(&p.name.name, type_ref_local_key(&p.ty, params, args));
    }
    emit_block(out, &m.body, 1, &mut ctx);
    emit_return_fallback(out, &m.return_type, checked, params, args);
    out.push_str("}\n");
}
