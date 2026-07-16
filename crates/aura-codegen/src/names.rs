//! C naming, monomorph keys, and type conversion.

use aura_ast::*;
use aura_sema::{nominal_key, nominal_mono_base, CheckedFile, Ty};

/// `iface` is a C mono base (`Named` or `demo_iface_Named`).
pub(crate) fn c_iface_type(iface_mono: &str) -> String {
    format!("aura_iface_{iface_mono}")
}

pub(crate) fn c_upcast_name(class_mono: &str, iface_mono: &str) -> String {
    format!("aura_upcast_{class_mono}_to_{iface_mono}")
}

pub(crate) fn c_iface_method_name(iface_mono: &str, method: &str) -> String {
    format!("aura_iface_{iface_mono}_{method}")
}

/// Package for an interface decl (C4d).
pub(crate) fn iface_decl_package(i: &InterfaceDecl, checked: &CheckedFile) -> String {
    if i.origin_package.is_empty() {
        checked.package.clone()
    } else {
        i.origin_package.clone()
    }
}

/// C mono id for an interface (`demo_iface_Named`).
pub(crate) fn iface_mono(i: &InterfaceDecl, checked: &CheckedFile) -> String {
    type_mono(&iface_decl_package(i, checked), &i.name.name, &[])
}

/// Resolve interface mono from a local/type key (simple name or already-mangled).
pub(crate) fn iface_mono_from_key(key: &str, checked: &CheckedFile) -> String {
    let (simple, pkg) = {
        // Local keys may be package mono already (`demo_iface_Named`) or simple.
        if let Some(i) = checked.ast.interfaces.iter().find(|i| {
            let m = type_mono(&iface_decl_package(i, checked), &i.name.name, &[]);
            m == key || i.name.name == key
        }) {
            return type_mono(&iface_decl_package(i, checked), &i.name.name, &[]);
        }
        if key.contains('@') {
            let (n, p) = aura_sema::split_nominal(key);
            return type_mono(p, n, &[]);
        }
        (key, "")
    };
    let _ = (simple, pkg);
    key.to_string()
}
pub(crate) fn mono_key(name: &str, args: &[Ty]) -> String {
    // `name` may already be a C mono base or a simple/nominal key.
    let base = if name.contains('@') {
        nominal_mono_base(name)
    } else {
        name.to_string()
    };
    if args.is_empty() {
        base
    } else {
        format!(
            "{base}_{}",
            args.iter()
                .map(|a| a.mono_suffix())
                .collect::<Vec<_>>()
                .join("_")
        )
    }
}

/// C monomorph id for a user type in a package (C3v).
pub(crate) fn type_mono(pkg: &str, name: &str, args: &[Ty]) -> String {
    mono_key(&nominal_key(pkg, name), args)
}

/// Package for a class/struct decl.
pub(crate) fn class_decl_package(c: &ClassDecl, checked: &CheckedFile) -> String {
    if c.origin_package.is_empty() {
        checked.package.clone()
    } else {
        c.origin_package.clone()
    }
}

/// Package for an enum decl.
pub(crate) fn enum_decl_package(e: &EnumDecl, checked: &CheckedFile) -> String {
    if e.origin_package.is_empty() {
        checked.package.clone()
    } else {
        e.origin_package.clone()
    }
}

pub(crate) fn c_class_type(mono: &str) -> String {
    format!("aura_cls_{mono}")
}

/// C3y: user `class` (not `struct`, not builtin Array) is a GC heap pointer.
pub(crate) fn is_heap_class_decl(c: &ClassDecl) -> bool {
    c.kind == NominalKind::Class
}

pub(crate) fn is_heap_class_mono(mono: &str, checked: &CheckedFile) -> bool {
    if mono == "Array" || mono.starts_with("Array_") {
        return false;
    }
    // Strip package mono to simple name via checking all classes.
    for c in &checked.ast.classes {
        if !is_heap_class_decl(c) {
            continue;
        }
        let pkg = class_decl_package(c, checked);
        let base_mono = type_mono(&pkg, &c.name.name, &[]);
        if mono == base_mono || mono == c.name.name {
            return true;
        }
        // Generic mono: demo_pkg_Box_String
        if mono.starts_with(&format!("{base_mono}_")) || mono.starts_with(&format!("{}_", c.name.name))
        {
            // Confirm a mono_classes entry exists for this simple name.
            if checked.mono_classes.iter().any(|(n, _)| n == &c.name.name) {
                return true;
            }
        }
        let full = type_mono(&pkg, &c.name.name, &[]);
        if mono.starts_with(&(full.clone() + "_")) {
            return true;
        }
    }
    false
}

/// Local/parameter C type: pointer for heap classes, value for structs/Array.
pub(crate) fn c_class_local_type(mono: &str, checked: &CheckedFile) -> String {
    let base = c_class_type(mono);
    if is_heap_class_mono(mono, checked) {
        format!("{base} *")
    } else {
        base
    }
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
    // Builtins only when package is empty (not std.io re-exports, C3z).
    if pkg.is_empty() {
        if name == "println" {
            return "aura_println".into();
        }
        if name == "assert" {
            return "aura_assert".into();
        }
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
        // Without CheckedFile, emit value type; callers with class context use c_class_local_type.
        Ty::Class(n) => c_class_type(&nominal_mono_base(n)),
        Ty::ClassApp { name, args } => c_class_type(&mono_key(name, args)),
        Ty::Enum(n) => c_enum_type(&nominal_mono_base(n)),
        Ty::EnumApp { name, args } => c_enum_type(&mono_key(name, args)),
        Ty::Interface(n) => c_iface_type(&nominal_mono_base(n)),
        Ty::TypeParam(_) => "/* unbound T */ int64_t".into(),
    }
}

