//! Enum typedefs and constructors.

use std::fmt::Write as _;

use aura_ast::*;
use aura_sema::{CheckedFile, Ty};

use crate::names::*;

fn enum_field_is_unit(
    field: &Param,
    params: &[String],
    args: &[Ty],
    _checked: &CheckedFile,
) -> bool {
    type_ref_local_key(&field.ty, params, args) == "Unit"
}

fn enum_field_c_type(
    field: &Param,
    params: &[String],
    args: &[Ty],
    checked: &CheckedFile,
) -> String {
    if enum_field_is_unit(field, params, args, checked) {
        // Unit has no C value. A byte keeps generic enum storage/layout valid;
        // constructors and match bindings treat it as an absent payload.
        "char".into()
    } else {
        c_type_ref_subst(&field.ty, checked, params, args)
    }
}

pub(crate) fn emit_enum_typedef(
    out: &mut String,
    checked: &CheckedFile,
    e: &EnumDecl,
    args: &[Ty],
) {
    let params: Vec<String> = e.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = enum_decl_package(e, checked);
    let mono = type_mono(&pkg, &e.name.name, args);
    let _ = writeln!(out, "typedef struct {} {{", c_enum_type(&mono));
    out.push_str("  int tag;\n  union {\n");
    for v in &e.variants {
        if v.fields.is_empty() {
            let _ = writeln!(out, "    char as_{};", mangle_ident(&v.name.name));
        } else {
            let _ = writeln!(out, "    struct {{");
            for f in &v.fields {
                let _ = writeln!(
                    out,
                    "      {} {};",
                    enum_field_c_type(f, &params, args, checked),
                    mangle_ident(&f.name.name)
                );
            }
            let _ = writeln!(out, "    }} {};", mangle_ident(&v.name.name));
        }
    }
    let _ = writeln!(out, "  }} data;\n}} {};\n", c_enum_type(&mono));
}

pub(crate) fn emit_enum_forwards(
    out: &mut String,
    checked: &CheckedFile,
    e: &EnumDecl,
    args: &[Ty],
) {
    let params: Vec<String> = e.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = enum_decl_package(e, checked);
    let mono = type_mono(&pkg, &e.name.name, args);
    for v in &e.variants {
        let _ = writeln!(
            out,
            "{};",
            c_variant_signature(e, v, checked, &params, args, &mono)
        );
    }
}

pub(crate) fn c_variant_signature(
    e: &EnumDecl,
    v: &EnumVariant,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
) -> String {
    let ret = c_enum_type(mono);
    let ps = if v.fields.is_empty()
        || v.fields
            .iter()
            .all(|f| enum_field_is_unit(f, params, args, checked))
    {
        "void".into()
    } else {
        v.fields
            .iter()
            .filter(|f| !enum_field_is_unit(f, params, args, checked))
            .map(|f| {
                format!(
                    "{} {}",
                    enum_field_c_type(f, params, args, checked),
                    mangle_ident(&f.name.name)
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let _ = e;
    format!("{ret} {}({ps})", c_variant_ctor_name(mono, &v.name.name))
}

pub(crate) fn emit_enum_defs(out: &mut String, checked: &CheckedFile, e: &EnumDecl, args: &[Ty]) {
    let params: Vec<String> = e.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = enum_decl_package(e, checked);
    let mono = type_mono(&pkg, &e.name.name, args);
    for (tag, v) in e.variants.iter().enumerate() {
        let _ = writeln!(
            out,
            "{} {{",
            c_variant_signature(e, v, checked, &params, args, &mono)
        );
        let _ = writeln!(out, "  {} self;", c_enum_type(&mono));
        let _ = writeln!(out, "  self.tag = {tag};");
        for f in &v.fields {
            if enum_field_is_unit(f, &params, args, checked) {
                continue;
            }
            let n = mangle_ident(&f.name.name);
            let _ = writeln!(
                out,
                "  self.data.{}.{} = {};",
                mangle_ident(&v.name.name),
                n,
                n
            );
        }
        out.push_str("  return self;\n}\n");
    }
}
