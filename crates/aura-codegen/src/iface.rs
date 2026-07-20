//! Interface upcasts and dispatch tables.

use std::fmt::Write as _;

use aura_ast::*;
use aura_sema::{CheckedFile, Ty};

use crate::names::*;

/// `iface_simple` is the interface simple name (from AST implements).
pub(crate) fn implementors<'a>(checked: &'a CheckedFile, iface_simple: &str) -> Vec<&'a ClassDecl> {
    checked
        .ast
        .classes
        .iter()
        .filter(|c| c.implements.iter().any(|i| i.name.name == iface_simple))
        .collect()
}

/// Prefer ClassSig implements keys (package-qualified) when available.
/// For mono ifaces, `args` must match the implemented InterfaceApp args.
pub(crate) fn implementors_for_iface<'a>(
    checked: &'a CheckedFile,
    iface: &InterfaceDecl,
    args: &[Ty],
) -> Vec<&'a ClassDecl> {
    let simple = iface.name.name.as_str();
    let pkg = iface_decl_package(iface, checked);
    let key = aura_sema::nominal_key(&pkg, simple);
    let from_sig: Vec<&str> = checked
        .classes
        .iter()
        .filter(|cs| {
            cs.implements.iter().any(|imp| match imp {
                Ty::Interface(k) if args.is_empty() => {
                    k == &key || k == simple || aura_sema::split_nominal(k).0 == simple
                }
                Ty::InterfaceApp {
                    name: k,
                    args: iargs,
                } if !args.is_empty() => {
                    (k == &key || aura_sema::split_nominal(k).0 == simple) && iargs == args
                }
                _ => false,
            })
        })
        .map(|cs| cs.name.as_str())
        .collect();
    if !from_sig.is_empty() {
        return checked
            .ast
            .classes
            .iter()
            .filter(|c| {
                from_sig.contains(&c.name.name.as_str())
                    && c.implements.iter().any(|i| {
                        i.name.name == simple
                            && if args.is_empty() {
                                i.type_args.is_empty()
                            } else {
                                // best-effort: type arg count match
                                i.type_args.len() == args.len()
                            }
                    })
            })
            .collect();
    }
    if args.is_empty() {
        implementors(checked, simple)
    } else {
        implementors(checked, simple)
            .into_iter()
            .filter(|c| {
                c.implements
                    .iter()
                    .any(|i| i.name.name == simple && i.type_args.len() == args.len())
            })
            .collect()
    }
}

pub(crate) fn emit_upcast(
    out: &mut String,
    checked: &CheckedFile,
    class: &ClassDecl,
    iface: &InterfaceDecl,
    args: &[Ty],
) {
    let pkg = class_decl_package(class, checked);
    let mono = type_mono(&pkg, &class.name.name, &[]);
    let imono = iface_mono_args(iface, checked, args);
    let param_ty = if is_heap_class_decl(class) {
        format!("{} *", c_class_type(&mono))
    } else {
        c_class_type(&mono)
    };
    let _ = writeln!(
        out,
        "{} {}({param_ty} v) {{",
        c_iface_type(&imono),
        c_upcast_name(&mono, &imono),
    );
    let _ = writeln!(out, "  {} i;", c_iface_type(&imono));
    let _ = writeln!(out, "  i.tag = AURA_TAG_{mono};");
    let _ = writeln!(out, "  i.data.as_{mono} = v;");
    out.push_str("  return i;\n}\n");
}

pub(crate) fn emit_iface_dispatch(
    out: &mut String,
    checked: &CheckedFile,
    iface: &InterfaceDecl,
    m: &MethodSig,
    args: &[Ty],
) {
    let imono = iface_mono_args(iface, checked, args);
    let tparams: Vec<String> = iface
        .type_params
        .iter()
        .map(|p| p.name.name.clone())
        .collect();
    let _ = writeln!(
        out,
        "{} {{",
        c_iface_method_signature_args(&imono, m, checked, &tparams, args)
    );
    let ret = c_type_from_opt_subst(&m.return_type, checked, &tparams, args);
    out.push_str("  switch (self->tag) {\n");
    for c in implementors_for_iface(checked, iface, args) {
        let pkg = class_decl_package(c, checked);
        let mono = type_mono(&pkg, &c.name.name, &[]);
        let margs = m
            .params
            .iter()
            .map(|p| mangle_ident(&p.name.name))
            .collect::<Vec<_>>();
        // Heap class: union holds pointer already; struct would need & (none today).
        let this_e = if is_heap_class_decl(c) {
            format!("self->data.as_{mono}")
        } else {
            format!("&self->data.as_{mono}")
        };
        let call_args = if margs.is_empty() {
            this_e
        } else {
            format!("{this_e}, {}", margs.join(", "))
        };
        if ret == "void" {
            let _ = writeln!(
                out,
                "  case AURA_TAG_{mono}: {}({}); return;",
                c_method_name(&mono, &m.name.name),
                call_args
            );
        } else {
            let _ = writeln!(
                out,
                "  case AURA_TAG_{mono}: return {}({});",
                c_method_name(&mono, &m.name.name),
                call_args
            );
        }
    }
    out.push_str("  default:\n");
    if ret == "void" {
        out.push_str("    return;\n");
    } else if ret == "int64_t" {
        out.push_str("    return 0;\n");
    } else if ret == "bool" {
        out.push_str("    return false;\n");
    } else if ret == "const char *" {
        out.push_str("    return \"\";\n");
    } else {
        let _ = writeln!(out, "    return ({ret}){{0}};");
    }
    out.push_str("  }\n}\n");
}

pub(crate) fn c_iface_method_signature(
    iface_mono: &str,
    m: &MethodSig,
    checked: &CheckedFile,
) -> String {
    c_iface_method_signature_args(iface_mono, m, checked, &[], &[])
}

pub(crate) fn c_iface_method_signature_args(
    iface_mono: &str,
    m: &MethodSig,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
) -> String {
    let ret = c_type_from_opt_subst(&m.return_type, checked, params, args);
    let mut cparams = vec![format!("{} *self", c_iface_type(iface_mono))];
    for p in &m.params {
        cparams.push(format!(
            "{} {}",
            c_type_ref_subst(&p.ty, checked, params, args),
            mangle_ident(&p.name.name)
        ));
    }
    format!(
        "{ret} {}({})",
        c_iface_method_name(iface_mono, &m.name.name),
        cparams.join(", ")
    )
}

fn c_type_from_opt_subst(
    ty: &Option<TypeRef>,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
) -> String {
    match ty {
        None => "void".into(),
        Some(t) => c_type_ref_subst(t, checked, params, args),
    }
}
