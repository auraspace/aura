//! Interface upcasts and dispatch tables.

use std::fmt::Write as _;

use aura_ast::*;
use aura_sema::{CheckedFile, Ty};

use crate::names::*;

/// One concrete implementor monomorph for an interface (C9a: class type args).
#[derive(Debug, Clone)]
pub(crate) struct MonoImplementor<'a> {
    pub class: &'a ClassDecl,
    pub class_args: Vec<Ty>,
}

/// `iface_simple` is the interface simple name (from AST implements).
pub(crate) fn implementors<'a>(checked: &'a CheckedFile, iface_simple: &str) -> Vec<&'a ClassDecl> {
    checked
        .ast
        .classes
        .iter()
        .filter(|c| c.implements.iter().any(|i| i.name.name == iface_simple))
        .collect()
}

/// Resolve AST type-ref args on implements into `Ty` (best-effort for concrete names).
fn type_ref_to_ty_open(t: &TypeRef, checked: &CheckedFile, class_tparams: &[String]) -> Option<Ty> {
    let name = t.name.name.as_str();
    if class_tparams.iter().any(|p| p == name) {
        return Some(Ty::TypeParam(name.to_string()));
    }
    match name {
        "Int" => Some(Ty::Int),
        "Bool" => Some(Ty::Bool),
        "String" => Some(Ty::String),
        "Unit" => Some(Ty::Unit),
        other => {
            // Prefer ClassSig / interface / enum via checked lists.
            if let Some(cs) = checked.classes.iter().find(|c| c.name == other) {
                let key = aura_sema::nominal_key(&cs.package, other);
                if t.type_args.is_empty() {
                    return Some(Ty::Class(key));
                }
                let args: Option<Vec<Ty>> = t
                    .type_args
                    .iter()
                    .map(|a| type_ref_to_ty_open(a, checked, class_tparams))
                    .collect();
                return args.map(|args| Ty::ClassApp { name: key, args });
            }
            if let Some(is) = checked.interfaces.iter().find(|i| i.name == other) {
                let key = aura_sema::nominal_key(&is.package, other);
                if t.type_args.is_empty() {
                    return Some(Ty::Interface(key));
                }
                let args: Option<Vec<Ty>> = t
                    .type_args
                    .iter()
                    .map(|a| type_ref_to_ty_open(a, checked, class_tparams))
                    .collect();
                return args.map(|args| Ty::InterfaceApp { name: key, args });
            }
            let pkg = checked
                .ast
                .classes
                .iter()
                .find(|c| c.name.name == other)
                .map(|c| class_decl_package(c, checked))
                .unwrap_or_default();
            Some(Ty::Class(aura_sema::nominal_key(&pkg, other)))
        }
    }
}

fn implements_matches_after_subst(
    cs: &aura_sema::ClassSig,
    class_args: &[Ty],
    iface_key: &str,
    iface_simple: &str,
    target_args: &[Ty],
) -> bool {
    let map = if !class_args.is_empty() && cs.type_params.len() == class_args.len() {
        Some(aura_sema::type_subst_map(&cs.type_params, class_args))
    } else {
        None
    };
    cs.implements.iter().any(|imp| {
        let concrete = if let Some(ref m) = map {
            aura_sema::subst_ty(imp, m)
        } else {
            imp.clone()
        };
        match &concrete {
            Ty::Interface(k) if target_args.is_empty() => {
                k == iface_key || k == iface_simple || aura_sema::split_nominal(k).0 == iface_simple
            }
            Ty::InterfaceApp { name: k, args: ia } if !target_args.is_empty() => {
                (k == iface_key || aura_sema::split_nominal(k).0 == iface_simple)
                    && ia == target_args
            }
            _ => false,
        }
    })
}

