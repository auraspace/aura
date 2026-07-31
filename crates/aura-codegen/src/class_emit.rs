//! Class/struct typedefs, constructors, and methods.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use aura_ast::*;
use aura_sema::{CheckedFile, Ty};

use crate::ctx::EmitCtx;
use crate::iface::emit_upcast;
use crate::names::*;
use crate::stmt::{emit_block, emit_return_fallback};

pub(crate) fn direct_superclass<'a>(
    checked: &'a CheckedFile,
    c: &ClassDecl,
) -> Option<&'a ClassDecl> {
    let parent_name = c
        .superclass
        .as_ref()
        .map(|parent| parent.name.name.clone())
        .or_else(|| {
            checked
                .classes
                .iter()
                .find(|sig| {
                    sig.name == c.name.name && sig.package == class_decl_package(c, checked)
                })
                .and_then(|sig| sig.superclass.as_ref())
                .and_then(Ty::class_name)
                .map(str::to_string)
        })?;
    checked.ast.classes.iter().find(|candidate| {
        candidate.name.name == parent_name
            && candidate.kind == NominalKind::Class
            && class_decl_package(candidate, checked) == class_decl_package(c, checked)
    })
}

/// Resolve a superclass together with its concrete type arguments for a class
/// monomorph. This keeps flattened C layouts consistent with sema's nominal
/// inheritance substitution.
pub(crate) fn direct_superclass_with_args<'a>(
    checked: &'a CheckedFile,
    c: &'a ClassDecl,
    args: &[Ty],
) -> Option<(&'a ClassDecl, Vec<Ty>)> {
    let package = class_decl_package(c, checked);
    let sig = checked
        .classes
        .iter()
        .find(|sig| sig.name == c.name.name && sig.package == package)?;
    let superclass = sig.superclass.as_ref()?;
    let subst = aura_sema::type_subst_map(&sig.type_params, args);
    let superclass = aura_sema::subst_ty(superclass, &subst);
    let (name, parent_args) = match superclass {
        Ty::Class(name) => (name, Vec::new()),
        Ty::ClassApp { name, args } => (name, args),
        _ => return None,
    };
    checked.ast.classes.iter().find_map(|candidate| {
        (candidate.kind == NominalKind::Class
            && aura_sema::nominal_key(
                &class_decl_package(candidate, checked),
                &candidate.name.name,
            ) == name)
            .then_some((candidate, parent_args.clone()))
    })
}

/// Whether a concrete class monomorph has a concrete superclass monomorph.
pub(crate) fn class_mono_extends(
    checked: &CheckedFile,
    child_mono: &str,
    parent_mono: &str,
) -> bool {
    for class in &checked.ast.classes {
        let package = class_decl_package(class, checked);
        let args = if class.type_params.is_empty() {
            Vec::new()
        } else if let Some((_, args)) = checked.mono_classes.iter().find(|(name, args)| {
            name == &class.name.name && type_mono(&package, name, args) == child_mono
        }) {
            args.clone()
        } else {
            continue;
        };
        if type_mono(&package, &class.name.name, &args) != child_mono {
            continue;
        }
        let mut current = direct_superclass_with_args(checked, class, &args);
        while let Some((parent, parent_args)) = current {
            if type_mono(
                &class_decl_package(parent, checked),
                &parent.name.name,
                &parent_args,
            ) == parent_mono
            {
                return true;
            }
            current = direct_superclass_with_args(checked, parent, &parent_args);
        }
    }
    false
}

pub(crate) fn method_owner<'a>(
    checked: &'a CheckedFile,
    c: &'a ClassDecl,
    method: &str,
) -> Option<&'a ClassDecl> {
    if c.methods.iter().any(|m| m.name.name == method) {
        return Some(c);
    }
    direct_superclass(checked, c).and_then(|parent| method_owner(checked, parent, method))
}

pub(crate) fn class_tag(checked: &CheckedFile, c: &ClassDecl) -> u32 {
    checked
        .ast
        .classes
        .iter()
        .position(|candidate| {
            candidate.name.name == c.name.name
                && class_decl_package(candidate, checked) == class_decl_package(c, checked)
        })
        .map(|index| index as u32 + 1)
        .unwrap_or(0)
}

pub(crate) fn virtual_overrides<'a>(
    checked: &'a CheckedFile,
    base: &'a ClassDecl,
    method: &str,
) -> Vec<&'a ClassDecl> {
    fn is_descendant(checked: &CheckedFile, candidate: &ClassDecl, base: &ClassDecl) -> bool {
        let mut current = direct_superclass(checked, candidate);
        while let Some(parent) = current {
            if parent.name.name == base.name.name {
                return true;
            }
            current = direct_superclass(checked, parent);
        }
        false
    }

    checked
        .ast
        .classes
        .iter()
        .filter(|candidate| {
            candidate.kind == NominalKind::Class
                && candidate.name.name != base.name.name
                && class_decl_package(candidate, checked) == class_decl_package(base, checked)
                && is_descendant(checked, candidate, base)
                && candidate.methods.iter().any(|m| {
                    m.name.name == method && m.modifiers.contains(&aura_ast::Modifier::Override)
                })
        })
        .collect()
}

pub(crate) fn has_field_in_hierarchy(checked: &CheckedFile, c: &ClassDecl, field: &str) -> bool {
    c.fields
        .iter()
        .any(|candidate| candidate.name.name == field)
        || direct_superclass(checked, c)
            .is_some_and(|parent| has_field_in_hierarchy(checked, parent, field))
}

