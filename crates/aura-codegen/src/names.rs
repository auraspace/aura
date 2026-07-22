//! C naming, monomorph keys, and type conversion.

use std::fmt::Write as _;

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

/// C mono id for an interface (`demo_iface_Named` or with type args).
pub(crate) fn iface_mono(i: &InterfaceDecl, checked: &CheckedFile) -> String {
    iface_mono_args(i, checked, &[])
}

pub(crate) fn iface_mono_args(i: &InterfaceDecl, checked: &CheckedFile, args: &[Ty]) -> String {
    type_mono(&iface_decl_package(i, checked), &i.name.name, args)
}

/// Resolve interface mono from a local/type key (simple name or already-mangled).
pub(crate) fn iface_mono_from_key(key: &str, checked: &CheckedFile) -> String {
    resolve_iface_mono_key(key, checked)
}

/// True if `key` names a (possibly monomorphized) interface type.
pub(crate) fn is_iface_type_key(key: &str, checked: &CheckedFile) -> bool {
    if checked.ast.interfaces.iter().any(|i| {
        let base = iface_mono(i, checked);
        i.name.name == key || base == key || key.starts_with(&format!("{base}_"))
    }) {
        return true;
    }
    checked.mono_interfaces.iter().any(|(n, args)| {
        checked
            .ast
            .interfaces
            .iter()
            .find(|i| i.name.name == *n)
            .map(|i| {
                let m = iface_mono_args(i, checked, args);
                m == key || key == *n || key.starts_with(&format!("{n}_"))
            })
            .unwrap_or(false)
    })
}

/// Full C mono id for an interface local/type key (incl. mono args).
pub(crate) fn resolve_iface_mono_key(key: &str, checked: &CheckedFile) -> String {
    for (name, args) in &checked.mono_interfaces {
        if let Some(i) = checked.ast.interfaces.iter().find(|i| i.name.name == *name) {
            let m = iface_mono_args(i, checked, args);
            if m == key {
                return m;
            }
            // Local key may be simple mono without package: Boxable_Int
            let simple_mono = if args.is_empty() {
                name.clone()
            } else {
                format!(
                    "{}_{}",
                    name,
                    args.iter()
                        .map(|t| t.mono_suffix())
                        .collect::<Vec<_>>()
                        .join("_")
                )
            };
            if key == simple_mono || key == *name {
                return m;
            }
        }
    }
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
    // Prefix match package mono base_
    for i in &checked.ast.interfaces {
        let base = iface_mono(i, checked);
        if key.starts_with(&format!("{base}_")) {
            return key.to_string();
        }
    }
    key.to_string()
}

