//! C naming, monomorph keys, and type conversion.

use aura_ast::*;
use aura_sema::{CheckedFile, Ty};

pub(crate) fn c_iface_type(name: &str) -> String {
    format!("aura_iface_{name}")
}

pub(crate) fn c_upcast_name(class: &str, iface: &str) -> String {
    format!("aura_upcast_{class}_to_{iface}")
}

pub(crate) fn c_iface_method_name(iface: &str, method: &str) -> String {
    format!("aura_iface_{iface}_{method}")
}
pub(crate) fn mono_key(name: &str, args: &[Ty]) -> String {
    if args.is_empty() {
        name.to_string()
    } else {
        format!(
            "{}_{}",
            name,
            args.iter()
                .map(|a| a.mono_suffix())
                .collect::<Vec<_>>()
                .join("_")
        )
    }
}

pub(crate) fn c_class_type(mono: &str) -> String {
    format!("aura_cls_{mono}")
}

pub(crate) fn c_enum_type(mono: &str) -> String {
    format!("aura_enum_{mono}")
}

pub(crate) fn c_variant_ctor_name(mono: &str, variant: &str) -> String {
    format!("aura_var_{mono}_{variant}")
}
pub(crate) fn is_enum_name(checked: &CheckedFile, name: &str) -> bool {
    checked.ast.enums.iter().any(|e| e.name.name == name)
}
/// Sanitize package path for C identifiers (`demo.math` → `demo_math`).
pub(crate) fn mangle_package(pkg: &str) -> String {
    pkg.chars()
        .map(|c| if c == '.' || c == '-' { '_' } else { c })
        .collect()
}

/// Free-function C symbol (C3o: package-prefixed except `main` / builtins).
pub(crate) fn c_fun_name(pkg: &str, name: &str, args: &[Ty]) -> String {
    if name == "main" {
        return "aura_fn_main".into();
    }
    if name == "println" {
        return "aura_println".into();
    }
    if name == "assert" {
        return "aura_assert".into();
    }
    let mono = mono_key(name, args);
    if pkg.is_empty() {
        format!("aura_fn_{mono}")
    } else {
        format!("aura_fn_{}_{mono}", mangle_package(pkg))
    }
}

/// Package of a function decl for mangling.
pub(crate) fn fun_decl_package(f: &FunDecl, checked: &CheckedFile) -> String {
    if f.origin_package.is_empty() {
        checked.package.clone()
    } else {
        f.origin_package.clone()
    }
}

pub(crate) fn c_ctor_name(mono: &str) -> String {
    format!("aura_new_{mono}")
}

pub(crate) fn c_method_name(mono: &str, method: &str) -> String {
    format!("aura_method_{mono}_{method}")
}

pub(crate) fn subst_type_ref(ty: &TypeRef, params: &[String], args: &[Ty]) -> String {
    // Resolve type param names to concrete Ty display / mono
    if let Some(idx) = params.iter().position(|p| p == &ty.name.name) {
        if let Some(t) = args.get(idx) {
            return ty_to_c(t);
        }
    }
    if !ty.type_args.is_empty() {
        let mono = type_ref_mono(ty, params, args);
        if is_primitive_name(&ty.name.name) {
            return ty_to_c(&Ty::Class(ty.name.name.clone())); // shouldn't
        }
        return c_class_type(&mono);
    }
    match ty.name.name.as_str() {
        "Int" => "int64_t".into(),
        "Bool" => "bool".into(),
        "String" => "const char *".into(),
        "Unit" => "void".into(),
        name => c_class_type(name),
    }
}

pub(crate) fn is_primitive_name(n: &str) -> bool {
    matches!(n, "Int" | "Bool" | "String" | "Unit")
}

pub(crate) fn type_ref_mono(ty: &TypeRef, params: &[String], args: &[Ty]) -> String {
    if let Some(idx) = params.iter().position(|p| p == &ty.name.name) {
        if let Some(t) = args.get(idx) {
            return t.mono_suffix();
        }
    }
    if ty.type_args.is_empty() {
        ty.name.name.clone()
    } else {
        let a = ty
            .type_args
            .iter()
            .map(|t| type_ref_mono(t, params, args))
            .collect::<Vec<_>>()
            .join("_");
        format!("{}_{a}", ty.name.name)
    }
}

