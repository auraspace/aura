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

fn task_result_string_ok(e: &EnumDecl, pkg: &str, args: &[Ty], variant: &str) -> bool {
    pkg == "std.io"
        && e.name.name == "Result"
        && variant == "Ok"
        && matches!(args, [Ty::String, Ty::Enum(name)] if name == "TaskError@std.io")
}

fn task_result_foreign_handle_ok(e: &EnumDecl, pkg: &str, args: &[Ty], variant: &str) -> bool {
    pkg == "std.io"
        && e.name.name == "Result"
        && variant == "Ok"
        && matches!(args, [Ty::ForeignHandle(_), Ty::Enum(name)] if name == "TaskError@std.io")
}

fn task_result_class_ok(
    e: &EnumDecl,
    pkg: &str,
    args: &[Ty],
    variant: &str,
    checked: &CheckedFile,
) -> bool {
    pkg == "std.io"
        && e.name.name == "Result"
        && variant == "Ok"
        && args.first().is_some_and(|ty| {
            matches!(ty, Ty::Class(_) | Ty::ClassApp { .. })
                && c_type_from_ty(ty, checked).trim_end().ends_with('*')
        })
}

fn task_result_enum_ok(e: &EnumDecl, pkg: &str, args: &[Ty], variant: &str) -> bool {
    pkg == "std.io"
        && e.name.name == "Result"
        && variant == "Ok"
        && args
            .first()
            .is_some_and(|ty| matches!(ty, Ty::Enum(_) | Ty::EnumApp { .. }))
}

fn shared_outcome_string_ok(e: &EnumDecl, pkg: &str, args: &[Ty], variant: &str) -> bool {
    pkg == "std.error"
        && e.name.name == "Outcome"
        && variant == "OutcomeOk"
        && matches!(args.first(), Some(Ty::String))
}

fn shared_outcome_error_class(
    e: &EnumDecl,
    pkg: &str,
    args: &[Ty],
    variant: &str,
    checked: &CheckedFile,
) -> bool {
    pkg == "std.error"
        && e.name.name == "Outcome"
        && variant == "OutcomeErr"
        && args.get(1).is_some_and(|ty| {
            matches!(ty, Ty::Class(_) | Ty::ClassApp { .. })
                && c_type_from_ty(ty, checked).trim_end().ends_with('*')
        })
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
    let task_error = pkg == "std.io" && e.name.name == "TaskError";
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
            if task_error && v.name.name == "Failed" {
                out.push_str("      bool owned;\n");
            }
            if task_result_string_ok(e, &pkg, args, &v.name.name) {
                out.push_str("      bool owned;\n");
            }
            if task_result_foreign_handle_ok(e, &pkg, args, &v.name.name) {
                out.push_str("      bool owned;\n");
            }
            if task_result_class_ok(e, &pkg, args, &v.name.name, checked) {
                out.push_str("      bool owned;\n");
            }
            if task_result_enum_ok(e, &pkg, args, &v.name.name) {
                out.push_str("      bool owned;\n");
            }
            if shared_outcome_string_ok(e, &pkg, args, &v.name.name) {
                out.push_str("      bool owned;\n");
            }
            if shared_outcome_error_class(e, &pkg, args, &v.name.name, checked) {
                out.push_str("      bool owned;\n");
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
    let cty = c_enum_type(&mono);
    let _ = writeln!(out, "{cty} {cty}_clone(const {cty} *source);");
    let _ = writeln!(out, "void {cty}_drop({cty} *value);");
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
    let task_error = pkg == "std.io" && e.name.name == "TaskError";
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
            if task_error && v.name.name == "Failed" {
                let _ = writeln!(
                    out,
                    "  self.data.{}.owned = false;",
                    mangle_ident(&v.name.name)
                );
            }
            if task_result_string_ok(e, &pkg, args, &v.name.name) {
                let _ = writeln!(out, "  self.data.Ok.owned = false;");
            }
            if task_result_foreign_handle_ok(e, &pkg, args, &v.name.name) {
                let _ = writeln!(out, "  self.data.Ok.owned = false;");
            }
            if task_result_class_ok(e, &pkg, args, &v.name.name, checked) {
                let _ = writeln!(out, "  self.data.Ok.owned = false;");
            }
            if task_result_enum_ok(e, &pkg, args, &v.name.name) {
                let _ = writeln!(out, "  self.data.Ok.owned = false;");
            }
            if shared_outcome_string_ok(e, &pkg, args, &v.name.name) {
                let _ = writeln!(out, "  self.data.OutcomeOk.owned = false;");
            }
            if shared_outcome_error_class(e, &pkg, args, &v.name.name, checked) {
                let _ = writeln!(out, "  self.data.OutcomeErr.owned = false;");
            }
        }
        out.push_str("  return self;\n}\n");
    }
    if task_error {
        let ctor = c_variant_ctor_name(&mono, "FailedOwned");
        let _ = writeln!(
            out,
            "{} {}(const char *error) {{ {} self; self.tag = 0; self.data.Failed.error = error; self.data.Failed.owned = true; return self; }}",
            c_enum_type(&mono),
            ctor,
            c_enum_type(&mono)
        );
    }
    if task_result_string_ok(e, &pkg, args, "Ok") {
        let ctor = c_variant_ctor_name(&mono, "OkOwned");
        let _ = writeln!(
            out,
            "{} {}(const char *value) {{ {} self; self.tag = 0; self.data.Ok.value = value; self.data.Ok.owned = true; return self; }}",
            c_enum_type(&mono),
            ctor,
            c_enum_type(&mono)
        );
    }
    if task_result_foreign_handle_ok(e, &pkg, args, "Ok") {
        let ctor = c_variant_ctor_name(&mono, "OkOwned");
        let _ = writeln!(
            out,
            "{} {}(AuraFfiOpaqueHandle *value) {{ {} self; self.tag = 0; self.data.Ok.value = value; self.data.Ok.owned = true; return self; }}",
            c_enum_type(&mono),
            ctor,
            c_enum_type(&mono)
        );
    }
    if task_result_class_ok(e, &pkg, args, "Ok", checked) {
        let ctor = c_variant_ctor_name(&mono, "OkOwned");
        let _ = writeln!(
            out,
            "{} {}({} value) {{ {} self; self.tag = 0; self.data.Ok.value = value; self.data.Ok.owned = true; return self; }}",
            c_enum_type(&mono),
            ctor,
            c_type_from_ty(&args[0], checked),
            c_enum_type(&mono)
        );
    }
    if shared_outcome_string_ok(e, &pkg, args, "OutcomeOk") {
        let ctor = c_variant_ctor_name(&mono, "OutcomeOkOwned");
        let _ = writeln!(
            out,
            "{} {}(const char *value) {{ {} self; self.tag = 0; size_t __len = value == NULL ? 0 : strlen(value); char *__copy = (char *)malloc(__len + 1); if (__copy == NULL) abort(); if (value != NULL) memcpy(__copy, value, __len + 1); else __copy[0] = '\\0'; self.data.OutcomeOk.value = __copy; self.data.OutcomeOk.owned = true; return self; }}",
            c_enum_type(&mono),
            ctor,
            c_enum_type(&mono)
        );
    }
    emit_enum_clone_drop(out, checked, e, args);
}