/// C type for `Array` elements (C4c): heap class refs are pointers.
pub(crate) fn ty_to_c_array_elem(t: &Ty) -> String {
    match t {
        Ty::Class(n) => format!("{} *", c_class_type(&nominal_mono_base(n))),
        Ty::ClassApp { name, args } => format!("{} *", c_class_type(&mono_key(name, args))),
        Ty::Nullable(inner) => ty_to_c_array_elem(inner),
        other => ty_to_c(other),
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
    if ty.nullable {
        // C4b: `T?` shares the C representation of `T` for pointer-like types
        // (heap class*, String, interfaces). Do not re-mangle via subst_type_ref
        // (drops package mono) or add an extra `*`.
        let non_null = TypeRef {
            qualifier: ty.qualifier.clone(),
            name: ty.name.clone(),
            type_args: ty.type_args.clone(),
            nullable: false,
            span: ty.span,
        };
        return c_type_ref_subst(&non_null, checked, params, args);
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
                let imono = iface_mono_from_key(name, checked);
                c_iface_type(&imono)
            }
            name if is_enum_name(checked, name) => {
                let pkg = resolve_type_ref_package(ty, checked);
                c_enum_type(&type_mono(&pkg, name, &[]))
            }
            name => {
                let pkg = resolve_type_ref_package(ty, checked);
                let mono = type_mono(&pkg, name, &[]);
                c_class_local_type(&mono, checked)
            }
        }
    } else {
        let pkg = resolve_type_ref_package(ty, checked);
        let targs: Vec<Ty> = ty
            .type_args
            .iter()
            .filter_map(|t| {
                if let Some(idx) = params.iter().position(|p| p == &t.name.name) {
                    return args.get(idx).cloned();
                }
                match t.name.name.as_str() {
                    "Int" => Some(Ty::Int),
                    "Bool" => Some(Ty::Bool),
                    "String" => Some(Ty::String),
                    other => {
                        // C4c: package-qualify class type args (Array<Box@pkg>).
                        let epkg = resolve_type_ref_package(t, checked);
                        if t.type_args.is_empty() {
                            Some(Ty::Class(nominal_key(&epkg, other)))
                        } else {
                            let nested: Vec<Ty> = t
                                .type_args
                                .iter()
                                .filter_map(|n| {
                                    match n.name.name.as_str() {
                                        "Int" => Some(Ty::Int),
                                        "Bool" => Some(Ty::Bool),
                                        "String" => Some(Ty::String),
                                        o => {
                                            let p = resolve_type_ref_package(n, checked);
                                            Some(Ty::Class(nominal_key(&p, o)))
                                        }
                                    }
                                })
                                .collect();
                            Some(Ty::ClassApp {
                                name: nominal_key(&epkg, other),
                                args: nested,
                            })
                        }
                    }
                }
            })
            .collect();
        let mono = if targs.len() == ty.type_args.len() {
            type_mono(&pkg, &ty.name.name, &targs)
        } else {
            type_ref_mono(ty, params, args)
        };
        if is_enum_name(checked, &ty.name.name) {
            c_enum_type(&mono)
        } else {
            c_class_local_type(&mono, checked)
        }
    }
}

/// Resolve declaring package for a TypeRef (qualifier alias or unique class/enum).
fn resolve_type_ref_package(ty: &TypeRef, checked: &CheckedFile) -> String {
    if let Some(q) = &ty.qualifier {
        if let Some(imp) = checked.ast.imports.iter().find(|i| {
            i.alias
                .as_ref()
                .map(|a| a.name == q.name)
                .unwrap_or(false)
        }) {
            return imp.path.display();
        }
    }
    let name = &ty.name.name;
    // Builtins are package-less (never inherit the file package).
    if name == "Array" || is_primitive_name(name) {
        return String::new();
    }
    let matches: Vec<_> = checked
        .ast
        .classes
        .iter()
        .filter(|c| c.name.name == *name)
        .map(|c| class_decl_package(c, checked))
        .collect();
    if matches.len() == 1 {
        return matches[0].clone();
    }
    let ematches: Vec<_> = checked
        .ast
        .enums
        .iter()
        .filter(|e| e.name.name == *name)
        .map(|e| enum_decl_package(e, checked))
        .collect();
    if ematches.len() == 1 {
        return ematches[0].clone();
    }
    // Fallback: file package (single-package programs).
    checked.package.clone()
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
    // Prefer mono suffix form (package-prefixed) when args are empty and name is a nominal.
    if ty.type_args.is_empty() {
        if is_primitive_name(&ty.name.name) {
            ty.name.name.clone()
        } else {
            // Without checked context, keep simple name; emit paths that have CheckedFile
            // recompute via c_type_ref_subst / infer.
            ty.name.name.clone()
        }
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