fn emit_layout_fields(
    out: &mut String,
    checked: &CheckedFile,
    c: &ClassDecl,
    params: &[String],
    args: &[Ty],
) {
    if let Some((parent, parent_args)) = direct_superclass_with_args(checked, c, args) {
        let parent_params = parent
            .type_params
            .iter()
            .map(|param| param.name.name.clone())
            .collect::<Vec<_>>();
        emit_layout_fields(out, checked, parent, &parent_params, &parent_args);
    }
    for f in &c.fields {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&f.ty, checked, params, args),
            mangle_ident(&f.name.name)
        );
    }
}

fn simple_ctor_expr(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(id) => Some(mangle_ident(&id.name)),
        Expr::Int(value) => Some(format!("INT64_C({})", value.value)),
        Expr::Bool(value) => Some(if value.value { "true" } else { "false" }.into()),
        Expr::String(value) => Some(format!(
            "\"{}\"",
            value.value.replace('\\', "\\\\").replace('"', "\\\"")
        )),
        Expr::Group(inner, _) => simple_ctor_expr(inner),
        _ => None,
    }
}

/// Class fields own independent String buffers. Constructors clone every
/// non-null source so literals and caller-owned strings are both safe to drop.
fn emit_string_copy_assignment(out: &mut String, target: &str, source: &str) {
    let _ = writeln!(out, "  {{");
    let _ = writeln!(out, "    const char *__aura_string_src = {source};");
    let _ = writeln!(
        out,
        "    if (__aura_string_src == NULL) {{ {target} = NULL; }} else {{"
    );
    let _ = writeln!(
        out,
        "      size_t __aura_string_len = strlen(__aura_string_src);"
    );
    let _ = writeln!(
        out,
        "      {target} = (char *)malloc(__aura_string_len + 1);"
    );
    out.push_str("      if (");
    out.push_str(target);
    out.push_str(" == NULL) { aura_throw_string(\"class String field allocation failed\"); }\n");
    let _ = writeln!(
        out,
        "      memcpy((char *){target}, __aura_string_src, __aura_string_len + 1);"
    );
    out.push_str("    }\n  }\n");
}

pub(crate) fn emit_class_typedef(
    out: &mut String,
    checked: &CheckedFile,
    c: &ClassDecl,
    args: &[Ty],
) {
    let params: Vec<String> = c.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = class_decl_package(c, checked);
    let mono = type_mono(&pkg, &c.name.name, args);
    // Body only — incomplete `typedef struct X X` may already exist (C4u forwards).
    let _ = writeln!(out, "struct {} {{", c_class_type(&mono));
    if is_heap_class_decl(c) {
        out.push_str("  uint32_t __aura_class_tag;\n");
    }
    emit_layout_fields(out, checked, c, &params, args);
    if c.fields.is_empty() && direct_superclass(checked, c).is_none() {
        out.push_str("  char _pad;\n");
    }
    out.push_str("};\n\n");
}

pub(crate) fn emit_class_forwards(
    out: &mut String,
    checked: &CheckedFile,
    c: &ClassDecl,
    args: &[Ty],
) {
    let params: Vec<String> = c.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = class_decl_package(c, checked);
    let mono = type_mono(&pkg, &c.name.name, args);
    let _ = writeln!(
        out,
        "{};",
        c_ctor_signature_mono(c, checked, &params, args, &mono)
    );
    for m in &c.methods {
        let _ = writeln!(
            out,
            "{};",
            c_method_signature_mono(c, m, checked, &params, args, &mono)
        );
    }
    // C9a: upcast forwards for non-generic and mono generic class implements.
    let mut emitted_upcasts = HashSet::new();
    for iface_ref in &c.implements {
        if let Some(iface) = checked
            .ast
            .interfaces
            .iter()
            .find(|i| i.name.name == iface_ref.name.name)
        {
            let iargs = crate::iface::iface_args_for_class_implements(c, iface_ref, checked, args);
            if iface_ref.type_args.len() != iargs.len() {
                continue;
            }
            if iargs.iter().any(|a| a.is_open()) {
                continue;
            }
            for (target, target_args) in crate::iface::interface_and_parents(checked, iface, &iargs)
            {
                let imono = iface_mono_args(target, checked, &target_args);
                if !emitted_upcasts.insert(imono.clone()) {
                    continue;
                }
                let param_ty = if is_heap_class_decl(c) {
                    format!("{} *", c_class_type(&mono))
                } else {
                    c_class_type(&mono)
                };
                let _ = writeln!(
                    out,
                    "{} {}({param_ty} v);",
                    c_iface_type(&imono),
                    c_upcast_name(&mono, &imono),
                );
            }
        }
    }
}

fn ownership_fields<'a>(
    c: &'a ClassDecl,
    checked: &'a CheckedFile,
    params: &[String],
    args: &[Ty],
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some((parent, parent_args)) = direct_superclass_with_args(checked, c, args) {
        let parent_params = parent
            .type_params
            .iter()
            .map(|param| param.name.name.clone())
            .collect::<Vec<_>>();
        out.extend(ownership_fields(
            parent,
            checked,
            &parent_params,
            &parent_args,
        ));
    }
    out.extend(c.fields.iter().map(|field| {
        (
            field.name.name.clone(),
            type_ref_local_key(&field.ty, params, args),
        )
    }));
    out
}

/// C7b: field names that are builtin `Array` (any element type).
fn array_field_names(
    c: &ClassDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
) -> Vec<String> {
    ownership_fields(c, checked, params, args)
        .into_iter()
        .filter(|(_, key)| crate::array_emit::is_array_type_key(key))
        .map(|(name, _)| name)
        .collect()
}

/// C7b: field names that are Array-of-heap-class (need mark_extras).
fn array_of_class_field_names(
    c: &ClassDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
) -> Vec<String> {
    ownership_fields(c, checked, params, args)
        .into_iter()
        .filter(|(_, key)| {
            let mono = crate::expr::full_type_mono(key, checked);
            crate::array_emit::is_array_of_heap_class(&mono, checked)
        })
        .map(|(name, _)| name)
        .collect()
}