fn variant_has_owned_field(
    e: &EnumDecl,
    pkg: &str,
    args: &[Ty],
    variant: &str,
    checked: &CheckedFile,
) -> bool {
    task_error_variant(e, pkg, variant)
        || task_result_string_ok(e, pkg, args, variant)
        || task_result_foreign_handle_ok(e, pkg, args, variant)
        || task_result_class_ok(e, pkg, args, variant, checked)
        || task_result_enum_ok(e, pkg, args, variant)
        || shared_outcome_string_ok(e, pkg, args, variant)
        || shared_outcome_error_class(e, pkg, args, variant, checked)
}

fn task_error_variant(e: &EnumDecl, pkg: &str, variant: &str) -> bool {
    pkg == "std.io" && e.name.name == "TaskError" && variant == "Failed"
}

fn emit_enum_clone_drop(out: &mut String, checked: &CheckedFile, e: &EnumDecl, args: &[Ty]) {
    let params: Vec<String> = e.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = enum_decl_package(e, checked);
    let mono = type_mono(&pkg, &e.name.name, args);
    let cty = c_enum_type(&mono);
    let _ = writeln!(out, "{cty} {cty}_clone(const {cty} *source) {{");
    let _ = writeln!(
        out,
        "  {cty} copy = source == NULL ? ({cty}){{0}} : *source;"
    );
    out.push_str("  if (source == NULL) return copy;\n  switch (source->tag) {\n");
    for (tag, variant) in e.variants.iter().enumerate() {
        let vn = mangle_ident(&variant.name.name);
        let _ = writeln!(out, "    case {tag}: {{");
        for field in &variant.fields {
            if enum_field_is_unit(field, &params, args, checked) {
                continue;
            }
            let key = type_ref_local_key_expand(&field.ty, &params, args, checked);
            let fnm = mangle_ident(&field.name.name);
            if key == "String" {
                let _ = writeln!(
                    out,
                    "      if (source->data.{vn}.{fnm} != NULL) {{ size_t len = strlen(source->data.{vn}.{fnm}); char *text = (char *)malloc(len + 1); if (text == NULL) abort(); memcpy(text, source->data.{vn}.{fnm}, len + 1); copy.data.{vn}.{fnm} = text; }}"
                );
                if variant_has_owned_field(e, &pkg, args, &variant.name.name, checked) {
                    let _ = writeln!(out, "      copy.data.{vn}.owned = true;");
                }
            }
        }
        out.push_str("      break;\n    }\n");
    }
    out.push_str("    default: break;\n  }\n  return copy;\n}\n");
    let _ = writeln!(out, "void {cty}_drop({cty} *value) {{");
    out.push_str("  if (value == NULL) return;\n  switch (value->tag) {\n");
    for (tag, variant) in e.variants.iter().enumerate() {
        let vn = mangle_ident(&variant.name.name);
        let _ = writeln!(out, "    case {tag}: {{");
        for field in &variant.fields {
            if enum_field_is_unit(field, &params, args, checked) {
                continue;
            }
            let key = type_ref_local_key_expand(&field.ty, &params, args, checked);
            let fnm = mangle_ident(&field.name.name);
            if key == "String" {
                if variant_has_owned_field(e, &pkg, args, &variant.name.name, checked) {
                    let _ = writeln!(
                        out,
                        "      if (value->data.{vn}.owned && value->data.{vn}.{fnm} != NULL) free((void *)value->data.{vn}.{fnm});"
                    );
                } else {
                    let _ = writeln!(
                        out,
                        "      if (value->data.{vn}.{fnm} != NULL) free((void *)value->data.{vn}.{fnm});"
                    );
                }
                let _ = writeln!(out, "      value->data.{vn}.{fnm} = NULL;");
            }
        }
        out.push_str("      break;\n    }\n");
    }
    out.push_str("    default: break;\n  }\n}\n");
}