/// Concrete monomorphs that implement `iface` with the given type args (C8c/C9a).
pub(crate) fn mono_implementors_for_iface<'a>(
    checked: &'a CheckedFile,
    iface: &InterfaceDecl,
    args: &[Ty],
) -> Vec<MonoImplementor<'a>> {
    let simple = iface.name.name.as_str();
    let pkg = iface_decl_package(iface, checked);
    let key = aura_sema::nominal_key(&pkg, simple);
    let mut out: Vec<MonoImplementor<'a>> = Vec::new();

    for cs in &checked.classes {
        if cs.is_struct {
            continue;
        }
        if cs.type_params.is_empty() {
            if implements_matches_after_subst(cs, &[], &key, simple, args) {
                if let Some(c) = checked.ast.classes.iter().find(|c| {
                    c.name.name == cs.name
                        && class_decl_package(c, checked) == cs.package
                        && c.kind == NominalKind::Class
                }) {
                    out.push(MonoImplementor {
                        class: c,
                        class_args: Vec::new(),
                    });
                }
            }
        } else {
            // C9a: each concrete mono of this generic class.
            for (name, cargs) in &checked.mono_classes {
                if name != &cs.name {
                    continue;
                }
                if cargs.len() != cs.type_params.len() {
                    continue;
                }
                if implements_matches_after_subst(cs, cargs, &key, simple, args) {
                    if let Some(c) = checked.ast.classes.iter().find(|c| {
                        c.name.name == cs.name
                            && c.kind == NominalKind::Class
                            && c.type_params.len() == cs.type_params.len()
                    }) {
                        // Avoid duplicates if same class multi-package (rare).
                        let already = out
                            .iter()
                            .any(|m| m.class.name.name == c.name.name && m.class_args == *cargs);
                        if !already {
                            out.push(MonoImplementor {
                                class: c,
                                class_args: cargs.clone(),
                            });
                        }
                    }
                }
            }
        }
    }

    // Fallback: AST implements only (non-generic path when ClassSig missing).
    if out.is_empty() {
        for c in implementors(checked, simple) {
            if !c.type_params.is_empty() {
                continue;
            }
            let ok = c.implements.iter().any(|i| {
                i.name.name == simple
                    && if args.is_empty() {
                        i.type_args.is_empty()
                    } else {
                        i.type_args.len() == args.len()
                    }
            });
            if ok {
                out.push(MonoImplementor {
                    class: c,
                    class_args: Vec::new(),
                });
            }
        }
    }
    out
}

/// Prefer ClassSig implements keys (package-qualified) when available.
/// For mono ifaces, `args` must match the implemented InterfaceApp args.
pub(crate) fn implementors_for_iface<'a>(
    checked: &'a CheckedFile,
    iface: &InterfaceDecl,
    args: &[Ty],
) -> Vec<&'a ClassDecl> {
    mono_implementors_for_iface(checked, iface, args)
        .into_iter()
        .map(|m| m.class)
        .collect()
}

pub(crate) fn emit_upcast(
    out: &mut String,
    checked: &CheckedFile,
    class: &ClassDecl,
    iface: &InterfaceDecl,
    iface_args: &[Ty],
    class_args: &[Ty],
) {
    let pkg = class_decl_package(class, checked);
    let mono = type_mono(&pkg, &class.name.name, class_args);
    let imono = iface_mono_args(iface, checked, iface_args);
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

/// Resolve implements type args on AST for a mono class (subst class type params).
pub(crate) fn iface_args_for_class_implements(
    class: &ClassDecl,
    iface_ref: &TypeRef,
    checked: &CheckedFile,
    class_args: &[Ty],
) -> Vec<Ty> {
    let tparams: Vec<String> = class
        .type_params
        .iter()
        .map(|p| p.name.name.clone())
        .collect();
    let map = if !class_args.is_empty() && tparams.len() == class_args.len() {
        Some(aura_sema::type_subst_map(&tparams, class_args))
    } else {
        None
    };
    iface_ref
        .type_args
        .iter()
        .filter_map(|t| {
            let open = type_ref_to_ty_open(t, checked, &tparams)?;
            Some(if let Some(ref m) = map {
                aura_sema::subst_ty(&open, m)
            } else {
                open
            })
        })
        .collect()
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
    for imp in mono_implementors_for_iface(checked, iface, args) {
        let pkg = class_decl_package(imp.class, checked);
        let mono = type_mono(&pkg, &imp.class.name.name, &imp.class_args);
        let margs = m
            .params
            .iter()
            .map(|p| mangle_ident(&p.name.name))
            .collect::<Vec<_>>();
        // Heap class: union holds pointer already; struct would need & (none today).
        let this_e = if is_heap_class_decl(imp.class) {
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