/// Interface decl + type args for a local/type key (C8c).
pub(crate) fn resolve_iface_decl_and_args<'a>(
    key: &str,
    checked: &'a CheckedFile,
) -> (Option<&'a InterfaceDecl>, Vec<Ty>) {
    for (name, args) in &checked.mono_interfaces {
        if let Some(i) = checked.ast.interfaces.iter().find(|i| i.name.name == *name) {
            let m = iface_mono_args(i, checked, args);
            let simple_mono = if args.is_empty() {
                name.clone()
            } else {
                format!(
                    "{}_{}",
                    name,
                    args.iter()
                        .map(|t| t.mono_suffix())
                        .collect::<Vec<_>>()
                        .join("_")
                )
            };
            if m == key || key == simple_mono {
                return (Some(i), args.clone());
            }
        }
    }
    if let Some(i) = checked.ast.interfaces.iter().find(|i| {
        let m = iface_mono(i, checked);
        i.name.name == key || m == key
    }) {
        return (Some(i), Vec::new());
    }
    (None, Vec::new())
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
        if mono.starts_with(&format!("{base_mono}_"))
            || mono.starts_with(&format!("{}_", c.name.name))
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
        if name == "print" {
            return "aura_print".into();
        }
        if name == "println" {
            return "aura_println".into();
        }
        if name == "eprint" {
            return "aura_eprint".into();
        }
        if name == "eprintln" {
            return "aura_eprintln".into();
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

/// Local/type key for tagged optional primitives (`Int?` / `Bool?`).
pub(crate) fn is_opt_prim_key(key: &str) -> bool {
    matches!(key, "Opt_Int" | "Opt_Bool")
}

pub(crate) fn opt_prim_c_type(key: &str) -> Option<&'static str> {
    match key {
        "Opt_Int" => Some("aura_opt_i64"),
        "Opt_Bool" => Some("aura_opt_bool"),
        _ => None,
    }
}

pub(crate) fn null_opt_prim(key: &str) -> String {
    match key {
        "Opt_Int" => "((aura_opt_i64){ .has = false, .value = INT64_C(0) })".into(),
        "Opt_Bool" => "((aura_opt_bool){ .has = false, .value = false })".into(),
        _ => "NULL".into(),
    }
}

pub(crate) fn wrap_opt_prim(key: &str, value_code: &str) -> String {
    match key {
        "Opt_Int" => format!("((aura_opt_i64){{ .has = true, .value = ({value_code}) }})"),
        "Opt_Bool" => format!("((aura_opt_bool){{ .has = true, .value = ({value_code}) }})"),
        _ => value_code.to_string(),
    }
}

/// Map non-null primitive key → optional key.
pub(crate) fn opt_key_for_prim(prim: &str) -> Option<&'static str> {
    match prim {
        "Int" => Some("Opt_Int"),
        "Bool" => Some("Opt_Bool"),
        _ => None,
    }
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
        // C7a: Int?/Bool? are tagged structs; other T? share T's rep (pointer-like).
        Ty::Nullable(inner) => match inner.as_ref() {
            Ty::Int => "aura_opt_i64".into(),
            Ty::Bool => "aura_opt_bool".into(),
            other => ty_to_c(other),
        },
        // Without CheckedFile, emit value type; callers with class context use c_class_local_type.
        Ty::Class(n) => c_class_type(&nominal_mono_base(n)),
        Ty::ClassApp { name, args } => c_class_type(&mono_key(name, args)),
        Ty::Enum(n) => c_enum_type(&nominal_mono_base(n)),
        Ty::EnumApp { name, args } => c_enum_type(&mono_key(name, args)),
        Ty::Interface(n) => c_iface_type(&nominal_mono_base(n)),
        Ty::InterfaceApp { name, args } => c_iface_type(&mono_key(name, args)),
        Ty::TypeParam(_) => "/* unbound T */ int64_t".into(),
        Ty::Fun { .. } => c_fun_typedef(&t.mono_suffix()),
    }
}

/// C type for `Array` elements (C4c/C4q/C6g): heap class refs are pointers;
/// structs and enums by value.
pub(crate) fn ty_to_c_array_elem(t: &Ty, checked: &CheckedFile) -> String {
    match t {
        Ty::Class(n) => {
            let mono = nominal_mono_base(n);
            c_class_local_type(&mono, checked)
        }
        Ty::ClassApp { name, args } => {
            let mono = mono_key(name, args);
            c_class_local_type(&mono, checked)
        }
        Ty::Nullable(inner) => match inner.as_ref() {
            Ty::Int => "aura_opt_i64".into(),
            Ty::Bool => "aura_opt_bool".into(),
            other => ty_to_c_array_elem(other, checked),
        },
        other => ty_to_c(other),
    }
}

/// C type for locals/params with CheckedFile (C4k: heap classes are pointers).
pub(crate) fn ty_to_c_local(t: &Ty, checked: &CheckedFile) -> String {
    match t {
        Ty::Class(n) => {
            let mono = nominal_mono_base(n);
            c_class_local_type(&mono, checked)
        }
        Ty::ClassApp { name, args } => {
            let mono = mono_key(name, args);
            c_class_local_type(&mono, checked)
        }
        Ty::Nullable(inner) => match inner.as_ref() {
            Ty::Int => "aura_opt_i64".into(),
            Ty::Bool => "aura_opt_bool".into(),
            other => ty_to_c_local(other, checked),
        },
        Ty::Interface(n) => c_iface_type(&nominal_mono_base(n)),
        Ty::InterfaceApp { name, args } => c_iface_type(&mono_key(name, args)),
        Ty::Enum(n) => c_enum_type(&nominal_mono_base(n)),
        Ty::EnumApp { name, args } => c_enum_type(&mono_key(name, args)),
        other => ty_to_c(other),
    }
}