fn array_of_interface_field_specs(
    c: &ClassDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
) -> Vec<(String, String)> {
    ownership_fields(c, checked, params, args)
        .into_iter()
        .filter_map(|(name, key)| {
            let elem = crate::array_emit::array_elem_key(&key)?;
            crate::names::is_iface_type_key(elem, checked).then(|| (name, elem.to_string()))
        })
        .collect()
}

fn string_field_names(
    c: &ClassDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
) -> Vec<String> {
    ownership_fields(c, checked, params, args)
        .into_iter()
        .filter(|(_, key)| key == "String")
        .map(|(name, _)| name)
        .collect()
}

fn c_dtor_name(mono: &str) -> String {
    format!("aura_dtor_{mono}")
}

fn c_exception_dtor_name(mono: &str) -> String {
    format!("aura_ex_dtor_{mono}")
}

fn c_markex_name(mono: &str) -> String {
    format!("aura_markex_{mono}")
}

/// Emit ownership hooks for heap classes.  Exception payload copies use the
/// same generated destructor as the GC object contract, so owned String and
/// Array fields are released when the exception frame disposes the copy.
fn emit_class_gc_hooks(
    out: &mut String,
    c: &ClassDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
) {
    if !is_heap_class_decl(c) {
        return;
    }
    let cty = c_class_type(mono);
    let arr_fields = array_field_names(c, checked, params, args);
    let string_fields = string_field_names(c, checked, params, args);
    let arr_cls_fields = array_of_class_field_names(c, checked, params, args);
    let arr_iface_fields = array_of_interface_field_specs(c, checked, params, args);
    {
        let _ = writeln!(out, "static void {}(void *p) {{", c_dtor_name(mono));
        let _ = writeln!(out, "  {cty} *self = ({cty} *)p;");
        out.push_str("  if (self == NULL) { return; }\n");
        for name in &string_fields {
            let f = mangle_ident(name);
            let _ = writeln!(
                out,
                "  if (self->{f} != NULL) {{ free((void *)self->{f}); self->{f} = NULL; }}"
            );
        }
        for name in &arr_fields {
            let f = mangle_ident(name);
            let _ = writeln!(
                out,
                "  if (self->{f}.data != NULL) {{ free(self->{f}.data); self->{f}.data = NULL; self->{f}.len = 0; self->{f}.cap = 0; }}"
            );
        }
        out.push_str("}\n\n");
        let _ = writeln!(
            out,
            "static void {}(void *p) {{ {}(p); free(p); }}\n",
            c_exception_dtor_name(mono),
            c_dtor_name(mono)
        );
    }
    if !arr_cls_fields.is_empty() || !arr_iface_fields.is_empty() {
        let _ = writeln!(out, "static void {}(void *p) {{", c_markex_name(mono));
        let _ = writeln!(out, "  {cty} *self = ({cty} *)p;");
        out.push_str("  if (self == NULL) { return; }\n");
        for name in &arr_cls_fields {
            let f = mangle_ident(name);
            let _ = writeln!(out, "  {{");
            let _ = writeln!(out, "    void **__data = (void **)self->{f}.data;");
            let _ = writeln!(out, "    int64_t __len = self->{f}.len;");
            out.push_str("    if (__data != NULL && __len > 0) {\n");
            out.push_str("      for (int64_t __i = 0; __i < __len; __i++) {\n");
            out.push_str("        aura_gc_mark_ptr(__data[__i]);\n");
            out.push_str("      }\n");
            out.push_str("    }\n");
            out.push_str("  }\n");
        }
        for (name, iface_key) in &arr_iface_fields {
            let f = mangle_ident(name);
            let (iface, iface_args) = crate::names::resolve_iface_decl_and_args(iface_key, checked);
            let Some(iface) = iface else {
                continue;
            };
            let imono = crate::names::iface_mono_args(iface, checked, &iface_args);
            let _ = writeln!(out, "  {{");
            let _ = writeln!(
                out,
                "    {} *__data = ({0} *)self->{f}.data;",
                crate::names::c_iface_type(&imono),
            );
            out.push_str("    int64_t __len = self->");
            out.push_str(&f);
            out.push_str(".len;\n");
            out.push_str("    if (__data != NULL && __len > 0) {\n");
            out.push_str("      for (int64_t __i = 0; __i < __len; __i++) {\n");
            out.push_str("        switch (__data[__i].tag) {\n");
            for imp in crate::iface::mono_implementors_for_iface(checked, iface, &iface_args) {
                let mono = type_mono(
                    &class_decl_package(imp.class, checked),
                    &imp.class.name.name,
                    &imp.class_args,
                );
                let _ = writeln!(
                    out,
                    "          case AURA_TAG_{mono}: aura_gc_mark_ptr(__data[__i].data.as_{mono}); break;"
                );
            }
            out.push_str("          default: break;\n");
            out.push_str("        }\n");
            out.push_str("      }\n");
            out.push_str("    }\n");
            out.push_str("  }\n");
        }
        out.push_str("}\n\n");
    }
}

