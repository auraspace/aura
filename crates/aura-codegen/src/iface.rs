//! Interface upcasts and dispatch tables.

use std::fmt::Write as _;

use aura_ast::*;
use aura_sema::CheckedFile;

use crate::names::*;

pub(crate) fn implementors<'a>(checked: &'a CheckedFile, iface: &str) -> Vec<&'a ClassDecl> {
    checked
        .ast
        .classes
        .iter()
        .filter(|c| c.implements.iter().any(|i| i.name == iface))
        .collect()
}

pub(crate) fn emit_upcast(out: &mut String, checked: &CheckedFile, class: &ClassDecl, iface: &str) {
    let pkg = class_decl_package(class, checked);
    let mono = type_mono(&pkg, &class.name.name, &[]);
    let simple = &class.name.name;
    let param_ty = if is_heap_class_decl(class) {
        format!("{} *", c_class_type(&mono))
    } else {
        c_class_type(&mono)
    };
    let _ = writeln!(
        out,
        "{} {}({param_ty} v) {{",
        c_iface_type(iface),
        c_upcast_name(simple, iface),
    );
    let _ = writeln!(out, "  {} i;", c_iface_type(iface));
    let _ = writeln!(out, "  i.tag = AURA_TAG_{mono};");
    let _ = writeln!(out, "  i.data.as_{mono} = v;");
    out.push_str("  return i;\n}\n");
}

pub(crate) fn emit_iface_dispatch(
    out: &mut String,
    checked: &CheckedFile,
    iface: &str,
    m: &MethodSig,
) {
    let _ = writeln!(out, "{} {{", c_iface_method_signature(iface, m, checked));
    let ret = c_type_from_opt(&m.return_type, checked, &[], &[]);
    out.push_str("  switch (self->tag) {\n");
    for c in implementors(checked, iface) {
        let pkg = class_decl_package(c, checked);
        let mono = type_mono(&pkg, &c.name.name, &[]);
        let args = m
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
        let call_args = if args.is_empty() {
            this_e
        } else {
            format!("{this_e}, {}", args.join(", "))
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

pub(crate) fn c_iface_method_signature(iface: &str, m: &MethodSig, checked: &CheckedFile) -> String {
    let ret = c_type_from_opt(&m.return_type, checked, &[], &[]);
    let mut params = vec![format!("{} *self", c_iface_type(iface))];
    for p in &m.params {
        params.push(format!(
            "{} {}",
            c_type_ref(&p.ty, checked),
            mangle_ident(&p.name.name)
        ));
    }
    format!(
        "{ret} {}({})",
        c_iface_method_name(iface, &m.name.name),
        params.join(", ")
    )
}
