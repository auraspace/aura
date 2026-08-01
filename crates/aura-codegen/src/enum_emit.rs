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

fn task_result_array_ok(e: &EnumDecl, pkg: &str, args: &[Ty], variant: &str) -> bool {
    pkg == "std.io"
        && e.name.name == "Result"
        && variant == "Ok"
        && args.first().is_some_and(|ty| {
            matches!(
                ty,
                Ty::Class(name) | Ty::ClassApp { name, .. }
                    if aura_sema::split_nominal(name).0 == "Array"
            )
        })
}

fn task_result_struct_ok(
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
            let name = match ty {
                Ty::Class(name) | Ty::Enum(name) => name,
                Ty::ClassApp { name, .. } | Ty::EnumApp { name, .. } => name,
                _ => return false,
            };
            let base = aura_sema::split_nominal(name).0;
            checked
                .ast
                .classes
                .iter()
                .any(|class| class.kind == NominalKind::Struct && class.name.name == base)
        })
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
            if task_result_array_ok(e, &pkg, args, &v.name.name) {
                out.push_str("      bool owned;\n");
            }
            if task_result_struct_ok(e, &pkg, args, &v.name.name, checked) {
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
    let _ = writeln!(out, "void {cty}_mark(const {cty} *value);");
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
            if task_result_array_ok(e, &pkg, args, &v.name.name) {
                let _ = writeln!(out, "  self.data.Ok.owned = false;");
            }
            if task_result_struct_ok(e, &pkg, args, &v.name.name, checked) {
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
    if task_result_enum_ok(e, &pkg, args, "Ok") {
        let ctor = c_variant_ctor_name(&mono, "OkOwned");
        let payload = c_type_from_ty(&args[0], checked);
        let _ = writeln!(
            out,
            "{} {}({} value) {{ {} self; self.tag = 0; self.data.Ok.value = value; self.data.Ok.owned = true; return self; }}",
            c_enum_type(&mono),
            ctor,
            payload,
            c_enum_type(&mono)
        );
    }
    if task_result_array_ok(e, &pkg, args, "Ok") {
        let ctor = c_variant_ctor_name(&mono, "OkOwned");
        let payload = c_type_from_ty(&args[0], checked);
        let _ = writeln!(
            out,
            "{} {}({} value) {{ {} self; self.tag = 0; self.data.Ok.value = value; self.data.Ok.owned = true; return self; }}",
            c_enum_type(&mono),
            ctor,
            payload,
            c_enum_type(&mono)
        );
    }
    if task_result_struct_ok(e, &pkg, args, "Ok", checked) {
        let ctor = c_variant_ctor_name(&mono, "OkOwned");
        let payload = c_type_from_ty(&args[0], checked);
        let _ = writeln!(
            out,
            "{} {}({} value) {{ {} self; self.tag = 0; self.data.Ok.value = value; self.data.Ok.owned = true; return self; }}",
            c_enum_type(&mono), ctor, payload, c_enum_type(&mono)
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
        || task_result_array_ok(e, pkg, args, variant)
        || task_result_struct_ok(e, pkg, args, variant, checked)
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
            let full_key = crate::expr::full_type_mono(&key, checked);
            let nested_enum =
                crate::expr::mono_split(&full_key, checked).is_some_and(|(base, _)| {
                    checked
                        .ast
                        .enums
                        .iter()
                        .any(|nested| nested.name.name == base)
                });
            let nested_struct =
                crate::expr::mono_base_name(&full_key, checked).is_some_and(|base| {
                    checked.ast.classes.iter().any(|nested| {
                        nested.kind == NominalKind::Struct && nested.name.name == base
                    })
                });
            if key == "String" {
                let _ = writeln!(
                    out,
                    "      if (source->data.{vn}.{fnm} != NULL) {{ size_t len = strlen(source->data.{vn}.{fnm}); char *text = (char *)malloc(len + 1); if (text == NULL) abort(); memcpy(text, source->data.{vn}.{fnm}, len + 1); copy.data.{vn}.{fnm} = text; }}"
                );
            } else if crate::array_emit::is_array_type_key(&full_key) {
                let clone = crate::names::c_method_name(&full_key, "clone");
                let array_cty = crate::stmt::local_key_to_c(&full_key, checked);
                let _ = writeln!(
                    out,
                    "      copy.data.{vn}.{fnm} = {clone}(({array_cty} *)&source->data.{vn}.{fnm});"
                );
            } else if is_heap_class_mono(&full_key, checked) {
                let _ = writeln!(
                    out,
                    "      copy.data.{vn}.{fnm} = source->data.{vn}.{fnm}; if (copy.data.{vn}.{fnm} != NULL) aura_gc_add_root((void **)&copy.data.{vn}.{fnm});"
                );
            } else if full_key == "ForeignHandle" || full_key.starts_with("ForeignHandle_") {
                let _ = writeln!(
                    out,
                    "      copy.data.{vn}.{fnm} = source->data.{vn}.{fnm}; if (copy.data.{vn}.{fnm} != NULL) (void)aura_ffi_handle_retain(copy.data.{vn}.{fnm});"
                );
            } else if nested_enum {
                let nested_cty = c_enum_type(&full_key);
                let _ = writeln!(
                    out,
                    "      copy.data.{vn}.{fnm} = {nested_cty}_clone(&source->data.{vn}.{fnm});"
                );
            } else if nested_struct {
                let nested_cty = crate::stmt::local_key_to_c(&full_key, checked);
                let _ = writeln!(
                    out,
                    "      copy.data.{vn}.{fnm} = {nested_cty}_clone(&source->data.{vn}.{fnm});"
                );
            }
        }
        if variant_has_owned_field(e, &pkg, args, &variant.name.name, checked) {
            let _ = writeln!(out, "      copy.data.{vn}.owned = true;");
        }
        let _ = writeln!(out, "      break;\n    }}");
    }
    out.push_str("    default: break;\n  }\n  return copy;\n}\n");
    let _ = writeln!(out, "void {cty}_drop({cty} *value) {{");
    out.push_str("  if (value == NULL) return;\n  switch (value->tag) {\n");
    for (tag, variant) in e.variants.iter().enumerate() {
        let vn = mangle_ident(&variant.name.name);
        let has_owned = variant_has_owned_field(e, &pkg, args, &variant.name.name, checked);
        let _ = writeln!(out, "    case {tag}: {{");
        if has_owned {
            let _ = writeln!(out, "      if (value->data.{vn}.owned) {{");
        }
        for field in &variant.fields {
            if enum_field_is_unit(field, &params, args, checked) {
                continue;
            }
            let key = type_ref_local_key_expand(&field.ty, &params, args, checked);
            let full_key = crate::expr::full_type_mono(&key, checked);
            let fnm = mangle_ident(&field.name.name);
            let nested_enum =
                crate::expr::mono_split(&full_key, checked).is_some_and(|(base, _)| {
                    checked
                        .ast
                        .enums
                        .iter()
                        .any(|nested| nested.name.name == base)
                });
            let nested_struct =
                crate::expr::mono_base_name(&full_key, checked).is_some_and(|base| {
                    checked.ast.classes.iter().any(|nested| {
                        nested.kind == NominalKind::Struct && nested.name.name == base
                    })
                });
            if key == "String" {
                let _ = writeln!(
                    out,
                    "        if (value->data.{vn}.{fnm} != NULL) free((void *)value->data.{vn}.{fnm}); value->data.{vn}.{fnm} = NULL;"
                );
            } else if crate::array_emit::is_array_type_key(&full_key) {
                let mut free = String::new();
                crate::array_emit::emit_array_contents_free_checked(
                    &mut free,
                    0,
                    &format!("value->data.{vn}.{fnm}"),
                    &full_key,
                    checked,
                );
                for line in free.lines() {
                    let _ = writeln!(out, "        {line}");
                }
            } else if is_heap_class_mono(&full_key, checked) {
                let _ = writeln!(
                    out,
                    "        if (value->data.{vn}.{fnm} != NULL) aura_gc_remove_root((void **)&value->data.{vn}.{fnm});"
                );
            } else if full_key == "ForeignHandle" || full_key.starts_with("ForeignHandle_") {
                let _ = writeln!(
                    out,
                    "        if (value->data.{vn}.{fnm} != NULL) (void)aura_ffi_handle_drop(&value->data.{vn}.{fnm});"
                );
            } else if nested_enum {
                let nested_cty = c_enum_type(&full_key);
                let _ = writeln!(out, "        {nested_cty}_drop(&value->data.{vn}.{fnm});");
            } else if nested_struct {
                let nested_cty = crate::stmt::local_key_to_c(&full_key, checked);
                let _ = writeln!(out, "        {nested_cty}_drop(&value->data.{vn}.{fnm});");
            }
        }
        if has_owned {
            let _ = writeln!(out, "        value->data.{vn}.owned = false;");
            out.push_str("      }\n");
        }
        out.push_str("      break;\n    }\n");
    }
    out.push_str("    default: break;\n  }\n}\n");

    let _ = writeln!(out, "void {cty}_mark(const {cty} *value) {{");
    out.push_str("  if (value == NULL) return;\n  switch (value->tag) {\n");
    for (tag, variant) in e.variants.iter().enumerate() {
        let vn = mangle_ident(&variant.name.name);
        let _ = writeln!(out, "    case {tag}: {{");
        for field in &variant.fields {
            if enum_field_is_unit(field, &params, args, checked) {
                continue;
            }
            let key = type_ref_local_key_expand(&field.ty, &params, args, checked);
            let full_key = crate::expr::full_type_mono(&key, checked);
            let fnm = mangle_ident(&field.name.name);
            if is_heap_class_mono(&full_key, checked) {
                let _ = writeln!(
                    out,
                    "      aura_gc_mark_ptr((void *)value->data.{vn}.{fnm});"
                );
            } else if crate::array_emit::is_array_of_heap_class(&full_key, checked) {
                let _ = writeln!(
                    out,
                    "      for (int64_t __gm = 0; __gm < value->data.{vn}.{fnm}.len; __gm++) aura_gc_mark_ptr((void *)value->data.{vn}.{fnm}.data[__gm]);"
                );
            } else if crate::expr::is_enum_mono(&full_key, checked) {
                let nested_cty = c_enum_type(&full_key);
                let _ = writeln!(out, "      {nested_cty}_mark(&value->data.{vn}.{fnm});");
            }
        }
        out.push_str("      break;\n    }\n");
    }
    out.push_str("    default: break;\n  }\n}\n");
}