#[allow(clippy::if_same_then_else)]
pub(crate) fn emit_class_defs(
    out: &mut String,
    checked: &CheckedFile,
    c: &ClassDecl,
    args: &[Ty],
    detector: bool,
) {
    let params: Vec<String> = c.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = class_decl_package(c, checked);
    let mono = type_mono(&pkg, &c.name.name, args);
    emit_class_gc_hooks(out, c, checked, &params, args, &mono);
    emit_ctor_mono(out, c, checked, &params, args, &mono);
    out.push('\n');
    for m in &c.methods {
        if class_decl_package(c, checked) == "std.http"
            && c.name.name == "RequestBody"
            && m.name.name == "readChunk"
        {
            emit_http_request_body_read_chunk_method(out, c, m, checked, &params, args, &mono);
        } else if class_decl_package(c, checked) == "std.http"
            && c.name.name == "Response"
            && m.name.name == "writeChunk"
        {
            emit_http_response_write_chunk_method(out, c, m, checked, &params, args, &mono);
        } else if class_decl_package(c, checked) == "std.sync"
            && c.name.name == "AtomicInt"
            && emit_atomic_int_method(out, c, m, checked, &params, args, &mono)
        {
            // The AtomicInt methods use compiler atomics rather than ordinary
            // field loads/stores; the fallback body remains only for unknown
            // methods added to the class.
        } else if class_decl_package(c, checked) == "std.sync"
            && c.name.name == "Mutex"
            && emit_mutex_method(out, c, m, checked, &params, args, &mono)
        {
        } else if class_decl_package(c, checked) == "std.sync"
            && c.name.name == "RwLock"
            && emit_rwlock_method(out, c, m, checked, &params, args, &mono)
        {
        } else if class_decl_package(c, checked) == "std.sync"
            && c.name.name == "Once"
            && emit_once_method(out, c, m, checked, &params, args, &mono)
        {
        } else if class_decl_package(c, checked) == "std.metrics"
            && c.name.name == "Counter"
            && emit_counter_method(out, c, m, checked, &params, args, &mono)
        {
        } else if is_async_class_method(m) {
            if !emit_async_class_method(out, c, m, checked, detector, &mono, &params, args) {
                emit_method_mono(out, c, m, checked, &params, args, &mono, detector);
            }
        } else {
            emit_method_mono(out, c, m, checked, &params, args, &mono, detector);
        }
        out.push('\n');
    }
    // C9a: emit upcasts for this class monomorph's implements.
    let mut emitted_upcasts = HashSet::new();
    for iface_ref in &c.implements {
        if let Some(iface) = checked
            .ast
            .interfaces
            .iter()
            .find(|i| i.name.name == iface_ref.name.name)
        {
            let iargs = crate::iface::iface_args_for_class_implements(c, iface_ref, checked, args);
            if iface_ref.type_args.len() != iargs.len() {
                continue;
            }
            if iargs.iter().any(|a| a.is_open()) {
                continue;
            }
            for (target, target_args) in crate::iface::interface_and_parents(checked, iface, &iargs)
            {
                let imono = iface_mono_args(target, checked, &target_args);
                if !emitted_upcasts.insert(imono) {
                    continue;
                }
                emit_upcast(out, checked, c, target, &target_args, args);
                out.push('\n');
            }
        }
    }
}

fn emit_atomic_int_method(
    out: &mut String,
    c: &ClassDecl,
    m: &FunDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
) -> bool {
    let name = m.name.name.as_str();
    let signature = c_method_signature_mono(c, m, checked, params, args, mono);
    match name {
        "load" if m.params.is_empty() => {
            let _ = writeln!(out, "{signature} {{ if (this == NULL) return 0; return __atomic_load_n(&this->value, __ATOMIC_SEQ_CST); }}");
            true
        }
        "store" if m.params.len() == 1 => {
            let value = mangle_ident(&m.params[0].name.name);
            let _ = writeln!(out, "{signature} {{ if (this != NULL) __atomic_store_n(&this->value, {value}, __ATOMIC_SEQ_CST); }}");
            true
        }
        "fetchAdd" if m.params.len() == 1 => {
            let delta = mangle_ident(&m.params[0].name.name);
            let _ = writeln!(out, "{signature} {{ if (this == NULL) return 0; return __atomic_fetch_add(&this->value, {delta}, __ATOMIC_SEQ_CST); }}");
            true
        }
        "compareExchange" if m.params.len() == 2 => {
            let expected = mangle_ident(&m.params[0].name.name);
            let desired = mangle_ident(&m.params[1].name.name);
            let _ = writeln!(out, "{signature} {{ if (this == NULL) return false; int64_t __expected = {expected}; return __atomic_compare_exchange_n(&this->value, &__expected, {desired}, false, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }}");
            true
        }
        _ => false,
    }
}

fn emit_mutex_method(
    out: &mut String,
    c: &ClassDecl,
    m: &FunDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
) -> bool {
    let signature = c_method_signature_mono(c, m, checked, params, args, mono);
    match (m.name.name.as_str(), m.params.len()) {
        ("tryLock", 0) => {
            let _ = writeln!(out, "{signature} {{ if (this == NULL) return false; int64_t __expected = 0; return __atomic_compare_exchange_n(&this->state, &__expected, 1, false, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }}");
            true
        }
        ("unlock", 0) => {
            let _ = writeln!(out, "{signature} {{ if (this != NULL) __atomic_store_n(&this->state, 0, __ATOMIC_SEQ_CST); }}");
            true
        }
        ("isLocked", 0) => {
            let _ = writeln!(out, "{signature} {{ return this != NULL && __atomic_load_n(&this->state, __ATOMIC_SEQ_CST) != 0; }}");
            true
        }
        _ => false,
    }
}

fn emit_once_method(
    out: &mut String,
    c: &ClassDecl,
    m: &FunDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
) -> bool {
    let signature = c_method_signature_mono(c, m, checked, params, args, mono);
    match (m.name.name.as_str(), m.params.len()) {
        ("tryEnter", 0) => {
            let _ = writeln!(out, "{signature} {{ if (this == NULL) return false; int64_t __expected = 0; return __atomic_compare_exchange_n(&this->state, &__expected, 1, false, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }}");
            true
        }
        ("isDone", 0) => {
            let _ = writeln!(out, "{signature} {{ return this != NULL && __atomic_load_n(&this->state, __ATOMIC_SEQ_CST) != 0; }}");
            true
        }
        _ => false,
    }
}