pub(crate) fn ty_to_c(t: &Ty) -> String {
    match t {
        Ty::Int => "int64_t".into(),
        Ty::Bool => "bool".into(),
        Ty::String => "const char *".into(),
        Ty::Unit => "void".into(),
        Ty::Null => "const char *".into(),
        Ty::Nullable(inner) => ty_to_c(inner),
        Ty::Class(n) => c_class_type(n),
        Ty::ClassApp { name, args } => c_class_type(&mono_key(name, args)),
        Ty::Enum(n) => c_enum_type(n),
        Ty::EnumApp { name, args } => c_enum_type(&mono_key(name, args)),
        Ty::Interface(n) => c_iface_type(n),
        Ty::TypeParam(_) => "/* unbound T */ int64_t".into(),
    }
}

pub(crate) fn c_type_ref(ty: &TypeRef, checked: &CheckedFile) -> String {
    c_type_ref_subst(ty, checked, &[], &[])
}

pub(crate) fn c_type_ref_subst(
    ty: &TypeRef,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
) -> String {
    let _ = checked;
    if ty.nullable {
        // nullable class pointer not fully supported
        let inner = subst_type_ref(
            &TypeRef {
                qualifier: ty.qualifier.clone(),
                name: ty.name.clone(),
                type_args: ty.type_args.clone(),
                nullable: false,
                span: ty.span,
            },
            params,
            args,
        );
        if inner.starts_with("aura_cls_") {
            return format!("{inner} *");
        }
        return inner;
    }
    // interface?
    // handled via name
    if ty.type_args.is_empty() {
        if let Some(idx) = params.iter().position(|p| p == &ty.name.name) {
            if let Some(t) = args.get(idx) {
                return ty_to_c(t);
            }
        }
        match ty.name.name.as_str() {
            "Int" => "int64_t".into(),
            "Bool" => "bool".into(),
            "String" => "const char *".into(),
            "Unit" => "void".into(),
            name if checked.ast.interfaces.iter().any(|i| i.name.name == name) => {
                c_iface_type(name)
            }
            name if is_enum_name(checked, name) => c_enum_type(name),
            name => c_class_type(name),
        }
    } else {
        let mono = type_ref_mono(ty, params, args);
        if is_enum_name(checked, &ty.name.name) {
            c_enum_type(&mono)
        } else {
            c_class_type(&mono)
        }
    }
}

pub(crate) fn c_type_from_opt(ret: &Option<TypeRef>, checked: &CheckedFile, params: &[String], args: &[Ty]) -> String {
    match ret {
        None => "void".into(),
        Some(t) if t.name.name == "Unit" && t.type_args.is_empty() => "void".into(),
        Some(t) => c_type_ref_subst(t, checked, params, args),
    }
}
pub(crate) fn mangle_ident(name: &str) -> String {
    match name {
        "int" | "bool" | "return" | "if" | "else" | "while" | "void" | "main" | "this" => {
            format!("a_{name}")
        }
        _ => name.to_string(),
    }
}
pub(crate) fn type_ref_local_key(ty: &TypeRef, params: &[String], args: &[Ty]) -> String {
    if let Some(idx) = params.iter().position(|p| p == &ty.name.name) {
        if let Some(t) = args.get(idx) {
            return match t {
                Ty::ClassApp { .. } | Ty::Class(_) | Ty::EnumApp { .. } | Ty::Enum(_) => {
                    t.mono_suffix()
                }
                other => other.display(),
            };
        }
    }
    if ty.type_args.is_empty() {
        ty.name.name.clone()
    } else {
        type_ref_mono(ty, params, args)
    }
}
pub(crate) fn escape_c_string(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_ascii_graphic() || c == ' ' => out.push(c),
            c => out.push_str(&format!("\\x{:02x}", c as u32)),
        }
    }
    out
}