/// Resolve a type reference under a concrete generic instantiation.
///
/// This must recurse through type arguments: a direct substitution handles `T`,
/// but a type such as `Array<Pair<K, V>>` needs both `K` and `V` replaced before
/// its monomorph key is used by C emission.
fn type_ref_to_ty_subst(ty: &TypeRef, checked: &CheckedFile, params: &[String], args: &[Ty]) -> Ty {
    if let Some(fun) = &ty.fun {
        let fun_ty = Ty::Fun {
            params: fun
                .params
                .iter()
                .map(|p| type_ref_to_ty_subst(p, checked, params, args))
                .collect(),
            ret: Box::new(type_ref_to_ty_subst(&fun.ret, checked, params, args)),
        };
        return if ty.nullable {
            Ty::Nullable(Box::new(fun_ty))
        } else {
            fun_ty
        };
    }
    if ty.type_args.is_empty() {
        if let Some(idx) = params.iter().position(|p| p == &ty.name.name) {
            if let Some(arg) = args.get(idx) {
                return if ty.nullable {
                    Ty::Nullable(Box::new(arg.clone()))
                } else {
                    arg.clone()
                };
            }
        }
        if let Some(alias) = checked
            .ast
            .type_aliases
            .iter()
            .find(|a| a.name.name == ty.name.name)
        {
            return type_ref_to_ty_subst(&alias.ty, checked, params, args);
        }
    }

    let type_args: Vec<Ty> = ty
        .type_args
        .iter()
        .map(|arg| type_ref_to_ty_subst(arg, checked, params, args))
        .collect();
    let pkg = resolve_type_ref_package(ty, checked);
    let base = match ty.name.name.as_str() {
        "Int" => Ty::Int,
        "Bool" => Ty::Bool,
        "String" => Ty::String,
        "Unit" => Ty::Unit,
        "Array" => Ty::ClassApp {
            name: "Array".into(),
            args: type_args,
        },
        name if checked.ast.enums.iter().any(|e| e.name.name == name) => {
            if type_args.is_empty() {
                Ty::Enum(nominal_key(&pkg, name))
            } else {
                Ty::EnumApp {
                    name: nominal_key(&pkg, name),
                    args: type_args,
                }
            }
        }
        name if checked.ast.interfaces.iter().any(|i| i.name.name == name) => {
            if type_args.is_empty() {
                Ty::Interface(nominal_key(&pkg, name))
            } else {
                Ty::InterfaceApp {
                    name: nominal_key(&pkg, name),
                    args: type_args,
                }
            }
        }
        name => {
            if type_args.is_empty() {
                Ty::Class(nominal_key(&pkg, name))
            } else {
                Ty::ClassApp {
                    name: nominal_key(&pkg, name),
                    args: type_args,
                }
            }
        }
    };
    if ty.nullable {
        Ty::Nullable(Box::new(base))
    } else {
        base
    }
}