fn emit_rwlock_method(
    out: &mut String,
    c: &ClassDecl,
    m: &FunDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
) -> bool {
    let signature = c_method_signature_mono(c, m, checked, params, args, mono);
    match (m.name.name.as_str(), m.params.len()) {
        ("tryRead", 0) => {
            let _ = writeln!(
                out,
                "{signature} {{ if (this == NULL) return false; int64_t __state = __atomic_load_n(&this->state, __ATOMIC_SEQ_CST); while (__state >= 0) {{ if (__state == INT64_MAX) return false; int64_t __next = __state + 1; if (__atomic_compare_exchange_n(&this->state, &__state, __next, false, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST)) return true; }} return false; }}"
            );
            true
        }
        ("tryWrite", 0) => {
            let _ = writeln!(out, "{signature} {{ if (this == NULL) return false; int64_t __expected = 0; return __atomic_compare_exchange_n(&this->state, &__expected, -1, false, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }}");
            true
        }
        ("unlockRead", 0) => {
            let _ = writeln!(out, "{signature} {{ if (this != NULL) {{ int64_t __state = __atomic_load_n(&this->state, __ATOMIC_SEQ_CST); while (__state > 0 && !__atomic_compare_exchange_n(&this->state, &__state, __state - 1, false, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST)) {{ }} }} }}");
            true
        }
        ("unlockWrite", 0) => {
            let _ = writeln!(out, "{signature} {{ if (this != NULL) {{ int64_t __expected = -1; (void)__atomic_compare_exchange_n(&this->state, &__expected, 0, false, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }} }}");
            true
        }
        ("readerCount", 0) => {
            let _ = writeln!(out, "{signature} {{ if (this == NULL) return 0; int64_t __state = __atomic_load_n(&this->state, __ATOMIC_SEQ_CST); return __state > 0 ? __state : 0; }}");
            true
        }
        ("isWriteLocked", 0) => {
            let _ = writeln!(out, "{signature} {{ return this != NULL && __atomic_load_n(&this->state, __ATOMIC_SEQ_CST) == -1; }}");
            true
        }
        _ => false,
    }
}

fn emit_counter_method(
    out: &mut String,
    c: &ClassDecl,
    m: &FunDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
) -> bool {
    let signature = c_method_signature_mono(c, m, checked, params, args, mono);
    match (m.name.name.as_str(), m.params.len()) {
        ("add", 1) => {
            let amount = mangle_ident(&m.params[0].name.name);
            let _ = writeln!(out, "{signature} {{ if (this == NULL) return 0; return __atomic_add_fetch(&this->value, {amount}, __ATOMIC_SEQ_CST); }}");
            true
        }
        ("increment", 0) => {
            let _ = writeln!(out, "{signature} {{ if (this == NULL) return 0; return __atomic_add_fetch(&this->value, 1, __ATOMIC_SEQ_CST); }}");
            true
        }
        ("get", 0) => {
            let _ = writeln!(out, "{signature} {{ if (this == NULL) return 0; return __atomic_load_n(&this->value, __ATOMIC_SEQ_CST); }}");
            true
        }
        ("reset", 0) => {
            let _ = writeln!(out, "{signature} {{ if (this != NULL) __atomic_store_n(&this->value, 0, __ATOMIC_SEQ_CST); }}");
            true
        }
        _ => false,
    }
}

fn is_async_class_method(m: &FunDecl) -> bool {
    m.return_type
        .as_ref()
        .is_some_and(|ret| ret.name.name == "Task" && ret.type_args.len() == 1)
}

#[allow(clippy::too_many_arguments)]
fn emit_async_class_method(
    out: &mut String,
    c: &ClassDecl,
    m: &FunDecl,
    checked: &CheckedFile,
    detector: bool,
    mono: &str,
    class_params: &[String],
    class_args: &[Ty],
) -> bool {
    let Some(task_ty) = m.return_type.as_ref() else {
        return false;
    };
    let Some(result_ty) = task_ty.type_args.first() else {
        return false;
    };
    let synthetic_name = if class_args.is_empty() {
        format!("{}_{}", c.name.name, m.name.name)
    } else {
        format!("{}_{}_{}", c.name.name, m.name.name, mono)
    };
    let concrete_this = TypeRef {
        qualifier: None,
        name: c.name.clone(),
        type_args: class_args
            .iter()
            .map(|ty| type_ref_from_ty(ty, c.span))
            .collect(),
        nullable: false,
        reference: false,
        span: c.span,
        fun: None,
    };
    let this_param = Param {
        attributes: Vec::new(),
        name: Ident {
            name: "this".into(),
            span: c.span,
        },
        ty: concrete_this,
        span: c.span,
    };
    let mut params = vec![this_param];
    params.extend(m.params.clone());
    let synthetic = AsyncFunDecl {
        is_pub: m.is_pub,
        origin_package: class_decl_package(c, checked),
        attributes: m.attributes.clone(),
        is_test: false,
        name: Ident {
            name: synthetic_name.clone(),
            span: m.name.span,
        },
        type_params: Vec::new(),
        params,
        return_type: Some(subst_type_ref(result_ty, class_params, class_args, c.span)),
        body: m.body.clone(),
        span: m.span,
    };
    crate::emit::emit_async_fun_decl(out, &synthetic, checked, detector);
    let wrapper = c_method_signature_mono(c, m, checked, &[], &[], mono);
    let call_name = c_fun_name(&class_decl_package(c, checked), &synthetic_name, &[]);
    let call_args = std::iter::once("this".to_string())
        .chain(m.params.iter().map(|p| mangle_ident(&p.name.name)))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(out, "{wrapper} {{ return {call_name}({call_args}); }}");
    true
}

fn subst_type_ref(ty: &TypeRef, params: &[String], args: &[Ty], span: Span) -> TypeRef {
    if let Some(index) = params.iter().position(|param| param == &ty.name.name) {
        if let Some(arg) = args.get(index) {
            let mut resolved = type_ref_from_ty(arg, span);
            resolved.nullable = ty.nullable;
            resolved.reference = ty.reference;
            return resolved;
        }
    }
    let mut resolved = ty.clone();
    resolved.type_args = ty
        .type_args
        .iter()
        .map(|arg| subst_type_ref(arg, params, args, span))
        .collect();
    resolved
}

fn type_ref_from_ty(ty: &Ty, span: Span) -> TypeRef {
    let (name, type_args) = match ty {
        Ty::Nullable(inner) => {
            return {
                let mut out = type_ref_from_ty(inner, span);
                out.nullable = true;
                out
            }
        }
        Ty::ClassApp { name, args }
        | Ty::EnumApp { name, args }
        | Ty::InterfaceApp { name, args } => (
            name.split('@').next().unwrap_or(name).to_string(),
            args.iter().map(|arg| type_ref_from_ty(arg, span)).collect(),
        ),
        Ty::Class(name) | Ty::Enum(name) | Ty::Interface(name) => (
            name.split('@').next().unwrap_or(name).to_string(),
            Vec::new(),
        ),
        _ => (ty.display(), Vec::new()),
    };
    TypeRef {
        qualifier: None,
        name: Ident { name, span },
        type_args,
        nullable: false,
        reference: false,
        span,
        fun: None,
    }
}