pub(crate) fn c_type_ref_subst(
    ty: &TypeRef,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
) -> String {
    // C10f: function type → typedef name.
    if let Some(fun) = &ty.fun {
        let params_ty: Vec<Ty> = fun
            .params
            .iter()
            .map(|p| type_ref_to_ty_subst(p, checked, params, args))
            .collect();
        let ret_ty = type_ref_to_ty_subst(&fun.ret, checked, params, args);
        let fun_ty = Ty::Fun {
            params: params_ty,
            ret: Box::new(ret_ty),
        };
        let key = fun_ty.mono_suffix();
        let base = c_fun_typedef(&key);
        if ty.nullable {
            return base; // pointer-like; no opt wrapper for fun
        }
        return base;
    }
    if ty.nullable {
        // C7a: Int?/Bool? → tagged optional structs.
        // C4b: other `T?` share the C representation of `T` (heap class*, String, ifaces).
        let non_null = TypeRef {
            qualifier: ty.qualifier.clone(),
            name: ty.name.clone(),
            type_args: ty.type_args.clone(),
            nullable: false,
            reference: false,
            span: ty.span,
            fun: ty.fun.clone(),
        };
        let inner = c_type_ref_subst(&non_null, checked, params, args);
        return match inner.as_str() {
            "int64_t" => "aura_opt_i64".into(),
            "bool" => "aura_opt_bool".into(),
            _ => inner,
        };
    }
    // C9f: type alias → underlying type.
    if ty.type_args.is_empty() {
        if let Some(alias) = checked
            .ast
            .type_aliases
            .iter()
            .find(|a| a.name.name == ty.name.name)
        {
            return c_type_ref_subst(&alias.ty, checked, params, args);
        }
    }
    // interface?
    // handled via name
    if ty.type_args.is_empty() {
        if let Some(idx) = params.iter().position(|p| p == &ty.name.name) {
            if let Some(t) = args.get(idx) {
                // C4k: monomorphized type params that are heap classes must be pointers.
                return ty_to_c_local(t, checked);
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
        // C8c: generic interface type refs → mono iface C type.
        if checked
            .ast
            .interfaces
            .iter()
            .any(|i| i.name.name == ty.name.name)
        {
            let targs: Vec<Ty> = ty
                .type_args
                .iter()
                .map(|t| type_ref_to_ty_subst(t, checked, params, args))
                .collect();
            let mono = type_mono(&pkg, &ty.name.name, &targs);
            return c_iface_type(&mono);
        }
        let targs: Vec<Ty> = ty
            .type_args
            .iter()
            .map(|t| type_ref_to_ty_subst(t, checked, params, args))
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
        if let Some(imp) = checked
            .ast
            .imports
            .iter()
            .find(|i| i.alias.as_ref().map(|a| a.name == q.name).unwrap_or(false))
        {
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
    // C8d: unique interface by simple name (e.g. Iterable from std.collections).
    let imatches: Vec<_> = checked
        .ast
        .interfaces
        .iter()
        .filter(|i| i.name.name == *name)
        .map(|i| iface_decl_package(i, checked))
        .collect();
    if imatches.len() == 1 {
        return imatches[0].clone();
    }
    // Fallback: file package (single-package programs).
    checked.package.clone()
}

pub(crate) fn c_type_from_opt(
    ret: &Option<TypeRef>,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
) -> String {
    match ret {
        None => "void".into(),
        Some(t) if t.name.name == "Unit" && t.type_args.is_empty() => "void".into(),
        Some(t) => c_type_ref_subst(t, checked, params, args),
    }
}
pub(crate) fn mangle_ident(name: &str) -> String {
    match name {
        // C keywords + common reserved names that break generated code.
        "int" | "bool" | "return" | "if" | "else" | "while" | "void" | "main" | "this"
        | "default" | "case" | "switch" | "break" | "continue" | "struct" | "enum" | "typedef"
        | "const" | "static" | "extern" | "sizeof" | "typeof" | "do" | "for" | "goto"
        | "register" | "signed" | "unsigned" | "union" | "volatile" | "auto" | "char"
        | "double" | "float" | "long" | "short" | "true" | "false" | "NULL" | "null" => {
            format!("a_{name}")
        }
        _ => name.to_string(),
    }
}
/// C10e: local/type key for function types (`Fun_Int__Int`).
pub(crate) fn is_fun_type_key(key: &str) -> bool {
    key.starts_with("Fun_") || key == "Fun"
}

/// C typedef name for a function-type mono key.
pub(crate) fn c_fun_typedef(key: &str) -> String {
    format!("aura_fp_{key}")
}

/// C type for a semantic `Ty` (primitives, fun pointers; classes as mono struct pointers).
/// C12k: capture type is a GC heap class pointer (not struct/Array/prim).
pub(crate) fn is_heap_class_capture_ty(ty: &Ty, checked: &CheckedFile) -> bool {
    match ty {
        Ty::Class(_) | Ty::ClassApp { .. } => {
            let mono = ty.mono_suffix();
            is_heap_class_mono(&mono, checked)
        }
        _ => false,
    }
}

/// C13e: capture type is a Fun fat pointer (nested env retain/release).
pub(crate) fn is_fun_capture_ty(ty: &Ty) -> bool {
    matches!(ty, Ty::Fun { .. })
}

pub(crate) fn is_array_capture_ty(ty: &Ty) -> bool {
    matches!(ty, Ty::ClassApp { name, .. } if aura_sema::split_nominal(name).0 == "Array")
}

/// C type of a lambda env field (C12m/C13f: by-ref → shared box pointer).
pub(crate) fn c_capture_field_type(ty: &Ty, by_ref: bool, checked: &CheckedFile) -> String {
    if by_ref {
        match ty {
            Ty::Bool => "aura_box_bool *".into(),
            Ty::String => "aura_box_str *".into(),
            Ty::Int => "aura_box_i64 *".into(),
            _ => "aura_box_ptr *".into(),
        }
    } else {
        c_type_from_ty(ty, checked)
    }
}

pub(crate) fn box_retain_fn(ty_key: &str) -> &'static str {
    match ty_key {
        "Bool" => "aura_box_bool_retain",
        "String" => "aura_box_str_retain",
        _ => "aura_box_i64_retain",
    }
}

pub(crate) fn box_release_fn(ty_key: &str) -> &'static str {
    match ty_key {
        "Bool" => "aura_box_bool_release",
        "String" => "aura_box_str_release",
        _ => "aura_box_i64_release",
    }
}

pub(crate) fn c_type_from_ty(ty: &Ty, checked: &CheckedFile) -> String {
    match ty {
        Ty::Unit => "void".into(),
        Ty::Int => "int64_t".into(),
        Ty::Bool => "bool".into(),
        Ty::String => "const char *".into(),
        Ty::Null => "void *".into(),
        Ty::Nullable(inner) => match inner.as_ref() {
            Ty::Int => "aura_opt_i64".into(),
            Ty::Bool => "aura_opt_bool".into(),
            other => c_type_from_ty(other, checked),
        },
        Ty::Fun { .. } => {
            let key = ty.mono_suffix();
            c_fun_typedef(&key)
        }
        Ty::Class(_) | Ty::ClassApp { .. } => {
            // Heap class → pointer; Array/struct → by-value (C12l Array view uses value header).
            let mono = ty.mono_suffix();
            c_class_local_type(&mono, checked)
        }
        Ty::Enum(_) | Ty::EnumApp { .. } => c_enum_type(&ty.mono_suffix()),
        Ty::Interface(_) | Ty::InterfaceApp { .. } => c_iface_type(&ty.mono_suffix()),
        Ty::TypeParam(n) => n.clone(),
    }
}

/// Best-effort TypeRef → Ty for fun-type keys (no full package resolution).
fn type_ref_to_ty_simple(ty: &TypeRef, params: &[String], args: &[Ty]) -> Ty {
    if let Some(fun) = &ty.fun {
        let ps = fun
            .params
            .iter()
            .map(|p| type_ref_to_ty_simple(p, params, args))
            .collect();
        let ret = type_ref_to_ty_simple(&fun.ret, params, args);
        let t = Ty::Fun {
            params: ps,
            ret: Box::new(ret),
        };
        return if ty.nullable {
            Ty::Nullable(Box::new(t))
        } else {
            t
        };
    }
    if let Some(idx) = params.iter().position(|p| p == &ty.name.name) {
        if let Some(a) = args.get(idx) {
            return if ty.nullable {
                Ty::Nullable(Box::new(a.clone()))
            } else {
                a.clone()
            };
        }
    }
    let base = match ty.name.name.as_str() {
        "Int" => Ty::Int,
        "Bool" => Ty::Bool,
        "String" => Ty::String,
        "Unit" => Ty::Unit,
        other => {
            if ty.type_args.is_empty() {
                Ty::Class(other.to_string())
            } else {
                Ty::ClassApp {
                    name: other.to_string(),
                    args: ty
                        .type_args
                        .iter()
                        .map(|a| type_ref_to_ty_simple(a, params, args))
                        .collect(),
                }
            }
        }
    };
    if ty.nullable {
        Ty::Nullable(Box::new(base))
    } else {
        base
    }
}

/// Emit fat-pointer Fun typedef (C10h): `{ void *env; ret (*fn)(void *env, …); }`.
pub(crate) fn emit_fun_typedef(out: &mut String, ty: &Ty, checked: &CheckedFile) {
    let Ty::Fun { params, ret } = ty else {
        return;
    };
    let key = ty.mono_suffix();
    let name = c_fun_typedef(&key);
    let ret_c = match ret.as_ref() {
        Ty::Unit => "void".to_string(),
        r => c_type_from_ty(r, checked),
    };
    let mut fn_params = vec!["void *env".to_string()];
    for p in params {
        fn_params.push(c_type_from_ty(p, checked));
    }
    let ps = fn_params.join(", ");
    let _ = writeln!(out, "typedef struct {{");
    let _ = writeln!(out, "  void *env;");
    let _ = writeln!(out, "  {ret_c} (*fn)({ps});");
    let _ = writeln!(out, "}} {name};");
}

pub(crate) fn type_ref_local_key(ty: &TypeRef, params: &[String], args: &[Ty]) -> String {
    // C10f: function type annotation.
    if let Some(fun) = &ty.fun {
        let params_ty: Vec<Ty> = fun
            .params
            .iter()
            .map(|p| type_ref_to_ty_simple(p, params, args))
            .collect();
        let ret_ty = type_ref_to_ty_simple(&fun.ret, params, args);
        let fun_ty = Ty::Fun {
            params: params_ty,
            ret: Box::new(ret_ty),
        };
        let key = fun_ty.mono_suffix();
        if ty.nullable {
            // Nullable fun not supported specially; keep key.
        }
        return key;
    }
    let base = if let Some(idx) = params.iter().position(|p| p == &ty.name.name) {
        if let Some(t) = args.get(idx) {
            match t {
                Ty::ClassApp { .. } | Ty::Class(_) | Ty::EnumApp { .. } | Ty::Enum(_) => {
                    t.mono_suffix()
                }
                // Preserve Opt_* from substituted nullable primitives.
                other => other.mono_suffix(),
            }
        } else {
            ty.name.name.clone()
        }
    } else if ty.type_args.is_empty() {
        if is_primitive_name(&ty.name.name) {
            ty.name.name.clone()
        } else {
            // Without checked context, keep simple name; emit paths that have CheckedFile
            // recompute via c_type_ref_subst / infer.
            ty.name.name.clone()
        }
    } else {
        type_ref_mono(ty, params, args)
    };
    // C7a: only Int?/Bool? get distinct keys; Class?/String? keep non-null key (pointer rep).
    if ty.nullable {
        if let Some(ok) = opt_key_for_prim(&base) {
            return ok.to_string();
        }
    }
    base
}

/// C9f: expand type aliases in a TypeRef to the underlying local key when possible.
pub(crate) fn type_ref_local_key_expand(
    ty: &TypeRef,
    params: &[String],
    args: &[Ty],
    checked: &CheckedFile,
) -> String {
    // Resolve alias name → underlying TypeRef (one hop).
    if let Some(alias) = checked
        .ast
        .type_aliases
        .iter()
        .find(|a| a.name.name == ty.name.name)
    {
        if ty.type_args.is_empty() {
            let mut key = type_ref_local_key_expand(&alias.ty, params, args, checked);
            if ty.nullable {
                if let Some(ok) = opt_key_for_prim(&key) {
                    key = ok.to_string();
                }
            }
            return key;
        }
    }
    type_ref_local_key(ty, params, args)
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