fn emit_http_response_write_chunk_method(
    out: &mut String,
    c: &ClassDecl,
    m: &FunDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
) {
    let data_ty = format!("aura_async_data_{mono}_writeChunk");
    let poll = format!("aura_async_poll_{mono}_writeChunk");
    let destroy = format!("aura_async_destroy_{mono}_writeChunk");
    let body = mangle_ident(&m.params[0].name.name);

    out.push_str("/* compiler-generated std.http.Response.writeChunk */\n");
    let _ = writeln!(
        out,
        "typedef struct {data_ty} {{ AuraFfiOpaqueHandle *response_handle; AuraFfiOpaqueHandle *connection_handle; AuraFfiHandlePin response_pin; AuraFfiHandlePin connection_pin; AuraHttpResponse *response; AuraHttpConnection *connection; bool response_pinned; bool connection_pinned; char *body; size_t body_length; char *output; size_t output_length; size_t output_offset; }} {data_ty};"
    );
    let _ = writeln!(
        out,
        "static void {destroy}(AuraTaskFrame *frame) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data != NULL) {{ free(data->body); free(data->output); if (data->connection_pinned) (void)aura_ffi_handle_unpin(&data->connection_pin); if (data->response_pinned) (void)aura_ffi_handle_unpin(&data->response_pin); }} }}"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (data == NULL || aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n  switch (aura_task_frame_resume_state(frame)) {\n    case 0: {\n      size_t headers = 0, chunk = 0, written = 0;\n      if (data->response_handle == NULL || data->connection_handle == NULL || data->body == NULL || data->body_length == 0) return AURA_TASK_FAILED;\n      if (aura_ffi_handle_pin_for_boundary(data->response_handle, AURA_FFI_BOUNDARY_TASK, &data->response_pin) != AURA_FFI_OK) return AURA_TASK_FAILED;\n      data->response_pinned = true; data->response = (AuraHttpResponse *)data->response_pin.resource;\n      if (aura_ffi_handle_pin_for_boundary(data->connection_handle, AURA_FFI_BOUNDARY_TASK, &data->connection_pin) != AURA_FFI_OK) return AURA_TASK_FAILED;\n      data->connection_pinned = true; data->connection = (AuraHttpConnection *)data->connection_pin.resource;\n      if (!aura_http_response_stream_started(data->response) && (aura_http_response_stream_begin(data->response, NULL, 0, &headers) != AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL || headers == 0)) return AURA_TASK_FAILED;\n      if (aura_http_response_stream_chunk(data->body, data->body_length, NULL, 0, &chunk) != AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL || chunk == 0 || headers > SIZE_MAX - chunk) return AURA_TASK_FAILED;\n      data->output_length = headers + chunk; data->output = (char *)malloc(data->output_length);\n      if (data->output == NULL || (headers != 0 && (aura_http_response_stream_begin(data->response, data->output, headers, &written) != AURA_HTTP_RESPONSE_OK || written != headers)) || aura_http_response_stream_chunk(data->body, data->body_length, data->output + headers, chunk, &written) != AURA_HTTP_RESPONSE_OK || written != chunk) return AURA_TASK_FAILED;\n      free(data->body); data->body = NULL; aura_task_frame_set_resume_state(frame, 1);\n    }\n    case 1: {\n      while (data->output_offset < data->output_length) { size_t written = 0; AuraTcpStatus status = aura_http_connection_stream_write(data->connection, data->output + data->output_offset, data->output_length - data->output_offset, &written); if (status == AURA_TCP_PENDING) { if (!aura_http_connection_wait_write(frame, data->connection)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; } if (status != AURA_TCP_OK || written == 0) return AURA_TASK_FAILED; data->output_offset += written; }\n      return AURA_TASK_COMPLETE;\n    }\n    default: return AURA_TASK_FAILED;\n  }\n}\n");
    let _ = writeln!(
        out,
        "{} {{",
        c_method_signature_mono(c, m, checked, params, args, mono)
    );
    let _ = writeln!(
        out,
        "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll}, {destroy});"
    );
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    let _ = writeln!(out, "  if (this == NULL || {body} == NULL || {body}[0] == '\\0') {{ aura_task_frame_destroy(frame); return NULL; }} data->response_handle = this->handle; data->connection_handle = this->connection; data->body_length = strlen({body}); data->body = aura_http_copy_bytes({body}, data->body_length); data->output = NULL; data->output_length = 0; data->output_offset = 0; data->response_pinned = false; data->connection_pinned = false; if (data->body == NULL) {{ aura_task_frame_destroy(frame); return NULL; }}");
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
}

fn emit_http_request_body_read_chunk_method(
    out: &mut String,
    c: &ClassDecl,
    m: &FunDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
) {
    let data_ty = format!("aura_async_data_{mono}_readChunk");
    let poll = format!("aura_async_poll_{mono}_readChunk");
    let destroy = format!("aura_async_destroy_{mono}_readChunk");
    let destroy_result = format!("aura_async_result_destroy_{mono}_readChunk");
    let destroy_error = format!("aura_async_error_destroy_{mono}_readChunk");
    let capacity = mangle_ident(&m.params[0].name.name);

    out.push_str("/* compiler-generated std.http.RequestBody.readChunk */\n");
    let _ = writeln!(
        out,
        "typedef struct {data_ty} {{ AuraFfiOpaqueHandle *handle; AuraFfiHandlePin pin; const AuraHttpRequest *request; bool pinned; bool read_claimed; size_t capacity; char *buffer; }} {data_ty};"
    );
    let _ = writeln!(
        out,
        "static void {destroy}(AuraTaskFrame *frame) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data != NULL) {{ free(data->buffer); if (data->read_claimed) aura_http_request_body_read_end(data->request); if (data->pinned) (void)aura_ffi_handle_unpin(&data->pin); }} }}"
    );
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *value, size_t size) {{ (void)size; if (value != NULL) {{ char **text = (char **)value; free(*text); free(text); }} }}"
    );
    let _ = writeln!(
        out,
        "static void {destroy_error}(void *value, size_t size) {{ (void)size; free(value); }}"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n  if (aura_task_frame_take_fd_wait_timeout(frame)) { const char *message = \"request body timeout\"; size_t length = strlen(message) + 1; char *error = (char *)malloc(length); if (error == NULL) return AURA_TASK_FAILED; memcpy(error, message, length); aura_task_frame_set_error_at(frame, error, length, ");
    out.push_str(&destroy_error);
    out.push_str(", UINT32_C(0)); return AURA_TASK_FAILED; }\n  switch (aura_task_frame_resume_state(frame)) {\n    case 0: {\n      if (data == NULL || data->handle == NULL || data->capacity == 0) return AURA_TASK_FAILED;\n      if (aura_ffi_handle_pin_for_boundary(data->handle, AURA_FFI_BOUNDARY_TASK, &data->pin) != AURA_FFI_OK) return AURA_TASK_FAILED;\n      data->pinned = true; data->request = (const AuraHttpRequest *)data->pin.resource; if (!aura_http_request_body_read_begin(data->request)) return AURA_TASK_FAILED; data->read_claimed = true; data->buffer = (char *)malloc(data->capacity + 1); if (data->buffer == NULL) return AURA_TASK_FAILED;\n      aura_task_frame_set_resume_state(frame, 1);\n    }\n    case 1: {\n      size_t count = 0; AuraTcpStatus status = aura_http_request_read_body(data->request, (unsigned char *)data->buffer, data->capacity, &count);\n      if (status == AURA_TCP_PENDING || status == AURA_TCP_TIMEOUT) { if (!aura_http_request_wait_body(frame, data->request)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n      if (status != AURA_TCP_OK && status != AURA_TCP_EOF) { const char *message = \"request body read failed\"; size_t length = strlen(message) + 1; char *error = (char *)malloc(length); if (error == NULL) return AURA_TASK_FAILED; memcpy(error, message, length); aura_task_frame_set_error_at(frame, error, length, ");
    out.push_str(&destroy_error);
    out.push_str(", UINT32_C(0)); return AURA_TASK_FAILED; }\n      data->buffer[count] = '\\0'; char **result = (char **)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = data->buffer; data->buffer = NULL; aura_http_request_body_read_end(data->request); data->read_claimed = false; aura_task_frame_set_result(frame, result, sizeof(*result), ");
    out.push_str(&destroy_result);
    out.push_str(
        "); return AURA_TASK_COMPLETE;\n    }\n    default: return AURA_TASK_FAILED;\n  }\n}\n",
    );
    let _ = writeln!(
        out,
        "{} {{",
        c_method_signature_mono(c, m, checked, params, args, mono)
    );
    let _ = writeln!(
        out,
        "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll}, {destroy});"
    );
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    let _ = writeln!(
        out,
        "  if (this == NULL || {capacity} <= 0) {{ aura_task_frame_destroy(frame); return NULL; }} data->handle = this->handle; data->request = NULL; data->capacity = (size_t){capacity}; if (data->capacity > 16384) data->capacity = 16384; data->pinned = false; data->read_claimed = false; data->buffer = NULL;"
    );
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
}

pub(crate) fn c_ctor_signature_mono(
    c: &ClassDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
) -> String {
    let ret = if is_heap_class_decl(c) {
        format!("{} *", c_class_type(mono))
    } else {
        c_class_type(mono)
    };
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
    format!(
        "{ret} {}({})",
        c_method_name(mono, &m.name.name),
        ps.join(", ")
    )
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
    let cty = c_class_type(mono);
    if is_heap_class_decl(c) {
        // C3y: allocate class instance on GC heap.
        // C7b: pass dtor / mark_extras when the class owns Array fields.
        let arr_cls = array_of_class_field_names(c, checked, params, args);
        let dtor = c_dtor_name(mono);
        let markex = if arr_cls.is_empty()
            && array_of_interface_field_specs(c, checked, params, args).is_empty()
        {
            "NULL".to_string()
        } else {
            c_markex_name(mono)
        };
        if dtor == "NULL" && markex == "NULL" {
            let _ = writeln!(
                out,
                "  {cty} *self = ({cty} *)aura_gc_alloc(sizeof({cty}));"
            );
        } else {
            let _ = writeln!(
                out,
                "  {cty} *self = ({cty} *)aura_gc_alloc_full(sizeof({cty}), {dtor}, {markex});"
            );
        }
        out.push_str("  memset(self, 0, sizeof(*self));\n");
        let _ = writeln!(
            out,
            "  self->__aura_class_tag = UINT32_C({});",
            class_tag(checked, c)
        );
        for f in &c.fields {
            let n = mangle_ident(&f.name.name);
            if type_ref_local_key(&f.ty, params, args) == "String" {
                emit_string_copy_assignment(out, &format!("self->{n}"), &n);
            } else {
                let _ = writeln!(out, "  self->{n} = {n};");
            }
        }
        if let Some((parent, parent_args)) = direct_superclass_with_args(checked, c, args) {
            let parent_params = parent
                .type_params
                .iter()
                .map(|param| param.name.name.clone())
                .collect::<Vec<_>>();
            for (field, arg) in parent.fields.iter().zip(c.superclass_args.iter()) {
                if let Some(value) = simple_ctor_expr(arg) {
                    let name = mangle_ident(&field.name.name);
                    if type_ref_local_key(&field.ty, &parent_params, &parent_args) == "String" {
                        emit_string_copy_assignment(out, &format!("self->{name}"), &value);
                    } else {
                        let _ = writeln!(out, "  self->{name} = {value};");
                    }
                }
            }
        }
        out.push_str("  return self;\n}\n");
    } else {
        let _ = writeln!(out, "  {cty} self;");
        for f in &c.fields {
            let n = mangle_ident(&f.name.name);
            let _ = writeln!(out, "  self.{n} = {n};");
        }
        out.push_str("  return self;\n}\n");
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_method_mono(
    out: &mut String,
    c: &ClassDecl,
    m: &FunDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
    detector: bool,
) {
    let _ = writeln!(
        out,
        "{} {{",
        c_method_signature_mono(c, m, checked, params, args, mono)
    );
    let ret_key = m
        .return_type
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, params, args, checked));
    let mut ctx = EmitCtx {
        checked,
        detector,
        method_class: Some(mono),
        type_params: params.to_vec(),
        type_args: args.to_vec(),
        locals: vec![HashMap::new()],
        array_owners: vec![std::collections::HashSet::new()],
        fun_owners: vec![std::collections::HashSet::new()],
        string_owners: vec![std::collections::HashSet::new()],
        channel_owners: vec![std::collections::HashSet::new()],
        task_result_owners: vec![std::collections::HashSet::new()],
        task_handle_owners: vec![std::collections::HashSet::new()],
        box_locals: vec![std::collections::HashSet::new()],
        box_owners: vec![std::collections::HashSet::new()],
        gc_roots: vec![std::collections::HashSet::new()],
        array_gc_roots: vec![std::collections::HashSet::new()],
        return_key: ret_key,
        lambda_ids: crate::emit::build_lambda_ids(checked),
        spawn_params: std::collections::HashSet::new(),
        mutable_spawn_captures: std::collections::HashSet::new(),
        async_frame: None,
        task_poller: false,
    };
    for f in &c.fields {
        let key = type_ref_local_key_expand(&f.ty, params, args, checked);
        let mono_key = crate::expr::full_type_mono(&key, checked);
        ctx.define_local(&f.name.name, mono_key);
    }
    for p in &m.params {
        let key = type_ref_local_key_expand(&p.ty, params, args, checked);
        let mono_key = crate::expr::full_type_mono(&key, checked);
        ctx.define_local(&p.name.name, mono_key.clone());
        // C6b/C21d: owning Array params own the buffer; `ref Array<T>` params
        // are header views over their caller's storage and must not free it.
        if !p.ty.reference && crate::array_emit::is_array_type_key(&key) {
            ctx.mark_array_owner(&p.name.name);
        }
        // Fun params own capture env (caller moves).
        if is_fun_type_key(&key) {
            ctx.mark_fun_owner(&p.name.name);
        }
        if is_heap_class_mono(&mono_key, checked) {
            ctx.mark_gc_root(&p.name.name);
            let n = mangle_ident(&p.name.name);
            let _ = writeln!(out, "  aura_gc_add_root((void **)&{n});");
        }
        // C6e: Array-of-class params keep element GC pointers alive.
        if crate::array_emit::is_array_of_heap_class(&mono_key, checked) {
            ctx.mark_array_gc_root(&p.name.name);
            let n = mangle_ident(&p.name.name);
            let _ = writeln!(
                out,
                "  aura_gc_add_array_root((void **)&{n}.data, &{n}.len);"
            );
        }
    }
    // `this` is a heap pointer for classes — root it for the method body.
    if is_heap_class_mono(mono, checked) {
        ctx.mark_gc_root("this");
        out.push_str("  aura_gc_add_root((void **)&this);\n");
    }
    emit_block(out, &m.body, 1, &mut ctx);
    crate::stmt::emit_release_task_handle_owners(out, 1, &ctx, &ctx.task_handle_owners_all());
    for name in ctx.array_gc_roots_all() {
        let n = mangle_ident(&name);
        let _ = writeln!(out, "  aura_gc_remove_array_root((void **)&{n}.data);");
    }
    for name in ctx.gc_roots_all() {
        let n = if name == "this" {
            "this".to_string()
        } else {
            mangle_ident(&name)
        };
        let _ = writeln!(out, "  aura_gc_remove_root((void **)&{n});");
    }
    crate::stmt::emit_free_fun_owners(out, 1, &ctx, &ctx.fun_owners_all());
    crate::stmt::emit_release_box_locals(out, 1, &ctx, &ctx.box_owners_all());
    emit_return_fallback(out, &m.return_type, checked, params, args);
    out.push_str("}\n");
}
